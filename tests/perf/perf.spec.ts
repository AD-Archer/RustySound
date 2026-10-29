// Perf baseline. Run with `just perf` (PERF=1) on perf-chromium-mac and perf-webkit-mac.
// Writes perf-<project>.json into RS_PERF_OUT (default tests/.out/perf); tests/perf/report.ts
// turns those into markdown and can diff against an earlier run.
import { execSync } from "node:child_process";
import { mkdirSync, readFileSync, statSync, writeFileSync, readdirSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join } from "node:path";
import type { BrowserContext, Page } from "@playwright/test";
import { audioState, expect, isolateNetwork, seedServer, test, waitForAppReady, type MockFixtures } from "../support/fixtures";
import { MOCK_URL, REPO_ROOT, SCRATCH_OUT, WEB_DIR, WEB_URL } from "../support/env";
import { FMC_SCRIPT_CONFIG, frameStats, instrument, longtaskStats, median, probeStats, round, type PerfWindow } from "./instrument";
import { summariseTrace } from "./trace";

const OUT = SCRATCH_OUT;
const COLD_RUNS = Number(process.env.RS_PERF_COLD_RUNS ?? 5);
const PLAYBACK_SECONDS = Number(process.env.RS_PERF_PLAYBACK_SECONDS ?? 20);
const TARGET_ROWS = Number(process.env.RS_PERF_TARGET_ROWS ?? 1000);

// Each test adds its section here; afterEach merges it into perf-<project>.json, so a failing
// test (which restarts the worker) never loses the other sections. Tests run one at a time
// (workers: 1 in PERF mode) but are not serial, so one failure doesn't skip the rest.
const results: Record<string, any> = {};
const RUN_ID = process.env.RS_PERF_RUN_ID ?? "local";

function contextOptions(use: Record<string, any>) {
  const keys = ["viewport", "deviceScaleFactor", "userAgent", "isMobile", "hasTouch", "screen", "locale"];
  return Object.fromEntries(keys.filter((k) => use[k] !== undefined).map((k) => [k, use[k]]));
}

/** For fresh contexts: hermetic network, seeded server, instrumentation. */
async function instrumented(context: BrowserContext) {
  await isolateNetwork(context);
  await seedServer(context);
  await instrumentOnly(context);
}

/** For the `connected` fixture page, which already has the network isolated and the server seeded. */
async function instrumentOnly(context: BrowserContext) {
  await context.addInitScript(instrument, FMC_SCRIPT_CONFIG);
}

const perf = (page: Page) => page.evaluate(() => (window as unknown as PerfWindow).__rsPerf !== undefined);

async function record(page: Page, ms: number) {
  await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.start());
  await page.waitForTimeout(ms);
  const raw = await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.stop());
  return summariseWindow(raw);
}

function summariseWindow(raw: { frames: number[]; probeGaps: number[]; longtasks: { duration: number }[]; mutations: number; elapsed: number }) {
  return {
    elapsedMs: round(raw.elapsed),
    ...frameStats(raw.frames),
    ...probeStats(raw.probeGaps),
    longtaskApi: longtaskStats(raw.longtasks),
    domMutations: raw.mutations,
    domMutationsPerSecond: round(raw.mutations / (raw.elapsed / 1000)),
  };
}

async function cdpMetrics(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Performance.enable");
  return async () => {
    const { metrics } = await cdp.send("Performance.getMetrics");
    return Object.fromEntries(metrics.map((m: { name: string; value: number }) => [m.name, m.value]));
  };
}

function metricDelta(before: Record<string, number>, after: Record<string, number>) {
  const ms = (k: string) => round((after[k] - before[k]) * 1000);
  return {
    taskMs: ms("TaskDuration"),
    scriptMs: ms("ScriptDuration"),
    layoutMs: ms("LayoutDuration"),
    recalcStyleMs: ms("RecalcStyleDuration"),
    layoutCount: after.LayoutCount - before.LayoutCount,
    recalcStyleCount: after.RecalcStyleCount - before.RecalcStyleCount,
    nodesAfter: after.Nodes,
    jsHeapUsedMbAfter: round(after.JSHeapUsedSize / 1048576),
  };
}

async function openAlbum(page: Page, mock: MockFixtures) {
  await page.goto("/");
  await waitForAppReady(page);
  await page.getByText(mock.album.name, { exact: true }).first().click();
  await expect(page.getByText(mock.albumSongs[0].title, { exact: true }).first()).toBeVisible();
}

const rowCount = (page: Page) =>
  page.evaluate(() => {
    const testid = document.querySelectorAll('[data-testid="track-row"]').length;
    if (testid) return testid;
    return document.querySelectorAll("main div.space-y-1 > div").length;
  });

