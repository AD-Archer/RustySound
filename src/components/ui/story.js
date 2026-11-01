// Story gestures for full-screen card stories (story.rs; Wrapped uses it).
// Loaded through document::eval after gesture_core.js, so it runs in the iOS
// WKWebView as well as the browser. Rust owns the index: this script follows
// the finger or the mouse and posts "next", "prev" or "close" through
// dioxus.send, and plays the change between two cards. Per move it writes one
// transform and one opacity, in an animation frame; no Rust work.
//
// 1. Drag (pointer events: touch and mouse). On the stage, a sideways drag
//    carries the card; past 22% of the width, or on a flick, it posts "next"
//    or "prev" (at either end it only stretches a little). A downward drag
//    sinks and shrinks the card; past 18% of the height, or on a flick, it
//    posts "close". Short of that the card springs back. Drags that start on
//    a control inside a card (a button, a select) are the control's; the
//    edge tap zones are not controls in that sense.
// 2. Card change. Rust renders the incoming card and keeps the outgoing one
//    for a moment with data-story-leaving. A MutationObserver sees both and
//    animates them with the Web Animations API: a short slide and fade, or a
//    plain cross-fade under reduced motion. (The reduced-motion reset in
//    base.css clamps CSS animations, not these, so the fade still shows.)
//
// DOM contract (story.rs):
//   .rs-story[data-story-index][data-story-count][data-story-dir]
//   .rs-story__stage            where drags start
//   .rs-story__track            holds the cards
//   [data-story-card]           a card; [data-story-leaving] when going away
//   [data-story-edge]           the tap zones at both sides
//
// window.__rsStory: setSender(fn), mounted(), state() "idle" | "drag",
// last() { mode: "slide" | "fade", dir } of the last change, for tests.

const SWIPE_COMMIT = 0.22;  // of the width
const CLOSE_COMMIT = 0.18;  // of the height
const FLICK_PX_PER_MS = 0.45;
const FLICK_MIN_PX = 24;
const SLIDE_PX = 36;        // how far a card slides in or out
const SLIDE_MS = 380;
const FADE_MS = 280;
const SPRING_MS = 220;
const CLICK_GUARD_MS = 400;
const EASE = "cubic-bezier(.2,.7,.2,1)";
const CONTROLS = "button, a, select, input, textarea, [role='slider']";

const story = window.__rsStory || (window.__rsStory = installStory());
story.setSender((message) => dioxus.send(message));
story.mounted();
await new Promise(() => {});

