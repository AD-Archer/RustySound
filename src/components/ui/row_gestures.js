// Row gestures for editable song lists: the queue page, Now Playing's Up Next
// and the user's own playlists. Loaded by `use_row_gestures` (row_gestures.rs)
// through document::eval, so it runs in the iOS WKWebView as well as the
// browser. Rust owns the lists: this script only follows the finger, mouse or
// keys, and posts the decision once through dioxus.send. Per move it writes
// transforms only (and one width), never layout that Rust renders.
//
// 1. Reorder. Pressing a row's grip handle ([data-row-handle]) lifts the row
//    at once: a floating copy follows the pointer, the rows in between slide
//    aside, and the scroll area auto-scrolls near its top and bottom edges.
//    Release posts {kind:"move", from, to, key}. The handle has
//    touch-action:none, so it never fights scrolling, and it stops the row's
//    long-press. Keyboard: focus the handle, Space or Enter picks up, arrows
//    move, Space or Enter drops, Escape cancels; an aria-live region says
//    what happened. Windowed lists (VirtualList) work too: the target comes
//    from geometry (index x row height), not from mounted rows, and rows that
//    mount during the drag get their offset on the next frame.
// 2. Swipe. Dragging a row ([data-rowg-swipe]) from left to right slides it
//    over the actions behind it (Remove + Add to..., or Play Next + Add to...).
//    Released past half the actions it stays open; a full swipe past 60% of
//    the width on an editable row slides it out, collapses it and posts
//    {kind:"remove"}. Action taps post {kind:"remove"|"add"|"next"}.
//    Touch uses touch events (a non-passive touchmove claims the gesture only
//    once it is clearly horizontal, so vertical drags still scroll); a mouse
//    drag does the same through pointer events.
//
// Coordination with the other gesture scripts:
// * edge_swipe.js owns touches that start within 24px of either screen edge
//   (its start zone). Row swipes never start there, and the edge swipe skips
//   [data-row-handle], so a handle at the trailing edge never navigates.
// * gestures.js (Now Playing) skips [data-row-handle]; its panel drag only
//   claims vertical moves and this script only horizontal ones, so the two
//   decide on the first few pixels without overlap.
// * TrackRow's long-press is cancelled with a synthetic pointercancel as soon
//   as a swipe or drag starts.
//
// DOM contract (rendered by RowGestureList and TrackRow):
//   [data-rowg="<list>"]            list wrapper; data-rowg-min / -max give
//                                   the reorderable index range (inclusive)
//   li[data-index][data-key]        rows; data-rowg-swipe="edit"|"browse"
//   [data-row-handle]               grip button
//   .rs-rowg__actions               the actions behind a row
//   [data-rowg-action="remove"|"add"|"next"]
//
// window.__rsRowG: register(list, token, send), unregister(list, token),
// state() for tests.

const LIST_ID = __LIST__;
const TOKEN = __TOKEN__;
const rowGestures = window.__rsRowG || (window.__rsRowG = install());
rowGestures.register(LIST_ID, TOKEN, (message) => dioxus.send(message));
await new Promise(() => {});

