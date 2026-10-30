// Fast scrolling through long lists must never show empty (black) screen.
//
// The owner's report: flinging through Albums, Songs or a playlist on the iPhone went black for
// seconds before rows appeared. Each test here flings a long list with momentum-style scrolling
// (tests/support/scroll-fill.ts: wall-clock driven, so a slow main thread shows up as empty
// space, not as a slower scroll) and measures, every frame, how much of the screen the list
// should fill has no row or tile. Thresholds are per frame and per run.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Page } from "@playwright/test";
import { REPO_ROOT } from "../../support/env";
import { expect, MOCK_SERVER_ID, test } from "../../support/fixtures";
import { installScrollFill, type FillConfig, type FillResult, type FillWindow } from "../../support/scroll-fill";
import { gotoReady, isCompact, tid } from "./helpers";

/** Mean empty share of the list area during the fling, and the worst frame (95th percentile). */
const MAX_MEAN_BLANK = Number(process.env.RS_FILL_MAX_MEAN ?? 0.02);
const MAX_P95_BLANK = Number(process.env.RS_FILL_MAX_P95 ?? 0.1);
/** No stretch with >10% empty longer than this (ms). */
const MAX_BLANK_RUN_MS = Number(process.env.RS_FILL_MAX_RUN_MS ?? 150);
const REPORT = process.env.RS_FILL_REPORT === "1";

type Fling = { flings: number; velocity: number; gapMs: number; decel: number; direction: 1 | -1 };
/** Repeated hard flicks, like thumbing quickly through a long list (about 4,000 px per flick). */
const HARD: Fling = { flings: 6, velocity: 8000, gapMs: 300, decel: 0.998, direction: 1 };

async function measure(page: Page, cfg: FillConfig, fling: Fling, label: string): Promise<FillResult> {
  await page.evaluate(installScrollFill);
  // Chromium only: main-thread cost of the fling (script, style, layout), from CDP.
  const cdp = REPORT && test.info().project.name.startsWith("chromium") ? await page.context().newCDPSession(page) : null;
  const metrics = async () => {
    if (!cdp) return {};
    const { metrics } = await cdp.send("Performance.getMetrics");
    return Object.fromEntries(metrics.map((m: { name: string; value: number }) => [m.name, m.value]));
  };
  if (cdp) await cdp.send("Performance.enable");
  const before = await metrics();
  await page.evaluate((c) => (window as FillWindow).__rsFill!.start(c), cfg);
  await page.evaluate((f) => (window as FillWindow).__rsFill!.fling(f), fling);
  const result = await page.evaluate(() => (window as FillWindow).__rsFill!.stop(4000));
  const after = await metrics();
  const cost = cdp
    ? ` | task ${Math.round((after.TaskDuration - before.TaskDuration) * 1000)} ms, script ${Math.round((after.ScriptDuration - before.ScriptDuration) * 1000)} ms, style ${Math.round((after.RecalcStyleDuration - before.RecalcStyleDuration) * 1000)} ms, layout ${Math.round((after.LayoutDuration - before.LayoutDuration) * 1000)} ms, nodes ${after.Nodes}`
    : "";
  const { samples, ...summary } = result;
  const line = `[scroll-fill] ${label}: mean ${(summary.meanBlank * 100).toFixed(1)}% p95 ${(summary.p95Blank * 100).toFixed(1)}% max ${(summary.maxBlank * 100).toFixed(1)}% longest ${summary.longestBlankMs} ms, art pending ${(summary.meanArtPending * 100).toFixed(0)}%, settle ${summary.settleMs ?? ">4000"} ms, ${summary.frames} frames @${summary.medianFrameMs} ms, ${summary.distancePx} px${cost}`;
  console.log(line);
  test.info().annotations.push({ type: "scroll-fill", description: line });
  if (REPORT) {
    // RS_FILL_REPORT=1 keeps every frame's sample, for charts and debugging.
    const file = join(REPO_ROOT, "tests/.out/scroll-fill", `${label}-${test.info().project.name}.json`);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, JSON.stringify({ summary, samples }));
  }
  return result;
}

function expectFilled(r: FillResult, label: string) {
  expect(r.distancePx, `${label}: the fling should travel`).toBeGreaterThan(2000);
  expect(r.meanBlank, `${label}: mean empty share`).toBeLessThanOrEqual(MAX_MEAN_BLANK);
  expect(r.p95Blank, `${label}: 95th percentile empty share`).toBeLessThanOrEqual(MAX_P95_BLANK);
  expect(r.longestBlankMs, `${label}: longest empty stretch`).toBeLessThanOrEqual(MAX_BLANK_RUN_MS);
}

const LIST_ITEM = "li";

/** A remote server: every page of songs or albums takes `ms` to arrive. */
async function slowPages(page: Page, ms: number) {
  await page.route(/\/(api\/song|rest\/getAlbumList2)\b/, async (route) => {
    await new Promise((r) => setTimeout(r, ms));
    await route.fallback();
  });
}

