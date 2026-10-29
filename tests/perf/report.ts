// Turn perf-<project>.json files into a markdown report, optionally diffed against an earlier run.
//
//   bun tests/perf/report.ts <dir> [--out report.md] [--compare <baselineDir>]
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2);
const dir = args[0] ?? "tests/.out/perf";
const outIdx = args.indexOf("--out");
const out = outIdx >= 0 ? args[outIdx + 1] : join(dir, "perf-report.md");
const cmpIdx = args.indexOf("--compare");
const cmpDir = cmpIdx >= 0 ? args[cmpIdx + 1] : null;

function load(d: string) {
  const map = new Map<string, any>();
  if (!existsSync(d)) return map;
  for (const f of readdirSync(d)) {
    if (/^perf-.*\.json$/.test(f)) {
      const j = JSON.parse(readFileSync(join(d, f), "utf8"));
      map.set(j.meta.project, j);
    }
  }
  return map;
}

const cur = load(dir);
const base = cmpDir ? load(cmpDir) : new Map<string, any>();
if (!cur.size) {
  console.error(`no perf-*.json in ${dir}`);
  process.exit(1);
}

const get = (o: any, path: string) => path.split(".").reduce((v, k) => (v == null ? v : v[k]), o);

type Row = [label: string, path: string, unit: string, lowerIsBetter?: boolean];
const ROWS: Row[] = [
  ["(a) Cold load: first meaningful content, median", "coldLoad.medianFmcMs", "ms"],
  ["(a) min / max of runs", "coldLoad.minFmcMs|coldLoad.maxFmcMs", "ms"],
  ["(a) First contentful paint, median", "coldLoad.medianFcpMs", "ms"],
  ["(a) wasm fetched / instantiated, median", "coldLoad.medianWasmFetchedMs|coldLoad.medianWasmInstantiatedMs", "ms"],
  ["(a) requests / cover requests at FMC", "coldLoad.medianRequestsAtFmc|coldLoad.medianImageRequestsAtFmc", ""],
  ["(b) Idle, nothing playing (10 s): DOM mutations/s", "playback.idlePaused10s.domMutationsPerSecond", "/s"],
  ["(b) Idle, nothing playing (10 s): dropped frames / p95 frame (noise floor)", "playback.idlePaused10s.droppedFrames|playback.idlePaused10s.p95FrameMs", "ms"],
  ["(b) Album queue: long tasks (Long Tasks API)", "playback.albumQueue.playing.longtaskApi.longTasks", ""],
  ["(b) Album queue: total blocking time (Long Tasks API)", "playback.albumQueue.playing.longtaskApi.totalBlockingMs", "ms"],
  ["(b) Album queue: probe long tasks / blocking", "playback.albumQueue.playing.probeLongTasks|playback.albumQueue.playing.probeBlockingMs", "ms"],
  ["(b) Album queue: dropped frames / max frame", "playback.albumQueue.playing.droppedFrames|playback.albumQueue.playing.maxFrameMs", "ms"],
  ["(b) Album queue: DOM mutations/s", "playback.albumQueue.playing.domMutationsPerSecond", "/s"],
  ["(b) Album queue: CDP script / task time", "playback.albumQueue.cdpMetricsDelta.scriptMs|playback.albumQueue.cdpMetricsDelta.taskMs", "ms"],
  ["(b) Album queue: trace main-thread busy %", "playback.albumQueue.trace.mainThreadBusyPct", "%"],
  ["(b) Album queue: trace tasks/s / longest task", "playback.albumQueue.trace.tasksPerSecond|playback.albumQueue.trace.longestTaskMs", "ms"],
  ["(b) 1,200-song queue: long tasks (Long Tasks API)", "playback.bigQueue.playing.longtaskApi.longTasks", ""],
  ["(b) 1,200-song queue: total blocking time (Long Tasks API)", "playback.bigQueue.playing.longtaskApi.totalBlockingMs", "ms"],
  ["(b) 1,200-song queue: probe long tasks / blocking", "playback.bigQueue.playing.probeLongTasks|playback.bigQueue.playing.probeBlockingMs", "ms"],
  ["(b) 1,200-song queue: dropped frames / max frame", "playback.bigQueue.playing.droppedFrames|playback.bigQueue.playing.maxFrameMs", "ms"],
  ["(b) 1,200-song queue: CDP script / task time", "playback.bigQueue.cdpMetricsDelta.scriptMs|playback.bigQueue.cdpMetricsDelta.taskMs", "ms"],
  ["(b) 1,200-song queue: trace main-thread busy %", "playback.bigQueue.trace.mainThreadBusyPct", "%"],
  ["(b) 1,200-song queue: trace tasks/s / longest task", "playback.bigQueue.trace.tasksPerSecond|playback.bigQueue.trace.longestTaskMs", "ms"],
  ["(c) Songs: rows rendered / View-more clicks", "songs.rowsRendered|songs.loadClicks", ""],
  ["(c) Songs: total time to load rows", "songs.loadTotalMs", "ms"],
  ["(c) Songs: first / median / last click to rows", "songs.firstClickMs|songs.medianClickMs|songs.lastClickMs", "ms"],
  ["(c) Songs load phase: dropped frames / long tasks (probe)", "songs.loadPhase.droppedFrames|songs.loadPhase.probeLongTasks", ""],
  ["(c) Songs scroll: duration / dropped frames", "songs.scroll.durationMs|songs.scroll.droppedFrames", ""],
  ["(c) Songs scroll: p95 / max frame", "songs.scroll.p95FrameMs|songs.scroll.maxFrameMs", "ms"],
  ["(d) After scroll: DOM nodes", "songs.memory.domNodes", ""],
  ["(d) After scroll: JS heap used / after GC", "songs.memory.jsHeapUsedMb|songs.memory.jsHeapUsedAfterGcMb", "MB"],
  ["(d) After scroll: wasm linear memory", "songs.memory.wasmMemoryMb", "MB"],
  ["(e) Albums page: cover requests initial / after scroll", "albumsImages.imageRequestsInitial|albumsImages.imageRequestsAfterScroll", ""],
  ["(e) Albums page: cover KB initial", "albumsImages.imageBytesInitialKb", "KB"],
  ["(e) Albums page: <img> total / lazy / sized", "albumsImages.imgElements|albumsImages.lazy|albumsImages.sized", ""],
];

