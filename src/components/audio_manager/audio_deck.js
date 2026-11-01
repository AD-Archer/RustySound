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
// Equalizer (`setEq`): the first time it's turned on, the deck builds a Web
// Audio graph: each element -> its own GainNode (the volume) -> a shared
// pre-gain -> ten peaking BiquadFilterNodes (the bands Rust sends, from
// `crate::equalizer::EQ_BANDS`) -> the output. An element joins the graph
// ("routed") only while the AudioContext runs and its source is readable:
// same-origin, or cross-origin loaded in CORS mode (crossOrigin=anonymous).
// A server that refuses CORS fails that load; the deck notes the origin and
// plays its songs on a fresh, unrouted element instead (a routed element can
// never leave the graph), so they play without the EQ rather than silently.
// A routed stream that stays digitally silent while it plays is treated the
// same way. Turning the EQ off flattens the filters; the graph stays. While
// an element is routed its volume lives in its GainNode and `el.volume`
// stays 1, so levelling, fades and the volume slider work the same on every
// engine. A song already playing when the EQ comes on moves onto the standby
// element, routed, and the two swap without a gap ("reseat").
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
//   setEq({enabled, preamp_db, bands: [{freq, q, gain}]})
//   eqStatus()                 {wanted, supported, context, currentRouted,
//                              routed, level, note}; `note` says why the EQ
//                              isn't shaping the current song, or is null
//
// window.__rsAudioDeck.setSinkId(id) -> Promise: sends every element of every
// deck to the output `id` ("" = system default; Chromium's setSinkId). Decks
// installed later start on it too. On failure the old output stays.

