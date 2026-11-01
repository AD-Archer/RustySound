// Row gestures (src/components/ui/row_gestures.js): drag the grip handle to reorder the
// queue page, Up Next and editable playlists; swipe a row for its actions; full swipe
// removes; Undo restores; keyboard reorder; and the edge swipe still goes back.
//
// Input per browser: Chromium drives real touches through CDP on the iPhone project (so
// touch-action and the non-passive touchmove are exercised) and the mouse on the Mac one.
// Playwright's WebKit has no touch-move API: swipes there dispatch TouchEvents at the
// element under the finger (what the page sees), and handle drags use the mouse, which
// the handle takes too.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { isCompact, tid } from "./helpers";

type Point = { x: number; y: number };
type Finger = { start: (p: Point) => Promise<unknown>; move: (p: Point) => Promise<unknown>; end: () => Promise<unknown> };

const usesTouch = (page: Page) => isCompact(page);

/** One touch: CDP on Chromium, dispatched TouchEvents on WebKit. */
async function touchFinger(page: Page, browserName: string): Promise<Finger> {
  if (browserName === "chromium") {
    const cdp = await page.context().newCDPSession(page);
    const send = (type: string, p?: Point) =>
      cdp.send("Input.dispatchTouchEvent", { type, touchPoints: p ? [{ x: Math.round(p.x), y: Math.round(p.y) }] : [] });
    return { start: (p) => send("touchStart", p), move: (p) => send("touchMove", p), end: () => send("touchEnd") };
  }
  await page.evaluate(() => {
    let target: Element | null = null;
    let last = { x: 0, y: 0 };
    const fire = (type: string, p: { x: number; y: number }, ended = false) => {
      const touch = { identifier: 9, target: target!, clientX: p.x, clientY: p.y, pageX: p.x, pageY: p.y, screenX: p.x, screenY: p.y };
      const live = ended ? [] : [touch];
      const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
      for (const [key, value] of Object.entries({ touches: live, targetTouches: live, changedTouches: [touch] })) {
        Object.defineProperty(event, key, { value });
      }
      target!.dispatchEvent(event);
    };
    (window as any).__rsRowTouch = {
      start(p: { x: number; y: number }) {
        target = document.elementFromPoint(p.x, p.y);
        last = p;
        fire("touchstart", p);
      },
      move(p: { x: number; y: number }) {
        last = p;
        fire("touchmove", p);
      },
      end() {
        fire("touchend", last, true);
      },
    };
  });
  const call = (fn: string, p?: Point) => page.evaluate(([fn, p]) => (window as any).__rsRowTouch[fn as string](p), [fn, p] as const);
  return { start: (p) => call("start", p), move: (p) => call("move", p), end: () => call("end") };
}

function mouseFinger(page: Page): Finger {
  return {
    start: async (p) => {
      await page.mouse.move(p.x, p.y);
      await page.mouse.down();
    },
    move: (p) => page.mouse.move(p.x, p.y),
    end: () => page.mouse.up(),
  };
}

/** A swipe: touch on the iPhone projects, the mouse on the Mac ones. */
async function swipeFinger(page: Page, browserName: string) {
  return usesTouch(page) ? touchFinger(page, browserName) : mouseFinger(page);
}

/** A handle drag: a real touch on Chromium iPhone, the mouse everywhere else. */
async function dragFinger(page: Page, browserName: string) {
  return usesTouch(page) && browserName === "chromium" ? touchFinger(page, browserName) : mouseFinger(page);
}

