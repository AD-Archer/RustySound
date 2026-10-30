// Redesign suite, compact layout (iPhone). Expected to fail until phase 2 lands.
import { audioState, expect, test } from "../../support/fixtures";
import { expectTrackMenuComplete, gotoReady, isCompact, longPress, openAlbum, playFirstTrack, tid } from "./helpers";

test.describe("compact layout", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(!isCompact(page), "compact-only (width < 760)");
  });

  test("tab bar navigates Discover / Library / Search", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(page.locator(".rs-app[data-layout='compact']")).toBeAttached();
    // The compact mini player only appears once something is loaded; nothing is yet.
    await expect(tid(page, "mini-player"), "no empty mini player on compact").toHaveCount(0);
    await expect(page.locator("[data-testid^='nav-']"), "no sidebar on compact").toHaveCount(0);

    await tid(page, "tab-library").click();
    await expect(page).toHaveURL(/\/library$/);
    await tid(page, "tab-search").click();
    await expect(tid(page, "search-input")).toBeVisible();
    await expect(page.locator("#global-search-input")).toBeAttached();
    await tid(page, "tab-discover").click();
    await expect(page).toHaveURL(/\/$/);
  });

  test("Discover shows Cover Flow and mixes", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(tid(page, "coverflow")).toBeVisible();
    await expect(tid(page, "coverflow-center")).toBeVisible();
    await expect(tid(page, "coverflow-center").locator("img").first()).toBeVisible();
    await expect.poll(() => tid(page, "mix-row").count()).toBeGreaterThan(0);
    await expect(tid(page, "settings-open")).toBeVisible();
  });

  test("long-press on a track row opens the track menu with every action", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await longPress(page, tid(page, "track-row").first());
    await expectTrackMenuComplete(page);
  });

  test("a short tap on a track row plays it instead of opening the menu", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "track-row").first().tap();
    await expect(tid(page, "track-menu")).toBeHidden();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[0].id}`);
  });

  test("mini player opens Now Playing; lyrics are visible", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    const mini = tid(page, "mini-player");
    await expect(mini).toBeVisible();
    // MediaSession presses play/pause, previous and next by id, so those must exist even on the
    // compact layout. The compact mini player stays minimal: shuffle, repeat, rating and add live
    // in Now Playing (checked below) and the track menu.
    for (const id of ["play-pause-btn", "prev-btn", "next-btn"]) await expect(page.locator(`#${id}`).first(), id).toBeAttached();
    await mini.click();
    const now = tid(page, "now-playing");
    await expect(now).toBeVisible();
    // The player bar keeps the MediaSession ids while the sheet is open, so the sheet's own
    // transport uses np-* ids (ids stay unique) and calls the same actions.
    for (const id of ["np-play-pause-btn", "np-prev-btn", "np-next-btn", "np-shuffle-btn", "np-repeat-btn"]) await expect(now.locator(`#${id}`)).toBeVisible();
    for (const id of ["play-pause-btn", "prev-btn", "next-btn"]) await expect(page.locator(`#${id}`)).toHaveCount(1);

    const lyrics = tid(page, "lyrics");
    if (!(await lyrics.isVisible().catch(() => false))) {
      await now.getByRole("button", { name: /lyrics/i }).or(now.getByRole("tab", { name: /lyrics/i })).first().click();
    }
    await expect(lyrics).toBeVisible();
    // Mock lyrics start with "(<song title>)" (tests/mock-server/server.ts lyricLines).
    await expect(lyrics).toContainText(`(${mock.albumSongs[0].title})`, { timeout: 20_000 });
  });

  test("settings opens from Discover and shows the server form", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await tid(page, "settings-open").click();
    await expect(page).toHaveURL(/\/settings/);
    await expect(page.getByText("Mock Navidrome").first()).toBeVisible();
  });
});
