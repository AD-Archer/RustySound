// Edge swipe between pages on the compact layout (src/components/ui/edge_swipe.js).
// Chromium drives real touch input through CDP, so touch-action and the non-passive
// touchmove listener are exercised; WebKit has no touch-move API in Playwright, so it
// dispatches TouchEvents at the element under the finger, which is what the page sees.
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

type Point = { x: number; y: number };

/** One finger: start, move, end. */
async function finger(page: Page, browserName: string) {
  if (browserName === "chromium") {
    const cdp = await page.context().newCDPSession(page);
    const send = (type: string, p?: Point) =>
      cdp.send("Input.dispatchTouchEvent", { type, touchPoints: p ? [{ x: Math.round(p.x), y: Math.round(p.y) }] : [] });
    return {
      start: (p: Point) => send("touchStart", p),
      move: (p: Point) => send("touchMove", p),
      end: () => send("touchEnd"),
    };
  }
  await page.evaluate(() => {
    const w = window as unknown as { __rsTouch: unknown };
    let target: Element | null = null;
    let last = { x: 0, y: 0 };
    // Playwright's WebKit has TouchEvent but no Touch constructor, so build the event
    // and give it the touch lists the page reads.
    const fire = (type: string, p: { x: number; y: number }, ended = false) => {
      const touch = { identifier: 7, target: target!, clientX: p.x, clientY: p.y, pageX: p.x, pageY: p.y, screenX: p.x, screenY: p.y };
      const live = ended ? [] : [touch];
      const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
      for (const [key, value] of Object.entries({ touches: live, targetTouches: live, changedTouches: [touch] })) {
        Object.defineProperty(event, key, { value });
      }
      target!.dispatchEvent(event);
    };
    w.__rsTouch = {
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
  const call = (fn: string, p?: Point) =>
    page.evaluate(([fn, p]) => (window as any).__rsTouch[fn as string](p), [fn, p] as const);
  return { start: (p: Point) => call("start", p), move: (p: Point) => call("move", p), end: () => call("end") };
}

/** Drag from `from` to `to` in `steps` moves, `stepMs` apart; `hold` runs before the release. */
async function swipe(page: Page, browserName: string, from: Point, to: Point, opts: { steps?: number; stepMs?: number; hold?: () => Promise<void> } = {}) {
  const { steps = 12, stepMs = 16, hold } = opts;
  const f = await finger(page, browserName);
  await f.start(from);
  for (let i = 1; i <= steps; i++) {
    await f.move({ x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
    await page.waitForTimeout(stepMs);
  }
  if (hold) await hold();
  await f.end();
}

const gestureState = (page: Page) => page.evaluate(() => (window as any).__rsEdgeSwipe?.state() ?? "missing");
const mainTransform = (page: Page) => page.locator(".rs-main").evaluate((el) => (el as HTMLElement).style.transform);

/** Home, then Library through the tab bar: one in-app page to go back to. */
async function homeThenLibrary(page: Page) {
  await gotoReady(page, "/");
  await expect(tid(page, "coverflow")).toBeVisible();
  await tid(page, "tab-library").click();
  await expect(page).toHaveURL(/\/library$/);
}

test.describe("edge swipe", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(!isCompact(page), "compact-only (width < 760)");
  });

  test("a swipe from the left edge goes back; the page follows the finger", async ({ connected: page, browserName }) => {
    await homeThenLibrary(page);
    const { width, height } = page.viewportSize()!;
    const y = height * 0.6;
    await swipe(page, browserName, { x: 6, y }, { x: width * 0.6, y: y + 6 }, {
      hold: async () => {
        await expect.poll(() => gestureState(page)).toBe("drag");
        await expect.poll(() => mainTransform(page)).toMatch(/translate3d\((1\d\d|2\d\d)(\.\d+)?px/);
        await expect(page.locator(".rs-edge-under")).toHaveCount(1);
      },
    });
    await expect(page).toHaveURL(/\/$/);
    await expect(tid(page, "coverflow")).toBeVisible();
    // Handed back untouched once the new page is in.
    await expect.poll(() => gestureState(page)).toBe("idle");
    expect(await mainTransform(page)).toBe("");
    await expect(page.locator(".rs-edge-under")).toHaveCount(0);
  });

  test("a fast flick commits a short swipe", async ({ connected: page, browserName }) => {
    await homeThenLibrary(page);
    const y = page.viewportSize()!.height * 0.6;
    await swipe(page, browserName, { x: 4, y }, { x: 120, y }, { steps: 6, stepMs: 0 });
    await expect(page).toHaveURL(/\/$/);
  });

  test("a short, slow swipe springs back", async ({ connected: page, browserName }) => {
    await homeThenLibrary(page);
    const y = page.viewportSize()!.height * 0.6;
    await swipe(page, browserName, { x: 6, y }, { x: 90, y }, {
      steps: 10,
      stepMs: 30,
      hold: () => page.waitForTimeout(250), // finger rests: no flick
    });
    await expect.poll(() => gestureState(page)).toBe("idle");
    await expect(page).toHaveURL(/\/library$/);
    expect(await mainTransform(page)).toBe("");
    await expect(page.locator(".rs-edge-under")).toHaveCount(0);
  });

  test("a vertical drag from the edge is left to scrolling", async ({ connected: page, browserName }) => {
    await homeThenLibrary(page);
    const y = page.viewportSize()!.height * 0.7;
    await swipe(page, browserName, { x: 8, y }, { x: 14, y: y - 200 });
    await page.waitForTimeout(300);
    await expect(page).toHaveURL(/\/library$/);
    expect(await gestureState(page)).toBe("idle");
  });

  test("swipes inside a shelf or Cover Flow don't navigate", async ({ connected: page, browserName }) => {
    // Library, then Home: Home has a page behind it, so only the targets can refuse.
    await gotoReady(page, "/library");
    await tid(page, "tab-discover").click();
    await expect(page).toHaveURL(/\/$/);
    const width = page.viewportSize()!.width;

    const flow = await tid(page, "coverflow").boundingBox();
    expect(flow).not.toBeNull();
    const flowY = flow!.y + flow!.height / 2;
    await swipe(page, browserName, { x: 6, y: flowY }, { x: width * 0.7, y: flowY });
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/\/$/);

    const shelf = tid(page, "shelf-playlists");
    await shelf.scrollIntoViewIfNeeded();
    const row = await shelf.locator("[role='listitem']").first().boundingBox();
    expect(row).not.toBeNull();
    const rowY = row!.y + row!.height / 2;
    await swipe(page, browserName, { x: 6, y: rowY }, { x: width * 0.7, y: rowY });
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/\/$/);
    expect(await gestureState(page)).toBe("idle");
  });

  test("forward from the right edge only when there is a forward page", async ({ connected: page, browserName }) => {
    await homeThenLibrary(page);
    const { width, height } = page.viewportSize()!;
    const y = height * 0.6;
    // Nothing ahead of Library yet.
    await swipe(page, browserName, { x: width - 6, y }, { x: width * 0.3, y });
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/\/library$/);

    await swipe(page, browserName, { x: 6, y }, { x: width * 0.7, y });
    await expect(page).toHaveURL(/\/$/);
    await expect.poll(() => gestureState(page)).toBe("idle");
    // Home's shelves reach the right edge; start the forward swipe below them.
    const pageY = height * 0.5;
    const hit = await page.evaluate(([x, y]) => document.elementFromPoint(x, y)?.closest("[data-testid^='shelf-'], [data-testid='coverflow']") === null, [width - 6, pageY]);
    test.skip(!hit, "no plain page at the right edge on Home in this viewport");
    await swipe(page, browserName, { x: width - 6, y: pageY }, { x: width * 0.3, y: pageY });
    await expect(page).toHaveURL(/\/library$/);
  });

  test("no swipe back from the first page", async ({ connected: page, browserName }) => {
    await gotoReady(page, "/library");
    const { width, height } = page.viewportSize()!;
    const y = height * 0.6;
    await swipe(page, browserName, { x: 6, y }, { x: width * 0.7, y });
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/\/library$/);
    expect(await mainTransform(page)).toBe("");
  });

  test("reduced motion: no slide, navigates on release", async ({ connected: page, browserName }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await homeThenLibrary(page);
    const { width, height } = page.viewportSize()!;
    const y = height * 0.6;
    await swipe(page, browserName, { x: 6, y }, { x: width * 0.6, y }, {
      hold: async () => {
        await expect.poll(() => gestureState(page)).toBe("drag");
        expect(await mainTransform(page)).toBe("");
      },
    });
    await expect(page).toHaveURL(/\/$/);
  });

  test("no gesture on the wide layout", async ({ connected: page, browserName }) => {
    await page.setViewportSize({ width: 1100, height: 800 });
    await gotoReady(page, "/");
    await expect(page.locator(".rs-app[data-layout='wide']")).toBeAttached();
    await tid(page, "nav-albums").click();
    await expect(page).toHaveURL(/\/albums/);
    await swipe(page, browserName, { x: 6, y: 500 }, { x: 800, y: 500 });
    await page.waitForTimeout(400);
    await expect(page).toHaveURL(/\/albums/);
    expect(await gestureState(page)).toBe("idle");
  });
});