async function stroke(page: Page, f: Finger, from: Point, to: Point, steps = 14, stepMs = 16) {
  await f.start(from);
  for (let i = 1; i <= steps; i++) {
    await f.move({ x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
    await page.waitForTimeout(stepMs);
  }
  await page.waitForTimeout(60);
  await f.end();
}

const rowAt = (list: Locator, index: number) => list.locator(`li[data-index="${index}"]`);
const gestureState = (page: Page) => page.evaluate(() => (window as any).__rsRowG?.state() ?? "missing");

/** Titles of the mounted rows of `list`, in index order. */
async function titles(list: Locator) {
  return list.evaluate((el) =>
    [...el.querySelectorAll("li[data-index]")]
      .map((li) => [Number((li as HTMLElement).dataset.index), li.querySelector(".rs-trow__title")?.textContent?.trim() ?? ""] as const)
      .sort((a, b) => a[0] - b[0])
      .map(([, t]) => t),
  );
}

async function idle(page: Page) {
  await expect.poll(() => gestureState(page)).toBe("idle");
}

/** Brings `el` into view clear of the tab bar and player at the bottom:
 *  "if needed" alone counts a row hidden under them as visible, and a drag
 *  that starts near the bottom edge auto-scrolls the list. */
async function centre(el: Locator) {
  await el.scrollIntoViewIfNeeded();
  const covered = await el.evaluate((node) => node.getBoundingClientRect().bottom > window.innerHeight - 180);
  if (!covered) return;
  await el.evaluate((node) => node.scrollIntoView({ block: "center" }));
  await el.page().evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
}

/** Drags row `from`'s handle by `rows` rows. */
async function dragHandle(page: Page, browserName: string, list: Locator, from: number, rows: number) {
  const handle = rowAt(list, from).locator('[data-testid="row-handle"]');
  await centre(handle);
  const box = (await handle.boundingBox())!;
  const rowBox = (await rowAt(list, from).boundingBox())!;
  const start = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const end = { x: start.x, y: start.y + rows * rowBox.height };
  await stroke(page, await dragFinger(page, browserName), start, end, 16, 20);
  await idle(page);
}

/** Swipes row `index` rightwards by `fraction` of its width. */
async function swipeRow(page: Page, browserName: string, list: Locator, index: number, fraction: number) {
  const row = rowAt(list, index);
  await centre(row);
  const box = (await row.boundingBox())!;
  const y = box.y + box.height / 2;
  // Start on the title, clear of the screen edge (the edge swipe's zone) and the art.
  const x0 = box.x + Math.min(90, box.width * 0.3);
  await stroke(page, await swipeFinger(page, browserName), { x: x0, y }, { x: x0 + box.width * fraction, y });
}

async function startPlayback(page: Page, mock: MockFixtures) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0.5);
}

async function openNowPlaying(page: Page) {
  if (isCompact(page)) await tid(page, "mini-player").locator("button").first().click();
  else await tid(page, "player-bar").getByRole("button", { name: "Open Now Playing" }).click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  return now;
}

/** The queue page with the album queued and its first song playing. */
async function openQueue(page: Page, mock: MockFixtures) {
  await startPlayback(page, mock);
  const now = await openNowPlaying(page);
  await now.getByRole("button", { name: "Open Queue" }).click();
  await expect(page).toHaveURL(/\/queue$/);
  const list = tid(page, "queue-list");
  await expect(rowAt(list, 0)).toBeVisible();
  return list;
}

async function playingSrc(page: Page) {
  return (await audioState(page))?.src ?? "";
}

// ---- mock server REST (per-project playlists, so parallel projects don't collide) ----

const auth = () => `u=${MOCK_USER}&p=enc:${Buffer.from(MOCK_PASSWORD).toString("hex")}&v=1.16.1&c=e2e&f=json`;
async function rest(method: string, extra = "") {
  const r = await fetch(`${MOCK_URL}/rest/${method}?${auth()}${extra}`);
  return (await r.json())["subsonic-response"];
}

const PROJECT_PLAYLIST: Record<string, string> = {
  "chromium-mac": "Focus Blocks",
  "webkit-iphone": "Rainy Sunday",
  "chromium-iphone": "Gym Rotation",
  "webkit-mac": "Road Trip 2024",
  "firefox-mac": "Throwbacks",
};

async function playlistByName(name: string) {
  const all = (await rest("getPlaylists")).playlists.playlist as { id: string; name: string; owner: string }[];
  return all.find((p) => p.name === name)!;
}

