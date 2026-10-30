// Compact Now Playing gestures (gestures.rs loads this through document::eval,
// so it runs in the iOS WKWebView as well as the browser). Rust owns state:
// this script only follows the finger and posts what was decided through
// dioxus.send. Per move it writes CSS variables and data attributes; the
// stylesheet turns them into transforms and opacity (player.css).
//
// 1. Panel focus (touch). The sheet has two states, full and focused; CSS
//    drives the moving parts from one progress variable, --np-p (0 = full,
//    1 = focused), set by the sheet's class and, while a finger drags, inline.
//    The panel itself rests without a transform (its menus are position:
//    fixed inside it), so each switch it gets a short animation from where
//    it was (data-np-anim, started by the class watcher in mounted()).
//    - Full: an upward drag that starts on the panel grows it into the
//      focused view. Posts "focus".
//    - Focused: a downward drag on the header strip or the segmented control,
//      or on the panel content while it is scrolled to its top, shrinks it
//      back. Posts "unfocus". Otherwise the content scrolls natively.
// 2. Cover swipe (pointer events, so touch and mouse). A horizontal drag that
//    starts on the big cover turns the cover away and brings the neighbouring
//    side cover in, Cover Flow style. Past 40% of the cover width (or a real
//    flick) it posts "next:<token>" / "prev:<token>", where the token names
//    the song the gesture started on, so Rust can refuse a stale one. Further
//    swipes are ignored until the new song has rendered and the covers have
//    settled: one gesture, one song.
//
// window.__rsNpGestures:
//   mounted()  the compact sheet is in the DOM: measure it, watch its size.
//   state()    "idle" | "panel" | "cover" | "busy", for tests.

const LOCK_PX = 10;            // travel before the direction is decided
const PANEL_COMMIT = 0.3;      // of the panel's travel
const COVER_COMMIT = 0.4;      // of the cover width
const FLICK_PX_PER_MS = 0.5;   // release speed that commits a shorter drag
const PANEL_FLICK_MIN_PX = 28;
const COVER_FLICK_MIN = 0.18;  // of the cover width: a flick, not a twitch
const SIDE_STEP = 0.62;        // side cover offset, in cover widths (player.css)
const PANEL_MS = 300;          // --np-dur in player.css
const COVER_MS = 240;          // swipe settle in player.css
const SONG_WAIT_MS = 1500;     // give up waiting for the new song after this
const SETTLE_MS = 120;         // quiet time after the swap before the next swipe
const CLICK_GUARD_MS = 400;    // swallow the click a finished drag may cause

const SHEET = ".rs-player-np--compact";
const FOCUSED = "rs-player-np--focused";
const SCROLLERS = ".rs-player-lyrics__lines, .rs-player-upnext, .rs-player-info";

const api = window.__rsNpGestures || (window.__rsNpGestures = install());
api.setSender((message) => dioxus.send(message));
await new Promise(() => {});