(() => {
  if (window.__rsAudioDeck) return;

  const decks = {};
  // The chosen output; `window.__rsSinkId` holds a choice made before the
  // first deck existed.
  let sinkId = window.__rsSinkId || "";

  function sinkAll(elements) {
    return Promise.all(
      elements
        .filter((el) => typeof el.setSinkId === "function" && el.sinkId !== sinkId)
        .map((el) => el.setSinkId(sinkId)),
    );
  }

  // Routed audio leaves through the AudioContext, not the element.
  function sinkContexts() {
    return Promise.all(
      Object.values(decks)
        .map((d) => d.eqContext())
        .filter((ctx) => ctx && typeof ctx.setSinkId === "function" && ctx.sinkId !== sinkId)
        .map((ctx) => ctx.setSinkId(sinkId)),
    );
  }

  async function setSinkId(id) {
    const elements = Object.values(decks).flatMap((d) => [d.current(), d.standby()]);
    const before = sinkId;
    sinkId = String(id || "");
    window.__rsSinkId = sinkId;
    try {
      await sinkAll(elements);
      await sinkContexts();
    } catch (err) {
      sinkId = before;
      window.__rsSinkId = before;
      await sinkAll(elements).catch(() => {});
      await sinkContexts().catch(() => {});
      throw err;
    }
  }
  // The most a gapless start begins early.
  const MAX_LEAD_MS = 400;

  const AudioCtx = window.AudioContext || window.webkitAudioContext;
  const NOTE_UNSUPPORTED = "This browser can't run the equalizer, so music plays without it.";
  const NOTE_START = "The equalizer starts after your next tap or click.";
  const NOTE_UNREADABLE = "This server doesn't let the app process its audio, so these songs play without the equalizer.";
  // Digital silence this long, while a routed cross-origin stream plays,
  // means the browser is hiding its samples.
  const SILENT_MS = 3500;

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
    let both = [cur, next];
    if (sinkId) sinkAll(both).catch(() => {});

    let armed = null;      // { src, from, to, volume, fadeMs, meta }
    let pendingArm = null; // an arm that waits for a crossfade to finish
    let fade = null;       // { old, timer }
    let ramp = null;       // setVolume ramp timer
    let target = 1;        // the current element's volume outside fades
    const handovers = [];
    const listeners = {};

    const clamp = (v) => Math.max(0, Math.min(1, Number(v) || 0));
    // Volume goes to the element, or to its GainNode once it's routed.
    const setVol = (el, v) => {
      const x = clamp(v);
      el.__rsVol = x;
      if (el.__rsGain) {
        const ctx = el.__rsGain.context;
        el.__rsGain.gain.setTargetAtTime(x, ctx.currentTime, 0.004);
        try { if (el.volume !== 1) el.volume = 1; } catch (_) {}
        return;
      }
      try { el.volume = x; } catch (_) {}
    };
    const getVol = (el) => (el.__rsGain ? (el.__rsVol ?? 1) : el.volume);
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
      el.__rsWantPlay = false;
      el.__rsPlayedMs = 0;
      el.preload = "metadata";
      try { el.load(); } catch (_) {}
    }

    // Short fade before retiring, so stopping mid-crossfade doesn't click.
    function retireSoftly(el) {
      const from = getVol(el);
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
      const from = getVol(old);
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

    // ---- Equalizer -----------------------------------------------------

    let eq = null;         // { ctx, pre, filters }
    let eqCfg = null;      // the last setEq() config
    let eqWanted = false;
    let reseating = null;  // { el, src } while the current song moves over
    let level = 0;         // last measured peak of the current routed element
    const badOrigins = new Set();
    const badSrcs = new Set();

    function originOf(src) {
      try {
        const u = new URL(src, location.href);
        return `${u.protocol}//${u.host}`;
      } catch (_) {
        return "";
      }
    }

    // "same": Web Audio may read it as is; "cors": cross-origin http(s),
    // readable when loaded in CORS mode; "bad": known unreadable.
    function readable(src) {
      if (!src) return "bad";
      if (badSrcs.has(src)) return "bad";
      let u;
      try {
        u = new URL(src, location.href);
      } catch (_) {
        return "bad";
      }
      if (u.protocol === "blob:" || u.protocol === "data:") return "same";
      const origin = `${u.protocol}//${u.host}`;
      if (origin === `${location.protocol}//${location.host}`) return "same";
      if (badOrigins.has(origin)) return "bad";
      return u.protocol === "http:" || u.protocol === "https:" ? "cors" : "bad";
    }

    const eqRunning = () => !!eq && eq.ctx.state === "running";

    function buildGraph() {
      if (eq || !AudioCtx) return;
      let ctx;
      try {
        ctx = new AudioCtx();
      } catch (_) {
        return;
      }
      const pre = ctx.createGain();
      let tail = pre;
      const filters = [];
      for (let i = 0; i < 10; i++) {
        const f = ctx.createBiquadFilter();
        f.type = "peaking";
        f.gain.value = 0;
        tail.connect(f);
        tail = f;
        filters.push(f);
      }
      tail.connect(ctx.destination);
      if (sinkId && typeof ctx.setSinkId === "function") ctx.setSinkId(sinkId).catch(() => {});
      eq = { ctx, pre, filters };
      ctx.addEventListener("statechange", () => {
        if (ctx.state === "running") attach();
        else if (eqWanted) wake();
      });
    }

    function applyEq() {
      if (!eq || !eqCfg) return;
      const now = eq.ctx.currentTime;
      const on = !!eqCfg.enabled;
      const preamp = on ? Math.pow(10, (Number(eqCfg.preamp_db) || 0) / 20) : 1;
      eq.pre.gain.setTargetAtTime(preamp, now, 0.02);
      const bands = Array.isArray(eqCfg.bands) ? eqCfg.bands : [];
      eq.filters.forEach((f, i) => {
        const b = bands[i] || {};
        if (b.freq) f.frequency.value = b.freq;
        if (b.q) f.Q.value = b.q;
        f.gain.setTargetAtTime(on ? Number(b.gain) || 0 : 0, now, 0.02);
      });
    }

    // Resumes the context (it may need a user gesture), then routes.
    function wake() {
      if (!eq || eq.ctx.state === "running") return attach();
      const p = eq.ctx.resume();
      if (p && p.then) p.then(attach, () => {});
    }

    function onGesture() {
      if (eqWanted && eq && eq.ctx.state !== "running") wake();
    }
    document.addEventListener("pointerdown", onGesture, true);
    document.addEventListener("keydown", onGesture, true);

    function route(el) {
      if (el.__rsNode || !eqRunning()) return;
      const vol = el.volume;
      try {
        el.__rsNode = eq.ctx.createMediaElementSource(el);
      } catch (_) {
        return;
      }
      const gain = eq.ctx.createGain();
      gain.gain.value = vol;
      el.__rsNode.connect(gain);
      gain.connect(eq.pre);
      // Taps the source before the volume, for the silence check.
      const analyser = eq.ctx.createAnalyser();
      analyser.fftSize = 256;
      el.__rsNode.connect(analyser);
      el.__rsGain = gain;
      el.__rsAnalyser = analyser;
      el.__rsSamples = new Float32Array(analyser.fftSize);
      el.__rsVol = vol;
      el.__rsPlayedMs = 0;
      el.__rsSilentMs = 0;
      try { el.volume = 1; } catch (_) {}
    }

    // Sets up `el` to load `src`: CORS mode when the source needs it, and
    // routed while the EQ is wanted. A routed element can't load an
    // unreadable source, so it's swapped for a fresh one. Returns the
    // element to load into.
    function prepare(el, src) {
      const r = readable(src);
      if (el.__rsNode && r === "bad") el = fresh(el);
      if (el.__rsNode || (eqWanted && eqRunning() && r !== "bad")) {
        if (r === "cors") el.crossOrigin = "anonymous";
        else el.removeAttribute("crossorigin");
        route(el);
      } else if (el.hasAttribute("crossorigin")) {
        el.removeAttribute("crossorigin");
      }
      return el;
    }

    // A new, unrouted element in `old`'s place (same id, listeners, output,
    // speed and volume). `old` is emptied.
    function fresh(old) {
      const el = document.createElement("audio");
      for (const name of ["preload", "style", "playsinline", "webkit-playsinline", "x-webkit-airplay"]) {
        const v = old.getAttribute(name);
        if (v !== null) el.setAttribute(name, v);
      }
      el.loop = old.loop;
      carrySpeed(old, el);
      try { el.volume = getVol(old); } catch (_) {}
      if (sinkId && typeof el.setSinkId === "function") el.setSinkId(sinkId).catch(() => {});
      const oldId = old.id;
      old.id = `${id}-retired`;
      el.id = oldId;
      old.replaceWith(el);
      wire(el);
      if (old === cur) cur = el;
      if (old === next) next = el;
      both = [cur, next];
      retire(old);
      return el;
    }

    // The source can't be read: play it again on an unrouted element, from
    // where it was.
    function fallBack(el, src) {
      const at = el.currentTime || 0;
      const wanted = !el.paused || !!el.__rsWantPlay;
      const target = el.__rsNode ? fresh(el) : el;
      target.removeAttribute("crossorigin");
      target.src = src;
      target.__rsSrc = src;
      if (at > 0) {
        target.addEventListener("loadedmetadata", () => {
          try { target.currentTime = at; } catch (_) {}
        }, { once: true });
      }
      if (wanted) play(target);
      if (target === next && armed && armed.src === src) {
        setVol(target, 0);
        try {
          target.defaultPlaybackRate = 0;
          target.playbackRate = 0;
        } catch (_) {}
      }
      return target;
    }

    // The song playing now moves onto the standby element, routed, then the
    // two swap. Skipped while the standby is busy (armed or fading): the
    // next song is routed when it loads.
    function reseat() {
      if (reseating || armed || fade || !cur.__rsSrc || cur.__rsNode) return;
      const src = cur.__rsSrc;
      if (readable(src) === "bad") return;
      let incoming = next;
      if (incoming.__rsRetire) retire(incoming);
      incoming = prepare(incoming, src);
      if (!incoming.__rsNode) return;
      const job = { el: incoming, src };
      reseating = job;
      const live = () => reseating === job && cur.__rsSrc === src && next === incoming;
      carrySpeed(cur, incoming);
      setVol(incoming, 0);
      incoming.preload = "auto";
      incoming.src = src;
      incoming.__rsSrc = src;
      const finish = () => {
        if (!live()) return;
        reseating = null;
        const old = cur;
        swap();
        setVol(cur, target);
        retireSoftly(old);
      };
      incoming.addEventListener("canplay", () => {
        if (!live()) return;
        if (cur.paused) {
          try { incoming.currentTime = cur.currentTime; } catch (_) {}
          finish();
          return;
        }
        // Aim for where the old one will be once this one is up.
        try { incoming.currentTime = cur.currentTime + (startLatency || 150) / 1000; } catch (_) {}
        incoming.addEventListener("playing", finish, { once: true });
        play(incoming);
      }, { once: true });
      incoming.addEventListener("error", () => {
        if (reseating === job) reseating = null;
      }, { once: true });
    }

    function cancelReseat() {
      if (!reseating) return;
      const el = reseating.el;
      reseating = null;
      if (el === next) retire(el);
    }

    // Routes what's loaded, once the context runs.
    function attach() {
      if (!eqWanted || !eqRunning()) return;
      if (cur.__rsSrc && !cur.__rsNode) {
        if (readable(cur.__rsSrc) === "same") route(cur);
        else reseat();
      }
      if (armed && !next.__rsNode && !reseating && readable(armed.src) !== "bad") {
        const o = armed;
        armed = null;
        retire(next);
        deck.arm(o);
      }
    }

    // Twice a second while something is routed: the level of the current
    // element, and the silence check for cross-origin and custom-scheme
    // sources (same-origin http(s) can't be hidden, so true silence there
    // is just a quiet intro).
    setInterval(() => {
      if (!eq) return;
      const el = cur;
      if (!el.__rsNode || el.paused || !el.__rsSrc || eq.ctx.state !== "running") return;
      const before = el.__rsLastTime || 0;
      el.__rsLastTime = el.currentTime;
      if (el.currentTime <= before) return;
      const data = el.__rsSamples;
      el.__rsAnalyser.getFloatTimeDomainData(data);
      let peak = 0;
      for (let i = 0; i < data.length; i++) peak = Math.max(peak, Math.abs(data[i]));
      level = peak;
      el.__rsPlayedMs = (el.__rsPlayedMs || 0) + 500;
      el.__rsSilentMs = peak === 0 ? (el.__rsSilentMs || 0) + 500 : 0;
      let scheme = "";
      try { scheme = new URL(el.__rsSrc, location.href).protocol; } catch (_) {}
      const hidden = scheme !== "blob:" && scheme !== "data:" &&
        (readable(el.__rsSrc) === "cors" || (scheme !== "http:" && scheme !== "https:"));
      if (hidden && el.__rsSilentMs >= SILENT_MS && el.__rsPlayedMs <= SILENT_MS + 1000) {
        badSrcs.add(el.__rsSrc);
        fallBack(el, el.__rsSrc);
      }
    }, 500);

    // ---- Events ----------------------------------------------------------

    // The deck's own listeners, and every `deck.on` type so far.
    function wire(el) {
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
        // A failing load can leave the element paused; only a real pause
        // (of a loaded song) cancels the wish to play, for `fallBack`.
        if (el.readyState > 0 && !el.error) el.__rsWantPlay = false;
      });
      el.addEventListener("play", () => {
        el.__rsWantPlay = true;
      });
      // A CORS-mode load the server refused: note its origin and play the
      // song without the EQ. Registered before any `deck.on` listener, so
      // Rust never hears about it.
      el.addEventListener("error", (event) => {
        const src = el.__rsSrc;
        if (!src || !el.hasAttribute("crossorigin") || readable(src) !== "cors") return;
        event.stopImmediatePropagation();
        badOrigins.add(originOf(src));
        if (reseating && reseating.el === el) {
          reseating = null;
          retire(el);
          return;
        }
        fallBack(el, src);
      });
      for (const type of Object.keys(listeners)) {
        el.addEventListener(type, (event) => {
          if (el === cur) for (const f of listeners[type]) f(event, el);
        });
      }
    }

    for (const el of both) wire(el);

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
        cancelReseat();
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
          const el = prepare(cur, src);
          el.src = src;
          el.__rsSrc = src;
          el.__rsPlayedMs = 0;
          el.__rsSilentMs = 0;
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
        cancelReseat();
        armed = { ...o };
        // A soft stop still running on this element would empty it.
        if (next.__rsRetire) retire(next);
        const el = prepare(next, o.src);
        el.preload = "auto";
        el.src = o.src;
        el.__rsSrc = o.src;
        el.__rsPlayedMs = 0;
        el.__rsSilentMs = 0;
        // "Playing" at rate 0 and volume 0: silent, but the engine has its
        // pipeline up, so the real start is quicker (WebKit: ~90 ms less).
        setVol(el, 0);
        try {
          el.defaultPlaybackRate = 0;
          el.playbackRate = 0;
          play(el);
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
        const from = getVol(cur);
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
        onGesture();
      },
      clear() {
        endFade(false);
        cancelReseat();
        deck.disarm();
        stopRamp();
        retire(cur);
      },
      setEq(cfg) {
        eqCfg = cfg || null;
        eqWanted = !!(cfg && cfg.enabled);
        if (eqWanted) buildGraph();
        applyEq();
        if (eqWanted) wake();
      },
      eqContext: () => (eq ? eq.ctx : null),
      eqStatus() {
        const src = cur.__rsSrc;
        let note = null;
        if (eqWanted) {
          if (!AudioCtx || !eq) note = NOTE_UNSUPPORTED;
          else if (eq.ctx.state !== "running") note = NOTE_START;
          else if (src && !cur.__rsNode && readable(src) === "bad") note = NOTE_UNREADABLE;
        }
        return {
          wanted: eqWanted,
          supported: !!AudioCtx,
          context: eq ? eq.ctx.state : "none",
          currentRouted: !!cur.__rsNode,
          routed: both.filter((el) => el.__rsNode).length,
          level,
          note,
        };
      },
    };

    decks[id] = deck;
    return deck;
  }

  window.__rsAudioDeck = { install, setSinkId };
})();
