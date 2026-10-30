// Edge swipe navigation for the compact layout (iPhone app, mobile web).
// Loaded by EdgeSwipe (edge_swipe.rs) through document::eval, so it runs in
// the iOS WKWebView as well as the browser. Rust owns navigation; this script
// only tracks the finger and posts "back" or "forward" through dioxus.send.
//
// Touch events, not pointer events: a non-passive touchmove can claim the
// gesture (preventDefault) once it is clearly horizontal, while a vertical
// drag from the edge still scrolls. Per move it stores the finger and asks
// for one animation frame; the frame writes transforms and opacity only.
//
// window.__rsEdgeSwipe:
//   sync(back, forward)  Rust's history flags (native MemoryHistory). The
//                        browser build passes nulls and the script asks the
//                        Navigation API / history index instead.
//   routed()             the route changed and the new page is in the DOM.
//   state()              phase name, for tests.

const EDGE_PX = 24;          // start zone at each screen edge
const LOCK_PX = 8;           // travel before the direction is decided
const COMMIT_RATIO = 0.35;   // of the width
const FLICK_PX_PER_MS = 0.5; // release speed that commits a shorter drag
const FLICK_MIN_PX = 24;
const ROUTE_WAIT_MS = 700;   // give up waiting for the new page after this

// Anything here (or inside it) keeps the gesture for itself.
const OWN_GESTURE =
  "input, textarea, select, [contenteditable='true'], [role='slider'], " +
  ".rs-coverflow, [data-testid='coverflow'], .rs-track, .rs-library-index, [data-no-edge-swipe]";
// Any of these open means a menu, sheet or Now Playing is up.
const OPEN_LAYER = ".rs-scrim, [aria-modal='true'], [role='dialog'], [role='menu'], [data-testid='now-playing']";

const api = window.__rsEdgeSwipe || (window.__rsEdgeSwipe = install());
api.setSender((message) => dioxus.send(message));
await new Promise(() => {});

