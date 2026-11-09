// Redesign suite: Wrapped, the year in review (Settings → "Your <year> in Music").
// The mock's play history is invented but deterministic: heavy-rotation albums were last
// played 0–45 days ago (at an hour picked from the song id), old favourites one to three
// years ago, so this year and last year both have listening.
//
// Compact (webkit-iphone): tap the edges, swipe left/right, swipe down to close.
// Wide (chromium-mac): arrow keys, click the edges, drag with the mouse, Escape.
// Share renders the summary card through the Lyrics Shot capture path; a page hook stands
// in for the share sheet and keeps the file it was given.
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

declare global {
  interface Window {
    __rsShared?: { name: string; type: string; size: number }[];
    __rsStory?: { last(): { mode: string; dir: string } | null; state(): string };
  }
}

type Point = { x: number; y: number };

async function openWrapped(page: Page) {
  // From the Listening page (no Last.fm account: its prompt offers the story).
  await gotoReady(page, "/listening");
  const entry = tid(page, "listening-story").locator("button");
  await expect(entry).toBeVisible();
  await entry.click();
  await expect(tid(page, "wrapped")).toBeVisible();
  // The first card once the year has loaded (a skeleton card shows meanwhile).
  await expect(tid(page, "wrapped-card-minutes")).toBeVisible({ timeout: 20_000 });
}

/** The card on show (the outgoing one is inert and goes away by itself). */
const liveCard = (page: Page) => page.locator(".rs-story__slot:not([data-story-leaving]) .rs-story-card");
const cardIs = (page: Page, id: string) => expect(liveCard(page)).toHaveAttribute("data-testid", `wrapped-card-${id}`);

async function stageBox(page: Page) {
  const box = await page.locator(".rs-story__stage").boundingBox();
  if (!box) throw new Error("no stage");
  return box;
}

/** A finger drag as pointer events (touch), the way the story script sees a swipe. */
async function swipe(page: Page, from: Point, to: Point, steps = 8, stepMs = 16) {
  await page.evaluate(
    async ([from, to, steps, stepMs]) => {
      const target = document.elementFromPoint(from.x, from.y)!;
      const fire = (type: string, x: number, y: number) =>
        target.dispatchEvent(
          new PointerEvent(type, {
            bubbles: true,
            cancelable: true,
            isPrimary: true,
            pointerId: 9,
            pointerType: "touch",
            button: 0,
            buttons: type === "pointerup" ? 0 : 1,
            clientX: x,
            clientY: y,
          }),
        );
      fire("pointerdown", from.x, from.y);
      for (let i = 1; i <= steps; i++) {
        await new Promise((r) => setTimeout(r, stepMs));
        fire("pointermove", from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps);
      }
      fire("pointerup", to.x, to.y);
    },
    [from, to, steps, stepMs] as const,
  );
}

async function goToCard(page: Page, id: string) {
  for (let i = 0; i < 14; i++) {
    if ((await liveCard(page).getAttribute("data-testid")) === `wrapped-card-${id}`) return;
    await tid(page, "story-next").click();
  }
  await cardIs(page, id);
}

