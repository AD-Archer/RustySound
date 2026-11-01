// Android shell glue (android_shell.rs). window.RustySoundAndroid is the
// activity's JavaScript interface (android/MainActivity.kt); elsewhere this
// script does nothing.
(() => {
  const shell = window.RustySoundAndroid;
  if (!shell) return;
  const root = document.documentElement;

  // System bar sizes as the --safe-* variables (env() stays 0 here).
  window.__rsApplyInsets = () => {
    try {
      const insets = JSON.parse(shell.insets());
      for (const side of ["top", "right", "bottom", "left"]) {
        root.style.setProperty(`--safe-${side}`, `${insets[side] || 0}px`);
      }
    } catch (_) {}
  };

  // Light or dark bar icons for what is under the status bar: the first
  // solid background up from the top of the page. An image or gradient
  // first (the Cover Flow stage, artwork headers) counts as dark.
  const isDarkAtTop = () => {
    for (let el = document.elementFromPoint(innerWidth / 2, 2); el; el = el.parentElement) {
      const style = getComputedStyle(el);
      const rgba = (style.backgroundColor.match(/[\d.]+/g) || []).map(Number);
      if (rgba.length >= 3 && (rgba.length < 4 || rgba[3] > 0.5)) {
        return 0.2126 * rgba[0] + 0.7152 * rgba[1] + 0.0722 * rgba[2] < 128;
      }
      if (style.backgroundImage !== "none") return true;
    }
    return matchMedia("(prefers-color-scheme: dark)").matches;
  };
  let dark = null;
  let queued = false;
  const syncBars = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      const isDark = isDarkAtTop();
      if (isDark !== dark) {
        dark = isDark;
        shell.setDarkBars(isDark);
      }
    });
  };
  window.__rsSyncBars = syncBars;
  if (!window.__rsAndroidBars) {
    window.__rsAndroidBars = true;
    const sync = () => window.__rsSyncBars();
    // Pages, sheets and themes change the DOM; scrolling moves what's on top.
    new MutationObserver(sync).observe(document.body, { childList: true, subtree: true });
    new MutationObserver(sync).observe(root, { attributes: true, attributeFilter: ["data-theme", "data-appearance"] });
    document.addEventListener("scroll", sync, { capture: true, passive: true });
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", sync);
  }

  // System Back: the top sheet, menu or Now Playing closes as on Escape;
  // otherwise Rust goes back a page or leaves the app.
  window.__rsAndroidBack = () => {
    const dialogs = document.querySelectorAll('[role="dialog"]');
    const top = dialogs[dialogs.length - 1];
    if (top) {
      const target = top.contains(document.activeElement) ? document.activeElement : top;
      target.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }),
      );
      return "dialog";
    }
    dioxus.send("back");
    return "page";
  };

  // Android TV: data-tv on <html> (bigger focus rings, base.css) and D-pad
  // focus that moves to the nearest control in the pressed direction, inside
  // the top sheet when one is open. Enter activates it; Back is above.
  if (shell.isTv() && !window.__rsTvNav) {
    window.__rsTvNav = true;
    root.setAttribute("data-tv", "");
    dioxus.send("tv");
    const FOCUSABLE =
      'a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"]), [role="button"], [role="slider"]';
    const CAROUSEL = '[aria-roledescription="carousel"]';
    const DIRS = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] };
    const editsText = (el) =>
      el.tagName === "TEXTAREA" || (el.tagName === "INPUT" && !/^(button|checkbox|radio|range|submit)$/.test(el.type));
    const shown = (el) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
    };
    document.addEventListener(
      "keydown",
      (e) => {
        const active = document.activeElement && document.activeElement !== document.body ? document.activeElement : null;
        if (e.key === "Enter" && active && active.matches('[role="button"], [tabindex]:not(a, button, input, select, textarea)')) {
          e.preventDefault();
          active.click();
          return;
        }
        const dir = DIRS[e.key];
        if (!dir || e.defaultPrevented) return;
        // Left/right stay with text fields, sliders and Cover Flow; every
        // arrow stays with a row picked up for reordering (row_gestures.js).
        const carousel = active && active.closest(CAROUSEL);
        if (active && dir[0] !== 0 && (editsText(active) || active.getAttribute("role") === "slider" || carousel)) return;
        if (active && active.closest(".rs-rowg-sorting")) return;
        const dialogs = document.querySelectorAll('[role="dialog"]');
        const scope = dialogs[dialogs.length - 1] || document;
        const from = active ? active.getBoundingClientRect() : { left: 0, top: 0, width: 0, height: 0 };
        const fx = from.left + from.width / 2;
        const fy = from.top + from.height / 2;
        let best = null;
        let bestScore = Infinity;
        for (const el of scope.querySelectorAll(FOCUSABLE)) {
          // A carousel is one stop; its covers move with left/right.
          if (el === active || !shown(el) || (el.parentElement && el.parentElement.closest(CAROUSEL))) continue;
          if (carousel && carousel.contains(el)) continue;
          const r = el.getBoundingClientRect();
          const dx = r.left + r.width / 2 - fx;
          const dy = r.top + r.height / 2 - fy;
          const along = dx * dir[0] + dy * dir[1];
          if (along <= 1) continue;
          const across = Math.abs(dx * dir[1] + dy * dir[0]);
          const score = along + across * 2;
          if (score < bestScore) (bestScore = score), (best = el);
        }
        if (!best) return;
        e.preventDefault();
        best.focus();
        best.scrollIntoView({ block: "nearest", inline: "nearest", behavior: "smooth" });
      },
      true,
    );
  }

  window.__rsApplyInsets();
  syncBars();
})();
