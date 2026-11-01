// Two <audio> elements for gapless playback and crossfades, used by the web
// build (wasm, through web-sys) and the desktop webview bridge
// (native_bridge_bootstrap.rs). This script is the one place that knows which
// element is current.
//
// The current element always carries the id passed to `install` (for example
// "rustysound-audio"); the standby one carries that id plus "-next". Rust,
// tests, the lyrics karaoke script and the MediaSession glue look the current
// element up by id each time, or listen through `deck.on`, which only passes
// on events from the current element.
//
// Near the end of a song Rust arms the standby element with the next song
// (`arm`; preload="auto", one song ahead), which then idles "playing" at rate
// 0 so its pipeline is warm. When the current song ends, the standby one
// starts at once and the ids swap ("handover"); once the engine's start-up
// delay is known, the start moves that much earlier. With a crossfade
// armed, the swap happens `fadeMs` before the end instead and the two
// elements overlap, the old one fading out while the new one fades in
// (equal-power curves). Each handover is reported as "from|to" (the song ids
// given to `arm`): `takeHandover()` for the web controller's poll, or the
// `onHandover` callback for the desktop bridge.
//
// window.__rsAudioDeck.install(id) -> deck (idempotent):
//   current() / standby()      the elements
//   on(type, fn)               fn(event, el) for events of the current element
//   load(src)                  plays `src` on the current element (takes the
//                              armed standby when it holds `src`); disarms
//   arm({src, from, to, volume, fadeMs, meta}) / disarm()
//   setVolume(v, rampMs)       the current element's volume, ramped when asked
//   takeHandover()             oldest unread "from|to", or null
//   unlock()                   inside a user gesture: lifts per-element
//                              autoplay limits on both elements
//   clear()                    stops and empties both elements

