// Generated mixes have their own page (/mix/<slug>) and carry the RustySound mark, so they
// read as RustySound's rather than as the user's server playlists. Runs on both layouts.
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady } from "../../support/fixtures";
import { gotoReady, tid } from "./helpers";

test.describe("mix page", () => {
  test("Home's mix cards carry the RustySound mark; playlist cards don't", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const shelf = tid(page, "shelf-playlists");
    const mix = shelf.locator("[data-testid='mix-card']").first();
    await expect(mix).toBeVisible();
    await expect(mix.locator("[data-testid='rs-mark']")).toBeVisible();
    await expect(mix.locator(".rs-hcard__sub")).toHaveText("RustySound");
    // The mark is decorative (the text names RustySound).
    await expect(mix.locator("[data-testid='rs-mark']")).toHaveAttribute("alt", "");
    await expect(mix.locator(".rs-badge"), "the old text tag is gone").toHaveCount(0);
    const playlist = shelf.locator("[data-testid='playlist-card']").first();
    await expect(playlist).toBeVisible();
    await expect(playlist.locator("[data-testid='rs-mark']")).toHaveCount(0);
    // The mark sits inside the art without spilling over the title.
    const art = await mix.locator(".rs-hcard__art").boundingBox();
    const mark = await mix.locator("[data-testid='rs-mark']").boundingBox();
    expect(art && mark).toBeTruthy();
    expect(mark!.x).toBeGreaterThanOrEqual(art!.x);
    expect(mark!.y + mark!.height).toBeLessThanOrEqual(art!.y + art!.height);
  });

  test("a mix card opens its page; Play plays; a reload deep-links back", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const card = tid(page, "shelf-playlists").locator("[data-testid='mix-card']").first();
    const name = (await card.locator(".rs-hcard__title").textContent())!.trim();
    await card.locator(".rs-hcard__main").click();
    await expect(page).toHaveURL(/\/mix\/[^/]+$/);
    const detail = tid(page, "mix-detail");
    await expect(detail).toBeVisible();
    await expect(detail.getByRole("heading", { level: 1 })).toHaveText(name);
    await expect(tid(detail, "mix-byline")).toHaveText("Made by RustySound");
    await expect(tid(detail, "mix-byline").locator("[data-testid='rs-mark']")).toBeVisible();
    await expect(detail.locator(".rs-library-hero__meta")).toContainText(/\d+ songs/);
    await expect(tid(detail, "track-row").first()).toBeVisible();

    await tid(detail, "mix-play").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "", { timeout: 20_000 }).toContain("stream");

    // The "…" menu holds Add to Queue and Save as Playlist.
    await tid(detail, "mix-more").click();
    await expect(tid(page, "mix-page-menu-queue")).toBeVisible();
    await expect(tid(page, "mix-page-menu-save")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(tid(page, "mix-page-menu")).toHaveCount(0);

    // Deep link: the same page after a reload (the mix is rebuilt or read from the cache).
    const url = page.url();
    await page.reload();
    await waitForAppReady(page);
    await expect(page).toHaveURL(url);
    await expect(tid(page, "mix-detail").getByRole("heading", { level: 1 })).toHaveText(name, { timeout: 30_000 });
    await expect(tid(page, "track-row").first()).toBeVisible();
  });

  test("an artist radio link from another day is rebuilt from its slug", async ({ connected: page, mock }) => {
    // Cold start, straight to a radio that today's Home may not list: the slug carries the
    // server and artist ids, so the page rebuilds it from the server.
    await gotoReady(page, `/mix/radio~${MOCK_SERVER_ID}~${mock.artist.id}`);
    const detail = tid(page, "mix-detail");
    await expect(detail).toBeVisible({ timeout: 30_000 });
    await expect(detail.getByRole("heading", { level: 1 })).toHaveText(`${mock.artist.name} Radio`);
    await expect(tid(page, "track-row").first()).toBeVisible();
  });

  test("an unknown mix shows a calm empty state with a way Home", async ({ connected: page }) => {
    await gotoReady(page, "/mix/no-such-mix");
    await expect(tid(page, "mix-missing")).toBeVisible({ timeout: 30_000 });
    await expect(tid(page, "mix-missing")).toContainText("This mix isn't available");
    await tid(page, "mix-missing-home").click();
    await expect(page).toHaveURL(/\/$/);
  });
});
