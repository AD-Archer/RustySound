// Baseline smoke suite against the CURRENT (pre-redesign) app. Selectors are ones that exist
// today: player button ids, global-search-input, aria-labels, roles and visible text.
import type { Page } from "@playwright/test";
import { audioState, expect, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";

async function openKnownAlbum(page: Page, mock: MockFixtures) {
  await page.goto("/");
  await waitForAppReady(page);
  // "Lanterns Over Harbor" is the most played fixture album, so it sits in "Most Played Albums".
  await page.getByText(mock.album.name, { exact: true }).first().click();
  await expect(page).toHaveURL(new RegExp(`/album/${mock.album.id}/`));
  await expect(page.getByText(mock.albumSongs[0].title, { exact: true }).first()).toBeVisible();
}

async function playFirstTrack(page: Page, mock: MockFixtures) {
  await openKnownAlbum(page, mock);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect
    .poll(async () => (await audioState(page))?.src ?? "", { message: "audio element gets the stream URL" })
    .toContain(`id=${mock.albumSongs[0].id}`);
  await expect
    .poll(async () => (await audioState(page))?.currentTime ?? 0, { message: "playback position advances", timeout: 20_000 })
    .toBeGreaterThan(1.0);
}

test.describe("baseline smoke", () => {
  test("app assets referenced by the web build all load", async ({ connected: page, baseURL }) => {
    const failed: string[] = [];
    page.on("response", (r) => {
      if (r.url().startsWith(baseURL!) && r.status() >= 400) failed.push(`${r.status()} ${new URL(r.url()).pathname}`);
    });
    await page.goto("/");
    await waitForAppReady(page);
    await page.waitForLoadState("networkidle");
    expect(failed, "same-origin requests that failed").toEqual([]);
  });

  test("home loads with library sections and artwork", async ({ connected: page }) => {
    await page.goto("/");
    await waitForAppReady(page);
    await expect(page.getByRole("heading", { name: "Recently Added Albums" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Most Played Albums" })).toBeVisible();
    // At least one cover from the mock server decoded.
    await expect
      .poll(() =>
        page.evaluate(() =>
          [...document.querySelectorAll<HTMLImageElement>('img[src*="getCoverArt"]')].filter((i) => i.complete && i.naturalWidth > 0).length,
        ),
      )
      .toBeGreaterThan(0);
  });

  test("opens an album", async ({ connected: page, mock }) => {
    await openKnownAlbum(page, mock);
    await expect(page.getByText(mock.album.artist).first()).toBeVisible();
    for (const s of mock.albumSongs.slice(0, 3)) {
      await expect(page.getByText(s.title, { exact: true }).first()).toBeAttached();
    }
  });

  test("plays a track and play/pause toggles", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    const btn = page.locator("#play-pause-btn");
    const playingIcon = await btn.innerHTML();

    await btn.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await expect.poll(() => btn.innerHTML()).not.toBe(playingIcon);
    const pausedAt = (await audioState(page))!.currentTime;
    await page.waitForTimeout(1200);
    expect((await audioState(page))!.currentTime).toBeCloseTo(pausedAt, 1);

    await btn.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);
    await expect.poll(() => btn.innerHTML()).toBe(playingIcon);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0).toBeGreaterThan(pausedAt + 0.5);
  });

  test("first tap after a cold deep-link load starts playback", async ({ connected: page, mock }) => {
    // Found while building the harness: on a fresh load, the first click that should start
    // playback leaves the audio element paused; any earlier click on the page avoids it.
    await page.goto(`/album/${mock.album.id}/mock-server-0001`);
    await waitForAppReady(page);
    const row = page.getByText(mock.albumSongs[0].title, { exact: true }).first();
    await row.click();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[0].id}`);
    await expect
      .poll(async () => (await audioState(page))?.currentTime ?? 0, { message: "playback starts on the first tap", timeout: 10_000 })
      .toBeGreaterThan(0.5);
  });

  test("playlist header shows the playlist's total duration", async ({ connected: page, mock }) => {
    // Subsonic returns playlist `duration` in seconds; the header must not read it as milliseconds.
    await page.goto(`/playlist/${mock.bigPlaylist.id}/mock-server-0001`);
    await waitForAppReady(page);
    await expect(page.getByText(`${mock.bigPlaylist.songCount} songs`)).toBeVisible();
    const secs = Math.floor(mock.bigPlaylist.duration / 1000);
    const misread = `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
    await expect(page.getByText(misread, { exact: true }), `duration rendered as ms (${misread})`).toHaveCount(0);
  });

  test("next track advances the queue", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    await page.locator("#next-btn").click();
    await expect
      .poll(async () => (await audioState(page))?.src ?? "", { message: "stream switches to track 2" })
      .toContain(`id=${mock.albumSongs[1].id}`);
    await expect
      .poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 })
      .toBeGreaterThan(0.5);
  });

  test("search finds a known fixture artist", async ({ connected: page, mock }) => {
    await page.goto("/search");
    await waitForAppReady(page);
    const input = page.locator("#global-search-input");
    await input.fill("Paper Comets");
    await expect(page.getByText(mock.artist.name, { exact: true }).first()).toBeVisible();
  });

  test("opens the queue from the header", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    await page.getByRole("button", { name: "Open queue" }).click();
    await expect(page).toHaveURL(/\/queue$/);
    await expect(page.getByText(mock.albumSongs[0].title, { exact: true }).first()).toBeVisible();
  });

  test("settings opens from the navigation menu", async ({ connected: page }) => {
    await page.goto("/");
    await waitForAppReady(page);
    await page.getByRole("button", { name: "Open menu" }).click();
    await page.getByRole("button", { name: "Settings" }).or(page.getByRole("link", { name: "Settings" })).first().click();
    await expect(page).toHaveURL(/\/settings$/);
    await expect(page.getByText("Mock Navidrome").first()).toBeVisible();
  });
});