(() => {
  if (window.__rsAudioDeck) return;

  const decks = {};
  // The most a gapless start begins early.
  const MAX_LEAD_MS = 400;

  function make(id) {
    const el = document.createElement("audio");
    el.id = id;
    el.preload = "metadata";
    el.style.display = "none";
    el.setAttribute("playsinline", "true");
    el.setAttribute("webkit-playsinline", "true");
    el.setAttribute("x-webkit-airplay", "allow");
    document.body.appendChild(el);
    return el;
  }

  function install(id) {
    if (decks[id]) return decks[id];
    const nextId = `${id}-next`;
    let cur = document.getElementById(id) || make(id);
    let next = document.getElementById(nextId) || make(nextId);
    const both = [cur, next];

    let armed = null;      // { src, from, to, volume, fadeMs, meta }
    let pendingArm = null; // an arm that waits for a crossfade to finish
    let fade = null;       // { old, timer }
    let ramp = null;       // setVolume ramp timer
    let target = 1;        // the current element's volume outside fades
    const handovers = [];
    const listeners = {};

    const clamp = (v) => Math.max(0, Math.min(1, Number(v) || 0));
    const setVol = (el, v) => {
      try { el.volume = clamp(v); } catch (_) {}
    };
    const play = (el) => {
      const p = el.play();
      if (p && p.catch) p.catch(() => {});
    };
    const stopRamp = () => {
      if (ramp) clearInterval(ramp);
      ramp = null;
    };

    function swap() {
      // Through a third id, so two elements never share `id`.
      cur.id = `${id}-swap`;
      next.id = id;
      cur.id = nextId;
      [cur, next] = [next, cur];
    }

    // Speed and pitch follow the song onto the other element.
    function carrySpeed(from, to) {
      to.defaultPlaybackRate = from.defaultPlaybackRate;
      to.playbackRate = from.playbackRate;
      to.preservesPitch = true;
      to.webkitPreservesPitch = true;
    }

    function retire(el) {
      if (el.__rsRetire) clearInterval(el.__rsRetire);
      el.__rsRetire = null;
      el.pause();
      el.removeAttribute("src");
      el.__rsSrc = null;
      el.preload = "metadata";
      try { el.load(); } catch (_) {}
    }

    // Short fade before retiring, so stopping mid-crossfade doesn't click.
    function retireSoftly(el) {
      const from = el.volume;
      const t0 = performance.now();
      if (el.__rsRetire) clearInterval(el.__rsRetire);
      el.__rsRetire = setInterval(() => {
        const t = Math.min(1, (performance.now() - t0) / 60);
        setVol(el, from * (1 - t));
        if (t >= 1) retire(el);
      }, 15);
    }

    function endFade(soft) {
      if (!fade) return;
      clearInterval(fade.timer);
      const old = fade.old;
      fade = null;
      if (soft) retireSoftly(old);
      else retire(old);
      setVol(cur, target);
      if (pendingArm) {
        const o = pendingArm;
        pendingArm = null;
        deck.arm(o);
      }
    }

    function startFade(old, incoming, ms) {
      const from = old.volume;
      const t0 = performance.now();
      const step = () => {
        const t = Math.min(1, (performance.now() - t0) / ms);
        setVol(old, from * Math.cos((t * Math.PI) / 2));
        setVol(incoming, target * Math.sin((t * Math.PI) / 2));
        if (t >= 1) endFade(false);
      };
      fade = { old, timer: setInterval(step, 30) };
    }

    // How long an element takes from play() to a moving clock (WebKit's
    // AVFoundation pipeline: 100-300 ms; Chromium: tens). Measured on each
    // gapless start; later starts begin part of that early (`lead`), while the
    // old song plays out its last moments. Only part, because the delay
    // varies and a short overlap is worse than a short gap.
    let startLatency = 0;
    let earlyTimer = null;
    const lead = () => startLatency * 0.6;

    function measureStart(el) {
      const t0 = performance.now();
      const timer = setInterval(() => {
        const ms = performance.now() - t0;
        if (el.currentTime > 0 || ms > 1000) {
          clearInterval(timer);
          if (el.currentTime > 0) {
            const sample = Math.min(MAX_LEAD_MS, ms);
            startLatency = startLatency ? (startLatency + sample) / 2 : sample;
          }
        }
      }, 10);
    }

    // `tailMs`: the old song still has this long to play (an early start).
    function handover(fadeMs, tailMs) {
      if (earlyTimer) clearTimeout(earlyTimer);
      earlyTimer = null;
      const info = armed;
      armed = null;
      const old = cur;
      const incoming = next;
      stopRamp();
      carrySpeed(old, incoming);
      target = clamp(info.volume);
      setVol(incoming, fadeMs > 0 ? 0 : target);
      swap();
      play(incoming);
      if (deck.onHandover) deck.onHandover(info);
      else handovers.push(`${info.from}|${info.to}`);
      if (fadeMs > 0) {
        startFade(old, incoming, fadeMs);
        return;
      }
      measureStart(incoming);
      if (tailMs > 0) fade = { old, timer: setTimeout(() => endFade(false), tailMs + 100) };
      else retire(old);
    }

    const standbyUsable = () => !!armed && !next.error && next.__rsSrc === armed.src;

    for (const el of both) {
      el.addEventListener("ended", () => {
        if (fade && fade.old === el) endFade(false);
        else if (el === cur && standbyUsable()) handover(0, 0);
      });
      el.addEventListener("timeupdate", () => {
        if (el !== cur || fade || earlyTimer || !standbyUsable()) return;
        const left = ((el.duration - el.currentTime) / (el.playbackRate || 1)) * 1000;
        if (!Number.isFinite(left) || left <= 0) return;
        if (armed.fadeMs > 0) {
          if (left <= armed.fadeMs) handover(left, 0);
        } else if (lead() > 15 && left <= lead() + 400) {
          // timeupdate comes every ~250 ms: aim the early start with a timer.
          earlyTimer = setTimeout(() => {
            earlyTimer = null;
            const rest = ((el.duration - el.currentTime) / (el.playbackRate || 1)) * 1000;
            // Not after a pause or a seek back.
            if (el === cur && !el.paused && standbyUsable() && rest <= lead() + 100) {
              handover(0, Math.max(0, rest));
            }
          }, Math.max(0, left - lead()));
        }
      });
      // Pausing (or a new song) mid-crossfade stops the old song too.
      el.addEventListener("pause", () => {
        if (el === cur && fade) endFade(true);
      });
    }

    const deck = {
      onHandover: null,
      current: () => cur,
      standby: () => next,
      on(type, fn) {
        if (!listeners[type]) {
          listeners[type] = [];
          for (const el of both) {
            el.addEventListener(type, (event) => {
              if (el === cur) for (const f of listeners[type]) f(event, el);
            });
          }
        }
        listeners[type].push(fn);
      },
      load(src) {
        endFade(true);
        if (standbyUsable() && armed.src === src) {
          // A skip to the song that's already buffered: switch over now.
          armed = null;
          const old = cur;
          carrySpeed(old, next);
          swap();
          retire(old);
          return cur;
        }
        deck.disarm();
        if (cur.__rsSrc !== src || !cur.getAttribute("src")) {
          cur.src = src;
          cur.__rsSrc = src;
        }
        return cur;
      },
      arm(o) {
        if (!o || !o.src) return deck.disarm();
        if (fade) {
          pendingArm = o;
          return;
        }
        if (armed && armed.src === o.src) {
          armed = { ...o };
          return;
        }
        armed = { ...o };
        // A soft stop still running on this element would empty it.
        if (next.__rsRetire) retire(next);
        next.preload = "auto";
        next.src = o.src;
        next.__rsSrc = o.src;
        // "Playing" at rate 0 and volume 0: silent, but the engine has its
        // pipeline up, so the real start is quicker (WebKit: ~90 ms less).
        setVol(next, 0);
        try {
          next.defaultPlaybackRate = 0;
          next.playbackRate = 0;
          play(next);
        } catch (_) {}
      },
      disarm() {
        pendingArm = null;
        if (earlyTimer) clearTimeout(earlyTimer);
        earlyTimer = null;
        if (!armed) return;
        armed = null;
        if (!fade) retire(next);
      },
      armedSrc: () => (armed ? armed.src : null),
      setVolume(v, rampMs) {
        target = clamp(v);
        stopRamp();
        if (fade) return; // the fade steps read `target`
        const ms = Number(rampMs) || 0;
        if (ms <= 0) {
          setVol(cur, target);
          return;
        }
        const from = cur.volume;
        const el = cur;
        const t0 = performance.now();
        ramp = setInterval(() => {
          const t = Math.min(1, (performance.now() - t0) / ms);
          setVol(el, from + (target - from) * t);
          if (t >= 1) stopRamp();
        }, 15);
      },
      takeHandover: () => handovers.shift() || null,
      unlock() {
        for (const el of both) {
          if (el.getAttribute("src") || !el.paused) continue;
          play(el);
          el.pause();
        }
      },
      clear() {
        endFade(false);
        deck.disarm();
        stopRamp();
        retire(cur);
      },
    };

    decks[id] = deck;
    return deck;
  }

  window.__rsAudioDeck = { install };
})();