test.describe("Wrapped", () => {
  test("Settings opens this year so far, with the cards and their honest labels", async ({ connected: page, consoleErrors }) => {
    await openWrapped(page);
    const year = new Date().getFullYear();
    const picker = tid(page, "wrapped-year");
    await expect(picker).toHaveValue(String(year));
    await expect(picker.locator("option:checked")).toHaveText("This year so far");
    const minutes = tid(page, "wrapped-minutes");
    await expect(minutes).toHaveText(/^\d{1,3}(,\d{3})*$/);
    await expect(liveCard(page)).toContainText("minutes with the songs you played this year");
    await expect(liveCard(page)).toContainText("An estimate.");
    // A segment per card, the first one on.
    const segments = tid(page, "story-progress").locator(".rs-story-progress__seg");
    expect(await segments.count()).toBeGreaterThanOrEqual(8);
    await expect(segments.first()).toHaveClass(/--on/);

    await goToCard(page, "top-artist");
    await expect(liveCard(page).locator(".rs-wrapped-name")).not.toBeEmpty();
    await goToCard(page, "top-song");
    await expect(tid(page, "wrapped-preview")).toBeVisible();
    await goToCard(page, "top-songs");
    await expect(liveCard(page)).toContainText("Top songs played this year");
    await expect(liveCard(page).locator(".rs-wrapped-list__row")).toHaveCount(5);
    await goToCard(page, "clock");
    await expect(liveCard(page).locator(".rs-wrapped-clock__bar")).toHaveCount(24);
    await goToCard(page, "personality");
    await expect(liveCard(page).locator(".rs-wrapped-name")).not.toBeEmpty();

    // An earlier year with listening: labelled by last play.
    await expect(picker.locator(`option[value="${year - 1}"]`)).toHaveCount(1, { timeout: 20_000 });
    await picker.selectOption(String(year - 1));
    await expect(tid(page, "wrapped-card-minutes")).toBeVisible({ timeout: 20_000 });
    await expect(liveCard(page)).toContainText(`the songs you last played in ${year - 1}`);
    await expect(segments.first()).toHaveClass(/--on/);
    expect(consoleErrors.filter((e) => !/Failed to load resource/i.test(e))).toEqual([]);
  });

  test("compact: tap the edges, swipe between cards, swipe down to close", async ({ connected: page }) => {
    test.skip(!isCompact(page), "compact gestures");
    await openWrapped(page);
    const box = await stageBox(page);
    const mid = { x: box.x + box.width / 2, y: box.y + box.height / 2 };

    // Tap the right side: next. Tap the left side: back.
    await page.touchscreen.tap(box.x + box.width * 0.85, mid.y);
    await cardIs(page, "top-artist");
    await expect.poll(() => page.evaluate(() => window.__rsStory?.last()?.mode)).toBe("slide");
    await page.touchscreen.tap(box.x + box.width * 0.1, mid.y);
    await cardIs(page, "minutes");

    // Swipe left: next; swipe right: back.
    await swipe(page, { x: mid.x + 100, y: mid.y }, { x: mid.x - 120, y: mid.y });
    await cardIs(page, "top-artist");
    await swipe(page, { x: mid.x - 100, y: mid.y }, { x: mid.x + 120, y: mid.y });
    await cardIs(page, "minutes");
    // A short drag springs back.
    await swipe(page, { x: mid.x, y: mid.y }, { x: mid.x - 30, y: mid.y }, 6, 60);
    await page.waitForTimeout(300);
    await cardIs(page, "minutes");
    // No card before the first one.
    await swipe(page, { x: mid.x - 100, y: mid.y }, { x: mid.x + 120, y: mid.y });
    await page.waitForTimeout(300);
    await cardIs(page, "minutes");

    // Swipe down: closed.
    await swipe(page, { x: mid.x, y: box.y + 80 }, { x: mid.x, y: box.y + 80 + box.height * 0.4 });
    await expect(tid(page, "wrapped")).toHaveCount(0);
    await expect(tid(page, "listening")).toBeVisible();
  });

  test("wide: arrow keys, edge clicks and a mouse drag; Escape closes", async ({ connected: page }) => {
    test.skip(isCompact(page), "wide controls");
    await openWrapped(page);
    await page.keyboard.press("ArrowRight");
    await cardIs(page, "top-artist");
    await page.keyboard.press("ArrowRight");
    await cardIs(page, "top-artists");
    await page.keyboard.press("ArrowLeft");
    await cardIs(page, "top-artist");
    await expect.poll(() => page.evaluate(() => window.__rsStory?.last())).toEqual({ mode: "slide", dir: "prev" });

    // Click the edges.
    const box = await stageBox(page);
    const mid = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    await page.mouse.click(box.x + box.width * 0.9, mid.y);
    await cardIs(page, "top-artists");
    await page.mouse.click(box.x + box.width * 0.08, mid.y);
    await cardIs(page, "top-artist");

    // Drag with the mouse: the card follows, then the next one comes in.
    await page.mouse.move(mid.x + 120, mid.y);
    await page.mouse.down();
    for (let i = 1; i <= 10; i++) await page.mouse.move(mid.x + 120 - i * 26, mid.y);
    await expect.poll(() => page.evaluate(() => window.__rsStory?.state())).toBe("drag");
    await page.mouse.up();
    await cardIs(page, "top-artists");
    // The drag's click on the edge zone was swallowed: still one step.
    await page.waitForTimeout(450);
    await cardIs(page, "top-artists");

    await page.keyboard.press("Escape");
    await expect(tid(page, "wrapped")).toHaveCount(0);
  });

  test("the summary card renders and Share produces an image", async ({ connected: page }) => {
    await page.addInitScript(() => {
      window.__rsShared = [];
      Object.defineProperty(Navigator.prototype, "canShare", { configurable: true, value: () => true });
      Object.defineProperty(Navigator.prototype, "share", {
        configurable: true,
        value: async (data: { files?: File[] }) => {
          for (const file of data.files ?? []) window.__rsShared!.push({ name: file.name, type: file.type, size: file.size });
        },
      });
    });
    await openWrapped(page);
    await goToCard(page, "summary");
    const summary = tid(page, "wrapped-summary");
    await expect(summary).toBeVisible();
    await expect(summary).toContainText(`My ${new Date().getFullYear()} so far in music`);
    await expect(summary.locator(".rs-wrapped-summary__cols li")).toHaveCount(10);
    await expect(summary.locator(".rs-wrapped-summary__value").first()).toHaveText(/^\d{1,3}(,\d{3})*$/);
    // The last card: nothing after it.
    await expect(tid(page, "story-next")).toBeDisabled();

    await tid(page, "wrapped-share").click();
    await expect(tid(page, "wrapped-share-note")).toHaveText("Shared.", { timeout: 30_000 });
    const shared = await page.evaluate(() => window.__rsShared ?? []);
    expect(shared).toHaveLength(1);
    expect(shared[0].type).toBe("image/png");
    expect(shared[0].name).toBe(`rustysound-${new Date().getFullYear()}-in-music.png`);
    expect(shared[0].size).toBeGreaterThan(5_000);
  });

  test("reduced motion cross-fades between cards", async ({ connected: page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openWrapped(page);
    await tid(page, "story-next").click();
    await cardIs(page, "top-artist");
    await expect.poll(() => page.evaluate(() => window.__rsStory?.last())).toEqual({ mode: "fade", dir: "next" });
    // The outgoing card fades where it is: no sideways move.
    await tid(page, "story-next").click();
    const leaving = page.locator(".rs-story__slot[data-story-leaving]");
    if ((await leaving.count()) > 0) {
      const transform = await leaving.first().evaluate((el) => getComputedStyle(el).transform);
      expect(["none", "matrix(1, 0, 0, 1, 0, 0)"]).toContain(transform);
    }
    await cardIs(page, "top-artists");
  });
});
