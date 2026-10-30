// Redesign suite, wide layout (Mac). Expected to fail until phase 2 lands.
import { audioState, expect, test } from "../../support/fixtures";
import { expectTrackMenuComplete, gotoReady, isCompact, KEPT_PLAYER_IDS, openAlbum, tid } from "./helpers";

const SIDEBAR: [testid: string, url: RegExp][] = [
  ["nav-search", /\/search$/],
  ["nav-playlists", /\/playlists$/],
  ["nav-artists", /\/artists$/],
  ["nav-albums", /\/albums$/],
  ["nav-songs", /\/songs$/],
  ["nav-favorites", /\/favorites$/],
  ["nav-downloads", /\/downloads$/],
  ["nav-radio", /\/radio$/],
  ["nav-stats", /\/stats$/],
  ["nav-discover", /\/$/],
];

test.describe("wide layout", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(isCompact(page), "wide-only (width >= 760)");
  });

  test("sidebar navigation reaches every library area", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(page.locator(".rs-app[data-layout='wide']")).toBeAttached();
    await expect(page.locator("[data-testid^='tab-']"), "no tab bar on wide").toHaveCount(0);
    for (const [id, url] of SIDEBAR) {
      await tid(page, id).click();
      await expect(page, id).toHaveURL(url);
    }
    await tid(page, "settings-open").click();
    await expect(page).toHaveURL(/\/settings/);
  });

  test("player bar carries the ids MediaSession presses", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const bar = tid(page, "player-bar");
    await expect(bar).toBeVisible();
    for (const id of KEPT_PLAYER_IDS) await expect(bar.locator(`#${id}`), id).toBeAttached();
  });

  test("Cover Flow responds to arrow keys", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const flow = tid(page, "coverflow");
    await expect(flow).toBeVisible();
    const center = tid(page, "coverflow-center");
    // Covers are images; the centred cover's accessible name says which album it is.
    const name = () => center.getAttribute("aria-label");
    const first = await name();
    expect(first).toBeTruthy();
    await flow.focus();
    await page.keyboard.press("ArrowRight");
    await expect.poll(name).not.toBe(first);
    await page.keyboard.press("ArrowLeft");
    await expect.poll(name).toBe(first);
  });

  test("Home shelves fill the content width and page with the arrows", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const shelf = tid(page, "shelf-new");
    await expect(shelf).toBeVisible();
    const row = shelf.locator(".rs-shelf__row");
    const width = page.viewportSize()!.width;
    // Cards reach the right edge of the window (less the page gutter): no empty right side.
    const cards = row.locator("[role='listitem']");
    const boxes = await cards.evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ left: r.left, right: r.right })));
    const visible = boxes.filter((b) => b.right <= width + 1);
    expect(visible.length, "whole cards on screen").toBeGreaterThanOrEqual(5);
    expect(width - Math.max(...visible.map((b) => b.right)), "gap at the right").toBeLessThan(48);
    // The stage spans the content width too.
    const stage = await page.locator(".rs-home__stage").boundingBox();
    expect(width - (stage!.x + stage!.width)).toBeLessThan(2);
    // Next pages the shelf; Previous comes back.
    const scrollLeft = () => row.evaluate((el) => el.scrollLeft);
    await shelf.getByRole("button", { name: /^Next/ }).click();
    await expect.poll(scrollLeft).toBeGreaterThan(100);
    await shelf.getByRole("button", { name: /^Previous/ }).click();
    await expect.poll(scrollLeft).toBeLessThan(2);
  });

  test("right-click on a track row opens the track menu with every action", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "track-row").first().click({ button: "right" });
    await expectTrackMenuComplete(page);
    await page.keyboard.press("Escape");
    await expect(tid(page, "track-menu")).toBeHidden();
  });

  test("album play starts track 1; album shuffle starts playback too", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "album-play").click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[0].id}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
    await page.locator("#play-pause-btn").first().click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await tid(page, "album-shuffle").click();
    await expect.poll(async () => (await audioState(page))?.paused, { timeout: 20_000 }).toBe(false);
  });

  test("search input keeps the macOS menu id and finds a fixture artist", async ({ connected: page, mock }) => {
    await gotoReady(page, "/search");
    const input = tid(page, "search-input");
    await expect(input).toHaveAttribute("id", "global-search-input");
    await input.fill("Paper Comets");
    await expect(page.getByText(mock.artist.name, { exact: true }).first()).toBeVisible();
  });

  test("Albums: sort and Grid / Cover Flow share one menu", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect(tid(page, "albums-view-toggle"), "no separate view toggle").toHaveCount(0);
    await tid(page, "albums-sort").click();
    await tid(page, "albums-view-flow").click();
    await expect(tid(page, "coverflow")).toBeVisible();
    await tid(page, "albums-sort").click();
    await tid(page, "albums-view-grid").click();
    await expect(tid(page, "coverflow")).toHaveCount(0);
  });

  test("Settings: the off-server switch comes first in Discovery; a server row opens its editor", async ({ connected: page }) => {
    await gotoReady(page, "/settings");
    await tid(page, "server-edit").first().click();
    const form = tid(page, "server-form");
    await expect(form).toBeVisible();
    await expect(tid(page, "server-delete")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(form).toHaveCount(0);

    await page.getByRole("button", { name: /^Discovery/ }).first().click();
    const first = page.locator(".rs-settings-row").first();
    await expect(first).toContainText("Show music not on your server");
    await expect(first.locator('[data-testid="discovery-offserver"]')).toBeVisible();
  });
});
