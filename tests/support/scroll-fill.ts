// Scroll-fill probe: while a long list is flung, how much of the screen has no row or tile?
//
// Installed into the page with `page.evaluate(installScrollFill)`. Every animation frame it
// measures the part of the main scroller that the list should fill (from the list's top edge,
// or the top of the scroller once the list starts above it, to the bottom of the visible area
// above the tab bar / player bar). That band counts as filled where a row or tile is rendered.
// Below the last loaded item the band still counts as empty while more pages are coming
// (a load-more sentinel is in the DOM), because that is what a user sees as black.
//
// The scroll driver is wall-clock based, like compositor (momentum) scrolling: if the main
// thread stalls, the next frame jumps to where the fling would be by then, so a slow render
// shows up as empty space instead of as a slower scroll.

export interface FillConfig {
  /** The list or grid container (its top edge starts the band). */
  list: string;
  /** Rendered items inside the container (rows, tiles, letter headers). */
  item: string;
  /** Vertical gaps up to this many px between items still count as filled (grid gaps). */
  gapPx?: number;
}

export interface FillSample {
  t: number;
  top: number;
  band: number;
  blank: number;
  art: number;
  artPending: number;
}

export interface FillResult {
  samples: FillSample[];
  /** Time-weighted mean empty share of the band while the fling ran (0..1). */
  meanBlank: number;
  /** Worst single frame (0..1). */
  maxBlank: number;
  /** 95th percentile frame (0..1). */
  p95Blank: number;
  /** Longest stretch with more than 10% of the band empty, in ms. */
  longestBlankMs: number;
  /** Frames sampled and their median interval. */
  frames: number;
  medianFrameMs: number;
  /** Covers on screen that had not loaded yet, time-weighted share (0..1). */
  meanArtPending: number;
  /** After the fling stopped: ms until the band was full and every visible cover loaded. */
  settleMs: number | null;
  distancePx: number;
}

type FillWindow = Window & {
  __rsFill?: {
    fling(opts: { flings: number; velocity: number; gapMs: number; decel: number; direction: 1 | -1 }): Promise<void>;
    start(cfg: FillConfig): void;
    stop(settleMs: number): Promise<FillResult>;
  };
};

