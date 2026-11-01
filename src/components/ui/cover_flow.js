// Cover Flow input: the wheel and the pointer swipe. Loaded by CoverFlow
// (cover_flow.rs) through document::eval, once per carousel, with
// gesture_core.js prepended. Rust owns the selection; this script only posts
// a step (-1 or 1) through dioxus.send, so a mouse moving over the covers
// never crosses the webview bridge.
//
// Wheel: a vertical-dominant wheel is left alone, so the page scrolls over
// the covers. A horizontal-dominant one is the flow's: it steps one cover per
// 40px of travel, but only once per gesture: after a step, further events in
// the same direction (trackpad inertia, a fast flick) are ignored until the
// wheel has rested WHEEL_IDLE_MS or turns the other way. Travel short of a
// step decays (half-life WHEEL_HALF_LIFE_MS), so slow drift never adds up.
//
// Swipe (pointer events, so touch and mouse): one step per `data-flow-step`
// px of travel (a third of a cover). The click that ends a swipe doesn't
// open or centre a cover. The flow has touch-action: pan-y, so a vertical
// drag still scrolls the page.
//
// DOM contract: the carousel element carries data-flow="<token>" and
// data-flow-step="<px>". window.__rsFlow: register(token, send),
// unregister(token), attach(token) (from the element's onmounted).

const flows = window.__rsFlow || (window.__rsFlow = installFlows());
flows.register(__FLOW__, (step) => dioxus.send(step));
await new Promise(() => {});

function installFlows() {
  const WHEEL_STEP_PX = 40;
  const WHEEL_IDLE_MS = 160;
  const WHEEL_HALF_LIFE_MS = 120;
  const senders = new Map(); // token -> send
  let swipe = null; // { el, send, id, x, step, moved }
  const guard = clickGuard(GESTURE_CLICK_GUARD_MS, "[data-flow]");

  const senderOf = (el) => senders.get(Number(el.dataset.flow));

  function onWheel(e) {
    const el = e.currentTarget;
    const send = senderOf(el);
    if (!send) return;
    const scale = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? el.clientWidth || 800 : 1;
    const dx = e.deltaX * scale;
    const dy = e.deltaY * scale;
    if (Math.abs(dx) <= Math.abs(dy)) return;
    e.preventDefault();
    const w = el.__rsFlowWheel || (el.__rsFlowWheel = { travel: 0, last: 0, locked: 0 });
    const t = nowMs();
    const gap = t - w.last;
    w.last = t;
    if (gap > WHEEL_IDLE_MS) {
      w.travel = 0;
      w.locked = 0;
    } else {
      w.travel *= Math.pow(0.5, gap / WHEEL_HALF_LIFE_MS);
    }
    const dir = Math.sign(dx);
    if (w.locked) {
      if (dir === w.locked) return;
      // Turning back is a new gesture.
      w.locked = 0;
      w.travel = 0;
    }
    w.travel += dx;
    if (Math.abs(w.travel) >= WHEEL_STEP_PX) {
      w.travel = 0;
      w.locked = dir;
      send(dir);
    }
  }

  function attach(token) {
    const el = document.querySelector(`[data-flow="${token}"]`);
    if (!el || el.__rsFlowAttached) return;
    el.__rsFlowAttached = true;
    // Not passive: a horizontal wheel must not also scroll or swipe the page back.
    el.addEventListener("wheel", onWheel, { passive: false });
  }

  function onPointerDown(e) {
    if (swipe || !e.isPrimary || (e.pointerType === "mouse" && e.button !== 0)) return;
    const el = e.target instanceof Element ? e.target.closest("[data-flow]") : null;
    const send = el && senderOf(el);
    if (!send || e.target.closest("input")) return;
    swipe = { el, send, id: e.pointerId, x: e.clientX, step: Number(el.dataset.flowStep) || 50, moved: false };
  }

  function onPointerMove(e) {
    if (!swipe || e.pointerId !== swipe.id) return;
    // Leaving the carousel ends the swipe.
    if (!(e.target instanceof Node) || !swipe.el.contains(e.target)) return end();
    const dx = e.clientX - swipe.x;
    if (Math.abs(dx) > swipe.step) {
      swipe.send(-Math.sign(dx));
      swipe.x = e.clientX;
      swipe.moved = true;
    }
  }

  function end(e) {
    if (!swipe || (e && e.pointerId !== swipe.id)) return;
    if (swipe.moved) guard.arm();
    swipe = null;
  }

  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("pointermove", onPointerMove, { capture: true, passive: true });
  window.addEventListener("pointerup", end, true);
  window.addEventListener("pointercancel", end, true);

  return {
    register(token, send) {
      senders.set(token, send);
      attach(token);
    },
    unregister(token) {
      senders.delete(token);
    },
    attach,
  };
}