function install() {
  let send = () => {};
  let native = { back: null, forward: null };
  // idle | pending | drag | settle
  let phase = "idle";
  let g = null; // the current gesture
  let frame = 0;
  let routeWait = null;
  const reduceMotion = window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };

  stampHistory();

  function canGo(dir) {
    const flag = dir > 0 ? native.back : native.forward;
    if (typeof flag === "boolean") return flag;
    const nav = window.navigation;
    if (nav && typeof nav.canGoBack === "boolean") return dir > 0 ? nav.canGoBack : nav.canGoForward;
    const index = historyIndex();
    if (dir > 0) return index > 0;
    try {
      return index < Number(sessionStorage.getItem("rs.history.max") || 0);
    } catch (_) {
      return false;
    }
  }

  function blocked(target) {
    const main = document.querySelector(".rs-app[data-layout='compact'] .rs-main");
    if (!main || !(target instanceof Element) || !main.contains(target)) return true;
    if (document.querySelector(OPEN_LAYER)) return true;
    if (target.closest(OWN_GESTURE)) return true;
    // Horizontal scrollers (shelves, card rows) swipe sideways themselves.
    for (let el = target; el && el !== main; el = el.parentElement) {
      if (el.scrollWidth > el.clientWidth + 1) {
        const overflow = getComputedStyle(el).overflowX;
        if (overflow === "auto" || overflow === "scroll") return true;
      }
    }
    return false;
  }

  function onStart(e) {
    if (phase !== "idle") return;
    if (e.touches.length !== 1) return;
    const t = e.touches[0];
    const width = window.innerWidth;
    const dir = t.clientX <= EDGE_PX ? 1 : t.clientX >= width - EDGE_PX ? -1 : 0;
    if (!dir || !canGo(dir) || blocked(e.target)) return;
    phase = "pending";
    g = { dir, id: t.identifier, x0: t.clientX, y0: t.clientY, dx: 0, width, samples: [], main: null, under: null };
  }

  function onMove(e) {
    if (phase !== "pending" && phase !== "drag") return;
    const t = touchById(e.touches, g.id);
    if (!t) return;
    const dx = t.clientX - g.x0;
    const dy = t.clientY - g.y0;
    if (phase === "pending") {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_PX) return;
      // Clearly sideways and away from the edge, or it is a scroll.
      if (dx * g.dir <= 0 || Math.abs(dx) < Math.abs(dy) * 1.2) {
        reset();
        return;
      }
      begin();
    }
    if (e.cancelable) e.preventDefault();
    g.dx = g.dir > 0 ? Math.min(Math.max(dx, 0), g.width) : Math.max(Math.min(dx, 0), -g.width);
    const now = performance.now();
    g.samples.push([now, t.clientX]);
    while (g.samples.length > 2 && now - g.samples[0][0] > 100) g.samples.shift();
    if (!frame && !reduceMotion.matches) frame = requestAnimationFrame(paint);
  }

  function onEnd(e) {
    if (phase === "pending") {
      if (!touchById(e.touches, g.id)) reset();
      return;
    }
    if (phase !== "drag" || touchById(e.touches, g.id)) return;
    const travel = Math.abs(g.dx);
    const speed = velocity() * g.dir; // px/ms toward the goal
    const commit =
      e.type === "touchend" &&
      !document.querySelector(OPEN_LAYER) &&
      ((travel > g.width * COMMIT_RATIO && speed > -0.2) || (speed > FLICK_PX_PER_MS && travel > FLICK_MIN_PX));
    if (commit) finish(speed);
    else springBack();
  }

  function begin() {
    phase = "drag";
    if (reduceMotion.matches) return;
    const main = document.querySelector(".rs-main");
    g.main = main;
    const under = document.createElement("div");
    under.className = "rs-edge-under";
    under.setAttribute("aria-hidden", "true");
    main.parentNode.insertBefore(under, main);
    g.under = under;
    main.classList.add("rs-edge-drag", g.dir > 0 ? "rs-edge-drag--back" : "rs-edge-drag--forward");
    main.style.transform = "translate3d(0,0,0)";
  }

  function paint() {
    frame = 0;
    if (!g || !g.main) return;
    g.main.style.transform = `translate3d(${g.dx}px,0,0)`;
    g.under.style.opacity = String(1 - Math.abs(g.dx) / g.width);
  }

  function velocity() {
    const s = g.samples;
    if (s.length < 2) return 0;
    const [t0, x0] = s[0];
    const [t1, x1] = s[s.length - 1];
    // A finger that stopped before lifting is not a flick.
    if (performance.now() - t1 > 80) return 0;
    return t1 > t0 ? (x1 - x0) / (t1 - t0) : 0;
  }

  function animate(to, ms, underOpacity, done) {
    const { main, under } = g;
    const ease = "cubic-bezier(.2,.75,.25,1)";
    main.style.transition = `transform ${ms}ms ${ease}`;
    under.style.transition = `opacity ${ms}ms ${ease}`;
    let finished = false;
    const end = () => {
      if (finished) return;
      finished = true;
      main.removeEventListener("transitionend", onTransitionEnd);
      done();
    };
    const onTransitionEnd = (ev) => {
      if (ev.target === main && ev.propertyName === "transform") end();
    };
    main.addEventListener("transitionend", onTransitionEnd);
    setTimeout(end, ms + 80);
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    // Commit the current position first so the transition starts from it.
    main.style.transform = `translate3d(${g.dx}px,0,0)`;
    main.getBoundingClientRect();
    main.style.transform = `translate3d(${to}px,0,0)`;
    under.style.opacity = String(underOpacity);
  }

  function springBack() {
    if (!g.main) return reset();
    phase = "settle";
    const ms = Math.round(Math.min(260, Math.max(140, Math.abs(g.dx))));
    animate(0, ms, 1, reset);
  }

  function finish(speed) {
    const message = g.dir > 0 ? "back" : "forward";
    if (!g.main) {
      // Reduced motion: no slide, just go.
      reset();
      send(message);
      return;
    }
    phase = "settle";
    const remaining = g.width - Math.abs(g.dx);
    const ms = Math.round(Math.min(280, Math.max(120, remaining / Math.max(speed, 1.4))));
    animate(g.dir * g.width, ms, 0, () => {
      const gesture = g;
      routeWait = () => arrive(gesture);
      setTimeout(() => routeWait && routeWait(), ROUTE_WAIT_MS);
      send(message);
    });
  }

  // The new page is in place: bring it in from slightly behind, like the
  // page underneath catching up, then hand the element back untouched.
  function arrive(gesture) {
    routeWait = null;
    if (g !== gesture || !g.main) return;
    const { main, under } = g;
    main.style.transition = "none";
    main.style.opacity = "0";
    main.style.transform = `translate3d(${-g.dir * 28}px,0,0)`;
    main.getBoundingClientRect();
    main.style.transition = "transform 200ms cubic-bezier(.2,.75,.25,1), opacity 160ms linear";
    main.style.transform = "translate3d(0,0,0)";
    main.style.opacity = "1";
    under.style.opacity = "0";
    setTimeout(() => {
      if (g === gesture) reset();
    }, 240);
  }

  function reset() {
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    if (g && g.main) {
      g.main.classList.remove("rs-edge-drag", "rs-edge-drag--back", "rs-edge-drag--forward");
      g.main.style.transform = "";
      g.main.style.transition = "";
      g.main.style.opacity = "";
    }
    if (g && g.under) g.under.remove();
    g = null;
    routeWait = null;
    phase = "idle";
  }

  // Something else navigated (Safari's own edge swipe, a link, the back
  // button) while a drag was up: drop it without animating.
  function onExternalNav() {
    if (phase === "pending" || phase === "drag") reset();
  }

  function touchById(list, id) {
    for (let i = 0; i < list.length; i++) if (list[i].identifier === id) return list[i];
    return null;
  }

  // ---- Browser history index ---------------------------------------------
  // Dioxus' WebHistory stores [scrollX, scrollY] as the entry state. Add the
  // entry's position as a third item so "is there an app page behind this
  // one" has an answer where the Navigation API is missing (and for Rust's
  // can_go_back on the web). Other state shapes pass through untouched.
  function historyIndex() {
    const state = history.state;
    return Array.isArray(state) && typeof state[2] === "number" ? state[2] : 0;
  }

  function stampHistory() {
    if (window.__rsHistoryStamped || typeof history === "undefined") return;
    window.__rsHistoryStamped = true;
    const push = history.pushState.bind(history);
    const replace = history.replaceState.bind(history);
    const stamp = (state, index) =>
      Array.isArray(state) && state.length >= 2 ? [state[0], state[1], index] : state;
    history.pushState = (state, title, url) => {
      const next = historyIndex() + 1;
      try {
        sessionStorage.setItem("rs.history.max", String(next));
      } catch (_) {}
      return push(stamp(state, next), title, url);
    };
    history.replaceState = (state, title, url) => replace(stamp(state, historyIndex()), title, url);
    if (Array.isArray(history.state) && history.state.length === 2) {
      replace(stamp(history.state, 0), "");
    }
  }

  const passive = { capture: true, passive: true };
  document.addEventListener("touchstart", onStart, passive);
  document.addEventListener("touchmove", onMove, { capture: true, passive: false });
  document.addEventListener("touchend", onEnd, passive);
  document.addEventListener("touchcancel", onEnd, passive);
  window.addEventListener("popstate", onExternalNav);
  window.addEventListener("blur", () => phase === "drag" && springBack());
  // Leaving the app mid-swipe may never deliver touchend/touchcancel; don't
  // come back to a half-moved page or a gesture that is still armed.
  document.addEventListener("visibilitychange", () => document.hidden && onExternalNav());

  return {
    setSender(fn) {
      send = fn;
    },
    sync(back, forward) {
      native = { back, forward };
    },
    routed() {
      if (routeWait) routeWait();
    },
    state() {
      return phase;
    },
    canGo,
  };
}
