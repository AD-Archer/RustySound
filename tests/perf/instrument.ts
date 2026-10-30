// In-page instrumentation injected with addInitScript before any app script runs.
// Works on Chromium and WebKit; uses the Long Tasks API where the engine has it (Chromium)
// and a timer-lateness probe everywhere so both engines get comparable numbers.

export interface PerfWindow {
  __rsPerf: {
    fmc: number | null;
    fmcDetail: string | null;
    mainFirstChild: number | null;
    wasmInstantiated: number | null;
    wasmMemory: WebAssembly.Memory | null;
    longtaskSupported: boolean;
    longtasks: { start: number; duration: number }[];
    recording: boolean;
    frames: number[];
    probeGaps: number[];
    mutations: number;
    start(): void;
    stop(): { frames: number[]; probeGaps: number[]; longtasks: { start: number; duration: number }[]; mutations: number; elapsed: number };
  };
}

/** Selectors that mean "the first meaningful content is on screen". Kept loose so the redesign also matches. */
export const FMC_SCRIPT_CONFIG = {
  blocking: /App initializing, please wait|Loading Home|Loading RustySound/i.source,
  /** pre-redesign Home section headings */
  headingText: ["Recently Added Albums", "Most Played Albums"],
  /**
   * Redesign: the Home view's own content. Text matching can't be used there because "Home"
   * is also the sidebar/tab label, which renders with the shell before any content.
   */
  contentSelectors: ['[data-testid="home"] [data-testid="coverflow-center"]', '[data-testid="home"] section[data-testid^="shelf-"]'],
  blockingSelectors: ['[data-testid="boot-overlay"]'],
};

export function instrument(cfg: typeof FMC_SCRIPT_CONFIG) {
  const w = window as unknown as PerfWindow;
  const blocking = new RegExp(cfg.blocking, "i");
  const state: PerfWindow["__rsPerf"] = {
    fmc: null,
    fmcDetail: null,
    mainFirstChild: null,
    wasmInstantiated: null,
    wasmMemory: null,
    longtaskSupported: false,
    longtasks: [],
    recording: false,
    frames: [],
    probeGaps: [],
    mutations: 0,
    start() {},
    stop() {
      return { frames: [], probeGaps: [], longtasks: [], mutations: 0, elapsed: 0 };
    },
  };
  w.__rsPerf = state;

  // Capture the wasm instance's linear memory (wasm-bindgen instantiates once).
  const wrap = (name: "instantiate" | "instantiateStreaming") => {
    const orig = (WebAssembly as any)[name];
    if (!orig) return;
    (WebAssembly as any)[name] = async function (...args: unknown[]) {
      const result = await orig.apply(this, args);
      try {
        const instance: WebAssembly.Instance = result.instance ?? result;
        const mem = Object.values(instance.exports).find((v) => v instanceof WebAssembly.Memory) as WebAssembly.Memory | undefined;
        if (mem) state.wasmMemory = mem;
        state.wasmInstantiated = performance.now();
      } catch {}
      return result;
    };
  };
  wrap("instantiateStreaming");
  wrap("instantiate");

  try {
    const po = new PerformanceObserver((list) => {
      if (!state.recording) return;
      for (const e of list.getEntries()) state.longtasks.push({ start: e.startTime, duration: e.duration });
    });
    po.observe({ type: "longtask", buffered: false });
    state.longtaskSupported = PerformanceObserver.supportedEntryTypes?.includes("longtask") ?? false;
  } catch {
    state.longtaskSupported = false;
  }

  // First meaningful content: blocking overlays gone, a known section heading present, a cover decoded.
  const checkFmc = () => {
    // Stop polling once found, once a measurement window starts, or after 30 s (pages other
    // than Home never match, and a rAF loop would otherwise run inside the playback windows).
    if (state.fmc !== null || state.recording || performance.now() > 30_000) return;
    const main = document.getElementById("main");
    if (main && main.childElementCount > 0 && state.mainFirstChild === null) state.mainFirstChild = performance.now();
    const body = document.body;
    if (!body) return requestAnimationFrame(checkFmc);
    const text = body.innerText || "";
    const heading =
      cfg.contentSelectors.find((s) => document.querySelector(s)) ?? cfg.headingText.find((h) => text.includes(h));
    const overlay = cfg.blockingSelectors.some((s) => document.querySelector(s));
    const imgs = document.querySelectorAll<HTMLImageElement>('img[src*="getCoverArt"]');
    let decoded = false;
    for (const img of imgs) {
      if (img.complete && img.naturalWidth > 0) {
        const r = img.getBoundingClientRect();
        if (r.width > 0 && r.bottom > 0 && r.top < innerHeight) {
          decoded = true;
          break;
        }
      }
    }
    if (heading && decoded && !overlay && !blocking.test(text)) {
      state.fmc = performance.now();
      state.fmcDetail = heading;
      return;
    }
    requestAnimationFrame(checkFmc);
  };
  requestAnimationFrame(checkFmc);

  let mo: MutationObserver | null = null;
  let t0 = 0;
  state.start = () => {
    state.frames = [];
    state.probeGaps = [];
    state.longtasks = [];
    state.mutations = 0;
    state.recording = true;
    t0 = performance.now();
    let last = performance.now();
    const frame = (t: number) => {
      if (!state.recording) return;
      state.frames.push(t - last);
      last = t;
      requestAnimationFrame(frame);
    };
    requestAnimationFrame((t) => {
      last = t;
      requestAnimationFrame(frame);
    });
    // Timer probe: a 10 ms heartbeat; any gap well beyond 10 ms means the main thread was busy.
    let prev = performance.now();
    const tick = () => {
      if (!state.recording) return;
      const now = performance.now();
      state.probeGaps.push(now - prev);
      prev = now;
      setTimeout(tick, 10);
    };
    setTimeout(tick, 10);
    mo = new MutationObserver((records) => {
      state.mutations += records.length;
    });
    mo.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
  };
  state.stop = () => {
    state.recording = false;
    mo?.disconnect();
    return {
      frames: state.frames.slice(1),
      probeGaps: state.probeGaps,
      longtasks: state.longtasks,
      mutations: state.mutations,
      elapsed: performance.now() - t0,
    };
  };
}