function installStory() {
  let send = () => {};
  let drag = null;
  let frame = 0;
  let last = null;
  let observer = null;
  const guard = clickGuard(CLICK_GUARD_MS, ".rs-story");
  const reduceMotion = prefersReducedMotion();

  const root = () => document.querySelector(".rs-story");
  const liveCard = (r) => r.querySelector("[data-story-card]:not([data-story-leaving])");

  function canGo(r, dir) {
    const index = Number(r.dataset.storyIndex || 0);
    const count = Number(r.dataset.storyCount || 0);
    return dir === "next" ? index < count - 1 : index > 0;
  }

  // ---- drag ------------------------------------------------------------------

  function down(e) {
    if (!e.isPrimary || drag) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const r = root();
    const target = e.target;
    if (!r || !(target instanceof Element) || !target.closest(".rs-story__stage")) return;
    if (target.closest(CONTROLS) && !target.closest("[data-story-edge]")) return;
    const card = liveCard(r);
    if (!card) return;
    drag = { id: e.pointerId, root: r, card, x0: e.clientX, y0: e.clientY, dx: 0, dy: 0, axis: null, samples: [] };
  }

  function move(e) {
    if (!drag || e.pointerId !== drag.id) return;
    const dx = e.clientX - drag.x0;
    const dy = e.clientY - drag.y0;
    if (!drag.axis) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < GESTURE_LOCK_PX) return;
      if (Math.abs(dx) > Math.abs(dy) * 1.2) drag.axis = "x";
      else if (dy > 0 && dy > Math.abs(dx) * 1.2) drag.axis = "y";
      else {
        drag = null;
        return;
      }
      drag.root.setAttribute("data-story-drag", drag.axis);
    }
    if (e.cancelable) e.preventDefault();
    const time = performance.now();
    if (drag.axis === "x") {
      const dir = dx < 0 ? "next" : "prev";
      // At either end the card only stretches.
      drag.dx = canGo(drag.root, dir) ? dx : dx * 0.25;
      pushSample(drag.samples, time, e.clientX);
    } else {
      drag.dy = Math.max(0, dy);
      pushSample(drag.samples, time, e.clientY);
    }
    if (!frame) frame = requestAnimationFrame(paint);
  }

  function paint() {
    frame = 0;
    if (!drag) return;
    const { card, root: r } = drag;
    if (drag.axis === "x") {
      const share = Math.min(1, Math.abs(drag.dx) / (r.clientWidth || 1));
      card.style.transform = `translate3d(${drag.dx.toFixed(1)}px,0,0)`;
      card.style.opacity = String(1 - share * 0.5);
    } else {
      const share = Math.min(1, drag.dy / (r.clientHeight || 1));
      card.style.transform = `translate3d(0,${drag.dy.toFixed(1)}px,0) scale(${(1 - share * 0.25).toFixed(4)})`;
      card.style.opacity = String(1 - share * 0.6);
    }
  }

  function up(e) {
    if (!drag || e.pointerId !== drag.id) return;
    const g = drag;
    if (!g.axis) {
      drag = null;
      return; // a tap: the edge zone's own click handles it
    }
    // Land the last move before deciding.
    cancelAnimationFrame(frame);
    paint();
    drag = null;
    guard.arm();
    g.root.removeAttribute("data-story-drag");
    const speed = velocity(g.samples);
    const released = e.type === "pointerup";
    if (g.axis === "x") {
      const dir = g.dx < 0 ? "next" : "prev";
      const toward = g.dx < 0 ? -speed : speed;
      const travel = Math.abs(g.dx);
      const commit =
        released &&
        canGo(g.root, dir) &&
        ((travel > g.root.clientWidth * SWIPE_COMMIT && toward > -0.2) || (toward > FLICK_PX_PER_MS && travel > FLICK_MIN_PX));
      // On commit the card stays where the finger left it; the change
      // animation takes it from there.
      if (commit) send(dir);
      else springBack(g.card);
    } else {
      const commit =
        released &&
        ((g.dy > g.root.clientHeight * CLOSE_COMMIT && speed > -0.2) || (speed > FLICK_PX_PER_MS && g.dy > FLICK_MIN_PX));
      if (commit) send("close");
      else springBack(g.card);
    }
  }

  function springBack(card) {
    const from = { transform: card.style.transform || "none", opacity: card.style.opacity || "1" };
    card.style.transform = "";
    card.style.opacity = "";
    if (!reduceMotion.matches && card.animate) {
      card.animate([from, { transform: "none", opacity: 1 }], { duration: SPRING_MS, easing: EASE });
    }
  }

  // ---- card change -------------------------------------------------------------

  function dirOf(r) {
    return r && r.dataset.storyDir === "prev" ? -1 : 1;
  }

  // Drops what a card was doing (an exit that holds it invisible, say).
  function settle(card) {
    if (card.getAnimations) for (const a of card.getAnimations()) a.cancel();
  }

  function animateIn(card) {
    settle(card);
    const r = root();
    const dir = dirOf(r);
    const mode = reduceMotion.matches ? "fade" : "slide";
    last = { mode, dir: dir > 0 ? "next" : "prev" };
    if (!card.animate) return;
    const frames =
      mode === "fade"
        ? [{ opacity: 0 }, { opacity: 1 }]
        : [{ opacity: 0, transform: `translate3d(${dir * SLIDE_PX}px,0,0)` }, { opacity: 1, transform: "none" }];
    card.animate(frames, { duration: mode === "fade" ? FADE_MS : SLIDE_MS, easing: EASE });
  }

  function animateOut(card) {
    settle(card);
    const dir = dirOf(root());
    const from = { transform: card.style.transform || "none", opacity: card.style.opacity || "1" };
    card.style.transform = "";
    card.style.opacity = "";
    if (!card.animate) {
      card.style.opacity = "0";
      return;
    }
    const fade = reduceMotion.matches;
    const to = fade ? { transform: from.transform, opacity: 0 } : { transform: `translate3d(${-dir * SLIDE_PX}px,0,0)`, opacity: 0 };
    card.animate([from, to], { duration: fade ? FADE_MS : SLIDE_MS, easing: EASE, fill: "forwards" });
  }

  function onMutations(records) {
    for (const record of records) {
      if (record.type === "attributes") {
        const el = record.target;
        if (!(el instanceof Element) || !el.hasAttribute("data-story-card")) continue;
        // Going away, or coming back before it had gone (a quick step back).
        if (el.hasAttribute("data-story-leaving")) animateOut(el);
        else animateIn(el);
        continue;
      }
      for (const node of record.addedNodes) {
        if (!(node instanceof Element) || !node.hasAttribute("data-story-card")) continue;
        if (node.hasAttribute("data-story-leaving")) animateOut(node);
        else animateIn(node);
      }
    }
  }

  // The story is in the DOM: watch its track for card changes.
  function mounted() {
    const track = document.querySelector(".rs-story__track");
    if (observer) observer.disconnect();
    observer = null;
    if (!track) return;
    observer = new MutationObserver(onMutations);
    observer.observe(track, { childList: true, attributes: true, subtree: true, attributeFilter: ["data-story-leaving"] });
  }

  window.addEventListener("pointerdown", down, true);
  window.addEventListener("pointermove", move, { capture: true, passive: false });
  window.addEventListener("pointerup", up, true);
  window.addEventListener("pointercancel", up, true);

  return {
    setSender(fn) {
      send = typeof fn === "function" ? fn : () => {};
    },
    mounted,
    state() {
      return drag && drag.axis ? "drag" : "idle";
    },
    last() {
      return last;
    },
  };
}