test.afterEach(async ({ browser }, testInfo) => {
  mkdirSync(OUT, { recursive: true });
  const file = join(OUT, `perf-${testInfo.project.name}.json`);
  let previous: Record<string, any> = {};
  try {
    const j = JSON.parse(readFileSync(file, "utf8"));
    if (j.meta?.runId === RUN_ID) previous = j;
  } catch {}
  const failures = { ...(previous.meta?.failures ?? {}) };
  if (testInfo.status !== testInfo.expectedStatus) failures[testInfo.title] = (testInfo.error?.message ?? "failed").split("\n")[0].slice(0, 300);
  let commit = "unknown";
  try {
    commit = execSync("git rev-parse --short HEAD", { cwd: REPO_ROOT }).toString().trim();
  } catch {}
  let wasmBytes: number | null = null;
  try {
    const f = readdirSync(join(WEB_DIR, "assets")).find((n) => n.endsWith(".wasm"));
    if (f) wasmBytes = statSync(join(WEB_DIR, "assets", f)).size;
  } catch {}
  const doc: Record<string, any> = {
    ...previous,
    meta: {
      runId: RUN_ID,
      failures,
      project: testInfo.project.name,
      browser: `${browser.browserType().name()} ${browser.version()}`,
      date: new Date().toISOString(),
      commit,
      webUrl: WEB_URL,
      mockUrl: MOCK_URL,
      mockLatencyMs: Number(process.env.MOCK_LATENCY_MS ?? 0),
      wasmBytes,
      viewport: testInfo.project.use.viewport,
      settings: { COLD_RUNS, PLAYBACK_SECONDS, TARGET_ROWS },
    },
  };
  // One level of merge so b1 and b2 can both contribute to `playback`.
  for (const [k, v] of Object.entries(results)) {
    doc[k] = v && typeof v === "object" && !Array.isArray(v) ? { ...(previous[k] ?? {}), ...v } : v;
  }
  writeFileSync(file, JSON.stringify(doc, null, 2));
});

test("a. cold load to first meaningful content", async ({ browser }, testInfo) => {
  const runs: Record<string, number | string | null>[] = [];
  for (let i = 0; i < COLD_RUNS; i++) {
    const context = await browser.newContext({ ...contextOptions(testInfo.project.use as any), baseURL: WEB_URL });
    await instrumented(context);
    const page = await context.newPage();
    let requests = 0;
    let imageRequests = 0;
    page.on("request", (r) => {
      requests++;
      if (r.url().includes("getCoverArt")) imageRequests++;
    });
    await page.goto("/");
    await page.waitForFunction(() => (window as unknown as PerfWindow).__rsPerf?.fmc !== null, null, { timeout: 60_000 });
    const m = await page.evaluate(() => {
      const p = (window as unknown as PerfWindow).__rsPerf;
      const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
      const fcp = performance.getEntriesByName("first-contentful-paint")[0]?.startTime ?? null;
      const wasm = (performance.getEntriesByType("resource") as PerformanceResourceTiming[]).find((r) => r.name.endsWith(".wasm"));
      return {
        fmcMs: p.fmc,
        fmcHeading: p.fmcDetail,
        firstContentfulPaintMs: fcp,
        domContentLoadedMs: nav?.domContentLoadedEventEnd ?? null,
        loadEventMs: nav?.loadEventEnd ?? null,
        wasmFetchedMs: wasm?.responseEnd ?? null,
        wasmInstantiatedMs: p.wasmInstantiated,
        appMountedMs: p.mainFirstChild,
      };
    });
    runs.push({ ...m, requestsAtFmc: requests, imageRequestsAtFmc: imageRequests });
    await context.close();
  }
  const pick = (k: string) => runs.map((r) => Number(r[k])).filter((n) => Number.isFinite(n));
  results.coldLoad = {
    runs: runs.length,
    medianFmcMs: round(median(pick("fmcMs"))),
    minFmcMs: round(Math.min(...pick("fmcMs"))),
    maxFmcMs: round(Math.max(...pick("fmcMs"))),
    medianFcpMs: round(median(pick("firstContentfulPaintMs"))),
    medianWasmFetchedMs: round(median(pick("wasmFetchedMs"))),
    medianWasmInstantiatedMs: round(median(pick("wasmInstantiatedMs"))),
    medianAppMountedMs: round(median(pick("appMountedMs"))),
    medianRequestsAtFmc: median(pick("requestsAtFmc")),
    medianImageRequestsAtFmc: median(pick("imageRequestsAtFmc")),
    detail: runs,
  };
  expect(results.coldLoad.medianFmcMs).toBeGreaterThan(0);
});

