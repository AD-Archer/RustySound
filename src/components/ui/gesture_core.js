// Shared gesture helpers (refactor plan D14). A gesture script that wants
// them is loaded as concat!(include_str!("gesture_core.js"), "\n",
// include_str!("<script>.js")), so each script keeps its own scope and these
// are plain functions in it: edge_swipe.js, page_transition.js,
// row_gestures.js, cover_flow.js, sheet_swipe.js and story.js (ui/), and
// song_details/gestures.js.
//
// Plain declarations only: nothing here may run on its own. Every name is
// prefixed or specific enough not to clash with a script's own constants.

// Travel before a gesture's direction is decided.
const GESTURE_LOCK_PX = 10;
// edge_swipe.js's start zone at each screen edge; other gestures stay out of it.
const GESTURE_EDGE_PX = 24;
// How long after a drag ends its click is swallowed (clickGuard).
const GESTURE_CLICK_GUARD_MS = 400;

const nowMs = () => performance.now();

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

// Swallows the click a finished drag causes, but not a new press's. `scope`
// says which clicks belong to the gesture: a selector the target sits in, or
// a function of the target. arm() after a drag ends; clear() drops it.
function clickGuard(ms, scope) {
  const owns = typeof scope === "function" ? scope : (el) => !!el.closest(scope);
  let until = 0;
  window.addEventListener("pointerdown", () => { until = 0; }, true);
  window.addEventListener(
    "click",
    (e) => {
      if (performance.now() < until && e.target instanceof Element && owns(e.target)) {
        until = 0;
        e.stopPropagation();
        e.preventDefault();
      }
    },
    true,
  );
  return {
    arm() { until = performance.now() + ms; },
    clear() { until = 0; },
  };
}

function prefersReducedMotion() {
  return window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };
}

// The position of the current entry in browser history, as edge_swipe.js
// stamps it into Dioxus' [scrollX, scrollY] entry state. Null when the entry
// has no stamp (another state shape, or the first load).
function historyIndex() {
  const state = typeof history !== "undefined" ? history.state : null;
  return Array.isArray(state) && typeof state[2] === "number" ? state[2] : null;
}
