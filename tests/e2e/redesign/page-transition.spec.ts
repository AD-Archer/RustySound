// Page transitions (src/components/ui/page_transition.js): an iOS push / pop on the compact
// layout, a short fade on wide, a cross-fade between tabs, a plain fade under reduced motion.
// Each test samples the live page (.rs-main) and the frozen copy of the old page
// (.rs-ptx-ghost) every frame while it navigates, so it checks what is on screen, not how.
import type { Page } from "@playwright/test";
import { expect, test } from "../../support/fixtures";
import { gotoReady, isCompact, tid } from "./helpers";

type Layer = { x: number; o: number; w: number } | null;
type Sample = { t: number; main: Layer; ghost: Layer; state: string };

/** Start sampling every animation frame for `ms`. */
async function record(page: Page, ms = 1200) {
  await page.evaluate((ms) => {
    const w = window as any;
    const samples: Sample[] = (w.__ptxSamples = []);
    const t0 = performance.now();
    const read = (el: Element | null): Layer => {
      if (!el) return null;
      const css = getComputedStyle(el);
      const m = css.transform && css.transform !== "none" ? new DOMMatrixReadOnly(css.transform) : null;
      return { x: m ? m.m41 : 0, o: Number(css.opacity), w: (el as HTMLElement).offsetWidth };
    };
    const tick = () => {
      samples.push({
        t: performance.now() - t0,
        main: read(document.querySelector(".rs-main")),
        ghost: read(document.querySelector(".rs-ptx-ghost")),
        state: w.__rsPageTx?.state() ?? "missing",
      });
      if (performance.now() - t0 < ms) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  }, ms);
}

async function samples(page: Page): Promise<Sample[]> {
  // Let the recording run out.
  await expect.poll(() => page.evaluate(() => (window as any).__ptxSamples.at(-1).t), { timeout: 5000 }).toBeGreaterThan(1000);
  return page.evaluate(() => (window as any).__ptxSamples);
}

const lastRun = (page: Page) => page.evaluate(() => (window as any).__rsPageTx?.last() ?? null);

/** Idle, no ghost or dim layer left, the page untransformed and unstyled. */
async function expectSettled(page: Page) {
  await expect.poll(() => page.evaluate(() => (window as any).__rsPageTx?.state())).toBe("idle");
  await expect(page.locator(".rs-ptx-ghost")).toHaveCount(0);
  await expect(page.locator(".rs-ptx-shade")).toHaveCount(0);
  const main = await page.locator(".rs-main").evaluate((el) => ({
    transform: getComputedStyle(el).transform,
    opacity: getComputedStyle(el).opacity,
    inline: (el as HTMLElement).getAttribute("style") ?? "",
    lead: el.classList.contains("rs-ptx-lead"),
    anims: el.getAnimations().length,
  }));
  expect(main).toEqual({ transform: "none", opacity: "1", inline: "", lead: false, anims: 0 });
}

async function openFirstAlbum(page: Page) {
  await gotoReady(page, "/albums");
  await expect(tid(page, "album-tile").first()).toBeVisible();
  await expectSettled(page);
  await record(page);
  await tid(page, "album-tile").first().click();
  await expect(page).toHaveURL(/\/album\//);
}

test.describe("page transitions, compact", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(!isCompact(page), "compact-only (width < 760)");
  });

  test("opening an album pushes it in from the right over the old page", async ({ connected: page }) => {
    await openFirstAlbum(page);
    const s = await samples(page);
    const width = s[0].main!.w;
    // The incoming page was seen partway in from the right edge...
    expect(s.some((f) => f.main && f.main.x > width * 0.1 && f.main.x < width * 0.95), "page sliding in").toBe(true);
    // ...over the old page shifting left (by up to 30%), not off screen.
    const ghostX = s.filter((f) => f.ghost).map((f) => f.ghost!.x);
    expect(Math.min(...ghostX)).toBeLessThan(-width * 0.05);
    expect(Math.min(...ghostX)).toBeGreaterThanOrEqual(-width * 0.31);
    expect(await lastRun(page)).toEqual({ kind: "push", mode: "push" });
    // Done within about a third of a second, then handed back untouched.
    const end = s.findIndex((f, i) => i > 0 && f.state === "idle" && s[i - 1].state !== "idle");
    expect(end).toBeGreaterThan(0);
    expect(s[end].t).toBeLessThan(700);
    await expectSettled(page);
  });

  test("back plays the reverse, from the button and from browser history", async ({ connected: page }) => {
    await openFirstAlbum(page);
    await expect(tid(page, "track-row").first()).toBeVisible();
    await expectSettled(page);

    await record(page);
    await page.locator(".rs-library-back").first().click();
    await expect(page).toHaveURL(/\/albums$/);
    let s = await samples(page);
    const width = s[0].main!.w;
    // The old page leaves to the right; the page underneath comes in from the left.
    expect(s.some((f) => f.ghost && f.ghost.x > width * 0.1), "old page sliding out").toBe(true);
    expect(s.some((f) => f.main && f.main.x < -width * 0.02 && f.main.x >= -width * 0.31), "page underneath catching up").toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "back", mode: "pop" });
    await expectSettled(page);

    // Forward through browser history pushes again; back through it pops.
    await record(page);
    await page.goForward();
    await expect(page).toHaveURL(/\/album\//);
    s = await samples(page);
    expect(s.some((f) => f.main && f.main.x > width * 0.1)).toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "forward", mode: "push" });
    await expectSettled(page);

    await record(page);
    await page.goBack();
    await expect(page).toHaveURL(/\/albums$/);
    s = await samples(page);
    expect(s.some((f) => f.ghost && f.ghost.x > width * 0.1)).toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "back", mode: "pop" });
    await expectSettled(page);
  });

  test("switching tabs cross-fades without a push", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expect(tid(page, "coverflow")).toBeVisible();
    await expectSettled(page);
    for (const tab of ["tab-library", "tab-search", "tab-discover"]) {
      await record(page);
      await tid(page, tab).click();
      const s = await samples(page);
      expect(s.every((f) => (!f.main || f.main.x === 0) && (!f.ghost || f.ghost.x === 0)), `${tab}: nothing slides`).toBe(true);
      expect(s.some((f) => f.ghost && f.ghost.o < 0.95), `${tab}: the old page fades`).toBe(true);
      expect(await lastRun(page)).toEqual({ kind: "tab", mode: "tab" });
      await expectSettled(page);
    }
  });

  test("the current tab from deeper in it pops back to its root", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect(tid(page, "album-tile").first()).toBeVisible();
    await expectSettled(page);
    await tid(page, "tab-library").click();
    await expect(page).toHaveURL(/\/library$/);
    expect(await lastRun(page)).toEqual({ kind: "back", mode: "pop" });
    await expectSettled(page);
  });

  test("reduced motion: a plain fade, nothing slides", async ({ connected: page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await openFirstAlbum(page);
    const s = await samples(page);
    expect(s.every((f) => (!f.main || f.main.x === 0) && (!f.ghost || f.ghost.x === 0))).toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "push", mode: "fade" });
    await expectSettled(page);
  });

  test("a fast double navigation lands on the last page with nothing left over", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect(tid(page, "album-tile").first()).toBeVisible();
    await expectSettled(page);
    // Album, then Search before the push has finished (or even started).
    await tid(page, "album-tile").first().click();
    await tid(page, "tab-search").click();
    await expect(page).toHaveURL(/\/search$/);
    await expectSettled(page);
    await expect(tid(page, "search-input")).toBeVisible();

    // And three in a row from inside one task.
    await page.evaluate(() => {
      (document.querySelector("[data-testid='tab-library']") as HTMLElement).click();
      (document.querySelector("[data-testid='tab-discover']") as HTMLElement).click();
      (document.querySelector("[data-testid='tab-search']") as HTMLElement).click();
    });
    await expect(page).toHaveURL(/\/search$/);
    await expectSettled(page);
    await expect(tid(page, "search-input")).toBeVisible();
  });
});