// ---------------------------------------------------------------- summaries (run in Node)

export function frameStats(frames: number[]) {
  const budget = 1000 / 60;
  let dropped = 0;
  let over50 = 0;
  for (const d of frames) {
    dropped += Math.max(0, Math.round(d / budget) - 1);
    if (d > 50) over50++;
  }
  const sorted = frames.slice().sort((a, b) => a - b);
  const pct = (p: number) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))] : 0);
  return {
    frames: frames.length,
    droppedFrames: dropped,
    framesOver50ms: over50,
    p50FrameMs: round(pct(0.5)),
    p95FrameMs: round(pct(0.95)),
    maxFrameMs: round(sorted[sorted.length - 1] ?? 0),
  };
}

/** Timer-probe estimate: gaps > 50 ms count as long tasks, blocking = gap - 50. */
export function probeStats(gaps: number[]) {
  const long = gaps.filter((g) => g > 50);
  return {
    probeLongTasks: long.length,
    probeBlockingMs: round(long.reduce((n, g) => n + (g - 50), 0)),
    probeMaxGapMs: round(Math.max(0, ...gaps)),
  };
}

export function longtaskStats(tasks: { duration: number }[]) {
  return {
    longTasks: tasks.length,
    totalBlockingMs: round(tasks.reduce((n, t) => n + Math.max(0, t.duration - 50), 0)),
    longestTaskMs: round(Math.max(0, ...tasks.map((t) => t.duration))),
  };
}

export function median(xs: number[]) {
  const s = xs.slice().sort((a, b) => a - b);
  if (!s.length) return 0;
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}

export function round(n: number, d = 1) {
  const f = 10 ** d;
  return Math.round(n * f) / f;
}
