// Baseline smoke suite: the core behaviours the app had before the Stage redesign (connect,
// Home, albums, playback, queue, search, settings, assets, the first-tap and playlist-duration
// regressions), ported to the redesigned UI. Selectors go through the data-testid contract
// (PLAN.md), the MediaSession player ids and roles; navigation goes through the real UI
// (sidebar on wide, tab bar and Library on compact) unless a test is about a deep link.
import type { Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "../redesign/helpers";

/** Open the Library area that holds `area` (albums, songs, …): sidebar on wide, Library tab on compact. */
async function openLibraryArea(page: Page, area: "albums" | "queue") {
  if (isCompact(page)) {
    await tid(page, "tab-library").click();
    await expect(page).toHaveURL(/\/library$/);
    await tid(page, `library-${area}`).click();
  } else if (area === "albums") {
    await tid(page, "nav-albums").click();
  } else {
    await tid(page, "nav-library").click();
    await tid(page, "library-queue").click();
  }
}

/** Home → Albums → filter by name → open the tile. */
async function openKnownAlbumInApp(page: Page, mock: MockFixtures) {
  await gotoReady(page, "/");
  await openLibraryArea(page, "albums");
  await expect(page).toHaveURL(/\/albums$/);
  await tid(page, "albums-search").fill(mock.album.name);
  await tid(page, "album-tile").filter({ hasText: mock.album.name }).first().click();
  await expect(page).toHaveURL(new RegExp(`/album/${mock.album.id}/`));
  await expect(tid(page, "track-row").first()).toBeVisible();
}

/** Deep link to the fixture album (the mock's id and server id). */
async function openKnownAlbumByRoute(page: Page, mock: MockFixtures) {
  await gotoReady(page, `/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await expect(tid(page, "track-row").first()).toBeVisible();
}

/** The row's main button plays the track (the sibling "…" opens the track menu). */
const trackRow = (page: Page, title: string) => tid(page, "track-row").filter({ hasText: title }).first();
const tapTrack = (page: Page, title: string) => trackRow(page, title).locator("button").first().click();

async function playFirstTrack(page: Page, mock: MockFixtures) {
  await openKnownAlbumByRoute(page, mock);
  await tapTrack(page, mock.albumSongs[0].title);
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

  test("home loads with library content and artwork", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(page.getByRole("heading", { level: 1, name: "Home" })).toBeVisible();
    // Recently played on the stage, then the library shelves.
    await expect(tid(page, "coverflow-center")).toBeVisible();
    await expect(tid(page, "shelf-albums")).toBeVisible();
    await expect(tid(page, "shelf-new").locator("[role='listitem']").first()).toBeVisible();
    // At least one cover from the mock server decoded.
    await expect
      .poll(() =>
        page.evaluate(() =>
          [...document.querySelectorAll<HTMLImageElement>('img[src*="getCoverArt"]')].filter((i) => i.complete && i.naturalWidth > 0).length,
        ),
      )
      .toBeGreaterThan(0);
  });

  test("opens an album from the library", async ({ connected: page, mock }) => {
    await openKnownAlbumInApp(page, mock);
    const detail = tid(page, "album-detail");
    await expect(detail.getByRole("heading", { name: mock.album.name })).toBeVisible();
    await expect(detail.getByText(mock.album.artist).first()).toBeVisible();
    await expect(tid(detail, "track-row")).toHaveCount(mock.albumSongs.length);
    for (const s of mock.albumSongs.slice(0, 3)) await expect(trackRow(page, s.title)).toBeAttached();
  });

  test("plays a track and play/pause toggles", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    const btn = page.locator("#play-pause-btn");
    const playingIcon = await btn.innerHTML();

    await btn.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await expect.poll(() => btn.innerHTML()).not.toBe(playingIcon);
    // Paused means the position holds still: two readings 1 s apart agree.
    const pausedAt = (await audioState(page))!.currentTime;
    await page.waitForTimeout(1000);
    expect((await audioState(page))!.currentTime).toBeCloseTo(pausedAt, 1);

    await btn.click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);
    await expect.poll(() => btn.innerHTML()).toBe(playingIcon);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0).toBeGreaterThan(pausedAt + 0.5);
  });

  test("first tap after a cold deep-link load starts playback", async ({ connected: page, mock }) => {
    // Found while building the harness: on a fresh load, the first click that should start
    // playback left the audio element paused; any earlier click on the page avoided it.
    await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    await tapTrack(page, mock.albumSongs[0].title);
    await expect.poll(async () => (await audioState(page))?.src ?? "").toContain(`id=${mock.albumSongs[0].id}`);
    await expect
      .poll(async () => (await audioState(page))?.currentTime ?? 0, { message: "playback starts on the first tap", timeout: 10_000 })
      .toBeGreaterThan(0.5);
  });

  test("playlist header shows the playlist's total duration", async ({ connected: page, mock }) => {
    // Subsonic returns playlist `duration` in seconds; the header must not read it as milliseconds.
    await gotoReady(page, `/playlist/${mock.bigPlaylist.id}/${MOCK_SERVER_ID}`);
    const secs = mock.bigPlaylist.duration;
    const hours = Math.floor(secs / 3600);
    const mins = [Math.floor((secs % 3600) / 60), Math.round((secs % 3600) / 60)];
    const expected = new RegExp(`${mock.bigPlaylist.songCount} songs, ${hours} hr (${mins.join("|")}) min`);
    // The header line (the list footer repeats it).
    await expect(tid(page, "playlist-detail").locator(".rs-library-hero__meta")).toHaveText(expected);
    const ms = Math.floor(secs / 1000);
    const misread = `${Math.floor(ms / 60)}:${String(ms % 60).padStart(2, "0")}`;
    await expect(page.getByText(misread, { exact: true }), `duration rendered as ms (${misread})`).toHaveCount(0);
  });

  test("next track advances the queue", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    // #next-btn is the id MediaSession presses (player bar on wide, mini player on compact).
    await page.locator("#next-btn").click();
    await expect
      .poll(async () => (await audioState(page))?.src ?? "", { message: "stream switches to track 2" })
      .toContain(`id=${mock.albumSongs[1].id}`);
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
  });

  test("search through the navigation finds and opens a fixture artist", async ({ connected: page, mock }) => {
    await gotoReady(page, "/");
    await tid(page, isCompact(page) ? "tab-search" : "nav-search").click();
    const input = tid(page, "search-input");
    await expect(input).toHaveAttribute("id", "global-search-input");
    await input.fill("Paper Comets");
    const card = tid(page, "artist-card").filter({ hasText: mock.artist.name }).first();
    await expect(card).toBeVisible();
    await card.click();
    await expect(page).toHaveURL(new RegExp(`/artist/${mock.artist.id}/`));
    await expect(tid(page, "artist-detail").getByRole("heading", { name: mock.artist.name })).toBeVisible();
  });

  test("opens the queue and it lists the playing track", async ({ connected: page, mock }) => {
    await playFirstTrack(page, mock);
    // Wide: the player bar's Up Next button. Compact: Library → Up Next.
    if (isCompact(page)) await openLibraryArea(page, "queue");
    else await tid(page, "player-queue").click();
    await expect(page).toHaveURL(/\/queue$/);
    const list = tid(page, "queue-list");
    await expect(list).toBeVisible();
    await expect(tid(list, "track-row").filter({ hasText: mock.albumSongs[0].title }).first()).toBeVisible();
    // Playing from the album queued the whole album.
    await expect.poll(() => tid(list, "track-row").count()).toBeGreaterThan(1);
  });

  test("settings opens from the navigation and lists the server", async ({ connected: page }) => {
    await gotoReady(page, "/");
    // Wide: the sidebar's Settings item. Compact: Library → Settings.
    if (isCompact(page)) {
      await tid(page, "tab-library").click();
      await tid(page, "library").getByTestId("settings-open").click();
    } else {
      await expect(tid(page, "server-status")).toContainText("Mock Navidrome");
      await tid(page, "nav-settings").click();
    }
    await expect(page).toHaveURL(/\/settings$/);
    // Compact shows the section list first (the Servers row names the server); wide shows the
    // Servers section beside it.
    if (isCompact(page)) {
      const servers = tid(page, "settings-nav-servers");
      await expect(servers).toContainText("Mock Navidrome");
      await servers.click();
    }
    await expect(tid(page, "server-row").filter({ hasText: "Mock Navidrome" })).toBeVisible();
  });
});