test.describe("page transitions, wide", () => {
  test.beforeEach(async ({ page }) => {
    test.skip(isCompact(page), "wide-only");
  });

  test("opening an album fades it in with a small shift, no push", async ({ connected: page }) => {
    await openFirstAlbum(page);
    const s = await samples(page);
    const moving = s.filter((f) => f.main && (f.main.x !== 0 || f.main.o < 1));
    expect(moving.length, "the page fades in").toBeGreaterThan(0);
    expect(moving.every((f) => Math.abs(f.main!.x) <= 12), "at most a 12px shift").toBe(true);
    expect(s.every((f) => !f.ghost || f.ghost.x === 0), "the old page doesn't move").toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "push", mode: "shift" });
    await expectSettled(page);
  });

  test("sidebar sections cross-fade", async ({ connected: page }) => {
    await gotoReady(page, "/");
    await expectSettled(page);
    await record(page);
    await tid(page, "nav-albums").click();
    await expect(page).toHaveURL(/\/albums/);
    const s = await samples(page);
    expect(s.every((f) => (!f.main || f.main.x === 0) && (!f.ghost || f.ghost.x === 0))).toBe(true);
    expect(await lastRun(page)).toEqual({ kind: "tab", mode: "tab" });
    await expectSettled(page);
  });
});