/** Runs in the page. Defines window.__rsFill. */
export function installScrollFill() {
  const w = window as FillWindow;
  if (w.__rsFill) return;

  const main = () => document.querySelector<HTMLElement>(".rs-main");

  function visibleBand(cfg: FillConfig) {
    const scroller = main();
    const list = document.querySelector<HTMLElement>(cfg.list);
    if (!scroller || !list) return null;
    const sr = scroller.getBoundingClientRect();
    let bottom = Math.min(sr.bottom, window.innerHeight);
    // Bars that float over the scroller.
    for (const sel of [".rs-tabbar", '[data-testid="mini-player"]', '[data-testid="player-bar"]']) {
      const el = document.querySelector<HTMLElement>(sel);
      if (!el) continue;
      const r = el.getBoundingClientRect();
      if (r.height > 0 && r.top < bottom && r.bottom > sr.top) bottom = Math.min(bottom, r.top);
    }
    const lr = list.getBoundingClientRect();
    const more = !!document.querySelector(".rs-library-sentinel, .rs-personal-more");
    const top = Math.max(sr.top, lr.top);
    const end = more ? bottom : Math.min(bottom, lr.bottom);
    return { list, top, bottom: end };
  }

  function sample(cfg: FillConfig, t: number): FillSample {
    const scroller = main();
    const band = visibleBand(cfg);
    const top = scroller?.scrollTop ?? 0;
    if (!band || band.bottom - band.top < 4) return { t, top, band: 0, blank: 0, art: 0, artPending: 0 };
    const gap = cfg.gapPx ?? 2;
    const spans: [number, number][] = [];
    let art = 0;
    let artPending = 0;
    for (const el of band.list.querySelectorAll<HTMLElement>(cfg.item)) {
      const r = el.getBoundingClientRect();
      if (r.height <= 0 || r.bottom <= band.top || r.top >= band.bottom) continue;
      spans.push([Math.max(r.top, band.top), Math.min(r.bottom, band.bottom)]);
      const box = el.querySelector<HTMLElement>(".rs-art");
      if (box) {
        art++;
        const img = box.querySelector("img");
        if (!img || !img.complete || img.naturalWidth === 0) artPending++;
      }
    }
    spans.sort((a, b) => a[0] - b[0]);
    let covered = 0;
    let cursor = band.top;
    for (const [a, b] of spans) {
      const start = a - cursor <= gap ? cursor : a;
      if (b > start) {
        covered += b - Math.max(start, cursor);
        cursor = Math.max(cursor, b);
      }
    }
    // A gap up to `gap` above the band's end is not "empty" either (last row's margin).
    if (band.bottom - cursor <= gap) covered += band.bottom - cursor;
    const height = band.bottom - band.top;
    return { t, top, band: height, blank: Math.max(0, height - covered) / height, art, artPending };
  }

  const frame = () => new Promise<number>((r) => requestAnimationFrame(r));

  w.__rsFill = {
    // Momentum flings: each adds `velocity` (px/s) to the current speed, which then decays
    // like UIScrollView (`decel` per ms, 0.998 is UIScrollView.DecelerationRate.normal).
    async fling({ flings, velocity, gapMs, decel, direction }) {
      const scroller = main();
      if (!scroller) throw new Error("no .rs-main");
      const start = performance.now();
      const kicks = Array.from({ length: flings }, (_, i) => start + i * gapMs);
      const lnd = Math.log(decel);
      // Distance travelled by a kick of v px/s after dt ms: v/1000 * (decel^dt - 1) / ln(decel).
      const travelled = (now: number) =>
        kicks.reduce((sum, k) => (now < k ? sum : sum + (velocity / 1000) * (Math.pow(decel, now - k) - 1) / lnd), 0);
      const origin = scroller.scrollTop;
      const last = kicks[kicks.length - 1];
      for (;;) {
        const now = await frame();
        const target = origin + direction * travelled(now);
        scroller.scrollTop = target;
        const speed = kicks.reduce((v, k) => (now < k ? v : v + velocity * Math.pow(decel, now - k)), 0);
        const atEnd = direction > 0 ? scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 1 : scroller.scrollTop <= 0;
        if (now > last && (speed < 20 || atEnd)) break;
      }
    },

    start(cfg) {
      const samples: FillSample[] = [];
      const scroller = main();
      const state = { cfg, samples, sampling: true, startTop: scroller?.scrollTop ?? 0, loop: Promise.resolve() };
      state.loop = (async () => {
        while (state.sampling) {
          const t = await frame();
          samples.push(sample(cfg, t));
        }
      })();
      (w as any).__rsFillState = state;
    },

    async stop(settleMs) {
      const state = (w as any).__rsFillState as { cfg: FillConfig; samples: FillSample[]; sampling: boolean; startTop: number; loop: Promise<void> };
      const { cfg, samples, startTop } = state;
      const scroller = main();
      const stopped = performance.now();
      // Settle: until the band is full and covers are in, or settleMs passes.
      let settledAt: number | null = null;
      while (performance.now() - stopped < settleMs) {
        const s = await frame();
        const now = sample(cfg, s);
        if (now.blank < 0.001 && now.artPending === 0) {
          settledAt = s;
          break;
        }
      }
      state.sampling = false;
      await state.loop;
      const during = samples.filter((s) => s.t <= stopped && s.band > 0);
      let weighted = 0;
      let artWeighted = 0;
      let total = 0;
      let longest = 0;
      let run = 0;
      for (let i = 1; i < during.length; i++) {
        const dt = during[i].t - during[i - 1].t;
        // Pessimistic: a stalled gap is charged at the worse of its two ends.
        const blank = Math.max(during[i].blank, during[i - 1].blank);
        weighted += blank * dt;
        const ap = (s: FillSample) => (s.art ? s.artPending / s.art : 0);
        artWeighted += Math.max(ap(during[i]), ap(during[i - 1])) * dt;
        total += dt;
        if (blank > 0.1) {
          run += dt;
          longest = Math.max(longest, run);
        } else run = 0;
      }
      const sorted = during.map((s) => s.blank).sort((a, b) => a - b);
      const gaps = during.slice(1).map((s, i) => s.t - during[i].t).sort((a, b) => a - b);
      return {
        samples,
        meanBlank: total ? weighted / total : 0,
        maxBlank: sorted.length ? sorted[sorted.length - 1] : 0,
        p95Blank: sorted.length ? sorted[Math.floor(sorted.length * 0.95)] : 0,
        longestBlankMs: Math.round(longest),
        frames: during.length,
        medianFrameMs: gaps.length ? Math.round(gaps[Math.floor(gaps.length / 2)] * 10) / 10 : 0,
        meanArtPending: total ? artWeighted / total : 0,
        settleMs: settledAt === null ? null : Math.round(settledAt - stopped),
        distancePx: Math.round((scroller?.scrollTop ?? 0) - startTop),
      };
    },
  };
}

export type { FillWindow };