function cell(doc: any, path: string) {
  if (!doc) return "–";
  return path
    .split("|")
    .map((p) => {
      const v = get(doc, p);
      return v === null || v === undefined ? "n/a" : String(v);
    })
    .join(" / ");
}

function delta(a: any, b: any, path: string) {
  if (!a || !b) return "";
  const parts = path.split("|").map((p) => {
    const x = get(a, p);
    const y = get(b, p);
    if (typeof x !== "number" || typeof y !== "number") return "–";
    const d = x - y;
    const pct = y !== 0 ? ` (${d >= 0 ? "+" : ""}${Math.round((d / y) * 100)}%)` : "";
    return `${d >= 0 ? "+" : ""}${Math.round(d * 10) / 10}${pct}`;
  });
  return parts.join(" / ");
}

const projects = [...cur.keys()].sort();
let md = `# RustySound perf ${cmpDir ? "comparison" : "baseline"}\n\n`;
for (const p of projects) {
  const m = cur.get(p).meta;
  md += `- **${p}**: ${m.browser}, commit ${m.commit}, ${m.date}, wasm ${m.wasmBytes ? (m.wasmBytes / 1048576).toFixed(2) + " MB" : "?"}, mock latency ${m.mockLatencyMs} ms, settings ${JSON.stringify(m.settings)}\n`;
  for (const [title, msg] of Object.entries(m.failures ?? {})) md += `  - FAILED: ${title}: ${msg}\n`;
}
if (cmpDir) md += `- compared against: \`${cmpDir}\`\n`;
md += `\n| Metric | ${projects.map((p) => (cmpDir ? `${p} | Δ vs baseline` : p)).join(" | ")} |\n`;
md += `|---|${projects.map(() => (cmpDir ? "---|---" : "---")).join("|")}|\n`;
for (const [label, path, unit] of ROWS) {
  md += `| ${label}${unit ? ` (${unit})` : ""} | ${projects
    .map((p) => (cmpDir ? `${cell(cur.get(p), path)} | ${delta(cur.get(p), base.get(p), path)}` : cell(cur.get(p), path)))
    .join(" | ")} |\n`;
}
const trace = (p: string, which: string) => get(cur.get(p), `playback.${which}.trace.breakdownMs`);
for (const p of projects) {
  for (const which of ["albumQueue", "bigQueue"]) {
    const b = trace(p, which);
    if (b) md += `\nTrace breakdown, ${p}, ${which} (${cur.get(p).meta.settings.PLAYBACK_SECONDS} s, ms on the renderer main thread): ${Object.entries(b).map(([k, v]) => `${k} ${v}`).join(", ")}\n`;
  }
}
writeFileSync(out, md);
console.log(md);
console.log(`wrote ${out}`);
