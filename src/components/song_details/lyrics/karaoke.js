// Word-by-word fill for word-timed lyrics (lines.rs loads this through
// document::eval, so it runs in the iOS WKWebView as well as the browser).
// Rust renders each word of a word-timed line as a span carrying its start
// and end (data-s / data-e, seconds) and tells this script the playback clock
// a few times a second. Per animation frame the script writes one CSS
// variable, --rs-word-p (0..1), on the words of the active line only, plus
// data-w ("now" while a word is sung, "sung" after); player.css turns the
// variable into a soft left-to-right fill. Nothing here goes through Dioxus
// per frame, and nothing runs while paused or under reduced motion (the CSS
// then lights the whole line, as for lines without word timing).
//
// The clock is the page's audio element when there is one (web, desktop),
// read each frame. Without one (iOS and Windows play natively) it is Rust's
// last reading, moved on by the time since.
//
// window.__rsKaraoke:
//   clock(listId, seconds, playing, offset)  playback position, and the
//                        lyrics offset in seconds (positive = earlier).
//   state()              "idle" | "running", for tests.

(() => {
  if (window.__rsKaraoke) return;

  const AUDIO_IDS = ["rustysound-audio", "rustysound-audio-native"];
  const HOLD_S = 0.3;      // small backward clock steps hold instead of unfilling
  const REQUERY_MS = 100;  // how often to look for a new active line
  const reduce = window.matchMedia ? window.matchMedia("(prefers-reduced-motion: reduce)") : { matches: false };

  let listId = null;
  let list = null;
  let anchor = { at: 0, wall: 0, playing: false };
  let offset = 0;
  let lastT = -1;
  let frame = 0;
  let line = null;
  let words = [];
  let lookedAt = 0;

  function audio() {
    for (const id of AUDIO_IDS) {
      const el = document.getElementById(id);
      if (el && el.currentSrc) return el;
    }
    return null;
  }

  function position() {
    const el = audio();
    let t;
    let playing;
    if (el) {
      t = el.currentTime;
      playing = !el.paused;
    } else {
      playing = anchor.playing;
      t = playing ? anchor.at + (performance.now() - anchor.wall) / 1000 : anchor.at;
    }
    if (playing && t < lastT && lastT - t < HOLD_S) t = lastT;
    lastT = t;
    return { t, playing };
  }

  // The active line, if it is word-timed. Cached until it stops being active.
  function activeLine(now) {
    if (!list || !list.isConnected) list = listId ? document.getElementById(listId) : null;
    if (!list) return null;
    if (line && line.isConnected && line.classList.contains("rs-player-lyric--now")) return line;
    if (now - lookedAt < REQUERY_MS) return null;
    lookedAt = now;
    line = list.querySelector(".rs-player-lyric--now.rs-player-lyric--words");
    words = line
      ? Array.from(line.querySelectorAll(".rs-player-word"), (el) => ({ el, s: +el.dataset.s, e: +el.dataset.e, p: -1 }))
      : [];
    return line;
  }

  function paint(now) {
    frame = 0;
    if (reduce.matches) return;
    const { t, playing } = position();
    if (activeLine(now || performance.now())) {
      const at = t + offset;
      for (const w of words) {
        let p = at >= w.e ? 1 : at <= w.s ? 0 : (at - w.s) / (w.e - w.s);
        p = Math.round(p * 100) / 100;
        if (p === w.p) continue;
        w.p = p;
        w.el.style.setProperty("--rs-word-p", String(p));
        if (p >= 1) w.el.dataset.w = "sung";
        else if (p > 0) w.el.dataset.w = "now";
        else delete w.el.dataset.w;
      }
    }
    if (playing && list && list.isConnected) frame = requestAnimationFrame(paint);
  }

  function kick() {
    if (!frame) frame = requestAnimationFrame(paint);
  }

  window.__rsKaraoke = {
    clock(id, seconds, playing, off) {
      if (id !== listId) {
        listId = id;
        list = null;
        line = null;
        words = [];
      }
      anchor = { at: seconds, wall: performance.now(), playing };
      offset = off;
      lookedAt = 0;
      kick();
    },
    state() {
      return frame ? "running" : "idle";
    },
  };
})();