function install() {
  let send = () => {};
  let panel = null; // panel drag
  let cover = null; // cover drag
  let busy = false; // a cover swipe is landing
  let clickGuardUntil = 0; // swallow the click a finished drag causes, not a new tap's
  let resize = null;
  let classWatch = null;
  let handoff = null; // a panel drag waiting for Rust to flip the class
  const reduceMotion = window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };

  const sheet = () => document.querySelector(SHEET);
  const isFocused = (s) => s.classList.contains(FOCUSED);
  const now = () => performance.now();

  // ---- measuring ---------------------------------------------------------

  // Layout position of `el` inside `root`, ignoring transforms.
  function offsetIn(el, root) {
    let x = 0;
    let y = 0;
    for (let node = el; node && node !== root; node = node.offsetParent) {
      x += node.offsetLeft;
      y += node.offsetTop;
      if (!node.offsetParent) break;
    }
    return { x, y, w: el.offsetWidth, h: el.offsetHeight };
  }

  // --np-panel-top: the panel's top in the full player (under the transport).
  // --np-shift: how far that is below its top in the focused view (under the strip).
  // --np-mini-*: the move that lands the big cover on the strip's cover.
  function measure() {
    const s = sheet();
    if (!s) return;
    const full = s.querySelector(".rs-player-np__full");
    const strip = s.querySelector(".rs-player-np__strip");
    if (full && strip) {
      const top = full.offsetTop + full.offsetHeight;
      const shift = top - (strip.offsetTop + strip.offsetHeight);
      s.style.setProperty("--np-panel-top", `${Math.round(top)}px`);
      s.style.setProperty("--np-shift", `${Math.max(0, Math.round(shift))}px`);
    }
    const big = s.querySelector(".rs-player-flow__cover");
    const mini = s.querySelector(".rs-player-np__strip-art");
    if (big && mini && big.offsetWidth > 0) {
      const a = offsetIn(big, s);
      const b = offsetIn(mini, s);
      s.style.setProperty("--np-mini-x", `${(b.x + b.w / 2 - (a.x + a.w / 2)).toFixed(1)}px`);
      s.style.setProperty("--np-mini-y", `${(b.y + b.h / 2 - (a.y + a.h / 2)).toFixed(1)}px`);
      s.style.setProperty("--np-mini-s", (b.w / a.w).toFixed(4));
    }
  }

  function mounted() {
    const s = sheet();
    if (!s) return;
    measure();
    if (resize) resize.disconnect();
    if (classWatch) classWatch.disconnect();
    if (typeof ResizeObserver === "function") {
      let queued = false;
      resize = new ResizeObserver(() => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => {
          queued = false;
          measure();
        });
      });
      resize.observe(s);
      const full = s.querySelector(".rs-player-np__full");
      if (full) resize.observe(full);
    }
    // Every switch, by tap or by drag, animates the panel from where it was.
    // This runs before the browser renders the new class, so nothing jumps.
    let wasFocused = isFocused(s);
    classWatch = new MutationObserver(() => {
      const focused = isFocused(s);
      if (focused === wasFocused) return;
      wasFocused = focused;
      const from = handoff && handoff.focused === focused ? handoff.p : null;
      if (handoff) releaseHandoff();
      animatePanel(s, focused, from);
    });
    classWatch.observe(s, { attributes: true, attributeFilter: ["class"] });
  }

  let animTimer = 0;
  function animatePanel(s, focused, from) {
    clearTimeout(animTimer);
    if (from === null) s.style.removeProperty("--np-from");
    else s.style.setProperty("--np-from", from.toFixed(4));
    s.setAttribute("data-np-anim", focused ? "up" : "down");
    animTimer = setTimeout(
      () => {
        s.removeAttribute("data-np-anim");
        s.style.removeProperty("--np-from");
      },
      reduceMotion.matches ? 0 : PANEL_MS + 40,
    );
  }

  // ---- panel focus ---------------------------------------------------------

  function scrollerFrom(target, stop) {
    const own = target.closest(SCROLLERS);
    if (own && stop.contains(own)) return own;
    for (let el = target; el && el !== stop; el = el.parentElement) {
      if (el.scrollHeight > el.clientHeight + 1) {
        const overflow = getComputedStyle(el).overflowY;
        if (overflow === "auto" || overflow === "scroll") return el;
      }
    }
    return null;
  }

  function panelStart(e) {
    if (handoff) return;
    if (e.touches.length !== 1) {
      if (panel) panelRelease(false);
      return;
    }
    const s = sheet();
    const target = e.target;
    if (!s || !(target instanceof Element) || !s.contains(target)) return;
    // Menus and sheets opened from a panel keep their own touches.
    if (target.closest(".rs-player-modals, .rs-sheet, [role='menu'], input, textarea, [role='slider']")) return;
    const t = e.touches[0];
    const focused = isFocused(s);
    const inPanels = target.closest(".rs-player-panels");
    const inStrip = target.closest(".rs-player-np__strip");
    let mode = null;
    if (!focused && inPanels) mode = "expand";
    if (focused && (inPanels || inStrip)) {
      const scroller = inPanels ? scrollerFrom(target, inPanels) : null;
      if (!scroller || scroller.scrollTop <= 1) mode = "collapse";
    }
    if (!mode) return;
    const shift = parseFloat(getComputedStyle(s).getPropertyValue("--np-shift")) || s.clientHeight * 0.45;
    panel = { mode, sheet: s, id: t.identifier, x0: t.clientX, y0: t.clientY, dy: 0, p: focused ? 1 : 0, shift, phase: "pending", samples: [] };
  }

  function panelMove(e) {
    if (!panel) return;
    const t = touchById(e.touches, panel.id);
    if (!t) return;
    const dx = t.clientX - panel.x0;
    const dy = t.clientY - panel.y0;
    if (panel.phase === "pending") {
      // Full never scrolls; in focused, hold a downward drag at the top so
      // the content doesn't bounce before the direction is decided.
      if (e.cancelable && (panel.mode === "expand" || dy > 0)) e.preventDefault();
      if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_PX) return;
      const vertical = Math.abs(dy) > Math.abs(dx) * 1.2;
      const toward = panel.mode === "expand" ? dy < 0 : dy > 0;
      if (!vertical || !toward) {
        panel = null;
        return;
      }
      panel.phase = "drag";
      clearTimeout(animTimer);
      panel.sheet.removeAttribute("data-np-anim");
      panel.sheet.setAttribute("data-np-drag", "");
    }
    if (e.cancelable) e.preventDefault();
    panel.dy = dy;
    const travel = Math.max(1, panel.shift);
    panel.p = panel.mode === "expand" ? clamp(-dy / travel, 0, 1) : clamp(1 - dy / travel, 0, 1);
    panel.sheet.style.setProperty("--np-p", panel.p.toFixed(4));
    const time = now();
    panel.samples.push([time, t.clientY]);
    while (panel.samples.length > 2 && time - panel.samples[0][0] > 100) panel.samples.shift();
  }

  function panelEnd(e) {
    if (!panel) return;
    if (touchById(e.touches, panel.id)) return;
    if (panel.phase !== "drag") {
      panel = null;
      return;
    }
    const speed = velocity(panel.samples); // px/ms, + is down
    const toward = panel.mode === "expand" ? -speed : speed;
    const moved = panel.mode === "expand" ? panel.p : 1 - panel.p; // share of the travel covered
    const commit =
      e.type === "touchend" &&
      ((moved > PANEL_COMMIT && toward > -0.2) || (toward > FLICK_PX_PER_MS && Math.abs(panel.dy) > PANEL_FLICK_MIN_PX));
    panelRelease(commit);
  }

  // Lets go of the drag: short of the commit point everything springs back
  // (CSS transitions). On commit the drag's position stays put until Rust
  // flips the class; the class watcher then animates on from there.
  function panelRelease(commit) {
    const g = panel;
    panel = null;
    if (!g) return;
    clickGuardUntil = now() + CLICK_GUARD_MS;
    if (!commit) {
      clearDrag(g.sheet);
      return;
    }
    const focused = g.mode === "expand";
    handoff = { sheet: g.sheet, focused, p: g.p, timer: setTimeout(releaseHandoff, 600) };
    send(focused ? "focus" : "unfocus");
  }

  function releaseHandoff() {
    if (!handoff) return;
    clearTimeout(handoff.timer);
    clearDrag(handoff.sheet);
    handoff = null;
  }

  function clearDrag(s) {
    s.style.removeProperty("--np-p");
    s.removeAttribute("data-np-drag");
  }

  // ---- cover swipe ---------------------------------------------------------

  function coverDown(e) {
    if (!e.isPrimary || cover || busy) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const s = sheet();
    const target = e.target;
    if (!s || !(target instanceof Element) || isFocused(s)) return;
    const big = target.closest(".rs-player-flow__cover");
    if (!big || !s.contains(big)) return;
    const flow = big.closest(".rs-player-flow");
    if (!flow || flow.hasAttribute("data-swipe-off")) return;
    cover = {
      id: e.pointerId,
      flow,
      x0: e.clientX,
      y0: e.clientY,
      dx: 0,
      width: big.offsetWidth || 1,
      token: flow.dataset.songToken || "",
      next: flow.dataset.hasNext === "true",
      prev: flow.dataset.hasPrev === "true",
      dir: 0, // -1 toward next (leftward), 1 toward previous
      phase: "pending",
      samples: [],
    };
  }

  function coverMove(e) {
    if (!cover || e.pointerId !== cover.id) return;
    const dx = e.clientX - cover.x0;
    const dy = e.clientY - cover.y0;
    if (cover.phase === "pending") {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_PX) return;
      // Clearly sideways, or it isn't ours (a vertical drag pulls the sheet down).
      const dir = dx < 0 ? -1 : 1;
      if (Math.abs(dx) < Math.abs(dy) * 1.5 || (dir < 0 && !cover.next) || (dir > 0 && !cover.prev)) {
        cover = null;
        return;
      }
      cover.dir = dir;
      cover.phase = "drag";
      cover.flow.setAttribute("data-swipe", dir < 0 ? "next" : "prev");
      cover.flow.setAttribute("data-swipe-phase", "drag");
    }
    // The sheet's swipe-down never sees a claimed sideways drag.
    e.stopPropagation();
    if (e.cancelable) e.preventDefault();
    // One direction per gesture: pulling back past the start just rests at 0.
    cover.dx = cover.dir < 0 ? Math.min(dx, 0) : Math.max(dx, 0);
    const time = now();
    cover.samples.push([time, e.clientX]);
    while (cover.samples.length > 2 && time - cover.samples[0][0] > 100) cover.samples.shift();
    paintCover(cover.flow, cover.dx / (cover.width * SIDE_STEP));
  }

  function paintCover(flow, sw) {
    const amount = clamp(sw, -1, 1);
    flow.style.setProperty("--sw", amount.toFixed(4));
    flow.style.setProperty("--swa", Math.abs(amount).toFixed(4));
    if (Math.abs(amount) > 0.5) flow.setAttribute("data-swipe-over", "");
    else flow.removeAttribute("data-swipe-over");
  }

  function coverUp(e) {
    if (!cover || e.pointerId !== cover.id) return;
    const g = cover;
    cover = null;
    if (g.phase !== "drag") return;
    e.stopPropagation();
    clickGuardUntil = now() + CLICK_GUARD_MS;
    const travel = Math.abs(g.dx);
    const speed = velocity(g.samples) * g.dir; // px/ms toward the goal (dir is the finger's way)
    const commit =
      e.type === "pointerup" &&
      ((travel > g.width * COVER_COMMIT && speed > -0.2) ||
        (speed > FLICK_PX_PER_MS && travel > g.width * COVER_FLICK_MIN));
    busy = true;
    g.flow.setAttribute("data-swipe-phase", "settle");
    const settleMs = reduceMotion.matches ? 0 : COVER_MS;
    if (!commit) {
      paintCover(g.flow, 0);
      setTimeout(() => {
        clearCover(g.flow);
        busy = false;
      }, settleMs);
      return;
    }
    // Finish the turn, then ask for the song; the new render matches this end
    // state, so the swap is invisible.
    paintCover(g.flow, g.dir < 0 ? -1 : 1);
    setTimeout(() => land(g), settleMs);
  }

  function land(g) {
    const flow = g.flow;
    let done = false;
    const finish = (changed) => {
      if (done) return;
      done = true;
      watch.disconnect();
      clearTimeout(timer);
      if (changed) {
        clearCover(flow);
      } else {
        // Nothing changed (the queue moved on, the song failed): turn back.
        flow.setAttribute("data-swipe-phase", "settle");
        paintCover(flow, 0);
      }
      // Two frames for the new covers to paint, then a short quiet time.
      requestAnimationFrame(() =>
        requestAnimationFrame(() =>
          setTimeout(() => {
            if (!changed) clearCover(flow);
            busy = false;
          }, changed ? SETTLE_MS : COVER_MS),
        ),
      );
    };
    const watch = new MutationObserver(() => {
      if (!flow.isConnected || flow.dataset.songToken !== g.token) finish(flow.isConnected);
    });
    watch.observe(flow, { attributes: true, attributeFilter: ["data-song-token"] });
    const timer = setTimeout(() => finish(false), SONG_WAIT_MS);
    send(`${g.dir < 0 ? "next" : "prev"}:${g.token}`);
  }

  // Drops the swipe state without a transition: the covers' resting layout
  // already shows the end of the turn.
  function clearCover(flow) {
    flow.setAttribute("data-swipe-phase", "instant");
    flow.style.removeProperty("--sw");
    flow.style.removeProperty("--swa");
    flow.removeAttribute("data-swipe");
    flow.removeAttribute("data-swipe-over");
    void flow.offsetWidth;
    flow.removeAttribute("data-swipe-phase");
  }

  function coverCancel(e) {
    if (!cover || e.pointerId !== cover.id) return;
    const g = cover;
    cover = null;
    if (g.phase !== "drag") return;
    busy = true;
    g.flow.setAttribute("data-swipe-phase", "settle");
    paintCover(g.flow, 0);
    setTimeout(() => {
      clearCover(g.flow);
      busy = false;
    }, reduceMotion.matches ? 0 : COVER_MS);
  }

  // ---- shared ----------------------------------------------------------------

  function touchById(list, id) {
    for (let i = 0; i < list.length; i++) if (list[i].identifier === id) return list[i];
    return null;
  }

  // Speed over the last ~100 ms, px/ms (along whatever axis was sampled).
  function velocity(samples) {
    if (samples.length < 2) return 0;
    const [t0, v0] = samples[0];
    const [t1, v1] = samples[samples.length - 1];
    return t1 > t0 ? (v1 - v0) / (t1 - t0) : 0;
  }

  function clamp(value, lo, hi) {
    return Math.min(hi, Math.max(lo, value));
  }

  function guardClick(e) {
    if (now() < clickGuardUntil && e.target instanceof Element && e.target.closest(SHEET)) {
      clickGuardUntil = 0;
      e.stopPropagation();
      e.preventDefault();
    }
  }

  // A new press is a new interaction: its click is wanted.
  function newPress() {
    clickGuardUntil = 0;
  }

  const passive = { passive: true, capture: true };
  window.addEventListener("touchstart", panelStart, passive);
  window.addEventListener("touchmove", panelMove, { passive: false, capture: true });
  window.addEventListener("touchend", panelEnd, passive);
  window.addEventListener("touchcancel", panelEnd, passive);
  window.addEventListener("pointerdown", newPress, true);
  window.addEventListener("pointerdown", coverDown, true);
  window.addEventListener("pointermove", coverMove, true);
  window.addEventListener("pointerup", coverUp, true);
  window.addEventListener("pointercancel", coverCancel, true);
  window.addEventListener("click", guardClick, true);

  return {
    setSender(fn) {
      send = typeof fn === "function" ? fn : () => {};
    },
    mounted,
    measure,
    state() {
      if (panel || handoff) return "panel";
      if (cover) return "cover";
      return busy ? "busy" : "idle";
    },
  };
}
