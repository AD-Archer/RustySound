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

  test("Home shows Recently Played on the stage, then horizontal shelves", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible();
    await expect(tid(page, "coverflow")).toBeVisible();
    await expect(tid(page, "coverflow-center")).toBeVisible();
    await expect(tid(page, "coverflow-center").locator("img").first()).toBeVisible();
    // The caption says when the centred album was played; play is one round icon button.
    await expect(page.locator(".rs-flow-caption em")).toContainText(/played/i);
    await expect(tid(page, "coverflow-play")).toBeVisible();
    await expect(tid(page, "settings-open")).toBeVisible();
    // Shelves in order; the old For You / New / Rewind segments are gone.
    for (const shelf of ["shelf-playlists", "shelf-songs", "shelf-albums", "shelf-new"]) {
      await expect(tid(page, shelf), shelf).toBeVisible();
    }
    await expect(tid(page, "discover-segments")).toHaveCount(0);
    const order = await page.locator("section[data-testid^='shelf-']").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
    expect(order.indexOf("shelf-playlists")).toBeLessThan(order.indexOf("shelf-songs"));
    expect(order.indexOf("shelf-songs")).toBeLessThan(order.indexOf("shelf-albums"));
    expect(order.indexOf("shelf-albums")).toBeLessThan(order.indexOf("shelf-new"));
    if (order.includes("shelf-offserver")) expect(order.at(-1)).toBe("shelf-offserver");
    // Playlists mix the user's playlists with generated mixes (tagged "Mix").
    const playlists = tid(page, "shelf-playlists");
    await expect.poll(() => playlists.locator("[data-testid='mix-card']").count()).toBeGreaterThan(0);
    await expect(playlists.locator("[data-testid='playlist-card']").first()).toBeVisible();
    await expect(playlists.locator("[data-testid='mix-card']").first().locator(".rs-badge")).toHaveText("Mix");
    // About 2.3 cards per screen: the third card peeks in at the right edge.
    const width = page.viewportSize()!.width;
    const third = await tid(page, "shelf-new").locator("[role='listitem']").nth(2).boundingBox();
    expect(third, "third card").not.toBeNull();
    expect(third!.x).toBeLessThan(width);
    expect(third!.x + third!.width).toBeGreaterThan(width);
  });

  test("a mix card's menu offers Save as Playlist; a song card plays", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const mix = tid(page, "shelf-playlists").locator("[data-testid='mix-card']").first();
    await mix.locator("[data-testid='card-more']").click();
    await expect(tid(page, "mix-menu")).toBeVisible();
    await expect(tid(page, "mix-menu-save")).toBeVisible();
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(tid(page, "mix-menu")).toHaveCount(0);
    const song = tid(page, "shelf-songs").locator("[data-testid='song-card']").first();
    await song.locator(".rs-hcard__main").tap();
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain("stream");
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

  test("settings opens from Home and shows the server form", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await tid(page, "settings-open").click();
    await expect(page).toHaveURL(/\/settings/);
    await expect(page.getByText("Mock Navidrome").first()).toBeVisible();
  });
});