test.describe("fast scrolling keeps lists filled", () => {
  test.describe.configure({ timeout: 120_000 });

  // The paged lists must ask for the next page early enough (and show placeholder rows while
  // it comes) that a fling never runs into the end of what's loaded.
  test("Songs on a slow server", async ({ connected: page }) => {
    await slowPages(page, 400);
    await gotoReady(page, "/songs");
    await expect(tid(page, "track-row").first()).toBeVisible();
    const r = await measure(page, { list: '[data-testid="song-list"]', item: LIST_ITEM }, HARD, "songs-slow");
    expectFilled(r, "Songs on a slow server");
  });

  test("Albums grid on a slow server", async ({ connected: page }) => {
    await slowPages(page, 400);
    await gotoReady(page, "/albums");
    await expect(tid(page, "album-tile").first()).toBeVisible();
    const r = await measure(page, { list: ".rs-library-covers", item: ".rs-library-covers__cell", gapPx: 32 }, HARD, "albums-grid-slow");
    expectFilled(r, "Albums grid on a slow server");
  });

  test("Songs", async ({ connected: page }) => {
    await gotoReady(page, "/songs");
    await expect(tid(page, "track-row").first()).toBeVisible();
    const r = await measure(page, { list: '[data-testid="song-list"]', item: LIST_ITEM }, HARD, "songs");
    expectFilled(r, "Songs");
  });

  test("Deep Cuts (1,200-song playlist)", async ({ connected: page, mock }) => {
    await gotoReady(page, `/playlist/${mock.bigPlaylist.id}/${MOCK_SERVER_ID}`);
    await expect(tid(page, "track-row").first()).toBeVisible({ timeout: 30_000 });
    const r = await measure(page, { list: '[data-testid="playlist-tracks"]', item: LIST_ITEM }, HARD, "deep-cuts");
    expectFilled(r, "Deep Cuts");
  });

  test("Albums grid", async ({ connected: page }) => {
    await gotoReady(page, "/albums");
    await expect(tid(page, "album-tile").first()).toBeVisible();
    const r = await measure(page, { list: ".rs-library-covers", item: ".rs-library-covers__cell", gapPx: 32 }, HARD, "albums-grid");
    expectFilled(r, "Albums grid");
  });

  test("Artists", async ({ connected: page }) => {
    await gotoReady(page, "/artists");
    await expect(tid(page, "artist-row").first()).toBeVisible();
    // 60 artists: a gentler fling still crosses the whole list.
    const r = await measure(page, { list: '[data-testid="artist-list"]', item: LIST_ITEM }, { ...HARD, flings: 2 }, "artists");
    expect(r.meanBlank).toBeLessThanOrEqual(MAX_MEAN_BLANK);
    expect(r.longestBlankMs).toBeLessThanOrEqual(MAX_BLANK_RUN_MS);
  });

  test("Playlists", async ({ connected: page }) => {
    await gotoReady(page, "/playlists");
    await expect(tid(page, "playlist-row").first()).toBeVisible();
    const r = await measure(page, { list: '[data-testid="playlist-list"]', item: LIST_ITEM }, { ...HARD, flings: 1 }, "playlists");
    expect(r.meanBlank).toBeLessThanOrEqual(MAX_MEAN_BLANK);
    expect(r.longestBlankMs).toBeLessThanOrEqual(MAX_BLANK_RUN_MS);
  });

  test("Favorites", async ({ connected: page }) => {
    await gotoReady(page, "/favorites");
    await expect(tid(page, "favorites-songs")).toBeVisible();
    const r = await measure(page, { list: '[data-testid="favorites-songs"]', item: LIST_ITEM }, { ...HARD, flings: 3 }, "favorites");
    expect(r.meanBlank).toBeLessThanOrEqual(MAX_MEAN_BLANK);
    expect(r.longestBlankMs).toBeLessThanOrEqual(MAX_BLANK_RUN_MS);
  });

  test("Albums in Cover Flow", async ({ connected: page }) => {
    test.skip(isCompact(page), "Cover Flow is a wide-layout view");
    await gotoReady(page, "/albums");
    await tid(page, "albums-sort").click();
    await tid(page, "albums-view-flow").click();
    const flow = tid(page, "coverflow");
    await expect(flow).toBeVisible();
    await flow.focus();
    // Hold the arrow key: one cover per keypress, as fast as key repeat.
    const t0 = Date.now();
    for (let i = 0; i < 60; i++) await page.keyboard.press("ArrowRight", { delay: 0 });
    // The centre cover and its neighbours should show art (or a placeholder box) quickly.
    await expect
      .poll(
        () =>
          page.evaluate(() => {
            const items = [...document.querySelectorAll<HTMLElement>('[data-testid="coverflow"] .rs-art')];
            const w = window.innerWidth;
            const onScreen = items.filter((el) => {
              const r = el.getBoundingClientRect();
              return r.width > 0 && r.right > 0 && r.left < w;
            });
            const pending = onScreen.filter((el) => {
              const img = el.querySelector("img");
              return img && (!img.complete || img.naturalWidth === 0);
            }).length;
            return { shown: onScreen.length, pending };
          }),
        { timeout: 5000 },
      )
      .toMatchObject({ pending: 0 });
    console.log(`[scroll-fill] coverflow: 60 steps + covers in ${Date.now() - t0} ms`);
  });
});
