// Library, Home and Search views after the D1 refactor: every song list swipes for Play Next
// and Add to… (Songs, search results), the wide sidebar lists the user's playlists as drop
// targets for a song carried with the mouse, and the album grid mounts only the rows near
// the viewport however long the library is.
//
// Touch is dispatched TouchEvents at the element under the finger (what the page sees), as in
// row-gestures.spec.ts: Playwright's WebKit has no touch-move API.
import type { Locator, Page } from "@playwright/test";
import { expect, MOCK_SERVER_ID, test } from "../../support/fixtures";
import { MOCK_PASSWORD, MOCK_URL, MOCK_USER } from "../../support/env";
import { gotoReady, isCompact, tid } from "./helpers";

type Point = { x: number; y: number };

/** One finger as dispatched TouchEvents. */
async function touch(page: Page) {
  await page.evaluate(() => {
    let target: Element | null = null;
    let last = { x: 0, y: 0 };
    const fire = (type: string, p: { x: number; y: number }, ended = false) => {
      const t = { identifier: 5, target: target!, clientX: p.x, clientY: p.y, pageX: p.x, pageY: p.y, screenX: p.x, screenY: p.y };
      const live = ended ? [] : [t];
      const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
      for (const [key, value] of Object.entries({ touches: live, targetTouches: live, changedTouches: [t] })) {
        Object.defineProperty(event, key, { value });
      }
      target!.dispatchEvent(event);
    };
    (window as any).__viewTouch = {
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
  const call = (fn: string, p?: Point) => page.evaluate(([fn, p]) => (window as any).__viewTouch[fn as string](p), [fn, p] as const);
  return {
    async swipe(from: Point, to: Point, steps = 14, stepMs = 16) {
      await call("start", from);
      for (let i = 1; i <= steps; i++) {
        await call("move", { x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
        await page.waitForTimeout(stepMs);
      }
      await page.waitForTimeout(60);
      await call("end");
    },
  };
}

const gestureState = (page: Page) => page.evaluate(() => (window as any).__rsRowG?.state() ?? "missing");

/** Swipes `row` rightwards by 30% of its width (reveals its actions). */
async function swipeOpen(page: Page, row: Locator) {
  await row.evaluate((node) => node.scrollIntoView({ block: "center" }));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const box = (await row.boundingBox())!;
  const y = box.y + box.height / 2;
  // Start on the title, clear of the screen edge (the edge swipe's zone) and the art.
  const x0 = box.x + Math.min(90, box.width * 0.3);
  await (await touch(page)).swipe({ x: x0, y }, { x: x0 + box.width * 0.3, y });
  await expect.poll(() => gestureState(page)).toBe("open");
}

// ---- mock server REST ----

const auth = () => `u=${MOCK_USER}&p=enc:${Buffer.from(MOCK_PASSWORD).toString("hex")}&v=1.16.1&c=e2e&f=json`;
async function rest(method: string, extra = "") {
  const r = await fetch(`${MOCK_URL}/rest/${method}?${auth()}${extra}`);
  return (await r.json())["subsonic-response"];
}

async function playlistByName(name: string) {
  const all = (await rest("getPlaylists")).playlists.playlist as { id: string; name: string }[];
  return all.find((p) => p.name === name)!;
}

async function playlistSongIds(id: string): Promise<string[]> {
  const pl = (await rest("getPlaylist", `&id=${id}`)).playlist;
  return (pl.entry ?? []).map((s: { id: string }) => s.id);
}

async function setPlaylistSongs(id: string, ids: string[]) {
  await rest("createPlaylist", `&playlistId=${id}${ids.map((s) => `&songId=${s}`).join("")}`);
}

test.describe("browse swipe (compact)", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(!isCompact(page), "touch swipes are compact only");
  });

  test("Songs: a row swipes for Play Next and Add to…; Play Next plays it on an empty queue", async ({ connected: page }) => {
    await gotoReady(page, "/songs");
    const list = tid(page, "song-list");
    const row = list.locator('li[data-index="2"]');
    await expect(row.locator('[data-testid="track-row"]')).toBeVisible();
    const title = (await row.locator(".rs-trow__title").textContent())!.trim();

    await swipeOpen(page, row);
    await expect(row.locator('[data-testid="row-swipe-next"]')).toBeVisible();
    await expect(row.locator('[data-testid="row-swipe-add"]')).toBeVisible();
    // Browse lists never offer Remove.
    await expect(list.locator('[data-testid="row-swipe-remove"]')).toHaveCount(0);

    await row.locator('[data-testid="row-swipe-next"]').click();
    await expect(tid(page, "mini-player")).toContainText(title, { timeout: 20_000 });
  });

  test("search results: a song swipes for Add to…, which opens the add sheet", async ({ connected: page }) => {
    await gotoReady(page, "/search");
    await tid(page, "search-input").fill("Paper Comets");
    const list = page.locator('[data-rowg="search-songs"]');
    const row = list.locator('li[data-index="0"]');
    await expect(row.locator('[data-testid="track-row"]')).toBeVisible();
    const title = (await row.locator(".rs-trow__title").textContent())!.trim();

    await swipeOpen(page, row);
    await expect(row.locator('[data-testid="row-swipe-next"]')).toBeVisible();
    await row.locator('[data-testid="row-swipe-add"]').click();
    await expect(tid(page, "add-menu-play-next")).toBeVisible();
    await expect(page.locator(".rs-player-addhead__title")).toHaveText(title);
  });
});

test.describe("sidebar playlists (wide)", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(isCompact(page), "the sidebar is wide only");
  });

  test("lists the user's own playlists, collapses, and shows the rest on Show All", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const section = tid(page, "sidebar-playlists");
    await expect(tid(section, "sidebar-playlist").first()).toBeVisible();
    // Eight at first; someone else's (auto-imported) playlist is never listed.
    await expect(tid(section, "sidebar-playlist")).toHaveCount(8);
    await expect(section).not.toContainText("Discover Weekly (imported)");
    await tid(section, "sidebar-playlists-more").click();
    expect(await tid(section, "sidebar-playlist").count()).toBeGreaterThan(8);
    await tid(section, "sidebar-playlists-toggle").click();
    await expect(tid(section, "sidebar-playlist")).toHaveCount(0);
    await expect(tid(section, "sidebar-playlists-toggle")).toHaveAttribute("aria-expanded", "false");
  });

  test("a song carried from Songs onto a sidebar playlist is added to it", async ({ connected: page }) => {
    const pl = await playlistByName("Dinner Party");
    const original = await playlistSongIds(pl.id);
    try {
      await gotoReady(page, "/songs");
      const row = tid(page, "song-list").locator('li[data-index="1"]');
      await expect(row.locator('[data-testid="track-row"]')).toBeVisible();
      const title = (await row.locator(".rs-trow__title").textContent())!.trim();
      const songId = (await row.getAttribute("data-key"))!.split("::")[1];

      const section = tid(page, "sidebar-playlists");
      if ((await section.getByRole("button", { name: pl.name, exact: true }).count()) === 0) {
        await tid(section, "sidebar-playlists-more").click();
      }
      const target = section.locator(`[data-drop-playlist="${pl.id}"]`);
      await expect(target).toBeVisible();
      await expect(target).toHaveAttribute("data-drop-server", MOCK_SERVER_ID);

      // Lift the row (leftward first, so the browse swipe doesn't claim it), carry, drop.
      const box = (await row.boundingBox())!;
      const start = { x: box.x + 160, y: box.y + box.height / 2 };
      await page.mouse.move(start.x, start.y);
      await page.mouse.down();
      await page.mouse.move(start.x - 30, start.y + 6, { steps: 4 });
      await expect.poll(() => gestureState(page)).toBe("carry");
      await expect(page.locator(".rs-carry")).toContainText(title);
      const drop = (await target.boundingBox())!;
      await page.mouse.move(drop.x + drop.width / 2, drop.y + drop.height / 2, { steps: 14 });
      await expect(target).toHaveClass(/rs-drop-over/);
      await page.mouse.up();

      await expect(tid(page, "status-toast")).toContainText(`Added "${title}" to the playlist.`);
      await expect.poll(() => playlistSongIds(pl.id)).toEqual([...original, songId]);
    } finally {
      await setPlaylistSongs(pl.id, original);
    }
  });
});