function install() {
  const EDGE_PX = 24; // edge_swipe.js start zone
  const LOCK_PX = 8; // travel before a swipe's direction is decided
  const FULL_RATIO = 0.6; // of the row width: a full swipe removes
  const FLICK_PX_PER_MS = 0.45;
  const AUTO_EDGE = 72; // px from the scroll area's edge where auto-scroll starts
  const AUTO_MAX = 18; // px per frame at the very edge
  const SETTLE_MS = 180;
  const WAIT_MS = 900; // give up waiting for Rust's re-render after this
  const CLICK_GUARD_MS = 400;
  const BOTTOM_CHROME = ".rs-tabbar, [data-testid='mini-player'], [data-testid='player-bar']";

  const reduceMotion = window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };
  const senders = new Map(); // list id -> { token, send }
  let drag = null; // reorder in progress (pointer or keys)
  let swipe = null; // swipe in progress
  let open = null; // row left open: { li, content, list, dx }
  let busy = 0; // removes and drops waiting for Rust
  let frame = 0;
  let clickGuardUntil = 0;
  let swallowTap = false;
  let liveRegion = null;

  const now = () => performance.now();
  const ms = (value) => (reduceMotion.matches ? 0 : value);
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const asElement = (t) => (t instanceof Element ? t : null);
  const listOf = (el) => el && el.closest("[data-rowg]");
  const rowOf = (el) => el && el.closest("li[data-index]");
  const indexOf = (li) => Number(li.dataset.index);
  const rowsIn = (list) => list.querySelectorAll("li[data-index]");
  const rowAt = (list, index) => list.querySelector(`li[data-index="${index}"]`);
  const keyAt = (list, index) => {
    const li = rowAt(list, index);
    return li ? li.dataset.key || "" : null;
  };
  const titleOf = (li) => {
    const el = li.querySelector(".rs-trow__title");
    return el ? el.textContent.trim() : "Song";
  };

  function post(list, message) {
    const entry = senders.get(list.dataset.rowg);
    if (entry) entry.send(message);
  }

  function rangeOf(list) {
    const min = Number(list.dataset.rowgMin);
    const max = Number(list.dataset.rowgMax);
    if (!Number.isFinite(min) || !Number.isFinite(max) || max <= min) return null;
    return { min, max };
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(paint);
  }

  function paint() {
    frame = 0;
    if (drag) paintDrag();
    if (swipe && swipe.phase === "drag") paintSwipe(swipe);
  }

  // TrackRow's long-press runs in Rust off pointer events; a synthetic
  // pointercancel tells it this press became a gesture.
  function cancelLongPress(el) {
    try {
      el.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true }));
    } catch (_) {}
  }

  // Calls `done` once the list re-rendered so that `check()` holds, or after
  // WAIT_MS if Rust refused. The observer callback runs before the next
  // paint, so styles reset in `done` never show the old order for a frame.
  function waitFor(list, check, done) {
    busy++;
    let finished = false;
    const finish = () => {
      if (finished) return;
      finished = true;
      busy--;
      observer.disconnect();
      clearTimeout(timer);
      done();
    };
    const observer = new MutationObserver(() => {
      if (!list.isConnected || check()) finish();
    });
    observer.observe(list, { subtree: true, childList: true, attributes: true, attributeFilter: ["data-key", "data-index"] });
    const timer = setTimeout(finish, WAIT_MS);
  }

  function announce(text) {
    if (!liveRegion || !liveRegion.isConnected) {
      liveRegion = document.createElement("div");
      liveRegion.className = "rs-sr-only";
      liveRegion.setAttribute("aria-live", "assertive");
      liveRegion.setAttribute("aria-atomic", "true");
      liveRegion.setAttribute("data-testid", "row-live");
      document.body.appendChild(liveRegion);
    }
    liveRegion.textContent = text;
  }

  // ---- scroll areas ------------------------------------------------------

  function scrollerOf(el) {
    for (let node = el.parentElement; node; node = node.parentElement) {
      // The nearest scroll container, even one that doesn't overflow yet:
      // looking further out could scroll the page behind Now Playing.
      const overflow = getComputedStyle(node).overflowY;
      if (overflow === "auto" || overflow === "scroll") return node;
    }
    return document.scrollingElement || document.documentElement;
  }

  // The part of the scroll area the user can see: the page scroller runs
  // under the tab bar and the floating player on compact layouts.
  function visibleRect(scroller) {
    const r = scroller.getBoundingClientRect();
    let top = Math.max(r.top, 0);
    let bottom = Math.min(r.bottom, window.innerHeight);
    if (scroller.classList.contains("rs-main")) {
      for (const el of document.querySelectorAll(BOTTOM_CHROME)) {
        const c = el.getBoundingClientRect();
        if (c.height > 0 && c.top < bottom && c.top > top + 120) bottom = c.top;
      }
    }
    return { top, bottom };
  }

  // ---- reorder -------------------------------------------------------------

  const GHOST_TOKENS = [
    "--surface", "--ink", "--muted", "--muted-strong", "--faint", "--line", "--accent",
    "--accent-text", "--tint", "--tint-strong", "--rowg-cover", "--skeleton",
  ];

  // The lifted row: a copy in the app root (not inside the list, whose
  // containers may clip or contain it), with the list's colour tokens.
  function makeGhost(list, li, rect) {
    const ul = document.createElement("ul");
    ul.className = "rs-list rs-rowg-ghost";
    ul.setAttribute("aria-hidden", "true");
    const styles = getComputedStyle(list.firstElementChild || list);
    for (const name of GHOST_TOKENS) {
      const value = styles.getPropertyValue(name);
      if (value) ul.style.setProperty(name, value.trim());
    }
    const copy = li.cloneNode(true);
    for (const el of [copy, ...copy.querySelectorAll("[id], [data-testid], [data-index], [data-row-handle]")]) {
      el.removeAttribute("id");
      el.removeAttribute("data-testid");
      el.removeAttribute("data-index");
      el.removeAttribute("data-row-handle");
    }
    copy.classList.remove("rs-rowg--lifted", "rs-rowg--active", "rs-rowg--armed");
    copy.style.transform = "";
    copy.style.transition = "";
    ul.appendChild(copy);
    ul.style.left = `${rect.left}px`;
    ul.style.top = `${rect.top}px`;
    ul.style.width = `${rect.width}px`;
    ul.style.height = `${rect.height}px`;
    (document.querySelector(".rs-app") || document.body).appendChild(ul);
    return ul;
  }

  function lift(list, li, range, mode) {
    const rect = li.getBoundingClientRect();
    const scroller = scrollerOf(list);
    if (open) closeOpen(false);
    cancelLongPress(li);
    const ghost = makeGhost(list, li, rect);
    list.classList.add("rs-rowg-sorting");
    const from = indexOf(li);
    drag = {
      mode,
      list,
      from,
      to: from,
      min: range.min,
      max: range.max,
      count: range.max - range.min + 1,
      rowH: rect.height || 58,
      rowTop0: rect.top,
      scroller,
      scroll0: scroller.scrollTop,
      ghost,
      key: li.dataset.key || "",
      title: titleOf(li),
      shifts: new Map(),
      y: rect.top + rect.height / 2,
      y0: rect.top + rect.height / 2,
      grab: rect.height / 2,
      pointerId: null,
      handle: null,
    };
    if (mode === "keys") ghost.classList.add("rs-rowg-ghost--keys");
    requestAnimationFrame(() => ghost.classList.add("rs-rowg-ghost--up"));
    schedule();
  }

  // Where row `from` would sit now (viewport y), given how far the area scrolled.
  function slotZero(d) {
    return d.rowTop0 - (d.scroller.scrollTop - d.scroll0);
  }

  function paintDrag() {
    const d = drag;
    let again = false;
    if (d.mode === "pointer") {
      // Near an edge, and only after moving toward it: a row picked up near
      // the bottom of a short panel doesn't run off on its own.
      const view = visibleRect(d.scroller);
      const edge = clamp((view.bottom - view.top) * 0.15, 24, AUTO_EDGE);
      let speed = 0;
      if (d.y < view.top + edge && d.y < d.y0 - 4) speed = -AUTO_MAX * Math.min(1, (view.top + edge - d.y) / edge);
      else if (d.y > view.bottom - edge && d.y > d.y0 + 4) speed = AUTO_MAX * Math.min(1, (d.y - (view.bottom - edge)) / edge);
      if (speed) {
        const before = d.scroller.scrollTop;
        d.scroller.scrollTop = before + speed;
        again = d.scroller.scrollTop !== before;
      }
    }
    const slot0 = slotZero(d);
    const minTop = slot0 + (d.min - d.from) * d.rowH;
    const maxTop = slot0 + (d.max - d.from) * d.rowH;
    let top;
    if (d.mode === "pointer") {
      top = clamp(d.y - d.grab, minTop, maxTop);
      d.to = clamp(d.from + Math.round((top - slot0) / d.rowH), d.min, d.max);
    } else {
      top = slot0 + (d.to - d.from) * d.rowH;
    }
    d.ghostTop = top;
    d.ghost.style.transform = `translate3d(0,${(top - d.rowTop0).toFixed(1)}px,0)`;
    shiftRows(d);
    if (again) schedule();
  }

  function shiftRows(d) {
    for (const li of rowsIn(d.list)) {
      const i = indexOf(li);
      li.classList.toggle("rs-rowg--lifted", i === d.from);
      let shift = 0;
      if (d.from < d.to && i > d.from && i <= d.to) shift = -d.rowH;
      else if (d.to < d.from && i >= d.to && i < d.from) shift = d.rowH;
      if (d.shifts.get(li) === shift) continue;
      if (!d.shifts.has(li)) {
        // Mounted during the drag (windowed list): place it, don't slide it.
        li.style.transition = "none";
        requestAnimationFrame(() => {
          li.style.transition = "";
        });
      }
      d.shifts.set(li, shift);
      li.style.transform = shift ? `translate3d(0,${shift}px,0)` : "";
    }
  }

  // Back to rest: no transitions, so the re-rendered order shows as is.
  function clearDrag(d) {
    d.list.classList.remove("rs-rowg-sorting");
    for (const li of rowsIn(d.list)) {
      li.classList.remove("rs-rowg--lifted");
      li.style.transition = "";
      li.style.transform = "";
    }
    d.ghost.remove();
  }

  // Lands the copy; no new gesture starts until it has.
  function moveGhostTo(d, top, then) {
    busy++;
    d.ghost.classList.remove("rs-rowg-ghost--up");
    d.ghost.style.transition = `transform ${ms(SETTLE_MS)}ms cubic-bezier(.2,.75,.25,1), box-shadow ${ms(SETTLE_MS)}ms`;
    d.ghost.style.transform = `translate3d(0,${(top - d.rowTop0).toFixed(1)}px,0)`;
    setTimeout(() => {
      busy--;
      then();
    }, ms(SETTLE_MS) + 20);
  }

  function position(d) {
    return `position ${d.to - d.min + 1} of ${d.count}`;
  }

  function drop() {
    const d = drag;
    drag = null;
    if (!d) return;
    releasePointer(d);
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    paintDragOnce(d);
    const target = slotZero(d) + (d.to - d.from) * d.rowH;
    const moved = d.to !== d.from;
    let landed = false;
    let rendered = !moved;
    const finish = () => {
      if (!landed || !rendered) return;
      clearDrag(d);
      if (d.mode === "keys") focusHandle(d.list, d.to);
    };
    moveGhostTo(d, target, () => {
      landed = true;
      finish();
    });
    if (moved) {
      post(d.list, { kind: "move", from: d.from, to: d.to, key: d.key });
      waitFor(d.list, () => keyAt(d.list, d.to) === d.key, () => {
        rendered = true;
        finish();
      });
    }
    announce(moved ? `${d.title}, dropped at ${position(d)}.` : `${d.title}, dropped at its old place.`);
  }

  function cancelDrag(say) {
    const d = drag;
    drag = null;
    if (!d) return;
    releasePointer(d);
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    d.to = d.from;
    shiftRows(d);
    moveGhostTo(d, slotZero(d), () => {
      clearDrag(d);
      if (d.mode === "keys") focusHandle(d.list, d.from);
    });
    if (say) announce(`Cancelled. ${d.title} is back at ${position(d)}.`);
  }

  // Paints synchronously (the drop needs the final target).
  function paintDragOnce(d) {
    const saved = drag;
    drag = d;
    paintDrag();
    drag = saved;
  }

  function releasePointer(d) {
    if (d.handle && d.pointerId !== null) {
      try {
        d.handle.releasePointerCapture(d.pointerId);
      } catch (_) {}
    }
  }

  function focusHandle(list, index) {
    const li = rowAt(list, index);
    const handle = li && li.querySelector("[data-row-handle]");
    if (handle) handle.focus({ preventScroll: true });
  }

  function startPointerDrag(e, handle) {
    if (drag || busy || (e.pointerType === "mouse" && e.button !== 0)) return;
    const li = rowOf(handle);
    const list = listOf(li);
    if (!li || !list || !senders.has(list.dataset.rowg)) return;
    const range = rangeOf(list);
    const index = indexOf(li);
    if (!range || index < range.min || index > range.max) return;
    e.preventDefault();
    e.stopPropagation();
    try {
      handle.setPointerCapture(e.pointerId);
    } catch (_) {}
    lift(list, li, range, "pointer");
    drag.pointerId = e.pointerId;
    drag.handle = handle;
    drag.y = e.clientY;
    drag.y0 = e.clientY;
    drag.grab = e.clientY - drag.rowTop0;
  }

  function startKeyDrag(handle) {
    if (drag || busy) return false;
    const li = rowOf(handle);
    const list = listOf(li);
    if (!li || !list || !senders.has(list.dataset.rowg)) return false;
    const range = rangeOf(list);
    const index = indexOf(li);
    if (!range || index < range.min || index > range.max) return false;
    lift(list, li, range, "keys");
    announce(
      `Picked up ${drag.title}, ${position(drag)}. Use the up and down arrow keys to move it, Space to drop it, Escape to cancel.`,
    );
    return true;
  }

  function keyStep(delta) {
    const d = drag;
    const next = clamp(d.to + delta, d.min, d.max);
    if (next === d.to) return;
    d.to = next;
    // Keep the target slot in view (windowed lists mount rows as it scrolls).
    const top = slotZero(d) + (d.to - d.from) * d.rowH;
    const view = visibleRect(d.scroller);
    if (top < view.top + 8) d.scroller.scrollTop -= view.top + 8 - top;
    else if (top + d.rowH > view.bottom - 8) d.scroller.scrollTop += top + d.rowH - (view.bottom - 8);
    schedule();
    announce(`${d.title}, ${position(d)}.`);
  }

  // ---- swipe ---------------------------------------------------------------

  function swipeTarget(target) {
    const el = asElement(target);
    if (!el || el.closest("[data-row-handle], [data-rowg-action]")) return null;
    const li = el.closest("li[data-rowg-swipe]");
    const list = listOf(li);
    if (!li || !list || !senders.has(list.dataset.rowg)) return null;
    const content = li.querySelector(":scope > .rs-trow");
    const actions = li.querySelector(":scope > .rs-rowg__actions");
    if (!content || !actions) return null;
    return { li, list, content, actions };
  }

  function revealWidth(actions) {
    return actions.offsetWidth || 152;
  }

  function beginSwipe(s) {
    s.phase = "drag";
    if (open && open.li !== s.li) closeOpen(false);
    if (open && open.li === s.li) open = null;
    cancelLongPress(s.content);
    s.reveal = revealWidth(s.actions);
    s.canRemove = s.li.dataset.rowgSwipe === "edit";
    s.li.classList.add("rs-rowg--active");
    s.content.style.transition = "none";
    s.actions.style.transition = "none";
  }

  function moveSwipe(e, x, y) {
    const s = swipe;
    const dx = x - s.x0;
    const dy = y - s.y0;
    if (s.phase === "pending") {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < LOCK_PX) return;
      const horizontal = Math.abs(dx) > Math.abs(dy) * 1.2;
      // A closed row only opens rightward; an open one can also close.
      if (!horizontal || (s.base === 0 && dx < 0)) {
        swipe = null;
        return;
      }
      beginSwipe(s);
    }
    if (e.cancelable) e.preventDefault();
    s.dx = clamp(s.base + dx, 0, s.width);
    s.armed = s.canRemove && s.dx > s.width * FULL_RATIO;
    const t = now();
    s.samples.push([t, x]);
    while (s.samples.length > 2 && t - s.samples[0][0] > 100) s.samples.shift();
    schedule();
  }

  // The actions keep their resting width until the row passes them; beyond
  // that they stretch to the row's edge (and Remove fills them when armed).
  function paintSwipe(s) {
    s.content.style.transform = s.dx ? `translate3d(${s.dx.toFixed(1)}px,0,0)` : "";
    s.actions.style.width = s.dx > s.reveal ? `${s.dx.toFixed(1)}px` : "";
    s.li.classList.toggle("rs-rowg--armed", !!s.armed);
  }

  function velocity(samples) {
    if (samples.length < 2) return 0;
    const [t0, x0] = samples[0];
    const [t1, x1] = samples[samples.length - 1];
    if (now() - t1 > 80) return 0;
    return t1 > t0 ? (x1 - x0) / (t1 - t0) : 0;
  }

  function endSwipe(cancelled) {
    const s = swipe;
    swipe = null;
    if (!s || s.phase !== "drag") return;
    if (frame) cancelAnimationFrame(frame), (frame = 0);
    clickGuardUntil = now() + CLICK_GUARD_MS;
    if (s.pointer) {
      try {
        s.li.releasePointerCapture(s.id);
      } catch (_) {}
    }
    const speed = velocity(s.samples);
    if (!cancelled && s.armed) {
      removeRow(s.li, s.list, s.content, s.dx);
    } else if (!cancelled && ((s.dx > s.reveal * 0.5 && speed > -0.3) || (speed > FLICK_PX_PER_MS && s.dx > 24))) {
      settle(s.li, s.content, s.reveal, () => {});
      open = { li: s.li, content: s.content, list: s.list, dx: s.reveal };
    } else {
      closeRow(s.li, s.content);
    }
  }

  function settle(li, content, to, then) {
    const actions = li.querySelector(":scope > .rs-rowg__actions");
    const dur = ms(SETTLE_MS);
    content.style.transition = `transform ${dur}ms cubic-bezier(.2,.75,.25,1)`;
    if (actions) actions.style.transition = `width ${dur}ms cubic-bezier(.2,.75,.25,1)`;
    li.classList.remove("rs-rowg--armed");
    content.style.transform = to ? `translate3d(${to}px,0,0)` : "";
    if (actions) actions.style.width = "";
    setTimeout(then, dur + 20);
  }

  function closeRow(li, content) {
    settle(li, content, 0, () => {
      if (open && open.li === li) return;
      if (swipe && swipe.li === li) return;
      resetRow(li, content);
    });
  }

  function resetRow(li, content) {
    li.classList.remove("rs-rowg--active", "rs-rowg--armed", "rs-rowg--removing");
    li.style.height = "";
    li.style.overflow = "";
    li.style.transition = "";
    if (content) {
      content.style.transition = "";
      content.style.transform = "";
    }
    const actions = li.querySelector(":scope > .rs-rowg__actions");
    if (actions) {
      actions.style.transition = "";
      actions.style.width = "";
    }
  }

  function closeOpen(animate) {
    const o = open;
    open = null;
    if (!o) return;
    if (animate) closeRow(o.li, o.content);
    else resetRow(o.li, o.content);
  }

  // Slides the row out to the right, collapses it, then asks Rust to remove
  // it. Rust shows the Undo toast.
  function removeRow(li, list, content, from) {
    const index = indexOf(li);
    const key = li.dataset.key || "";
    if (open && open.li === li) open = null;
    const width = li.offsetWidth;
    const actions = li.querySelector(":scope > .rs-rowg__actions");
    li.classList.add("rs-rowg--active", "rs-rowg--armed", "rs-rowg--removing");
    const slide = ms(160);
    const collapse = ms(170);
    content.style.transition = `transform ${slide}ms ease-in`;
    content.style.transform = `translate3d(${width}px,0,0)`;
    if (actions) {
      actions.style.transition = `width ${slide}ms ease-in`;
      actions.style.width = `${Math.max(from, 1)}px`;
      void actions.offsetWidth;
      actions.style.width = `${width}px`;
    }
    busy++;
    setTimeout(() => {
      li.style.height = `${li.offsetHeight}px`;
      li.style.overflow = "hidden";
      void li.offsetHeight;
      li.style.transition = `height ${collapse}ms ease`;
      li.style.height = "0px";
      setTimeout(() => {
        busy--;
        post(list, { kind: "remove", index, key });
        waitFor(list, () => keyAt(list, index) !== key, () => resetRow(li, content));
      }, collapse + 10);
    }, slide + 10);
  }

  function onAction(e, button) {
    const li = rowOf(button);
    const list = listOf(li);
    if (!li || !list) return;
    e.preventDefault();
    e.stopPropagation();
    const content = li.querySelector(":scope > .rs-trow");
    const kind = button.dataset.rowgAction;
    if (kind === "remove") {
      removeRow(li, list, content, open && open.li === li ? open.dx : 0);
      return;
    }
    post(list, { kind, index: indexOf(li), key: li.dataset.key || "" });
    if (open && open.li === li) open = null;
    closeRow(li, content);
  }

  // ---- event wiring --------------------------------------------------------

  function touchById(list, id) {
    for (let i = 0; i < list.length; i++) if (list[i].identifier === id) return list[i];
    return null;
  }

  function onTouchStart(e) {
    swallowTap = false;
    if (drag) return;
    if (e.touches.length !== 1) {
      if (swipe) endSwipe(true);
      return;
    }
    const target = swipeTarget(e.target);
    const el = asElement(e.target);
    if (open && !(el && open.li.contains(el) && el.closest("[data-rowg-action]"))) {
      // Any touch outside the open row's actions closes it, and only closes it.
      const sameRow = target && target.li === open.li;
      if (!sameRow) {
        closeOpen(true);
        swallowTap = true;
        return;
      }
    }
    if (!target || busy) return;
    const t = e.touches[0];
    if (t.clientX <= EDGE_PX || t.clientX >= window.innerWidth - EDGE_PX) return;
    startSwipe(target, t.identifier, t.clientX, t.clientY, false);
  }

  function startSwipe(target, id, x, y, pointer) {
    const base = open && open.li === target.li ? open.dx : 0;
    swipe = {
      ...target,
      id,
      pointer,
      x0: x,
      y0: y,
      base,
      dx: base,
      width: target.li.offsetWidth || 1,
      phase: "pending",
      samples: [],
      armed: false,
      reveal: 0,
    };
    // Tapping an open row closes it (and doesn't play).
    if (base) swipe.tapCloses = true;
  }

  function onTouchMove(e) {
    if (drag) {
      if (e.cancelable) e.preventDefault();
      return;
    }
    if (!swipe || swipe.pointer) return;
    const t = touchById(e.touches, swipe.id);
    if (t) moveSwipe(e, t.clientX, t.clientY);
  }

  function onTouchEnd(e) {
    if (!swipe || swipe.pointer || touchById(e.touches, swipe.id)) return;
    if (swipe.phase === "pending") {
      const s = swipe;
      swipe = null;
      if (s.tapCloses) {
        closeOpen(true);
        swallowTap = true;
      }
      return;
    }
    endSwipe(e.type === "touchcancel");
  }

  function onPointerDown(e) {
    const el = asElement(e.target);
    const handle = el && el.closest("[data-row-handle]");
    if (handle) {
      startPointerDrag(e, handle);
      return;
    }
    if (e.pointerType !== "mouse") return;
    swallowTap = false;
    if (e.button !== 0 || drag) return;
    const target = swipeTarget(e.target);
    if (open && !(el && open.li.contains(el) && el.closest("[data-rowg-action]"))) {
      if (!target || target.li !== open.li) {
        closeOpen(true);
        swallowTap = true;
        return;
      }
    }
    if (!target || busy) return;
    startSwipe(target, e.pointerId, e.clientX, e.clientY, true);
  }

  function onPointerMove(e) {
    if (drag && drag.mode === "pointer" && e.pointerId === drag.pointerId) {
      drag.y = e.clientY;
      if (e.cancelable) e.preventDefault();
      schedule();
      return;
    }
    if (swipe && swipe.pointer && e.pointerId === swipe.id) {
      const wasPending = swipe.phase === "pending";
      moveSwipe(e, e.clientX, e.clientY);
      if (wasPending && swipe && swipe.phase === "drag") {
        try {
          swipe.li.setPointerCapture(e.pointerId);
        } catch (_) {}
      }
    }
  }

  function onPointerUp(e) {
    if (drag && drag.mode === "pointer" && e.pointerId === drag.pointerId) {
      if (e.type === "pointercancel") cancelDrag(false);
      else drop();
      return;
    }
    if (swipe && swipe.pointer && e.pointerId === swipe.id) {
      if (swipe.phase === "pending") {
        const s = swipe;
        swipe = null;
        if (s.tapCloses) {
          closeOpen(true);
          swallowTap = true;
        }
        return;
      }
      endSwipe(e.type === "pointercancel");
    }
  }

  function onClick(e) {
    const el = asElement(e.target);
    const action = el && el.closest("[data-rowg-action]");
    if (action && listOf(action)) {
      onAction(e, action);
      return;
    }
    if (swallowTap || (now() < clickGuardUntil && el && listOf(el))) {
      swallowTap = false;
      clickGuardUntil = 0;
      e.preventDefault();
      e.stopPropagation();
    }
  }

  // Window capture: ahead of the app's Space-to-play shortcut, which skips
  // events that are already handled.
  function onKeyDown(e) {
    if (drag && drag.mode === "keys") {
      switch (e.key) {
        case "ArrowUp":
        case "ArrowLeft":
          keyStep(-1);
          break;
        case "ArrowDown":
        case "ArrowRight":
          keyStep(1);
          break;
        case "Home":
          keyStep(drag.min - drag.to);
          break;
        case "End":
          keyStep(drag.max - drag.to);
          break;
        case " ":
        case "Spacebar":
        case "Enter":
          drop();
          break;
        case "Escape":
          cancelDrag(true);
          break;
        case "Tab":
          cancelDrag(true);
          return;
        default:
          return;
      }
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (drag) {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        cancelDrag(false);
      }
      return;
    }
    if (open && e.key === "Escape") {
      closeOpen(true);
      return;
    }
    const el = asElement(e.target);
    const handle = el && el.closest("[data-row-handle]");
    if (!handle || e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === " " || e.key === "Spacebar" || e.key === "Enter") {
      if (startKeyDrag(handle)) {
        e.preventDefault();
        e.stopPropagation();
      }
    }
  }

  function onScroll(e) {
    if (!open || swipe) return;
    const target = e.target;
    if (target === document || (target instanceof Node && target.contains(open.li))) closeOpen(true);
  }

  // Images inside rows would start a native drag under the mouse.
  function onDragStart(e) {
    const el = asElement(e.target);
    if (el && listOf(el) && rowOf(el)) e.preventDefault();
  }

  function onBlur() {
    if (drag && drag.mode === "pointer") cancelDrag(false);
    if (swipe) endSwipe(true);
  }

  window.addEventListener("touchstart", onTouchStart, { capture: true, passive: true });
  window.addEventListener("touchmove", onTouchMove, { capture: true, passive: false });
  window.addEventListener("touchend", onTouchEnd, { capture: true, passive: true });
  window.addEventListener("touchcancel", onTouchEnd, { capture: true, passive: true });
  window.addEventListener("pointerdown", onPointerDown, true);
  window.addEventListener("pointermove", onPointerMove, { capture: true, passive: false });
  window.addEventListener("pointerup", onPointerUp, true);
  window.addEventListener("pointercancel", onPointerUp, true);
  window.addEventListener("click", onClick, true);
  window.addEventListener("keydown", onKeyDown, true);
  window.addEventListener("dragstart", onDragStart, true);
  window.addEventListener("blur", onBlur);
  document.addEventListener("scroll", onScroll, { capture: true, passive: true });

  return {
    register(list, token, send) {
      senders.set(list, { token, send: typeof send === "function" ? send : () => {} });
    },
    unregister(list, token) {
      const entry = senders.get(list);
      if (entry && entry.token === token) senders.delete(list);
    },
    state() {
      if (drag) return drag.mode === "keys" ? "keys" : "drag";
      if (swipe) return swipe.phase === "drag" ? "swipe" : "pending";
      if (busy) return "busy";
      return open ? "open" : "idle";
    },
  };
}
