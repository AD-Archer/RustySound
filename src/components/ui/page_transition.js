// Page transitions between routes. Loaded once by PageTransition
// (page_transition.rs) through document::eval, so it runs in the iOS
// WKWebView and the macOS webview as well as the browser.
//
// Dioxus swaps the outlet in place, so the old page is gone by the time the
// new one renders. Rust therefore announces a navigation *before* it changes
// the route (`prepare(kind)`): the script clones what is on screen into an
// inert "ghost" layer over the page, and once the new route has rendered
// (`routed()`, from a route effect) it animates the ghost and the live page
// (.rs-main) against each other, then removes the ghost.
//
//   compact push / forward  the live page slides in from the right over the
//                           ghost, which shifts left 30% and dims
//   compact back            the ghost slides out to the right, uncovering
//                           the live page as it comes in from -30%, undimming
//   tab (tab bar, sidebar)  the ghost fades out over the live page
//   wide push / back        the ghost fades out while the live page fades in
//                           from 10px to the side
//   reduced motion          the ghost fades out, nothing moves
//
// Transforms and opacity only, through the Web Animations API (so the
// reduced-motion reset in base.css, which clamps CSS transitions, does not
// apply, and running animations override inline styles without touching
// them). The live page stays interactive throughout; the ghost takes no
// input. A new navigation while one is running ends it on the spot.
//
// Browser history (back/forward buttons, Safari's toolbar) reaches the
// router as popstate without passing through Rust, so the script announces
// those itself (the Navigation API's navigate event, or a capture popstate
// listener), with the direction from the entry index (the Navigation API's,
// or the one edge_swipe.js stamps into each entry). Navigations the edge
// swipe commits are its own animation and are skipped here.
//
// window.__rsPageTx:
//   prepare(kind)  kind: "push" | "forward" | "back" | "tab"
//   routed()       the route changed and the new page is in the DOM
//   settle()       end any transition now (the edge swipe starting a drag)
//   state()        "idle" | "waiting" | "running", for tests
//   last()         { kind, mode } of the last transition that ran, for tests

if (!window.__rsPageTx) window.__rsPageTx = install();

