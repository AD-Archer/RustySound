// Summarise a Chromium trace (from browser.startTracing / stopTracing) for the renderer main thread.
import { round } from "./instrument";

interface TraceEvent {
  name: string;
  cat: string;
  ph: string;
  ts: number;
  dur?: number;
  pid: number;
  tid: number;
  args?: { name?: string; data?: Record<string, unknown> };
}

const TOP_LEVEL = new Set(["RunTask", "ThreadControllerImpl::RunTask"]);
const BREAKDOWN = ["FunctionCall", "TimerFire", "EventDispatch", "FireAnimationFrame", "EvaluateScript", "v8.callFunction", "UpdateLayoutTree", "Layout", "PrePaint", "Paint", "Layerize", "MajorGC", "MinorGC", "V8.GC_SCAVENGER", "ParseHTML", "XHRReadyStateChange", "ResourceReceivedData"];

export function summariseTrace(buf: Buffer, windowMs: number) {
  const parsed = JSON.parse(buf.toString("utf8"));
  const events: TraceEvent[] = Array.isArray(parsed) ? parsed : parsed.traceEvents;
  const mainThreads = events.filter((e) => e.name === "thread_name" && e.args?.name === "CrRendererMain").map((e) => `${e.pid}:${e.tid}`);
  // Pick the renderer main thread that did the most top-level work (the page under test).
  const busy = new Map<string, number>();
  for (const e of events) {
    if (e.ph === "X" && TOP_LEVEL.has(e.name)) {
      const k = `${e.pid}:${e.tid}`;
      if (mainThreads.includes(k)) busy.set(k, (busy.get(k) ?? 0) + (e.dur ?? 0));
    }
  }
  const main = [...busy.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
  if (!main) return { error: "no renderer main thread found" };
  const onMain = events.filter((e) => `${e.pid}:${e.tid}` === main && e.ph === "X" && typeof e.dur === "number");
  const tasks = onMain.filter((e) => TOP_LEVEL.has(e.name));
  const durMs = tasks.map((t) => (t.dur ?? 0) / 1000);
  const long = durMs.filter((d) => d > 50);
  const breakdown: Record<string, number> = {};
  for (const name of BREAKDOWN) {
    const total = onMain.filter((e) => e.name === name).reduce((n, e) => n + (e.dur ?? 0), 0) / 1000;
    if (total > 0) breakdown[name] = round(total);
  }
  const busyMs = durMs.reduce((n, d) => n + d, 0);
  return {
    tasks: tasks.length,
    tasksPerSecond: round(tasks.length / (windowMs / 1000)),
    mainThreadBusyMs: round(busyMs),
    mainThreadBusyPct: round((busyMs / windowMs) * 100),
    longTasks: long.length,
    totalBlockingMs: round(long.reduce((n, d) => n + d - 50, 0)),
    longestTaskMs: round(Math.max(0, ...durMs)),
    tasksOver16ms: durMs.filter((d) => d > 16).length,
    breakdownMs: breakdown,
  };
}
