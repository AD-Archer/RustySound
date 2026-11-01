// Compact Now Playing gestures (src/components/song_details/gestures.js): the focused panel
// view, and swiping the big cover to change song.
//
// Chromium drives real touch input through CDP, so touch-action, native scrolling and the
// pointer events a touch makes are all exercised. Playwright's WebKit has no touch-move API:
// panel gestures there dispatch TouchEvents at the element under the finger (what the page
// sees), and cover swipes use the mouse, which the cover takes too.
import type { Locator, Page } from "@playwright/test";
import { audioState, expect, MOCK_SERVER_ID, test, waitForAppReady, type MockFixtures } from "../../support/fixtures";
import { isCompact, tid } from "./helpers";

type Point = { x: number; y: number };
type Finger = { start: (p: Point) => Promise<unknown>; move: (p: Point) => Promise<unknown>; end: () => Promise<unknown> };

test.skip(({ viewport }) => (viewport?.width ?? 1440) >= 760, "compact layout only");

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
      const touch = { identifier: 7, target: target!, clientX: p.x, clientY: p.y, pageX: p.x, pageY: p.y, screenX: p.x, screenY: p.y };
      const live = ended ? [] : [touch];
      const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
      for (const [key, value] of Object.entries({ touches: live, targetTouches: live, changedTouches: [touch] })) {
        Object.defineProperty(event, key, { value });
      }
      target!.dispatchEvent(event);
    };
    (window as any).__rsNpTouch = {
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
  const call = (fn: string, p?: Point) => page.evaluate(([fn, p]) => (window as any).__rsNpTouch[fn as string](p), [fn, p] as const);
  return { start: (p) => call("start", p), move: (p) => call("move", p), end: () => call("end") };
}

/** Pointer for the cover: a real touch on Chromium, the mouse on WebKit. */
async function coverFinger(page: Page, browserName: string): Promise<Finger> {
  if (browserName === "chromium") return touchFinger(page, browserName);
  return {
    start: async (p) => {
      await page.mouse.move(p.x, p.y);
      await page.mouse.down();
    },
    move: (p) => page.mouse.move(p.x, p.y),
    end: () => page.mouse.up(),
  };
}

