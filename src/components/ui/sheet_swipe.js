// Swipe-to-dismiss for the compact bottom sheets (Sheet and the kit menu
// sheets). Loaded by `use_sheet_swipe` (sheet.rs) through document::eval,
// once per open sheet, with gesture_core.js prepended. Rust owns whether the
// sheet is open; this script follows the finger and posts "close" once.
//
// A downward drag that starts on the sheet's grab strip or head, or anywhere
// in it while its content is scrolled to the top, moves the sheet with the
// finger and fades the scrim. An upward drag only gives a little (a rubber
// band). Released past 30% of the sheet's height, or on a downward flick,
// the sheet slides out and the script posts "close"; otherwise it springs
// back. Reduced motion: no slide, the sheet just closes. Touch events, like
// the other gesture scripts: a non-passive touchmove claims the gesture only
// once it is clearly downward, so the content still scrolls.
//
// DOM contract: the sheet carries data-sheet-swipe="<token>"; its scrim is
// the element just before it (.rs-scrim). Only on the compact layout.
// window.__rsSheetSwipe: register(token, send), unregister(token), state().

const sheets = window.__rsSheetSwipe || (window.__rsSheetSwipe = installSheetSwipe());
sheets.register(__SHEET__, (message) => dioxus.send(message));
await new Promise(() => {});

function installSheetSwipe() {
  const CLOSE_RATIO = 0.3; // of the sheet's height
  const FLICK_PX_PER_MS = 0.5;
  const FLICK_MIN_PX = 24;
  const RUBBER = 0.35; // share of an upward drag the sheet follows, tapering
  const SPRING_MS = 220;
  const CLOSE_WAIT_MS = 500; // put the sheet back if Rust kept it open
  // Inside the sheet, these keep their own touches.
  const OWN = "input, textarea, select, [role='slider'], [data-row-handle], [data-rowg-swipe]";
  const HANDLES = ".rs-sheet__grab, .rs-sheet__head, .rs-menu__head";

  const senders = new Map(); // token -> send
  const reduceMotion = prefersReducedMotion();
  const guard = clickGuard(GESTURE_CLICK_GUARD_MS, "[data-sheet-swipe]");
  let g = null; // the current drag
  let frame = 0;

  function compact() {
    return !!document.querySelector(".rs-app[data-layout='compact']");
  }

  // The scroller between the target and the sheet, if any is scrolled down.
  function scrolledAway(target, sheet) {
    for (let el = target; el && el !== sheet.parentElement; el = el.parentElement) {
      if (el.scrollTop > 0) return true;
      if (el === sheet) break;
    }
    return false;
  }

  function onStart(e) {
    if ((g && g.sheet.isConnected) || e.touches.length !== 1 || !compact()) return;
    const target = e.target instanceof Element ? e.target : null;
    const sheet = target && target.closest("[data-sheet-swipe]");
    if (!sheet || !senders.has(Number(sheet.dataset.sheetSwipe)) || target.closest(OWN)) return;
    if (!target.closest(HANDLES) && scrolledAway(target, sheet)) return;
    const t = e.touches[0];
    const scrim = sheet.previousElementSibling && sheet.previousElementSibling.classList.contains("rs-scrim")
      ? sheet.previousElementSibling
      : null;
    g = { sheet, scrim, id: t.identifier, y0: t.clientY, x0: t.clientX, dy: 0, phase: "pending", samples: [], height: sheet.offsetHeight || 1 };
  }

  function onMove(e) {
    if (!g || g.phase === "settle") return;
    const t = touchById(e.touches, g.id);
    if (!t) return;
    const dy = t.clientY - g.y0;
    const dx = t.clientX - g.x0;
    if (g.phase === "pending") {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < GESTURE_LOCK_PX) return;
      if (dy <= 0 || Math.abs(dy) < Math.abs(dx) * 1.2) {
        g = null;
        return;
      }
      g.phase = "drag";
      g.sheet.classList.add("rs-sheet--dragging");
      g.sheet.style.transition = "none";
      if (g.scrim) g.scrim.style.transition = "none";
    }
    if (e.cancelable) e.preventDefault();
    // Downward follows the finger; upward gives a little, less the further it goes.
    g.dy = dy >= 0 ? dy : -g.height * RUBBER * (1 - 1 / (1 - dy / g.height));
    pushSample(g.samples, nowMs(), t.clientY);
    if (!frame) frame = requestAnimationFrame(paint);
  }

  function paint() {
    frame = 0;
    if (!g || g.phase !== "drag") return;
    g.sheet.style.transform = `translate3d(0,${g.dy.toFixed(1)}px,0)`;
    if (g.scrim) g.scrim.style.opacity = String(clamp(1 - g.dy / g.height, 0, 1));
  }

  function onEnd(e) {
    if (!g || g.phase === "settle" || touchById(e.touches, g.id)) return;
    if (g.phase === "pending") {
      g = null;
      return;
    }
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    guard.arm();
    const speed = velocity(g.samples); // px/ms, + is down
    const close =
      e.type === "touchend" &&
      ((g.dy > g.height * CLOSE_RATIO && speed > -0.2) || (speed > FLICK_PX_PER_MS && g.dy > FLICK_MIN_PX));
    if (close) dismiss(g, speed);
    else springBack(g);
  }

  function animate(d, to, ms, scrimTo, done) {
    const ease = "cubic-bezier(.2,.75,.25,1)";
    d.phase = "settle";
    d.sheet.style.transition = `transform ${ms}ms ${ease}`;
    d.sheet.style.transform = `translate3d(0,${to}px,0)`;
    if (d.scrim) {
      d.scrim.style.transition = `opacity ${ms}ms linear`;
      d.scrim.style.opacity = String(scrimTo);
    }
    setTimeout(done, ms + 20);
  }

  function springBack(d) {
    animate(d, 0, reduceMotion.matches ? 0 : SPRING_MS, 1, () => reset(d));
  }

  function dismiss(d, speed) {
    const send = senders.get(Number(d.sheet.dataset.sheetSwipe));
    const finish = () => {
      if (send) send("close");
      // Normally the sheet is gone now; if Rust kept it, put it back.
      setTimeout(() => d.sheet.isConnected && reset(d), CLOSE_WAIT_MS);
    };
    if (reduceMotion.matches) {
      d.phase = "settle";
      finish();
      return;
    }
    const remaining = d.height - Math.max(d.dy, 0);
    const ms = Math.round(clamp(remaining / Math.max(speed, 1.2), 120, 260));
    animate(d, d.height, ms, 0, finish);
  }

  function reset(d) {
    d.sheet.classList.remove("rs-sheet--dragging");
    d.sheet.style.transition = "";
    d.sheet.style.transform = "";
    if (d.scrim) {
      d.scrim.style.transition = "";
      d.scrim.style.opacity = "";
    }
    if (g === d) g = null;
  }

  const passive = { capture: true, passive: true };
  window.addEventListener("touchstart", onStart, passive);
  window.addEventListener("touchmove", onMove, { capture: true, passive: false });
  window.addEventListener("touchend", onEnd, passive);
  window.addEventListener("touchcancel", onEnd, passive);

  return {
    register(token, send) {
      senders.set(token, send);
    },
    unregister(token) {
      senders.delete(token);
      if (g && Number(g.sheet.dataset.sheetSwipe) === token) g = null;
    },
    state() {
      return g ? g.phase : "idle";
    },
  };
}