function install() {
  const ROUTE_WAIT_MS = 600; // drop the ghost if no new page arrives by then
  const reduceMotion = window.matchMedia
    ? window.matchMedia("(prefers-reduced-motion: reduce)")
    : { matches: false };

  // idle | waiting (ghost up, new page not rendered yet) | running
  let phase = "idle";
  let t = null; // the current transition
  let last = null;
  let lastIndex = historyIndex();
  let expectPopUntil = 0;
  let motion = null;

  // Durations and curves come from tokens.css (--dur-push, --ease-push, …).
  function tokens() {
    if (motion) return motion;
    const css = getComputedStyle(document.documentElement);
    const ms = (name, fallback) => {
      const raw = css.getPropertyValue(name).trim();
      const value = raw.endsWith("ms") ? parseFloat(raw) : raw.endsWith("s") ? parseFloat(raw) * 1000 : NaN;
      return Number.isFinite(value) ? value : fallback;
    };
    const ease = (name, fallback) => css.getPropertyValue(name).trim() || fallback;
    motion = {
      push: ms("--dur-push", 340),
      pushEase: ease("--ease-push", "cubic-bezier(.32,.72,0,1)"),
      shift: ms("--dur-page", 200),
      fade: ms("--dur-fast", 150),
      out: ease("--ease-out", "cubic-bezier(.2,.7,.2,1)"),
    };
    return motion;
  }

  function edgeSwipeBusy() {
    const swipe = window.__rsEdgeSwipe;
    return !!(swipe && swipe.state && swipe.state() !== "idle");
  }

  function prepare(kind, fromPopstate) {
    // Rust's go_back / go_forward end in a popstate on the web, which may
    // come before or after the router has already rendered the new route
    // (Chromium re-renders first). That popstate is this navigation, not a
    // new one.
    if (!fromPopstate && (kind === "back" || kind === "forward")) expectPopUntil = performance.now() + 1000;
    if (edgeSwipeBusy()) return;
    if (phase === "waiting" && t) {
      // A second tap before the first page arrived: the ghost is still what
      // is on screen, so keep it and play the newest kind.
      t.kind = kind;
      armTimer();
      return;
    }
    settle();
    const main = document.querySelector(".rs-main");
    if (!main || !main.parentNode || document.hidden) return;
    const app = main.closest(".rs-app");
    t = {
      kind,
      compact: !!app && app.getAttribute("data-layout") === "compact",
      main,
      ghost: makeGhost(main),
      shade: null,
      anims: [],
      timer: 0,
    };
    phase = "waiting";
    armTimer();
  }

  function armTimer() {
    clearTimeout(t.timer);
    const current = t;
    t.timer = setTimeout(() => {
      if (t === current && phase === "waiting") cleanup();
    }, ROUTE_WAIT_MS);
  }

  function routed() {
    lastIndex = historyIndex();
    if (phase !== "waiting" || !t) return;
    clearTimeout(t.timer);
    if (!t.main.isConnected) return cleanup();
    run();
  }

  function modeFor(tr) {
    if (reduceMotion.matches) return "fade";
    if (tr.kind === "tab") return "tab";
    if (!tr.compact) return "shift";
    return tr.kind === "back" ? "pop" : "push";
  }

  function run() {
    phase = "running";
    const { main, ghost } = t;
    const m = tokens();
    const mode = modeFor(t);
    const back = t.kind === "back";
    last = { kind: t.kind, mode };
    const go = (el, frames, duration, easing, fill) =>
      t.anims.push(el.animate(frames, { duration, easing, fill: fill || "none" }));
    const x = (v) => ({ transform: `translate3d(${v},0,0)` });
    let duration;

    if (mode === "push") {
      duration = m.push;
      const shade = addShade(main);
      ghost.style.zIndex = "1";
      shade.style.zIndex = "2";
      main.style.zIndex = "3";
      // The page has no background of its own; it slides over the ghost.
      main.style.backgroundColor = "var(--page)";
      main.classList.add("rs-ptx-lead");
      go(main, [x("100%"), x("0px")], duration, m.pushEase);
      go(ghost, [x("0px"), x("-30%")], duration, m.pushEase, "forwards");
      go(shade, [{ opacity: 0 }, { opacity: 1 }], duration, m.pushEase, "forwards");
    } else if (mode === "pop") {
      duration = m.push;
      const shade = addShade(main);
      main.style.zIndex = "1";
      shade.style.zIndex = "2";
      ghost.style.zIndex = "3";
      ghost.classList.add("rs-ptx-lead");
      go(main, [x("-30%"), x("0px")], duration, m.pushEase);
      go(shade, [{ opacity: 1 }, { opacity: 0 }], duration, m.pushEase, "forwards");
      go(ghost, [x("0px"), x("100%")], duration, m.pushEase, "forwards");
    } else if (mode === "shift") {
      duration = m.shift;
      const dx = back ? "-10px" : "10px";
      go(main, [{ opacity: 0, transform: `translate3d(${dx},0,0)` }, { opacity: 1, transform: "translate3d(0px,0,0)" }], duration, m.out);
      go(ghost, [{ opacity: 1 }, { opacity: 0 }], Math.round(duration * 0.7), m.out, "forwards");
    } else {
      // tab, fade: a quick dissolve to the page underneath.
      duration = mode === "tab" && !t.compact ? m.shift : m.fade;
      go(ghost, [{ opacity: 1 }, { opacity: 0 }], duration, "linear", "forwards");
    }

    const current = t;
    const done = () => {
      if (t === current) cleanup();
    };
    Promise.all(t.anims.map((a) => a.finished)).then(done, () => {});
    // Hidden tabs and throttled timelines may never finish; don't leave the
    // ghost up.
    t.timer = setTimeout(done, duration + 200);
  }

  function addShade(main) {
    const shade = document.createElement("div");
    shade.className = "rs-edge-under rs-ptx-shade";
    shade.setAttribute("aria-hidden", "true");
    place(shade, main);
    main.parentNode.insertBefore(shade, main);
    t.shade = shade;
    return shade;
  }

  function place(el, main) {
    el.style.top = `${main.offsetTop}px`;
    el.style.left = `${main.offsetLeft}px`;
    el.style.width = `${main.offsetWidth}px`;
    el.style.height = `${main.offsetHeight}px`;
  }

  // A copy of what the page shows right now, frozen at its scroll position.
  function makeGhost(main) {
    const ghost = document.createElement("div");
    ghost.className = "rs-ptx-ghost";
    ghost.setAttribute("aria-hidden", "true");
    ghost.inert = true;
    place(ghost, main);
    const content = document.createElement("div");
    content.className = "rs-ptx-ghost__content";
    const css = getComputedStyle(main);
    content.style.padding = `${css.paddingTop} ${css.paddingRight} 0 ${css.paddingLeft}`;
    content.style.minHeight = `${main.clientHeight}px`;
    content.style.transform = `translate3d(0,${-main.scrollTop}px,0)`;
    const scrolled = [];
    for (const child of main.children) {
      const copy = child.cloneNode(true);
      scrub(child, copy, scrolled);
      content.appendChild(copy);
    }
    ghost.appendChild(content);
    ghost.style.zIndex = "4";
    main.parentNode.insertBefore(ghost, main);
    // Shelves and other inner scrollers keep their offsets.
    for (const [el, left, top] of scrolled) {
      el.scrollLeft = left;
      el.scrollTop = top;
    }
    return ghost;
  }

  // Make the copy inert to everything that looks things up in the page:
  // ids (MediaSession presses player buttons by id), test ids, Dioxus' node
  // ids, radio groups. Images were already loaded; show them at once.
  function scrub(source, copy, scrolled) {
    const from = [source, ...source.querySelectorAll("*")];
    const to = [copy, ...copy.querySelectorAll("*")];
    for (let i = 0; i < to.length; i++) {
      const el = to[i];
      if (el.id) el.removeAttribute("id");
      el.removeAttribute("data-testid");
      el.removeAttribute("data-dioxus-id");
      el.removeAttribute("autofocus");
      const tag = el.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA" || tag === "BUTTON") el.removeAttribute("name");
      else if (tag === "IMG") {
        el.loading = "eager";
        el.decoding = "sync";
      }
      const src = from[i];
      if (src && (src.scrollLeft || src.scrollTop)) scrolled.push([el, src.scrollLeft, src.scrollTop]);
    }
  }

  function cleanup() {
    if (!t) return;
    clearTimeout(t.timer);
    for (const a of t.anims) {
      try {
        a.cancel();
      } catch (_) {}
    }
    const { main, ghost, shade } = t;
    ghost.remove();
    if (shade) shade.remove();
    main.style.zIndex = "";
    main.style.backgroundColor = "";
    main.classList.remove("rs-ptx-lead");
    t = null;
    phase = "idle";
  }

  // End whatever is running, leaving the live page where it belongs.
  function settle() {
    if (t) cleanup();
  }

  function historyIndex() {
    const state = typeof history !== "undefined" ? history.state : null;
    return Array.isArray(state) && typeof state[2] === "number" ? state[2] : null;
  }

  // Browser back/forward. The old page must still be in the DOM when the
  // ghost is taken, i.e. before the router's own popstate listener runs.
  // Where the Navigation API exists, its navigate event fires before any
  // popstate. Otherwise a capture listener: WebKit runs capture listeners
  // first at the target. (Chromium runs them in registration order, after
  // the router's, but has the Navigation API.)
  let navigateSeenUntil = 0;

  function traverse(kind, uaTransition) {
    if (performance.now() < expectPopUntil) {
      expectPopUntil = 0; // Rust announced it (a Back button, the edge swipe)
      return;
    }
    // Safari's own swipe already animated a snapshot of the page.
    if (uaTransition) return settle();
    prepare(kind, true);
  }

  const nav = window.navigation;
  if (nav && typeof nav.addEventListener === "function") {
    nav.addEventListener("navigate", (event) => {
      if (event.navigationType !== "traverse" || !event.destination || !event.destination.sameDocument) return;
      navigateSeenUntil = performance.now() + 1000;
      const from = nav.currentEntry ? nav.currentEntry.index : -1;
      const to = event.destination.index;
      traverse(to >= 0 && from >= 0 && to < from ? "back" : "forward", event.hasUAVisualTransition);
    });
  }

  window.addEventListener(
    "popstate",
    (event) => {
      const index = historyIndex();
      const previous = lastIndex;
      lastIndex = index;
      if (performance.now() < navigateSeenUntil) {
        navigateSeenUntil = 0;
        return;
      }
      const kind = index !== null && previous !== null && index < previous ? "back" : "forward";
      traverse(kind, event && event.hasUAVisualTransition);
    },
    true,
  );

  document.addEventListener("visibilitychange", () => document.hidden && settle());

  return {
    prepare,
    routed,
    settle,
    state() {
      return phase;
    },
    last() {
      return last;
    },
  };
}