async function drag(f: Finger, page: Page, from: Point, to: Point, opts: { steps?: number; stepMs?: number; holdMs?: number } = {}) {
  const { steps = 12, stepMs = 16, holdMs = 0 } = opts;
  await f.start(from);
  for (let i = 1; i <= steps; i++) {
    await f.move({ x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
    if (stepMs) await page.waitForTimeout(stepMs);
  }
  if (holdMs) await page.waitForTimeout(holdMs);
  await f.end();
}

async function box(locator: Locator) {
  const b = await locator.boundingBox();
  if (!b) throw new Error("no box");
  return b;
}

const gestureState = (page: Page) => page.evaluate(() => (window as any).__rsNpGestures?.state() ?? "missing");
const playingId = async (page: Page) => new URL((await audioState(page))?.src ?? "http://x/").searchParams.get("id");

/** Plays album song `index` from the album page and opens Now Playing (full). */
async function openOn(page: Page, mock: MockFixtures, index: number) {
  await page.goto(`/album/${mock.album.id}/${MOCK_SERVER_ID}`);
  await waitForAppReady(page);
  await page.getByText(mock.albumSongs[index].title, { exact: true }).first().click();
  await expect.poll(() => playingId(page)).toBe(mock.albumSongs[index].id);
  await tid(page, "mini-player").locator("button").first().click();
  const now = tid(page, "now-playing");
  await expect(now).toBeVisible();
  await expect(now).toHaveAttribute("data-focused", "false");
  // The opening slide is done and the script has measured the sheet.
  await expect.poll(() => now.evaluate((el) => (el as HTMLElement).style.getPropertyValue("--np-shift"))).not.toBe("");
  await page.waitForTimeout(450);
  return now;
}

/** A point inside the panel area that is on screen in the full player. */
async function panelPoint(now: Locator): Promise<Point> {
  const seg = await box(tid(now, "now-playing-panels"));
  const vh = now.page().viewportSize()!.height;
  return { x: seg.x + seg.width / 2, y: Math.min(seg.y + seg.height + 60, vh - 20) };
}

async function fullPieces(now: Locator) {
  return {
    cover: await box(now.locator(".rs-player-flow__cover")),
    meta: await box(now.locator(".rs-player-meta")),
    transport: await box(now.locator(".rs-player-transport")),
  };
}

async function expandBySwipe(page: Page, browserName: string, now: Locator) {
  const from = await panelPoint(now);
  const f = await touchFinger(page, browserName);
  await drag(f, page, from, { x: from.x, y: from.y - 260 });
  await expect(now).toHaveAttribute("data-focused", "true");
  await expect.poll(() => gestureState(page)).toBe("idle");
  await page.waitForTimeout(400);
}

test.describe("now playing: focused panel view", () => {
  test("in the full player the sheet never scrolls; the panel doesn't drag the cover, meta or transport", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 0);
    expect(await now.evaluate((el) => getComputedStyle(el).overflowY)).toBe("clip");
    const before = await fullPieces(now);

    // Wheel over the panel (Chromium; mobile WebKit has no wheel), even a script asking the
    // sheet to scroll, a downward drag on the panel, and a short upward drag that springs back.
    const p = await panelPoint(now);
    if (browserName === "chromium") {
      await page.mouse.move(p.x, p.y);
      await page.mouse.wheel(0, 400);
    }
    await now.evaluate((el) => {
      el.scrollTop = 300;
    });
    const f = await touchFinger(page, browserName);
    await drag(f, page, p, { x: p.x, y: p.y + 120 });
    await drag(f, page, p, { x: p.x, y: p.y - 30 }, { stepMs: 30, holdMs: 200 });
    await page.waitForTimeout(450);

    expect(await now.evaluate((el) => el.scrollTop)).toBe(0);
    await expect(now).toHaveAttribute("data-focused", "false");
    const after = await fullPieces(now);
    for (const key of ["cover", "meta", "transport"] as const) {
      expect(Math.abs(after[key].y - before[key].y), key).toBeLessThan(1);
    }
    await expect(now.locator("#np-play-pause-btn")).toBeVisible();
  });

  test("swipe up enters the focused view; tap the strip, swipe it down or press Escape to leave", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 0);
    const strip = tid(now, "now-playing-strip");
    await expect(strip).toBeHidden();

    await expandBySwipe(page, browserName, now);
    await expect(strip).toBeVisible();
    await expect(strip).toContainText(mock.albumSongs[0].title);
    await expect(now.locator("#np-play-pause-btn")).toBeHidden();
    await expect(tid(now, "now-playing-panels")).toBeVisible();
    // The panel takes most of the screen, right under the strip.
    const vh = page.viewportSize()!.height;
    const panels = await box(now.locator(".rs-player-panels"));
    const head = await box(strip);
    expect(panels.y).toBeLessThan(head.y + head.height + 2);
    expect(panels.height).toBeGreaterThan(vh * 0.7);
    // The strip's play/pause works.
    await tid(strip, "now-playing-strip-play").click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(true);
    await tid(strip, "now-playing-strip-play").click();
    await expect.poll(async () => (await audioState(page))?.paused).toBe(false);

    // Tap the strip: back to full.
    await tid(strip, "now-playing-collapse").click();
    await expect(now).toHaveAttribute("data-focused", "false");
    await expect(now.locator("#np-play-pause-btn")).toBeVisible();
    await expect(strip).toBeHidden();

    // Tapping the active segment again toggles; so does the chevron.
    const active = tid(now, "now-playing-panels").locator("[aria-pressed='true']");
    await active.click();
    await expect(now).toHaveAttribute("data-focused", "true");
    await page.waitForTimeout(400);
    await tid(now, "now-playing-expand").click();
    await expect(now).toHaveAttribute("data-focused", "false");
    await page.waitForTimeout(400);
    await tid(now, "now-playing-expand").click();
    await expect(now).toHaveAttribute("data-focused", "true");
    await page.waitForTimeout(400);

    // Drag the strip down: back to full, and Now Playing stays open.
    const s = await box(strip);
    const f = await touchFinger(page, browserName);
    await drag(f, page, { x: s.x + s.width / 2, y: s.y + s.height / 2 }, { x: s.x + s.width / 2, y: s.y + s.height / 2 + 300 });
    await expect(now).toHaveAttribute("data-focused", "false");
    await expect(tid(page, "now-playing")).toHaveCount(1);

    // Escape backs out one level at a time.
    await active.click();
    await expect(now).toHaveAttribute("data-focused", "true");
    await now.press("Escape");
    await expect(now).toHaveAttribute("data-focused", "false");
    await expect(tid(page, "now-playing")).toHaveCount(1);
    await now.press("Escape");
    await expect(tid(page, "now-playing")).toHaveCount(0);
  });

  test("a short swipe up springs back to the full player", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 0);
    const p = await panelPoint(now);
    const f = await touchFinger(page, browserName);
    await drag(f, page, p, { x: p.x, y: p.y - 45 }, { stepMs: 30, holdMs: 200 });
    await page.waitForTimeout(450);
    await expect(now).toHaveAttribute("data-focused", "false");
    expect(await now.evaluate((el) => (el as HTMLElement).style.getPropertyValue("--np-p"))).toBe("");
    expect(await now.evaluate((el) => el.hasAttribute("data-np-drag"))).toBe(false);
  });

  test("lyrics and Up Next scroll inside the focused view", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 2);
    await expect(tid(now, "lyrics")).toContainText(`(${mock.albumSongs[2].title})`, { timeout: 20_000 });
    await expandBySwipe(page, browserName, now);
    const sheetTop = await now.evaluate((el) => el.scrollTop);

    // Lyrics: the list scrolls, the sheet doesn't.
    const lines = now.locator(".rs-player-lyrics__lines");
    const lyricsScroll = await lines.evaluate((el) => ({ h: el.scrollHeight, c: el.clientHeight }));
    expect(lyricsScroll.h).toBeGreaterThan(lyricsScroll.c);
    const l = await box(lines);
    const y0 = await lines.evaluate((el) => el.scrollTop);
    if (browserName === "chromium") {
      const f = await touchFinger(page, browserName);
      await drag(f, page, { x: l.x + l.width / 2, y: l.y + l.height * 0.7 }, { x: l.x + l.width / 2, y: l.y + l.height * 0.3 });
    } else {
      // Mobile WebKit in Playwright can't touch-scroll or wheel: scroll the list by script.
      await lines.evaluate((el) => el.scrollBy(0, 160));
    }
    await expect.poll(() => lines.evaluate((el) => el.scrollTop)).toBeGreaterThan(y0 + 20);
    await expect(now).toHaveAttribute("data-focused", "true");

    // Up Next, picked in the focused view: opens on the playing song, scrolls inside.
    await tid(now, "now-playing-panels").getByRole("button", { name: "Up Next" }).click();
    await expect(now).toHaveAttribute("data-focused", "true");
    const list = tid(now, "queue-list");
    await expect(list).toBeVisible();
    await expect
      .poll(() =>
        list.evaluate((panel) => {
          const label = panel.querySelector("[data-upnext-now]") as HTMLElement;
          return Math.abs(label.getBoundingClientRect().top - panel.getBoundingClientRect().top);
        }),
      )
      .toBeLessThan(24);
    const u = await box(list);
    const u0 = await list.evaluate((el) => el.scrollTop);
    if (browserName === "chromium") {
      const f = await touchFinger(page, browserName);
      await drag(f, page, { x: u.x + u.width / 2, y: u.y + u.height * 0.7 }, { x: u.x + u.width / 2, y: u.y + u.height * 0.3 });
    } else {
      await list.evaluate((el) => el.scrollBy(0, 200));
    }
    await expect.poll(() => list.evaluate((el) => el.scrollTop)).toBeGreaterThan(u0 + 20);
    // Scrolled away from the top, a drag down scrolls back instead of leaving the view.
    if (browserName === "chromium") {
      const f = await touchFinger(page, browserName);
      await drag(f, page, { x: u.x + u.width / 2, y: u.y + u.height * 0.3 }, { x: u.x + u.width / 2, y: u.y + u.height * 0.5 });
      await page.waitForTimeout(400);
    }
    await expect(now).toHaveAttribute("data-focused", "true");
    expect(await now.evaluate((el) => el.scrollTop)).toBe(sheetTop);
  });
});

