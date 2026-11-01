// Shared gesture helpers (refactor plan D14). A gesture script that wants
// them is loaded as concat!(include_str!("gesture_core.js"), "\n",
// include_str!("<script>.js")), so each script keeps its own scope and these
// are plain functions in it. Today the story pager (story.js) uses them;
// edge_swipe.js, row_gestures.js and song_details/gestures.js still carry
// their own copies until D14 moves them here.

// Travel before a gesture's direction is decided.
const GESTURE_LOCK_PX = 10;

// The touch with this identifier in a TouchList, or null.
function touchById(list, id) {
  for (let i = 0; i < list.length; i++) if (list[i].identifier === id) return list[i];
  return null;
}

// Records a [time, position] sample and keeps only the last ~100 ms.
function pushSample(samples, time, value) {
  samples.push([time, value]);
  while (samples.length > 2 && time - samples[0][0] > 100) samples.shift();
}

// Speed over the sampled window, px/ms along the sampled axis. A finger
// that stopped before lifting (no sample in the last 80 ms) is not a flick.
function velocity(samples) {
  if (samples.length < 2) return 0;
  const [t0, v0] = samples[0];
  const [t1, v1] = samples[samples.length - 1];
  if (performance.now() - t1 > 80) return 0;
  return t1 > t0 ? (v1 - v0) / (t1 - t0) : 0;
}

function clamp(value, lo, hi) {
  return Math.min(hi, Math.max(lo, value));
}

// Swallows the click a finished drag causes (inside `scope`, a selector),
// but not a new press's. arm() after a drag ends.
function clickGuard(ms, scope) {
  let until = 0;
  window.addEventListener("pointerdown", () => { until = 0; }, true);
  window.addEventListener(
    "click",
    (e) => {
      if (performance.now() < until && e.target instanceof Element && e.target.closest(scope)) {
        until = 0;
        e.stopPropagation();
        e.preventDefault();
      }
    },
    true,
  );
  return { arm() { until = performance.now() + ms; } };
}

function prefersReducedMotion() {
  return window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };
}