test.describe("album grid", () => {
  test("stays windowed: scrolling a long grid keeps the mounted tiles bounded", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    const grid = tid(page, "album-grid");
    await expect(tid(grid, "album-tile").first()).toBeVisible();
    const first = (await tid(grid, "album-tile").first().locator(".rs-library-tile__title").textContent())!.trim();
    const total = (await rest("getAlbumList2", "&type=alphabeticalByName&size=500")).albumList2.album.length as number;
    expect(total, "the mock library has a long album list").toBeGreaterThanOrEqual(150);

    // Scroll to the end, page after page, until every album has loaded.
    let height = 0;
    for (let i = 0; i < 20; i++) {
      height = await page.evaluate(() => {
        const main = document.querySelector(".rs-main")!;
        main.scrollTop = main.scrollHeight;
        return main.scrollHeight;
      });
      await page.waitForTimeout(250);
      const grown = await page.evaluate(() => document.querySelector(".rs-main")!.scrollHeight);
      const loading = await page.locator(".rs-library-sentinel").count();
      if (grown === height && loading === 0) break;
    }
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));

    const mounted = await tid(grid, "album-tile").count();
    expect(mounted, "tiles near the end are mounted").toBeGreaterThan(0);
    expect(mounted, `only a window of the ${total} albums is mounted`).toBeLessThan(Math.min(total, 120));
    // The first tile scrolled out of the window and was unmounted.
    const titles = await grid.locator(".rs-library-tile__title").allTextContents();
    expect(titles.map((t) => t.trim())).not.toContain(first);
    // The grid still has the full length: rows above are sized, not missing.
    const sizer = await grid.locator(".rs-vlist__sizer").evaluate((el) => el.getBoundingClientRect().height);
    expect(sizer).toBeGreaterThan(height * 0.5);
  });
});