async function playlistSongIds(id: string): Promise<string[]> {
  const pl = (await rest("getPlaylist", `&id=${id}`)).playlist;
  return (pl.entry ?? []).map((s: { id: string }) => s.id);
}

async function setPlaylistSongs(id: string, ids: string[]) {
  await rest("createPlaylist", `&playlistId=${id}${ids.map((s) => `&songId=${s}`).join("")}`);
}

// ---------------------------------------------------------------------------

test.describe("row gestures", () => {
  test("queue page: the handle drags a row to a new place; the playing song is unchanged", async ({ connected: page, mock, browserName }) => {
    const list = await openQueue(page, mock);
    const before = await titles(list);
    const src = await playingSrc(page);
    await expect(rowAt(list, 1).locator('[data-testid="row-handle"]')).toBeVisible();

    await dragHandle(page, browserName, list, 1, 2);
    const expected = [...before];
    const [moved] = expected.splice(1, 1);
    expected.splice(3, 0, moved);
    await expect.poll(() => titles(list)).toEqual(expected);
    // The playing row is still the first one, and the audio didn't change.
    await expect(rowAt(list, 0).locator(".rs-trow--playing")).toHaveCount(1);
    expect(await playingSrc(page)).toBe(src);
    // Kept: Up Next shows the new order too.
    const now = await openNowPlaying(page);
    await tid(now, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
    await expect(tid(now, "queue-list").locator('li[data-index="3"] .rs-trow__title')).toHaveText(moved);
  });

  test("Up Next: the handle reorders what's to come; played rows have no handle", async ({ connected: page, mock, browserName }) => {
    await startPlayback(page, mock);
    const now = await openNowPlaying(page);
    await tid(now, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
    // The focused view: the full player's panel shows only a couple of rows.
    if (isCompact(page)) {
      await tid(now, "now-playing-expand").click();
      await expect(now).toHaveAttribute("data-focused", "true");
      await page.waitForTimeout(400);
    }
    const panel = tid(now, "queue-list");
    const upNext = panel.locator('[data-rowg="upnext"][data-rowg-min]');
    await expect(rowAt(upNext, 1)).toBeVisible();
    const before = await titles(upNext);
    const src = await playingSrc(page);

    await dragHandle(page, browserName, upNext, 1, 2);
    const expected = [...before];
    const [moved] = expected.splice(0, 1);
    expected.splice(2, 0, moved);
    await expect.poll(() => titles(upNext)).toEqual(expected);
    expect(await playingSrc(page)).toBe(src);
    if (isCompact(page)) {
      await tid(now, "now-playing-expand").click();
      await expect(now).toHaveAttribute("data-focused", "false");
    }

    // Move on two songs: the first two land in Played, which doesn't reorder.
    await now.locator("#np-next-btn").click();
    await now.locator("#np-next-btn").click();
    const history = panel.locator(".rs-player-upnext__history");
    await expect(history.locator('[data-testid="track-row"]')).toHaveCount(2);
    await expect(history.locator('[data-testid="row-handle"]')).toHaveCount(0);
  });

  test("keyboard: Space picks up, arrows move, Space drops; Escape cancels", async ({ connected: page, mock }) => {
    const list = await openQueue(page, mock);
    const before = await titles(list);
    const handle = rowAt(list, 1).locator('[data-testid="row-handle"]');
    await handle.focus();
    await page.keyboard.press("Space");
    await expect.poll(() => gestureState(page)).toBe("keys");
    await expect(tid(page, "row-live")).toContainText("Picked up");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(tid(page, "row-live")).toContainText("position 4 of");
    await page.keyboard.press("Space");
    const expected = [...before];
    const [moved] = expected.splice(1, 1);
    expected.splice(3, 0, moved);
    await expect.poll(() => titles(list)).toEqual(expected);
    await expect(tid(page, "row-live")).toContainText("dropped");
    // Space didn't also pause the music.
    expect((await audioState(page))?.paused).toBe(false);
    // Focus follows the song to its new place.
    await expect(rowAt(list, 3).locator('[data-testid="row-handle"]')).toBeFocused();

    // Escape puts it back.
    await page.keyboard.press("Space");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Escape");
    await idle(page);
    await expect(tid(page, "row-live")).toContainText("Cancelled");
    await expect.poll(() => titles(list)).toEqual(expected);
  });

  test("swipe: a partial swipe reveals Remove and Add to…; Add to… opens the sheet", async ({ connected: page, mock, browserName }) => {
    const list = await openQueue(page, mock);
    const row = rowAt(list, 2);
    await swipeRow(page, browserName, list, 2, 0.3);
    await expect.poll(() => gestureState(page)).toBe("open");
    await expect(row.locator('[data-testid="row-swipe-remove"]')).toBeVisible();
    await expect(row.locator('[data-testid="row-swipe-add"]')).toBeVisible();
    await row.locator('[data-testid="row-swipe-add"]').click();
    await expect(tid(page, "add-menu-play-next")).toBeVisible();
    await expect(page.locator(".rs-player-addhead__title")).toHaveText(await row.locator(".rs-trow__title").innerText());
  });

  test("swipe: Remove takes the song out of the queue and Undo puts it back", async ({ connected: page, mock, browserName }) => {
    const list = await openQueue(page, mock);
    const before = await titles(list);
    await swipeRow(page, browserName, list, 2, 0.3);
    await rowAt(list, 2).locator('[data-testid="row-swipe-remove"]').click();
    const without = before.filter((_, i) => i !== 2);
    await expect.poll(() => titles(list)).toEqual(without);
    await expect(tid(page, "undo-toast")).toContainText(before[2]);
    await tid(page, "undo-toast-undo").click();
    await expect.poll(() => titles(list)).toEqual(before);
    await expect(tid(page, "undo-toast")).toHaveCount(0);
  });

  test("swipe: a full swipe removes the song directly; Undo restores it", async ({ connected: page, mock, browserName }) => {
    const list = await openQueue(page, mock);
    const before = await titles(list);
    await swipeRow(page, browserName, list, 3, 0.75);
    const without = before.filter((_, i) => i !== 3);
    await expect.poll(() => titles(list)).toEqual(without);
    await tid(page, "undo-toast-undo").click();
    await expect.poll(() => titles(list)).toEqual(before);
  });

  test("the track menu offers Remove from Queue, with Undo", async ({ connected: page, mock }) => {
    const list = await openQueue(page, mock);
    const before = await titles(list);
    await rowAt(list, 1).getByRole("button", { name: /^More for / }).click();
    await tid(page, "track-menu-remove").click();
    await expect.poll(() => titles(list)).toEqual(before.filter((_, i) => i !== 1));
    await tid(page, "undo-toast-undo").click();
    await expect.poll(() => titles(list)).toEqual(before);
  });

  test("a vertical drag doesn't swipe", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page), "touch scrolling (compact)");
    const list = await openQueue(page, mock);
    const before = await titles(list);
    const box = (await rowAt(list, 2).boundingBox())!;
    const x = box.x + 100;
    await stroke(page, await touchFinger(page, browserName), { x, y: box.y + box.height / 2 }, { x: x + 10, y: box.y - 120 });
    await idle(page);
    await expect(list.locator(".rs-rowg--active")).toHaveCount(0);
    expect(await titles(list)).toEqual(before);
  });

  test("an edge swipe on a row still goes back", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page), "edge swipe is compact only");
    const list = await openQueue(page, mock);
    // Let the page's push transition finish, as a person would.
    await page.waitForTimeout(600);
    const count = await list.locator("li[data-index]").count();
    const box = (await rowAt(list, 2).boundingBox())!;
    const width = page.viewportSize()!.width;
    const y = box.y + box.height / 2;
    await stroke(page, await touchFinger(page, browserName), { x: Math.max(box.x + 3, 18), y }, { x: width * 0.6, y });
    // Back on the album page the queue was opened from, and no row was swiped away.
    await expect(page).toHaveURL(/\/album\//);
    await expect(tid(page, "undo-toast")).toHaveCount(0);
    expect(count).toBeGreaterThan(2);
  });

  test("editable playlist: drag, remove, full swipe and Undo all reach the server", async ({ connected: page, browserName }, testInfo) => {
    const name = PROJECT_PLAYLIST[testInfo.project.name] ?? "Focus Blocks";
    const pl = await playlistByName(name);
    const original = await playlistSongIds(pl.id);
    try {
      await page.goto(`/playlist/${pl.id}/${MOCK_SERVER_ID}`);
      await waitForAppReady(page);
      const list = page.locator('[data-rowg="playlist"]');
      await expect(rowAt(list, 0)).toBeVisible();
      await expect(tid(page, "playlist-add-songs")).toBeVisible();
      const before = await titles(list);

      // Drag row 0 down two rows: saved.
      await dragHandle(page, browserName, list, 0, 2);
      const moved = [...before];
      const [first] = moved.splice(0, 1);
      moved.splice(2, 0, first);
      await expect.poll(() => titles(list)).toEqual(moved);
      const movedIds = [...original];
      movedIds.splice(2, 0, movedIds.splice(0, 1)[0]);
      await expect.poll(() => playlistSongIds(pl.id)).toEqual(movedIds);

      // Full swipe removes it on the server; Undo puts it back in place.
      await swipeRow(page, browserName, list, 1, 0.75);
      await expect.poll(() => playlistSongIds(pl.id)).toEqual(movedIds.filter((_, i) => i !== 1));
      await tid(page, "undo-toast-undo").click();
      await expect.poll(() => playlistSongIds(pl.id)).toEqual(movedIds);
      await expect.poll(() => titles(list)).toEqual(moved);

      // The track menu says "Remove from Playlist".
      await rowAt(list, 0).getByRole("button", { name: /^More for / }).click();
      await expect(tid(page, "track-menu-remove")).toHaveText(/Remove from Playlist/);
      await page.keyboard.press("Escape");

      // The order survives a reload (it's the server's).
      await page.reload();
      await waitForAppReady(page);
      await expect.poll(() => titles(page.locator('[data-rowg="playlist"]'))).toEqual(moved);
    } finally {
      await setPlaylistSongs(pl.id, original);
    }
  });

  test("someone else's playlist: no handle and no Remove; swipe offers Play Next and Add to…", async ({ connected: page, browserName }) => {
    const pl = await playlistByName("Discover Weekly (imported)");
    expect(pl.owner).not.toBe(MOCK_USER);
    await page.goto(`/playlist/${pl.id}/${MOCK_SERVER_ID}`);
    await waitForAppReady(page);
    const list = page.locator('[data-rowg="playlist"]');
    await expect(rowAt(list, 0)).toBeVisible();
    await expect(list.locator('[data-testid="row-handle"]')).toHaveCount(0);
    await expect(tid(page, "playlist-add-songs")).toHaveCount(0);
    await swipeRow(page, browserName, list, 1, 0.3);
    const row = rowAt(list, 1);
    await expect(row.locator('[data-testid="row-swipe-next"]')).toBeVisible();
    await expect(row.locator('[data-testid="row-swipe-add"]')).toBeVisible();
    await expect(list.locator('[data-testid="row-swipe-remove"]')).toHaveCount(0);
    await page.keyboard.press("Escape");
    await idle(page);
    // A full swipe doesn't remove anything here; the row just stays open. (Bring the row into
    // view first: scrolling changes which rows the windowed list has mounted.)
    await centre(rowAt(list, 2));
    const before = await titles(list);
    await swipeRow(page, browserName, list, 2, 0.75);
    await page.waitForTimeout(400);
    expect(await titles(list)).toEqual(before);
    await expect.poll(() => gestureState(page)).toBe("open");
    await page.keyboard.press("Escape");
    await idle(page);
    await rowAt(list, 0).getByRole("button", { name: /^More for / }).click();
    await expect(tid(page, "track-menu")).toBeVisible();
    await expect(tid(page, "track-menu-remove")).toHaveCount(0);
  });
});