test.describe("now playing: cover swipe", () => {
  async function coverGeometry(now: Locator) {
    const c = await box(now.locator(".rs-player-flow__cover"));
    return { c, cx: c.x + c.width / 2, cy: c.y + c.height / 2 };
  }

  async function settled(page: Page, now: Locator) {
    await expect.poll(() => gestureState(page)).toBe("idle");
    await expect(now.locator(".rs-player-flow[data-swipe]")).toHaveCount(0);
  }

  test("a full swipe left goes exactly one song forward, and right one back", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 2);
    const f = await coverFinger(page, browserName);

    let { c, cx, cy } = await coverGeometry(now);
    await drag(f, page, { x: cx + c.width * 0.3, y: cy }, { x: cx - c.width * 0.35, y: cy });
    await expect.poll(() => playingId(page)).toBe(mock.albumSongs[3].id);
    await settled(page, now);
    await expect(now.locator(".rs-player-meta__title")).toHaveText(mock.albumSongs[3].title);
    await page.waitForTimeout(600);
    expect(await playingId(page), "one song only").toBe(mock.albumSongs[3].id);
    await expect(now).toHaveCount(1);

    ({ c, cx, cy } = await coverGeometry(now));
    await drag(f, page, { x: cx - c.width * 0.3, y: cy }, { x: cx + c.width * 0.35, y: cy });
    await expect.poll(() => playingId(page)).toBe(mock.albumSongs[2].id);
    await settled(page, now);
    await page.waitForTimeout(600);
    expect(await playingId(page), "back one song, no restart rule").toBe(mock.albumSongs[2].id);
  });

  test("a short swipe springs back", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 2);
    const f = await coverFinger(page, browserName);
    const { c, cx, cy } = await coverGeometry(now);
    await drag(f, page, { x: cx, y: cy }, { x: cx - c.width * 0.22, y: cy }, { stepMs: 30, holdMs: 200 });
    await settled(page, now);
    await page.waitForTimeout(300);
    expect(await playingId(page)).toBe(mock.albumSongs[2].id);
    const after = await box(now.locator(".rs-player-flow__cover"));
    expect(Math.abs(after.x - c.x)).toBeLessThan(1);
  });

  test("a fast double swipe moves one song before settling", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 1);
    const f = await coverFinger(page, browserName);
    const { c, cx, cy } = await coverGeometry(now);
    const from = { x: cx + c.width * 0.3, y: cy };
    const to = { x: cx - c.width * 0.4, y: cy };
    await drag(f, page, from, to, { steps: 4, stepMs: 8 });
    await drag(f, page, from, to, { steps: 4, stepMs: 8 });
    await expect.poll(() => playingId(page)).toBe(mock.albumSongs[2].id);
    await settled(page, now);
    await page.waitForTimeout(800);
    expect(await playingId(page)).toBe(mock.albumSongs[2].id);
  });

  test("a vertical or diagonal drag on the cover doesn't change song", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    const now = await openOn(page, mock, 2);
    const f = await coverFinger(page, browserName);
    const { c, cx, cy } = await coverGeometry(now);
    await drag(f, page, { x: cx, y: cy + c.height * 0.3 }, { x: cx, y: cy - c.height * 0.4 });
    await drag(f, page, { x: cx, y: cy }, { x: cx - c.width * 0.45, y: cy - c.height * 0.5 });
    await drag(f, page, { x: cx, y: cy - 40 }, { x: cx - 4, y: cy + 60 }, { stepMs: 30, holdMs: 100 });
    await page.waitForTimeout(600);
    expect(await playingId(page)).toBe(mock.albumSongs[2].id);
    await expect(tid(page, "now-playing")).toHaveCount(1);
  });

  test("the app's edge swipe stays refused while Now Playing is open", async ({ connected: page, mock, browserName }) => {
    test.skip(!isCompact(page));
    await page.goto("/");
    await waitForAppReady(page);
    const now = await openOn(page, mock, 2);
    const url = page.url();
    const vh = page.viewportSize()!.height;
    const f = await touchFinger(page, browserName);
    await f.start({ x: 6, y: vh * 0.45 });
    for (let i = 1; i <= 10; i++) {
      await f.move({ x: 6 + i * 22, y: vh * 0.45 });
      await page.waitForTimeout(16);
      expect(await page.evaluate(() => (window as any).__rsEdgeSwipe?.state() ?? "missing")).not.toBe("drag");
    }
    await f.end();
    await page.waitForTimeout(500);
    expect(page.url()).toBe(url);
    await expect(now).toBeVisible();
    expect(await playingId(page)).toBe(mock.albumSongs[2].id);
  });
});