/** Measure one playback window (plus a traced window on Chromium). Audio must already be playing. */
async function measurePlayback(page: Page, label: string, isChromium: boolean, projectName: string) {
  await page.waitForTimeout(2000);
  const metrics = isChromium ? await cdpMetrics(page) : null;
  const before = metrics ? await metrics() : null;
  const t0 = (await audioState(page))!.currentTime;
  const playing = await record(page, PLAYBACK_SECONDS * 1000);
  const t1 = (await audioState(page))!.currentTime;
  const after = metrics ? await metrics() : null;

  let trace: unknown = null;
  if (isChromium) {
    // Second window under CDP tracing (separate so tracing overhead doesn't skew the first).
    const browser = page.context().browser()!;
    await browser.startTracing(page, { categories: ["toplevel", "devtools.timeline", "disabled-by-default-devtools.timeline", "v8"] });
    await page.waitForTimeout(PLAYBACK_SECONDS * 1000);
    const buf = await browser.stopTracing();
    trace = summariseTrace(buf, PLAYBACK_SECONDS * 1000);
    mkdirSync(join(OUT, "traces"), { recursive: true });
    writeFileSync(join(OUT, "traces", `playback-${label}-${projectName}.json.gz`), gzipSync(buf));
  }
  expect(t1 - t0, "audio kept playing through the window").toBeGreaterThan(PLAYBACK_SECONDS * 0.8);
  return {
    audioAdvancedSeconds: round(t1 - t0),
    playing,
    cdpMetricsDelta: before && after ? metricDelta(before, after) : null,
    trace,
  };
}

test("b1. main thread during playback, album queue (app idle)", async ({ connected: page, mock }, testInfo) => {
  const isChromium = testInfo.project.name.includes("chromium");
  await instrumentOnly(page.context());
  await openAlbum(page, mock);
  expect(await perf(page)).toBe(true);
  const longtaskSupported = await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.longtaskSupported);

  // Reference: the same screen, nothing playing.
  await page.waitForTimeout(1500);
  const idlePaused = await record(page, 10_000);

  await page.getByText(mock.albumSongs[0].title, { exact: true }).first().click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 20_000 }).toBeGreaterThan(1);
  const m = await measurePlayback(page, "album", isChromium, testInfo.project.name);

  results.playback = {
    seconds: PLAYBACK_SECONDS,
    longtaskApiSupported: longtaskSupported,
    idlePaused10s: idlePaused,
    albumQueue: { queueLength: mock.albumSongs.length, screen: "album detail", ...m },
  };
});

test("b2. main thread during playback, 1,200-song queue (app idle on Home)", async ({ connected: page, mock }, testInfo) => {
  const isChromium = testInfo.project.name.includes("chromium");
  await instrumentOnly(page.context());
  await page.goto(`/playlist/${mock.bigPlaylist.id}/mock-server-0001`);
  await waitForAppReady(page);
  await page.getByTitle("Play playlist").click();
  await expect.poll(async () => (await audioState(page))?.currentTime ?? 0, { timeout: 30_000 }).toBeGreaterThan(1);
  // Go Home in-app (page.goto would reload and drop the queue).
  await page.getByRole("button", { name: "Open menu" }).click();
  await page.getByRole("button", { name: "Home", exact: true }).or(page.getByRole("link", { name: "Home", exact: true })).first().click();
  await waitForAppReady(page);
  await expect(page.getByRole("heading", { name: "Recently Added Albums" })).toBeVisible();
  const m = await measurePlayback(page, "bigqueue", isChromium, testInfo.project.name);
  results.playback = { ...(results.playback ?? {}), bigQueue: { queueLength: mock.bigPlaylist.songCount, screen: "home", ...m } };
});

