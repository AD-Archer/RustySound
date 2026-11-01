// Ambient mode's page side (ambient/mod.rs): hides the controls after a
// few seconds without input, closes on Escape or a swipe down, keeps the
// screen awake where the Wake Lock API exists, and sends the time of day
// once a minute. Posts "close" and "time:<text>" to Rust. Only listeners and
// one timer per minute: nothing runs per frame.
window.__rsAmbient?.stop();
{
  const root = () => document.getElementById("rs-ambient");
  // Tests shorten the idle delay with window.__rsAmbientIdleMs.
  const idleMs = Number(window.__rsAmbientIdleMs) || 3000;
  let idleTimer = 0;
  let clockTimer = 0;
  let lock = null;
  let start = null;

  const wake = () => {
    const el = root();
    if (el) el.removeAttribute("data-idle");
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => root()?.setAttribute("data-idle", "true"), idleMs);
  };

  const onKey = (e) => {
    if (e.key === "Escape") {
      // Ours alone: Now Playing underneath must not close too.
      e.preventDefault();
      e.stopPropagation();
      dioxus.send("close");
      return;
    }
    wake();
  };

  const onDown = (e) => {
    wake();
    start = e.isPrimary ? { x: e.clientX, y: e.clientY } : null;
  };
  const onUp = (e) => {
    if (!start) return;
    const dy = e.clientY - start.y;
    const dx = Math.abs(e.clientX - start.x);
    start = null;
    if (dy > 90 && dy > dx * 1.5) dioxus.send("close");
  };

  const time = () => {
    dioxus.send("time:" + new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }));
    clearTimeout(clockTimer);
    clockTimer = setTimeout(time, 60_000 - (Date.now() % 60_000) + 50);
  };

  const holdScreen = async () => {
    try {
      if (document.visibilityState === "visible" && navigator.wakeLock) lock = await navigator.wakeLock.request("screen");
    } catch {}
  };
  const onVisible = () => {
    if (document.visibilityState === "visible") holdScreen();
  };

  const moves = ["pointermove", "wheel", "touchstart"];
  moves.forEach((t) => window.addEventListener(t, wake, { passive: true }));
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("pointerdown", onDown, true);
  window.addEventListener("pointerup", onUp, true);
  document.addEventListener("visibilitychange", onVisible);
  wake();
  time();
  holdScreen();

  window.__rsAmbient = {
    stop() {
      moves.forEach((t) => window.removeEventListener(t, wake));
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("pointerup", onUp, true);
      document.removeEventListener("visibilitychange", onVisible);
      clearTimeout(idleTimer);
      clearTimeout(clockTimer);
      lock?.release().catch(() => {});
      lock = null;
      window.__rsAmbient = null;
      // Back to Now Playing, so Escape and the A key work there again.
      document.querySelector('[data-testid="now-playing"]')?.focus({ preventScroll: true });
    },
  };
}
await new Promise(() => {});
