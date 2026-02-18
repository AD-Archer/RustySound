// Desktop-webview JavaScript bridge used by non-wasm native targets.
/// Audio controller hook - manages playback imperatively.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "windows")
))]
const NATIVE_AUDIO_BOOTSTRAP_JS: &str = r#"
(() => {
  if (window.__rustysoundAudioBridge) {
    return true;
  }

  const existing = document.getElementById("rustysound-audio-native");
  const audio = existing || document.createElement("audio");
  if (!existing) {
    audio.id = "rustysound-audio-native";
    audio.preload = "metadata";
    audio.style.display = "none";
    audio.setAttribute("playsinline", "true");
    audio.setAttribute("webkit-playsinline", "true");
    audio.setAttribute("x-webkit-airplay", "allow");
    document.body.appendChild(audio);
  }

  const safePlay = async () => {
    try {
      await audio.play();
    } catch (_err) {}
  };

  let isLiveStream = false;

  const setMetadata = (meta) => {
    if (!meta || !("mediaSession" in navigator) || typeof MediaMetadata === "undefined") {
      return;
    }

    isLiveStream = !!meta.is_live;

    const artwork = meta.artwork
      ? [{ src: meta.artwork, sizes: "512x512", type: "image/png" }]
      : undefined;

    try {
      navigator.mediaSession.metadata = new MediaMetadata({
        title: meta.title || "",
        artist: meta.artist || "",
        album: meta.album || "",
        artwork,
      });
    } catch (_err) {}
  };

  const setPlaybackState = () => {
    if (!("mediaSession" in navigator)) return;
    try {
      navigator.mediaSession.playbackState = audio.paused ? "paused" : "playing";
    } catch (_err) {}
  };

  const updatePositionState = () => {
    if (!("mediaSession" in navigator) || !navigator.mediaSession.setPositionState) {
      return;
    }
    if (isLiveStream || !Number.isFinite(audio.duration) || audio.duration <= 0) {
      try {
        navigator.mediaSession.setPositionState();
      } catch (_err) {}
      return;
    }
    try {
      navigator.mediaSession.setPositionState({
        duration: audio.duration,
        playbackRate: audio.playbackRate || 1,
        position: Math.max(0, Math.min(audio.currentTime || 0, audio.duration)),
      });
    } catch (_err) {}
  };

  const isEditableTarget = (target) => {
    let element = target;
    while (element && element.tagName) {
      const tag = (element.tagName || "").toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") {
        return true;
      }

      const contentEditable = element.getAttribute && element.getAttribute("contenteditable");
      if (contentEditable !== null && String(contentEditable).toLowerCase() !== "false") {
        return true;
      }

      element = element.parentElement || null;
    }
    return false;
  };

  const bridge = {
    audio,
    currentSongId: null,
    remoteActions: [],
    apply(cmd) {
      if (!cmd || !cmd.type) return;

      switch (cmd.type) {
        case "load":
          if (cmd.src && audio.src !== cmd.src) {
            audio.src = cmd.src;
          }
          if (typeof cmd.volume === "number") {
            audio.volume = Math.max(0, Math.min(1, cmd.volume));
          }
          if (typeof cmd.position === "number" && Number.isFinite(cmd.position)) {
            try {
              audio.currentTime = Math.max(0, cmd.position);
            } catch (_err) {}
          }
          bridge.currentSongId = cmd.song_id || null;
          setMetadata(cmd.meta || null);
          updatePositionState();
          if (cmd.play === true) {
            safePlay();
          } else if (cmd.play === false) {
            audio.pause();
          }
          setPlaybackState();
          break;
        case "play":
          safePlay();
          setPlaybackState();
          break;
        case "pause":
          audio.pause();
          setPlaybackState();
          break;
        case "seek":
          if (typeof cmd.position === "number" && Number.isFinite(cmd.position)) {
            try {
              audio.currentTime = Math.max(0, cmd.position);
            } catch (_err) {}
          }
          updatePositionState();
          break;
        case "volume":
          if (typeof cmd.value === "number") {
            audio.volume = Math.max(0, Math.min(1, cmd.value));
          }
          break;
        case "loop":
          audio.loop = !!cmd.enabled;
          break;
        case "metadata":
          setMetadata(cmd.meta || null);
          break;
        case "clear":
          audio.pause();
          audio.removeAttribute("src");
          audio.load();
          bridge.currentSongId = null;
          isLiveStream = false;
          if ("mediaSession" in navigator) {
            try {
              navigator.mediaSession.metadata = null;
              navigator.mediaSession.playbackState = "none";
              if (navigator.mediaSession.setPositionState) {
                navigator.mediaSession.setPositionState();
              }
            } catch (_err) {}
          }
          break;
      }
    },
    snapshot() {
      const duration = Number.isFinite(audio.duration) ? audio.duration : 0;
      return {
        current_time: Number.isFinite(audio.currentTime) ? audio.currentTime : 0,
        duration,
        paused: !!audio.paused,
        ended: !!audio.ended,
        song_id: bridge.currentSongId,
        action: bridge.remoteActions.shift() || null,
      };
    },
  };

  const pushRemoteAction = (action) => {
    if (!action) return;
    bridge.remoteActions.push(action);
  };

  const handleShortcutKeyDown = (event) => {
    if (!event || event.defaultPrevented || event.isComposing) return;
    if (isEditableTarget(event.target)) return;

    const key = event.key || "";
    const code = event.code || "";
    const keyCode = event.keyCode || event.which || 0;
    const metaOrCtrl = !!(event.metaKey || event.ctrlKey);

    if (
      key === "MediaTrackNext" ||
      key === "MediaNextTrack" ||
      key === "AudioTrackNext" ||
      key === "AudioNext" ||
      key === "NextTrack" ||
      code === "MediaTrackNext" ||
      key === "F9" ||
      keyCode === 176
    ) {
      event.preventDefault();
      pushRemoteAction("next");
      return;
    }
    if (
      key === "MediaTrackPrevious" ||
      key === "MediaPreviousTrack" ||
      code === "MediaTrackPrevious" ||
      key === "AudioTrackPrevious" ||
      key === "AudioPrev" ||
      key === "PreviousTrack" ||
      key === "F7" ||
      keyCode === 177
    ) {
      event.preventDefault();
      pushRemoteAction("previous");
      return;
    }
    if (
      key === "MediaPlayPause" ||
      code === "MediaPlayPause" ||
      key === "AudioPlay" ||
      key === "AudioPause" ||
      key === "F8" ||
      keyCode === 179
    ) {
      event.preventDefault();
      pushRemoteAction("toggle_play");
      return;
    }

    if (metaOrCtrl && !event.altKey && !event.shiftKey) {
      if (key === "ArrowRight") {
        event.preventDefault();
        pushRemoteAction("next");
        return;
      }
      if (key === "ArrowLeft") {
        event.preventDefault();
        pushRemoteAction("previous");
        return;
      }
    }

    if (!event.metaKey && !event.ctrlKey && !event.altKey) {
      if (key === " " || key === "Spacebar" || code === "Space") {
        event.preventDefault();
        pushRemoteAction("toggle_play");
      }
    }
  };

  if ("mediaSession" in navigator) {
    const session = navigator.mediaSession;
    try {
      session.setActionHandler("play", () => {
        safePlay();
        pushRemoteAction("play");
      });
    } catch (_err) {}
    try {
      session.setActionHandler("pause", () => {
        audio.pause();
        pushRemoteAction("pause");
      });
    } catch (_err) {}
    try {
      session.setActionHandler("seekto", (details) => {
        if (isLiveStream || !Number.isFinite(audio.duration) || audio.duration <= 0) {
          return;
        }
        if (details && typeof details.seekTime === "number") {
          try {
            audio.currentTime = Math.max(0, details.seekTime);
          } catch (_err) {}
          updatePositionState();
        }
      });
    } catch (_err) {}
    try {
      session.setActionHandler("nexttrack", () => {
        if (isLiveStream) return;
        bridge.remoteActions.push("next");
      });
    } catch (_err) {}
    try {
      session.setActionHandler("previoustrack", () => {
        if (isLiveStream) return;
        bridge.remoteActions.push("previous");
      });
    } catch (_err) {}
    try {
      // Map macOS +/- controls to track skip when present.
      session.setActionHandler("seekforward", () => {
        if (isLiveStream) return;
        bridge.remoteActions.push("next");
      });
    } catch (_err) {}
    try {
      session.setActionHandler("seekbackward", () => {
        if (isLiveStream) return;
        bridge.remoteActions.push("previous");
      });
    } catch (_err) {}
  }

  audio.addEventListener("timeupdate", updatePositionState);
  audio.addEventListener("durationchange", updatePositionState);
  audio.addEventListener("ratechange", updatePositionState);
  audio.addEventListener("play", () => {
    setPlaybackState();
    bridge.remoteActions.push("play");
  });
  audio.addEventListener("pause", () => {
    setPlaybackState();
    bridge.remoteActions.push("pause");
  });
  audio.addEventListener("ended", () => bridge.remoteActions.push("ended"));
  document.addEventListener("keydown", handleShortcutKeyDown, true);

  window.__rustysoundAudioBridge = bridge;
  return true;
})();
"#;