test("c+d. songs view: load ~1000 rows, scroll, heap", async ({ connected: page }, testInfo) => {
  const isChromium = testInfo.project.name.includes("chromium");
  await instrumentOnly(page.context());
  await page.goto("/songs");
  await waitForAppReady(page);
  await expect.poll(() => rowCount(page), { timeout: 30_000 }).toBeGreaterThan(0);

  // Load phase: keep pressing "View more" (the current app has no infinite scroll) until TARGET_ROWS.
  const clickMs: number[] = [];
  await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.start());
  const loadStart = Date.now();
  let rows = await rowCount(page);
  for (let i = 0; i < 200 && rows < TARGET_ROWS; i++) {
    const more = page.getByRole("button", { name: /view more|load more|show more/i }).last();
    const before = rows;
    const t = Date.now();
    if (await more.isVisible().catch(() => false)) {
      await more.scrollIntoViewIfNeeded();
      await more.click();
    } else {
      await page.evaluate(() => {
        const el = document.querySelector("main") ?? document.scrollingElement!;
        el.scrollTop = el.scrollHeight;
      });
    }
    await expect.poll(() => rowCount(page), { timeout: 30_000 }).toBeGreaterThan(before);
    rows = await rowCount(page);
    clickMs.push(Date.now() - t);
  }
  const loadMs = Date.now() - loadStart;
  const loadWindow = summariseWindow(await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.stop()));

  // Scroll phase: top to bottom at a steady 100 px per frame.
  await page.evaluate(() => {
    const el = document.querySelector("main") ?? document.scrollingElement!;
    el.scrollTop = 0;
  });
  await page.waitForTimeout(500);
  await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.start());
  const scrollStart = Date.now();
  const scrolled = await page.evaluate(
    () =>
      new Promise<number>((resolve) => {
        const el = (document.querySelector("main") ?? document.scrollingElement!) as HTMLElement;
        const step = () => {
          const before = el.scrollTop;
          el.scrollTop = before + 100;
          if (el.scrollTop === before || el.scrollTop + el.clientHeight >= el.scrollHeight - 1) resolve(el.scrollHeight);
          else requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      }),
  );
  const scrollMs = Date.now() - scrollStart;
  const scrollWindow = summariseWindow(await page.evaluate(() => (window as unknown as PerfWindow).__rsPerf.stop()));

  // d. memory after the scroll.
  const domNodes = await page.evaluate(() => document.getElementsByTagName("*").length);
  const wasmMemoryMb = await page.evaluate(() => {
    const m = (window as unknown as PerfWindow).__rsPerf.wasmMemory;
    return m ? Math.round((m.buffer.byteLength / 1048576) * 10) / 10 : null;
  });
  let heap: Record<string, number> | null = null;
  if (isChromium) {
    const cdp = await page.context().newCDPSession(page);
    const pre = await cdp.send("Runtime.getHeapUsage");
    await cdp.send("HeapProfiler.enable");
    await cdp.send("HeapProfiler.collectGarbage");
    const post = await cdp.send("Runtime.getHeapUsage");
    const counters = await cdp.send("Memory.getDOMCounters");
    heap = {
      jsHeapUsedMb: round(pre.usedSize / 1048576),
      jsHeapTotalMb: round(pre.totalSize / 1048576),
      jsHeapUsedAfterGcMb: round(post.usedSize / 1048576),
      documents: counters.documents,
      nodes: counters.nodes,
      jsEventListeners: counters.jsEventListeners,
    };
  }

  results.songs = {
    targetRows: TARGET_ROWS,
    rowsRendered: rows,
    loadClicks: clickMs.length,
    loadTotalMs: loadMs,
    firstClickMs: clickMs[0] ?? null,
    medianClickMs: median(clickMs),
    lastClickMs: clickMs[clickMs.length - 1] ?? null,
    clickMs,
    loadPhase: loadWindow,
    scroll: { scrollHeightPx: scrolled, durationMs: scrollMs, ...scrollWindow },
    memory: { domNodes, wasmMemoryMb, ...(heap ?? {}) },
  };
  expect(rows).toBeGreaterThanOrEqual(Math.min(TARGET_ROWS, 900));
});

test("e. image requests on the albums page", async ({ browser }, testInfo) => {
  const context = await browser.newContext({ ...contextOptions(testInfo.project.use as any), baseURL: WEB_URL });
  await instrumented(context);
  const page = await context.newPage();
  const images: { url: string; bytes: number }[] = [];
  page.on("requestfinished", async (r) => {
    if (!r.url().includes("getCoverArt")) return;
    const sizes = await r.sizes().catch(() => null);
    images.push({ url: r.url(), bytes: sizes?.responseBodySize ?? 0 });
  });
  await page.goto("/albums");
  await waitForAppReady(page);
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(1500);
  const initial = images.length;
  const initialBytes = images.reduce((n, i) => n + i.bytes, 0);
  const imgInfo = await page.evaluate(() => {
    const imgs = [...document.querySelectorAll("img")];
    return {
      imgElements: imgs.length,
      coverImgElements: imgs.filter((i) => i.src.includes("getCoverArt")).length,
      lazy: imgs.filter((i) => i.loading === "lazy").length,
      sized: imgs.filter((i) => i.hasAttribute("width") && i.hasAttribute("height")).length,
      inViewport: imgs.filter((i) => {
        const r = i.getBoundingClientRect();
        return r.bottom > 0 && r.top < innerHeight && r.width > 0;
      }).length,
    };
  });
  await page.evaluate(() => {
    const el = document.querySelector("main") ?? document.scrollingElement!;
    el.scrollTop = el.scrollHeight;
  });
  await page.waitForLoadState("networkidle");
  await page.waitForTimeout(1500);
  const sizesRequested = [...new Set(images.map((i) => new URL(i.url).searchParams.get("size")))];
  results.albumsImages = {
    imageRequestsInitial: initial,
    imageBytesInitialKb: round(initialBytes / 1024),
    imageRequestsAfterScroll: images.length,
    uniqueImageUrls: new Set(images.map((i) => i.url)).size,
    sizesRequested,
    ...imgInfo,
  };
  await context.close();
  expect(initial).toBeGreaterThan(0);
});
