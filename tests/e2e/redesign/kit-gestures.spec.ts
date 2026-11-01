// Kit gestures after the refactor (src/components/ui/*.js on gesture_core.js): the
// long-press runs in row_gestures.js and raises the row's contextmenu, compact sheets
// swipe down to close (sheet_swipe.js), a song row can be carried onto a drop target
// with the mouse, and Cover Flow takes only horizontal wheels (cover_flow.js).
//
// Touch on WebKit is dispatched TouchEvents at the element under the finger, like
// row-gestures.spec.ts: Playwright's WebKit has no touch-move API.
import type { Page } from "@playwright/test";
import { audioState, expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, longPress, openAlbum, tid } from "./helpers";

type Point = { x: number; y: number };

/** One finger as dispatched TouchEvents (what the page sees). */
async function touch(page: Page) {
  await page.evaluate(() => {
    let target: Element | null = null;
    let last = { x: 0, y: 0 };
    const fire = (type: string, p: { x: number; y: number }, ended = false) => {
      const t = { identifier: 7, target: target!, clientX: p.x, clientY: p.y, pageX: p.x, pageY: p.y, screenX: p.x, screenY: p.y };
      const live = ended ? [] : [t];
      const event = new Event(type, { bubbles: true, cancelable: true, composed: true });
      for (const [key, value] of Object.entries({ touches: live, targetTouches: live, changedTouches: [t] })) {
        Object.defineProperty(event, key, { value });
      }
      target!.dispatchEvent(event);
    };
    (window as any).__kitTouch = {
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
  const call = (fn: string, p?: Point) => page.evaluate(([fn, p]) => (window as any).__kitTouch[fn as string](p), [fn, p] as const);
  return {
    async swipe(from: Point, to: Point, steps = 12, stepMs = 16) {
      await call("start", from);
      for (let i = 1; i <= steps; i++) {
        await call("move", { x: from.x + ((to.x - from.x) * i) / steps, y: from.y + ((to.y - from.y) * i) / steps });
        await page.waitForTimeout(stepMs);
      }
      await call("end");
    },
  };
}

test.describe("kit gestures: compact", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(!isCompact(page), "compact-only (width < 760)");
  });

  test("a long-press opens the track menu from JS, and its click doesn't play", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    const row = tid(page, "track-row").first();
    // No Rust pointer handlers on the row: the press is row_gestures.js's.
    await expect(row).toHaveAttribute("data-long-press", "true");
    await longPress(page, row);
    await expect(tid(page, "track-menu")).toBeVisible();
    // The click that ends the press is swallowed: nothing starts playing.
    await row.locator(".rs-trow__main").dispatchEvent("click");
    await page.waitForTimeout(400);
    expect((await audioState(page))?.currentTime ?? 0).toBe(0);
  });

  test("a short press is not a long-press", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await longPress(page, tid(page, "track-row").first(), 200);
    await page.waitForTimeout(400);
    await expect(tid(page, "track-menu")).toHaveCount(0);
  });

  test("the track menu sheet swipes down to close, and springs back from a short pull", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await longPress(page, tid(page, "track-row").first());
    const menu = tid(page, "track-menu");
    await expect(menu).toBeVisible();
    // Let the entrance animation finish.
    await page.waitForTimeout(450);
    const sheet = page.locator("[data-sheet-swipe]");
    await expect(sheet).toHaveCount(1);
    const box = (await sheet.boundingBox())!;
    const finger = await touch(page);

    // A short, slow pull: back where it was.
    const start = { x: box.x + box.width / 2, y: box.y + 30 };
    await finger.swipe(start, { x: start.x, y: start.y + 40 }, 10, 30);
    await page.waitForTimeout(400);
    await expect(menu).toBeVisible();
    await expect.poll(() => sheet.evaluate((el) => (el as HTMLElement).style.transform)).toBe("");

    // A long pull past 30% of the sheet: it closes.
    await finger.swipe(start, { x: start.x, y: start.y + box.height * 0.6 }, 14, 16);
    await expect(menu).toHaveCount(0);
    await expect(page.locator(".rs-scrim")).toHaveCount(0);
  });

  test("a kit ListRow with a menu long-presses too (Bookmarks)", async ({ connected: page }) => {
    await gotoReady(page, "/bookmarks");
    const row = tid(page, "bookmark-row").first();
    await expect(row).toBeVisible();
    await longPress(page, row.locator("button.rs-row"));
    await expect(page.getByRole("menu")).toBeVisible();
  });
});

