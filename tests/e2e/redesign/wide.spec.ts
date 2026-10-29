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
    await expect(page.locator("[data-layout='wide']")).toBeAttached();
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
});