test.describe("kit gestures: wide", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(isCompact(page), "wide-only");
  });

  test("right-click still opens the track menu", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    await tid(page, "track-row").first().click({ button: "right" });
    await expect(tid(page, "track-menu")).toBeVisible();
  });

  test("a song row carried onto a queue target goes to the queue", async ({ connected: page, mock }) => {
    await openAlbum(page, mock);
    const row = tid(page, "track-row").nth(1);
    const title = (await row.locator(".rs-trow__title").textContent())!.trim();

    // No drop target on the page: a mouse drag carries nothing. (The sidebar's
    // playlists are targets; set them aside for this part.)
    await page.evaluate(() => {
      for (const el of document.querySelectorAll("[data-drop-playlist]")) el.removeAttribute("data-drop-playlist");
    });
    let box = (await row.boundingBox())!;
    await page.mouse.move(box.x + 120, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 120, box.y + box.height / 2 + 60, { steps: 6 });
    expect(await page.evaluate(() => (window as any).__rsRowG.state())).not.toBe("carry");
    await page.mouse.up();
    await page.waitForTimeout(450);
    expect((await audioState(page))?.currentTime ?? 0).toBe(0);

    // A temporary target, as the player bar or a sidebar playlist would be.
    await page.evaluate(() => {
      const target = document.createElement("div");
      target.setAttribute("data-drop-queue", "");
      target.setAttribute("data-testid", "test-drop");
      target.style.cssText = "position:fixed;right:24px;bottom:120px;width:200px;height:80px;z-index:50;background:#ccc";
      document.body.appendChild(target);
    });
    const drop = (await tid(page, "test-drop").boundingBox())!;
    box = (await row.boundingBox())!;
    await page.mouse.move(box.x + 120, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 140, box.y + box.height / 2 + 30, { steps: 4 });
    await expect.poll(() => page.evaluate(() => (window as any).__rsRowG.state())).toBe("carry");
    await expect(page.locator(".rs-carry")).toContainText(title);
    await page.mouse.move(drop.x + drop.width / 2, drop.y + drop.height / 2, { steps: 12 });
    await expect(tid(page, "test-drop")).toHaveClass(/rs-drop-over/);
    await page.mouse.up();

    await expect(tid(page, "status-toast")).toContainText(`Added "${title}" to the queue.`);
    await expect(page.locator(".rs-carry")).toHaveCount(0, { timeout: 2_000 });
    // The queue was empty, so the carried song plays.
    await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(0);
    await expect(tid(page, "player-bar")).toContainText(title);
  });

  test("Cover Flow: a vertical wheel scrolls the page, a horizontal gesture steps once", async ({ connected: page }) => {
    await gotoReady(page, "/");
    const flow = tid(page, "coverflow");
    await expect(tid(page, "coverflow-center")).toBeVisible();
    const center = () => tid(page, "coverflow-center").getAttribute("aria-label");
    const first = await center();
    const box = (await flow.boundingBox())!;
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);

    // Vertical: the page scrolls under the covers, the selection stays.
    const scrollTop = () => page.locator(".rs-main").evaluate((el) => el.scrollTop);
    const before = await scrollTop();
    await page.mouse.wheel(0, 240);
    await expect.poll(scrollTop).toBeGreaterThan(before);
    expect(await center()).toBe(first);
    await page.locator(".rs-main").evaluate((el) => (el.scrollTop = 0));
    await page.waitForTimeout(300);
    const again = (await flow.boundingBox())!;
    await page.mouse.move(again.x + again.width / 2, again.y + again.height / 2);

    // One trackpad swipe with its inertia tail: one cover.
    for (const dx of [12, 30, 48, 40, 32, 24, 18, 12, 8, 6, 4, 3, 2, 1]) {
      await page.mouse.wheel(dx, 2);
      await page.waitForTimeout(16);
    }
    await expect.poll(center).not.toBe(first);
    const second = await center();
    await page.waitForTimeout(300);
    expect(await center()).toBe(second);

    // After a rest, the next gesture steps again. (Chromium halves wheel deltas at
    // deviceScaleFactor 2.)
    for (const dx of [40, 80, 60, 20]) {
      await page.mouse.wheel(dx, 0);
      await page.waitForTimeout(16);
    }
    await expect.poll(center).not.toBe(second);
  });
});
