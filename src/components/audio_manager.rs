//! Audio Manager - Handles audio playback outside of the component render cycle.
//! Keeps audio side-effects isolated and defers signal writes to avoid borrow loops.
#![cfg_attr(
    all(not(target_arch = "wasm32"), target_os = "ios"),
    allow(unexpected_cfgs)
)]

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::api::*;
#[cfg(target_arch = "wasm32")]
use crate::components::{PlaybackPositionSignal, SeekRequestSignal, VolumeSignal};
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{PlaybackPositionSignal, SeekRequestSignal, VolumeSignal};
#[cfg(target_arch = "wasm32")]
use crate::db::{AppSettings, RepeatMode};
#[cfg(not(target_arch = "wasm32"))]
use crate::db::{AppSettings, RepeatMode};

#[cfg(target_arch = "wasm32")]
use js_sys;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use objc::declare::ClassDecl;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use objc::runtime::{Object, BOOL, YES};
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use objc::{class, msg_send, sel, sel_impl};
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use rand::seq::SliceRandom;
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::collections::VecDeque;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::ptr;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::sync::{Mutex, Once};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlAudioElement, HtmlElement, KeyboardEvent};

/// Global audio state that persists across renders.
#[derive(Clone)]
pub struct AudioState {
    pub current_time: Signal<f64>,
    pub duration: Signal<f64>,
    pub playback_error: Signal<Option<String>>,
    #[allow(dead_code)]
    pub is_initialized: Signal<bool>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            current_time: Signal::new(0.0),
            duration: Signal::new(0.0),
            playback_error: Signal::new(None),
            is_initialized: Signal::new(false),
        }
    }
}

/// Initialize the global audio element once.
#[cfg(target_arch = "wasm32")]
pub fn get_or_create_audio_element() -> Option<HtmlAudioElement> {
    let document = window()?.document()?;

    if let Some(existing) = document.get_element_by_id("rustysound-audio") {
        return existing.dyn_into::<HtmlAudioElement>().ok();
    }

    let audio: HtmlAudioElement = document.create_element("audio").ok()?.dyn_into().ok()?;
    audio.set_id("rustysound-audio");
    audio.set_attribute("preload", "metadata").ok()?;
    document.body()?.append_child(&audio).ok()?;

    Some(audio)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_or_create_audio_element() -> Option<()> {
    None
}

#[cfg(target_arch = "wasm32")]
fn web_playback_error_message(audio: &HtmlAudioElement, song: Option<&Song>) -> Option<String> {
    let audio_js = wasm_bindgen::JsValue::from(audio.clone());
    let error_js = js_sys::Reflect::get(&audio_js, &"error".into()).ok()?;
    if error_js.is_null() || error_js.is_undefined() {
        return None;
    }
    let code = js_sys::Reflect::get(&error_js, &"code".into())
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0) as u16;
    let is_radio = song.map(|s| s.server_name == "Radio").unwrap_or(false);
    let station_name = song
        .and_then(|s| s.album.clone().or_else(|| s.artist.clone()))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "this station".to_string());

    Some(match code {
        1 => "Playback was aborted before the stream loaded.".to_string(),
        2 => {
            if is_radio {
                format!("No station found: \"{station_name}\" is unreachable right now.")
            } else {
                "Network error while loading this track.".to_string()
            }
        }
        3 => "Audio playback failed due to a decode error.".to_string(),
        4 => {
            if is_radio {
                format!("No station found: \"{station_name}\" has no supported stream source.")
            } else {
                "Failed to load audio because no supported source was found.".to_string()
            }
        }
        _ => {
            if is_radio {
                format!("No station found: \"{station_name}\" could not be loaded.")
            } else {
                "Unable to load this audio source.".to_string()
            }
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn defer_signal_update<F>(f: F)
where
    F: FnOnce() + 'static,
{
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        f();
    });
}

#[cfg(target_arch = "wasm32")]
fn is_editable_shortcut_target(event: &KeyboardEvent) -> bool {
    let Some(target) = event.target() else {
        return false;
    };

    let mut current = target.dyn_into::<web_sys::Element>().ok();
    while let Some(element) = current {
        let tag = element.tag_name().to_ascii_lowercase();
        if tag == "input" || tag == "textarea" || tag == "select" {
            return true;
        }
        if element.has_attribute("contenteditable")
            && element
                .get_attribute("contenteditable")
                .map(|v| v.to_ascii_lowercase() != "false")
                .unwrap_or(true)
        {
            return true;
        }
        current = element.parent_element();
    }

    false
}

#[cfg(target_arch = "wasm32")]
fn shortcut_action_from_key(event: &KeyboardEvent) -> Option<&'static str> {
    if event.default_prevented() || event.is_composing() || is_editable_shortcut_target(event) {
        return None;
    }

    let key = event.key();
    let code = event.code();
    let key_code = event.key_code();
    let meta_or_ctrl = event.meta_key() || event.ctrl_key();

    if key == "MediaTrackNext"
        || key == "MediaNextTrack"
        || key == "AudioTrackNext"
        || key == "AudioNext"
        || key == "NextTrack"
        || code == "MediaTrackNext"
        || key == "F9"
        || key_code == 176
    {
        return Some("next");
    }
    if key == "MediaTrackPrevious"
        || key == "MediaPreviousTrack"
        || code == "MediaTrackPrevious"
        || key == "AudioTrackPrevious"
        || key == "AudioPrev"
        || key == "PreviousTrack"
        || key == "F7"
        || key_code == 177
    {
        return Some("previous");
    }
    if key == "MediaPlayPause"
        || code == "MediaPlayPause"
        || key == "AudioPlay"
        || key == "AudioPause"
        || key == "F8"
        || key_code == 179
    {
        return Some("toggle_play");
    }

    if meta_or_ctrl && !event.alt_key() && !event.shift_key() {
        if key == "ArrowRight" {
            return Some("next");
        }
        if key == "ArrowLeft" {
            return Some("previous");
        }
    }

    if !event.meta_key()
        && !event.ctrl_key()
        && !event.alt_key()
        && (key == " " || key == "Spacebar" || code == "Space")
    {
        return Some("toggle_play");
    }

    None
}

#[cfg(target_arch = "wasm32")]
fn click_player_control_button(id: &str) {
    if let Some(doc) = window().and_then(|w| w.document()) {
        if let Some(element) = doc.get_element_by_id(id) {
            if let Ok(html) = element.dyn_into::<HtmlElement>() {
                html.click();
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn ensure_web_media_session_shortcuts() {
    let _ = js_sys::eval(
        r#"
(() => {
  if (window.__rustysoundWebMediaSessionInit) {
    return true;
  }

  const audio = document.getElementById("rustysound-audio");
  if (!audio) {
    return false;
  }

  if (!("mediaSession" in navigator)) {
    window.__rustysoundWebMediaSessionInit = true;
    return true;
  }

  const clickById = (id) => {
    const element = document.getElementById(id);
    if (element && typeof element.click === "function") {
      element.click();
    }
  };

  const updatePlaybackState = () => {
    try {
      navigator.mediaSession.playbackState = audio.paused ? "paused" : "playing";
    } catch (_err) {}
  };

  const updatePositionState = () => {
    if (!navigator.mediaSession.setPositionState) return;
    if (!Number.isFinite(audio.duration) || audio.duration <= 0) return;
    try {
      navigator.mediaSession.setPositionState({
        duration: audio.duration,
        playbackRate: audio.playbackRate || 1,
        position: Math.max(0, Math.min(audio.currentTime || 0, audio.duration)),
      });
    } catch (_err) {}
  };

  try {
    navigator.mediaSession.setActionHandler("play", () => {
      audio.play().catch(() => {});
    });
  } catch (_err) {}
  try {
    navigator.mediaSession.setActionHandler("pause", () => audio.pause());
  } catch (_err) {}
  try {
    navigator.mediaSession.setActionHandler("nexttrack", () => clickById("next-btn"));
  } catch (_err) {}
  try {
    navigator.mediaSession.setActionHandler("previoustrack", () => clickById("prev-btn"));
  } catch (_err) {}
  try {
    navigator.mediaSession.setActionHandler("seekto", (details) => {
      if (details && typeof details.seekTime === "number") {
        try {
          audio.currentTime = Math.max(0, details.seekTime);
        } catch (_err) {}
        updatePositionState();
      }
    });
  } catch (_err) {}

  audio.addEventListener("play", updatePlaybackState);
  audio.addEventListener("pause", updatePlaybackState);
  audio.addEventListener("timeupdate", updatePositionState);
  audio.addEventListener("durationchange", updatePositionState);
  audio.addEventListener("ratechange", updatePositionState);

  window.__rustysoundWebMediaSessionInit = true;
  return true;
})();
"#,
    );
}

/// Audio controller hook - manages playback imperatively.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
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

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct NativeAudioSnapshot {
    current_time: f64,
    duration: f64,
    paused: bool,
    ended: bool,
    #[serde(default)]
    action: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Deserialize, Serialize)]
struct NativeTrackMetadata {
    title: String,
    artist: String,
    album: String,
    artwork: Option<String>,
    duration: f64,
    is_live: bool,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMTimeMakeWithSeconds(seconds: f64, preferred_timescale: i32) -> CMTime;
    fn CMTimeGetSeconds(time: CMTime) -> f64;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[link(name = "MediaPlayer", kind = "framework")]
unsafe extern "C" {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe extern "C" {
    static MPMediaItemPropertyTitle: *mut Object;
    static MPMediaItemPropertyArtist: *mut Object;
    static MPMediaItemPropertyAlbumTitle: *mut Object;
    static MPMediaItemPropertyArtwork: *mut Object;
    static MPMediaItemPropertyPlaybackDuration: *mut Object;
    static MPNowPlayingInfoPropertyElapsedPlaybackTime: *mut Object;
    static MPNowPlayingInfoPropertyPlaybackRate: *mut Object;
    static MPNowPlayingInfoPropertyDefaultPlaybackRate: *mut Object;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
struct IosAudioPlayer {
    player: *mut Object,
    current_song_id: Option<String>,
    metadata: Option<NativeTrackMetadata>,
    ended_sent_for_song: Option<String>,
    last_known_elapsed: f64,
    last_known_duration: f64,
    pending_seek_target: Option<f64>,
    pending_seek_ticks: u8,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe impl Send for IosAudioPlayer {}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl Drop for IosAudioPlayer {
    fn drop(&mut self) {
        unsafe {
            let _: () = msg_send![self.player, pause];
            let _: () = msg_send![self.player, release];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl IosAudioPlayer {
    fn new() -> Option<Self> {
        configure_ios_audio_session();
        configure_ios_remote_commands();
        let player = unsafe {
            let player_cls = class!(AVPlayer);
            let player_alloc: *mut Object = msg_send![player_cls, alloc];
            if player_alloc.is_null() {
                return None;
            }
            let player: *mut Object = msg_send![player_alloc, init];
            if player.is_null() {
                return None;
            }
            player
        };

        Some(Self {
            player,
            current_song_id: None,
            metadata: None,
            ended_sent_for_song: None,
            last_known_elapsed: 0.0,
            last_known_duration: 0.0,
            pending_seek_target: None,
            pending_seek_ticks: 0,
        })
    }

    fn apply(&mut self, cmd: serde_json::Value) {
        let Some(cmd_type) = cmd.get("type").and_then(|v| v.as_str()) else {
            return;
        };

        match cmd_type {
            "load" => {
                let src = cmd.get("src").and_then(|v| v.as_str()).unwrap_or_default();
                if src.is_empty() {
                    return;
                }

                let position = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                let volume = cmd.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let should_play = cmd.get("play").and_then(|v| v.as_bool()).unwrap_or(false);
                let song_id = cmd
                    .get("song_id")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                let metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());

                unsafe {
                    if let Some(item) = make_player_item(src) {
                        let _: () = msg_send![self.player, replaceCurrentItemWithPlayerItem: item];
                        observe_ios_item_end(item);
                        let _: () =
                            msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
                        self.seek(position);
                        if should_play {
                            let _: () = msg_send![self.player, play];
                        } else {
                            let _: () = msg_send![self.player, pause];
                        }
                    }
                }

                self.current_song_id = song_id;
                self.metadata = metadata;
                self.ended_sent_for_song = None;
                self.last_known_elapsed = position;
                self.last_known_duration = self
                    .metadata
                    .as_ref()
                    .map(|m| m.duration)
                    .unwrap_or(0.0)
                    .max(0.0);
                self.pending_seek_target = Some(position);
                self.pending_seek_ticks = 20;
                self.update_now_playing_info_cached(if should_play { 1.0 } else { 0.0 });
            }
            "play" => unsafe {
                let _: () = msg_send![self.player, play];
                self.update_now_playing_info_cached(1.0);
            },
            "pause" => unsafe {
                let _: () = msg_send![self.player, pause];
                self.update_now_playing_info_cached(0.0);
            },
            "seek" => {
                let target = cmd
                    .get("position")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0)
                    .max(0.0);
                self.seek(target);
                self.ended_sent_for_song = None;
                self.last_known_elapsed = target;
                self.pending_seek_target = Some(target);
                self.pending_seek_ticks = 20;
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
            }
            "volume" => {
                let volume = cmd.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);
                unsafe {
                    let _: () = msg_send![self.player, setVolume: volume.clamp(0.0, 1.0) as f32];
                }
            }
            "metadata" => {
                self.metadata = cmd
                    .get("meta")
                    .cloned()
                    .and_then(|value| serde_json::from_value::<NativeTrackMetadata>(value).ok());
                self.last_known_duration = self
                    .metadata
                    .as_ref()
                    .map(|m| m.duration)
                    .unwrap_or(self.last_known_duration)
                    .max(0.0);
                let rate: f32 = unsafe { msg_send![self.player, rate] };
                self.update_now_playing_info_cached(if rate > 0.0 { 1.0 } else { 0.0 });
            }
            "clear" => {
                unsafe {
                    let _: () = msg_send![self.player, pause];
                    let nil_item: *mut Object = ptr::null_mut();
                    let _: () = msg_send![self.player, replaceCurrentItemWithPlayerItem: nil_item];
                }
                self.current_song_id = None;
                self.metadata = None;
                self.ended_sent_for_song = None;
                self.last_known_elapsed = 0.0;
                self.last_known_duration = 0.0;
                self.pending_seek_target = None;
                self.pending_seek_ticks = 0;
                observe_ios_item_end(ptr::null_mut());
                clear_ios_now_playing_info();
            }
            "loop" => {}
            _ => {}
        }
    }

    fn seek(&self, position: f64) {
        unsafe {
            let time = CMTimeMakeWithSeconds(position.max(0.0), 1000);
            let _: () = msg_send![self.player, seekToTime: time];
        }
    }

    fn snapshot(&mut self) -> NativeAudioSnapshot {
        let (current_time, duration) = self.current_time_and_duration();

        let paused = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate <= 0.0
        };

        let ended = duration > 0.0 && current_time >= (duration - 0.35).max(0.0) && paused;
        let mut action = pop_ios_remote_action();

        if ended {
            let current_song = self.current_song_id.clone();
            if self.ended_sent_for_song != current_song {
                self.ended_sent_for_song = current_song;
                if action.is_none() {
                    action = Some("ended".to_string());
                }
            }
        } else {
            self.ended_sent_for_song = None;
        }

        // Keep lock-screen/command-center progress fresh while preserving
        // monotonic time through transient AVPlayer glitches.
        self.update_now_playing_info();

        NativeAudioSnapshot {
            current_time,
            duration,
            paused,
            ended,
            action,
        }
    }

    fn current_time_and_duration(&mut self) -> (f64, f64) {
        let mut current_time = unsafe {
            let current: CMTime = msg_send![self.player, currentTime];
            cmtime_seconds(current)
        };
        let playing = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate > 0.0
        };

        let mut duration = unsafe {
            let current_item: *mut Object = msg_send![self.player, currentItem];
            if current_item.is_null() {
                0.0
            } else {
                let duration: CMTime = msg_send![current_item, duration];
                cmtime_seconds(duration)
            }
        };

        if duration <= 0.0 {
            if self.last_known_duration.is_finite() && self.last_known_duration > 0.0 {
                duration = self.last_known_duration;
            }
        }

        if duration <= 0.0 {
            if let Some(meta) = &self.metadata {
                if meta.duration.is_finite() && meta.duration > 0.0 {
                    duration = meta.duration;
                }
            }
        }

        if duration > 0.0 {
            current_time = current_time.min(duration);
        }

        if let Some(target) = self.pending_seek_target {
            let clamped_target = if duration > 0.0 {
                target.min(duration)
            } else {
                target.max(0.0)
            };
            let close_enough = (current_time - clamped_target).abs() <= 1.5;
            if !close_enough && self.pending_seek_ticks > 0 {
                current_time = clamped_target;
                self.last_known_elapsed = clamped_target.max(0.0);
                self.pending_seek_ticks = self.pending_seek_ticks.saturating_sub(1);
            } else {
                self.pending_seek_target = None;
                self.pending_seek_ticks = 0;
            }
        }

        // AVPlayer can briefly report 0 during app/background transitions.
        // Preserve elapsed position unless we have a trustworthy newer value.
        if current_time <= 0.05 && self.last_known_elapsed > 0.25 {
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else if playing
            && self.last_known_elapsed > 1.0
            && current_time + 1.5 < self.last_known_elapsed
        {
            // Ignore abrupt backwards jumps while actively playing.
            current_time = if duration > 0.0 {
                self.last_known_elapsed.min(duration)
            } else {
                self.last_known_elapsed
            };
        } else {
            self.last_known_elapsed = if duration > 0.0 {
                current_time.min(duration).max(0.0)
            } else {
                current_time.max(0.0)
            };
        }

        if duration.is_finite() && duration > 0.0 {
            self.last_known_duration = duration;
        }

        (current_time, duration)
    }

    fn update_now_playing_info_cached(&mut self, rate: f64) {
        let Some(meta) = self.metadata.clone() else {
            clear_ios_now_playing_info();
            return;
        };

        let mut duration = self.last_known_duration;
        if !(duration.is_finite() && duration > 0.0)
            && meta.duration.is_finite()
            && meta.duration > 0.0
        {
            duration = meta.duration;
        }

        let mut elapsed = self.last_known_elapsed.max(0.0);
        if duration.is_finite() && duration > 0.0 {
            elapsed = elapsed.min(duration);
            self.last_known_duration = duration;
        }

        set_ios_now_playing_info(&meta, elapsed, duration.max(0.0), rate.max(0.0));
    }

    fn update_now_playing_info(&mut self) {
        let Some(meta) = self.metadata.clone() else {
            clear_ios_now_playing_info();
            return;
        };

        let (elapsed, duration) = self.current_time_and_duration();
        let paused = unsafe {
            let rate: f32 = msg_send![self.player, rate];
            rate <= 0.0
        };
        set_ios_now_playing_info(&meta, elapsed, duration, if paused { 0.0 } else { 1.0 });
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_AUDIO_PLAYER: Lazy<Mutex<Option<IosAudioPlayer>>> = Lazy::new(|| Mutex::new(None));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_ACTIONS: Lazy<Mutex<VecDeque<String>>> =
    Lazy::new(|| Mutex::new(VecDeque::new()));
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_INIT: Once = Once::new();
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_REMOTE_OBSERVER: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn with_ios_player<R>(f: impl FnOnce(&mut IosAudioPlayer) -> R) -> Option<R> {
    let mut guard = IOS_AUDIO_PLAYER.lock().ok()?;
    if guard.is_none() {
        *guard = IosAudioPlayer::new();
    }
    guard.as_mut().map(f)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn push_ios_remote_action(action: &str) {
    if let Ok(mut actions) = IOS_REMOTE_ACTIONS.lock() {
        actions.push_back(action.to_string());
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn pop_ios_remote_action() -> Option<String> {
    IOS_REMOTE_ACTIONS
        .lock()
        .ok()
        .and_then(|mut actions| actions.pop_front())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_remote_observer(observer: *mut Object) {
    if let Ok(mut slot) = IOS_REMOTE_OBSERVER.lock() {
        *slot = observer as usize;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn get_ios_remote_observer() -> *mut Object {
    IOS_REMOTE_OBSERVER
        .lock()
        .ok()
        .map(|slot| *slot as *mut Object)
        .unwrap_or(ptr::null_mut())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn cmtime_seconds(time: CMTime) -> f64 {
    unsafe {
        let seconds = CMTimeGetSeconds(time);
        if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ns_string(value: &str) -> Option<*mut Object> {
    unsafe {
        let ns_string_cls = class!(NSString);
        let alloc: *mut Object = msg_send![ns_string_cls, alloc];
        if alloc.is_null() {
            return None;
        }

        // UTF-8 encoding.
        let encoded: *mut Object = msg_send![alloc,
            initWithBytes: value.as_ptr()
            length: value.len()
            encoding: 4usize
        ];

        if encoded.is_null() {
            None
        } else {
            Some(encoded)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn make_player_item(src: &str) -> Option<*mut Object> {
    unsafe {
        let src_str = ns_string(src)?;
        let url_cls = class!(NSURL);
        let url: *mut Object = msg_send![url_cls, URLWithString: src_str];
        let _: () = msg_send![src_str, release];
        if url.is_null() {
            return None;
        }

        let item_cls = class!(AVPlayerItem);
        let item: *mut Object = msg_send![item_cls, playerItemWithURL: url];
        if item.is_null() {
            None
        } else {
            Some(item)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn configure_ios_audio_session() {
    unsafe {
        let session_cls = class!(AVAudioSession);
        let session: *mut Object = msg_send![session_cls, sharedInstance];
        if session.is_null() {
            return;
        }

        let Some(category) = ns_string("AVAudioSessionCategoryPlayback") else {
            return;
        };

        let _: BOOL =
            msg_send![session, setCategory: category error: ptr::null_mut::<*mut Object>()];
        let _: () = msg_send![category, release];

        let _: BOOL = msg_send![session, setActive: YES error: ptr::null_mut::<*mut Object>()];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn remote_handler_class() -> &'static objc::runtime::Class {
    static REGISTER: Once = Once::new();
    static mut CLASS_PTR: *const objc::runtime::Class = std::ptr::null();

    REGISTER.call_once(|| unsafe {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("RustySoundRemoteCommandHandler", superclass)
            .expect("failed to create remote command handler class");

        decl.add_method(
            sel!(handlePlay:),
            ios_handle_play as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handlePause:),
            ios_handle_pause as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleNext:),
            ios_handle_next as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handlePrevious:),
            ios_handle_previous as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleSeek:),
            ios_handle_seek as extern "C" fn(&Object, objc::runtime::Sel, *mut Object) -> i64,
        );
        decl.add_method(
            sel!(handleEnded:),
            ios_handle_ended as extern "C" fn(&Object, objc::runtime::Sel, *mut Object),
        );

        let cls = decl.register();
        CLASS_PTR = cls;
    });

    unsafe { &*CLASS_PTR }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_play(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "play" })));
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_pause(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    let _ = with_ios_player(|player| player.apply(serde_json::json!({ "type": "pause" })));
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_next(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    push_ios_remote_action("next");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_previous(_: &Object, _: objc::runtime::Sel, _: *mut Object) -> i64 {
    push_ios_remote_action("previous");
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_seek(_: &Object, _: objc::runtime::Sel, event: *mut Object) -> i64 {
    unsafe {
        if !event.is_null() {
            let position: f64 = msg_send![event, positionTime];
            let clamped = position.max(0.0);
            let _ = with_ios_player(|player| {
                player.apply(serde_json::json!({
                    "type": "seek",
                    "position": clamped,
                }));
            });
            push_ios_remote_action(&format!("seek:{clamped}"));
        }
    }
    0
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
extern "C" fn ios_handle_ended(_: &Object, _: objc::runtime::Sel, _: *mut Object) {
    push_ios_remote_action("ended");
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn configure_ios_remote_commands() {
    IOS_REMOTE_INIT.call_once(|| unsafe {
        let cls = remote_handler_class();
        let observer: *mut Object = msg_send![cls, new];
        if observer.is_null() {
            return;
        }
        set_ios_remote_observer(observer);

        let center_cls = class!(MPRemoteCommandCenter);
        let center: *mut Object = msg_send![center_cls, sharedCommandCenter];
        if center.is_null() {
            return;
        }

        let play_cmd: *mut Object = msg_send![center, playCommand];
        let pause_cmd: *mut Object = msg_send![center, pauseCommand];
        let next_cmd: *mut Object = msg_send![center, nextTrackCommand];
        let previous_cmd: *mut Object = msg_send![center, previousTrackCommand];
        let seek_cmd: *mut Object = msg_send![center, changePlaybackPositionCommand];
        let skip_forward_cmd: *mut Object = msg_send![center, skipForwardCommand];
        let skip_backward_cmd: *mut Object = msg_send![center, skipBackwardCommand];

        let _: () = msg_send![play_cmd, addTarget: observer action: sel!(handlePlay:)];
        let _: () = msg_send![pause_cmd, addTarget: observer action: sel!(handlePause:)];
        let _: () = msg_send![next_cmd, addTarget: observer action: sel!(handleNext:)];
        let _: () = msg_send![previous_cmd, addTarget: observer action: sel!(handlePrevious:)];
        let _: () = msg_send![seek_cmd, addTarget: observer action: sel!(handleSeek:)];

        let _: () = msg_send![play_cmd, setEnabled: YES];
        let _: () = msg_send![pause_cmd, setEnabled: YES];
        let _: () = msg_send![next_cmd, setEnabled: YES];
        let _: () = msg_send![previous_cmd, setEnabled: YES];
        let _: () = msg_send![seek_cmd, setEnabled: YES];

        let no: BOOL = false;
        let _: () = msg_send![skip_forward_cmd, setEnabled: no];
        let _: () = msg_send![skip_backward_cmd, setEnabled: no];
    });
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn observe_ios_item_end(item: *mut Object) {
    unsafe {
        let observer = get_ios_remote_observer();
        if observer.is_null() {
            return;
        }

        let center_cls = class!(NSNotificationCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            return;
        }

        let Some(notification_name) = ns_string("AVPlayerItemDidPlayToEndTimeNotification") else {
            return;
        };

        let _: () = msg_send![center,
            removeObserver: observer
            name: notification_name
            object: ptr::null_mut::<Object>()
        ];
        let _: () = msg_send![center, addObserver: observer selector: sel!(handleEnded:) name: notification_name object: item];
        let _: () = msg_send![notification_name, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_string(dict: *mut Object, key: *mut Object, value: &str) {
    if value.is_empty() {
        return;
    }
    unsafe {
        if key.is_null() {
            return;
        }
        let Some(value_obj) = ns_string(value) else {
            return;
        };
        let _: () = msg_send![dict, setObject: value_obj forKey: key];
        let _: () = msg_send![value_obj, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_now_playing_number(dict: *mut Object, key: *mut Object, value: f64) {
    unsafe {
        if key.is_null() {
            return;
        }
        let number_cls = class!(NSNumber);
        let value_obj: *mut Object = msg_send![number_cls, numberWithDouble: value];
        if !value_obj.is_null() {
            let _: () = msg_send![dict, setObject: value_obj forKey: key];
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn make_now_playing_artwork(artwork_url: &str) -> Option<*mut Object> {
    unsafe {
        let url_str = ns_string(artwork_url)?;
        let url_cls = class!(NSURL);
        let url: *mut Object = msg_send![url_cls, URLWithString: url_str];
        let _: () = msg_send![url_str, release];
        if url.is_null() {
            return None;
        }

        let data_cls = class!(NSData);
        let data: *mut Object = msg_send![data_cls, dataWithContentsOfURL: url];
        if data.is_null() {
            return None;
        }

        let image_cls = class!(UIImage);
        let image: *mut Object = msg_send![image_cls, imageWithData: data];
        if image.is_null() {
            return None;
        }

        let artwork_cls = class!(MPMediaItemArtwork);
        let artwork_alloc: *mut Object = msg_send![artwork_cls, alloc];
        if artwork_alloc.is_null() {
            return None;
        }

        let artwork: *mut Object = msg_send![artwork_alloc, initWithImage: image];
        if artwork.is_null() {
            None
        } else {
            Some(artwork)
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn set_ios_now_playing_info(meta: &NativeTrackMetadata, elapsed: f64, duration: f64, rate: f64) {
    unsafe {
        let center_cls = class!(MPNowPlayingInfoCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            return;
        }

        let dict_cls = class!(NSMutableDictionary);
        let dict_alloc: *mut Object = msg_send![dict_cls, alloc];
        if dict_alloc.is_null() {
            return;
        }
        let dict: *mut Object = msg_send![dict_alloc, init];
        if dict.is_null() {
            return;
        }

        set_now_playing_string(dict, MPMediaItemPropertyTitle, &meta.title);
        set_now_playing_string(dict, MPMediaItemPropertyArtist, &meta.artist);
        set_now_playing_string(dict, MPMediaItemPropertyAlbumTitle, &meta.album);
        set_now_playing_number(
            dict,
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            elapsed.max(0.0),
        );
        if duration.is_finite() && duration > 0.0 {
            set_now_playing_number(dict, MPMediaItemPropertyPlaybackDuration, duration);
        }
        set_now_playing_number(dict, MPNowPlayingInfoPropertyPlaybackRate, rate.max(0.0));
        set_now_playing_number(dict, MPNowPlayingInfoPropertyDefaultPlaybackRate, 1.0);
        if let Some(artwork_url) = &meta.artwork {
            if let Some(artwork_obj) = make_now_playing_artwork(artwork_url) {
                if !MPMediaItemPropertyArtwork.is_null() {
                    let _: () =
                        msg_send![dict, setObject: artwork_obj forKey: MPMediaItemPropertyArtwork];
                }
                let _: () = msg_send![artwork_obj, release];
            }
        }

        let _: () = msg_send![center, setNowPlayingInfo: dict];
        let _: () = msg_send![dict, release];
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn clear_ios_now_playing_info() {
    unsafe {
        let center_cls = class!(MPNowPlayingInfoCenter);
        let center: *mut Object = msg_send![center_cls, defaultCenter];
        if center.is_null() {
            return;
        }
        let nil_info: *mut Object = ptr::null_mut();
        let _: () = msg_send![center, setNowPlayingInfo: nil_info];
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn ensure_native_audio_bridge() {
    let _ = document::eval(NATIVE_AUDIO_BOOTSTRAP_JS);
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ensure_native_audio_bridge() {
    let _ = with_ios_player(|_| ());
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn native_audio_command(value: serde_json::Value) {
    ensure_native_audio_bridge();
    let payload = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    let script = format!(
        r#"(function () {{
            const bridge = window.__rustysoundAudioBridge;
            if (!bridge) return false;
            bridge.apply({payload});
            return true;
        }})();"#
    );
    let _ = document::eval(&script);
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn native_audio_command(value: serde_json::Value) {
    let _ = with_ios_player(|player| player.apply(value));
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    ensure_native_audio_bridge();
    let eval = document::eval(
        r#"return (function () {
            const bridge = window.__rustysoundAudioBridge;
            const raw = (bridge && typeof bridge.snapshot === "function")
              ? (bridge.snapshot() || {})
              : {};
            const currentTime = Number.isFinite(raw.current_time) ? raw.current_time : 0;
            const duration = Number.isFinite(raw.duration) ? raw.duration : 0;
            const paused = !!raw.paused;
            const ended = !!raw.ended;
            const action = typeof raw.action === "string" ? raw.action : null;
            return {
              current_time: currentTime,
              duration,
              paused,
              ended,
              action,
            };
        })();"#,
    );
    eval.join::<NativeAudioSnapshot>().await.ok()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    with_ios_player(|player| player.snapshot())
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
async fn native_delay_ms(ms: u64) {
    let script = format!(
        r#"return (async function () {{
            await new Promise(resolve => setTimeout(resolve, {ms}));
            return true;
        }})();"#
    );
    let _ = document::eval(&script).await;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
async fn native_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn song_metadata(song: &Song, servers: &[ServerConfig]) -> NativeTrackMetadata {
    let is_live = song.server_name == "Radio";
    let title = if is_live && song.title.trim().eq_ignore_ascii_case("unknown song") {
        song.artist
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown Song".to_string())
    } else {
        song.title.clone()
    };
    let artist = song
        .artist
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if is_live {
                "Internet Radio".to_string()
            } else {
                "Unknown Artist".to_string()
            }
        });

    let mut album = if is_live {
        "LIVE".to_string()
    } else {
        song.album
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown Album".to_string())
    };
    if !is_live {
        if let Some(year) = song.year {
            album = format!("{album} ({year})");
        }
    }

    let artwork = servers
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            song.cover_art
                .as_ref()
                .map(|cover| NavidromeClient::new(server.clone()).get_cover_art_url(cover, 512))
        });

    NativeTrackMetadata {
        title,
        artist,
        album,
        artwork,
        duration: song.duration as f64,
        is_live,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let seek_request = use_context::<SeekRequestSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();

    let last_song_id = use_signal(|| None::<String>);
    let last_src = use_signal(|| None::<String>);
    let last_bookmark = use_signal(|| None::<(String, u64)>);
    let last_song_for_bookmark = use_signal(|| None::<Song>);
    let last_ended_song = use_signal(|| None::<String>);
    let repeat_one_replayed_song = use_signal(|| None::<String>);

    // One-time setup: bootstrap audio bridge and poll playback state.
    {
        let servers = servers.clone();
        let queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let repeat_mode = repeat_mode.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        let app_settings = app_settings.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_ended_song = last_ended_song.clone();
        let mut repeat_one_replayed_song = repeat_one_replayed_song.clone();

        use_effect(move || {
            ensure_native_audio_bridge();
            audio_state.write().is_initialized.set(true);

            spawn(async move {
                let mut paused_streak: u8 = 0;
                let mut playing_streak: u8 = 0;
                loop {
                    native_delay_ms(250).await;

                    let Some(snapshot) = native_audio_snapshot().await else {
                        continue;
                    };

                    let mut effective_duration = *audio_state.peek().duration.peek();
                    if snapshot.duration.is_finite() && snapshot.duration > 0.0 {
                        effective_duration = snapshot.duration;
                        audio_state.write().duration.set(snapshot.duration);
                    }

                    let mut current_time = snapshot.current_time.max(0.0);
                    if effective_duration.is_finite() && effective_duration > 0.0 {
                        current_time = current_time.min(effective_duration);
                    }
                    playback_position.set(current_time);
                    audio_state.write().current_time.set(current_time);

                    if !snapshot.paused && app_settings.peek().bookmark_auto_save {
                        if let Some(song) = now_playing.peek().clone() {
                            if can_save_server_bookmark(&song) {
                                let position_ms = (current_time * 1000.0).round().max(0.0) as u64;
                                if position_ms > 1500 {
                                    let should_save = match last_bookmark.peek().clone() {
                                        Some((id, pos)) => {
                                            id != song.id || position_ms.abs_diff(pos) >= 15_000
                                        }
                                        None => true,
                                    };
                                    if should_save {
                                        last_bookmark.set(Some((song.id.clone(), position_ms)));
                                        let servers_snapshot = servers.peek().clone();
                                        let bookmark_limit =
                                            app_settings.peek().bookmark_limit.clamp(1, 5000)
                                                as usize;
                                        let song_id = song.id.clone();
                                        let server_id = song.server_id.clone();
                                        spawn(async move {
                                            if let Some(server) = servers_snapshot
                                                .iter()
                                                .find(|s| s.id == server_id)
                                                .cloned()
                                            {
                                                let client = NavidromeClient::new(server);
                                                let _ = client
                                                    .create_bookmark_with_limit(
                                                        &song_id,
                                                        position_ms,
                                                        None,
                                                        Some(bookmark_limit),
                                                    )
                                                    .await;
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }

                    let has_selected_song = now_playing.peek().is_some();
                    if has_selected_song {
                        if snapshot.paused {
                            paused_streak = paused_streak.saturating_add(1);
                            playing_streak = 0;
                        } else {
                            playing_streak = playing_streak.saturating_add(1);
                            paused_streak = 0;
                        }

                        // Debounced sync from native player to UI state:
                        // avoid immediate false flips while a track is starting.
                        if *is_playing.peek() && paused_streak >= 3 && !snapshot.ended {
                            is_playing.set(false);
                        } else if !*is_playing.peek() && playing_streak >= 2 {
                            is_playing.set(true);
                        }
                    } else {
                        paused_streak = 0;
                        playing_streak = 0;
                        if *is_playing.peek() {
                            is_playing.set(false);
                        }
                    }

                    let currently_playing = *is_playing.peek();

                    let ended_action = matches!(snapshot.action.as_deref(), Some("ended"));

                    if let Some(action) = snapshot.action.as_deref() {
                        if now_playing.peek().is_none() {
                            continue;
                        }
                        let current_is_radio = now_playing
                            .peek()
                            .as_ref()
                            .map(|song| song.server_name == "Radio")
                            .unwrap_or(false);
                        let queue_snapshot = queue.peek().clone();
                        let idx = *queue_index.peek();
                        let repeat = *repeat_mode.peek();
                        let shuffle = *shuffle_enabled.peek();
                        let servers_snapshot = servers.peek().clone();
                        let resume_after_skip = currently_playing;

                        if let Some(raw_seek) = action.strip_prefix("seek:") {
                            if let Ok(target) = raw_seek.parse::<f64>() {
                                let mut clamped = target.max(0.0);
                                if effective_duration.is_finite() && effective_duration > 0.0 {
                                    clamped = clamped.min(effective_duration);
                                }
                                playback_position.set(clamped);
                                audio_state.write().current_time.set(clamped);
                            }
                            continue;
                        }

                        match action {
                            "toggle_play" | "playpause" => {
                                is_playing.set(!currently_playing);
                            }
                            "play" => {
                                is_playing.set(true);
                            }
                            "pause" => {
                                is_playing.set(false);
                            }
                            "next" => {
                                if current_is_radio {
                                    continue;
                                }
                                let len = queue_snapshot.len();
                                if len == 0 {
                                    spawn_shuffle_queue(
                                        servers_snapshot,
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        now_playing.peek().clone(),
                                        Some(resume_after_skip),
                                    );
                                } else if repeat == RepeatMode::Off && shuffle {
                                    if idx < len.saturating_sub(1) {
                                        if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                            queue_index.set(idx + 1);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else {
                                        spawn_shuffle_queue(
                                            servers_snapshot,
                                            queue.clone(),
                                            queue_index.clone(),
                                            now_playing.clone(),
                                            is_playing.clone(),
                                            now_playing.peek().clone(),
                                            Some(resume_after_skip),
                                        );
                                    }
                                } else if idx < len.saturating_sub(1) {
                                    if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                        queue_index.set(idx + 1);
                                        now_playing.set(Some(song));
                                        is_playing.set(resume_after_skip);
                                    }
                                } else if repeat == RepeatMode::All {
                                    if let Some(song) = queue_snapshot.first().cloned() {
                                        queue_index.set(0);
                                        now_playing.set(Some(song));
                                        is_playing.set(resume_after_skip);
                                    }
                                } else if len <= 1 {
                                    spawn_shuffle_queue(
                                        servers_snapshot,
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        now_playing.peek().clone(),
                                        Some(resume_after_skip),
                                    );
                                } else {
                                    native_audio_command(serde_json::json!({
                                        "type": "seek",
                                        "position": 0.0
                                    }));
                                    is_playing.set(false);
                                }
                            }
                            "previous" => {
                                if current_is_radio {
                                    continue;
                                }
                                let len = queue_snapshot.len();
                                if len > 0 {
                                    if idx > 0 {
                                        if let Some(song) = queue_snapshot.get(idx - 1).cloned() {
                                            queue_index.set(idx - 1);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else if repeat == RepeatMode::All {
                                        let last_idx = len.saturating_sub(1);
                                        if let Some(song) = queue_snapshot.get(last_idx).cloned() {
                                            queue_index.set(last_idx);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else {
                                        native_audio_command(serde_json::json!({
                                            "type": "seek",
                                            "position": 0.0
                                        }));
                                        if resume_after_skip {
                                            native_audio_command(serde_json::json!({
                                                "type": "play"
                                            }));
                                        }
                                    }
                                }
                            }
                            "ended" => {}
                            _ => {}
                        }
                    }

                    if snapshot.ended || ended_action {
                        let current_song = now_playing.peek().clone();
                        let current_id = current_song.as_ref().map(|s| s.id.clone());
                        if *last_ended_song.peek() == current_id {
                            continue;
                        }
                        last_ended_song.set(current_id.clone());

                        let queue_snapshot = queue.peek().clone();
                        let idx = *queue_index.peek();
                        let repeat = *repeat_mode.peek();
                        let shuffle = *shuffle_enabled.peek();
                        let servers_snapshot = servers.peek().clone();

                        if repeat != RepeatMode::One && repeat_one_replayed_song.peek().is_some() {
                            repeat_one_replayed_song.set(None);
                        }

                        if let Some(song) = current_song.clone() {
                            scrobble_song(&servers_snapshot, &song, true);
                        }

                        if repeat == RepeatMode::One {
                            if let Some(song_id) = current_id.clone() {
                                if repeat_one_replayed_song.peek().as_ref() != Some(&song_id) {
                                    repeat_one_replayed_song.set(Some(song_id));
                                    native_audio_command(serde_json::json!({
                                        "type": "seek",
                                        "position": 0.0
                                    }));
                                    if *is_playing.peek() {
                                        native_audio_command(serde_json::json!({
                                            "type": "play"
                                        }));
                                    }
                                } else {
                                    // Repeat-one should replay exactly once, then stop.
                                    repeat_one_replayed_song.set(None);
                                    is_playing.set(false);
                                }
                            } else {
                                is_playing.set(false);
                            }
                            continue;
                        }

                        let len = queue_snapshot.len();
                        if len == 0 {
                            is_playing.set(false);
                            continue;
                        }

                        let should_shuffle = repeat == RepeatMode::Off && shuffle;
                        if should_shuffle {
                            if idx < len.saturating_sub(1) {
                                if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                    queue_index.set(idx + 1);
                                    now_playing.set(Some(song));
                                    is_playing.set(true);
                                }
                            } else {
                                spawn_shuffle_queue(
                                    servers_snapshot,
                                    queue.clone(),
                                    queue_index.clone(),
                                    now_playing.clone(),
                                    is_playing.clone(),
                                    current_song,
                                    Some(true),
                                );
                            }
                            continue;
                        }

                        if idx < len.saturating_sub(1) {
                            if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                queue_index.set(idx + 1);
                                now_playing.set(Some(song));
                                is_playing.set(true);
                            }
                        } else if repeat == RepeatMode::All {
                            if let Some(song) = queue_snapshot.first().cloned() {
                                queue_index.set(0);
                                now_playing.set(Some(song));
                                is_playing.set(true);
                            }
                        } else {
                            is_playing.set(false);
                        }
                    } else if last_ended_song.peek().is_some() {
                        last_ended_song.set(None);
                    }
                }
            });
        });
    }

    // Keep queue_index aligned when now_playing changes and the song is in the queue.
    {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let song = now_playing();
            let queue_list = queue();
            if let Some(song) = song {
                if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
                    if pos != queue_index() {
                        queue_index.set(pos);
                    }
                }
            }
        });
    }

    // Update source + metadata and persist bookmark when songs change.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let volume = volume.clone();
        let now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut seek_request = seek_request.clone();
        let mut last_song_id = last_song_id.clone();
        let mut last_src = last_src.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_song_for_bookmark = last_song_for_bookmark.clone();

        use_effect(move || {
            let song = now_playing();
            let song_id = song.as_ref().map(|s| s.id.clone());
            let previous_song = last_song_for_bookmark.peek().clone();

            if let Some(prev) = previous_song {
                if Some(prev.id.clone()) != song_id {
                    let position_ms = (playback_position.peek().max(0.0) * 1000.0).round() as u64;
                    if position_ms > 1500 && can_save_server_bookmark(&prev) {
                        if app_settings.peek().bookmark_auto_save {
                            let servers_snapshot = servers.peek().clone();
                            let bookmark_limit =
                                app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                            let song_id = prev.id.clone();
                            let server_id = prev.server_id.clone();
                            spawn(async move {
                                last_bookmark.set(Some((song_id.clone(), position_ms)));
                                if let Some(server) =
                                    servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                                {
                                    let client = NavidromeClient::new(server);
                                    let _ = client
                                        .create_bookmark_with_limit(
                                            &song_id,
                                            position_ms,
                                            None,
                                            Some(bookmark_limit),
                                        )
                                        .await;
                                }
                            });
                        }
                    }
                }
            }

            if song_id != *last_song_id.peek() {
                last_song_id.set(song_id.clone());
                last_song_for_bookmark.set(song.clone());
            }

            let Some(song) = song else {
                native_audio_command(serde_json::json!({ "type": "clear" }));
                last_src.set(None);
                is_playing.set(false);
                audio_state.write().playback_error.set(None);
                return;
            };

            let servers_snapshot = servers.peek().clone();
            if let Some(url) = resolve_stream_url(&song, &servers_snapshot) {
                let requested_seek = seek_request.peek().clone().and_then(|(song_id, position)| {
                    if song_id == song.id {
                        Some(position)
                    } else {
                        None
                    }
                });

                let should_reload = Some(url.clone()) != *last_src.peek();
                let metadata = song_metadata(&song, &servers_snapshot);
                let known_duration = if song.duration > 0 {
                    song.duration as f64
                } else {
                    0.0
                };
                let mut target_start = requested_seek.unwrap_or(0.0).max(0.0);
                if known_duration > 0.0 {
                    target_start = target_start.min(known_duration);
                }

                if should_reload {
                    last_src.set(Some(url.clone()));
                    playback_position.set(target_start);
                    audio_state.write().current_time.set(target_start);
                    audio_state.write().playback_error.set(None);
                    if known_duration > 0.0 {
                        audio_state.write().duration.set(known_duration);
                    } else {
                        audio_state.write().duration.set(0.0);
                    }
                    native_audio_command(serde_json::json!({
                        "type": "load",
                        "src": url,
                        "song_id": song.id,
                        "position": target_start,
                        "volume": volume.peek().clamp(0.0, 1.0),
                        "play": *is_playing.peek(),
                        "meta": metadata,
                    }));
                } else if let Some(target_pos) = requested_seek {
                    native_audio_command(serde_json::json!({
                        "type": "seek",
                        "position": target_pos,
                    }));
                } else {
                    native_audio_command(serde_json::json!({
                        "type": "metadata",
                        "meta": metadata,
                    }));
                }

                if let Some(target_pos) = requested_seek {
                    let mut clamped_pos = target_pos.max(0.0);
                    let current_duration = *audio_state.peek().duration.peek();
                    if current_duration > 0.0 {
                        clamped_pos = clamped_pos.min(current_duration);
                    }
                    playback_position.set(clamped_pos);
                    audio_state.write().current_time.set(clamped_pos);
                    seek_request.set(None);
                }

                scrobble_song(&servers_snapshot, &song, false);
            } else {
                native_audio_command(serde_json::json!({ "type": "clear" }));
                last_src.set(None);
                is_playing.set(false);
                let message = if song.server_name == "Radio" {
                    "No station found: this station has no stream URL.".to_string()
                } else {
                    "Unable to load this audio source.".to_string()
                };
                audio_state.write().playback_error.set(Some(message));
            }
        });
    }

    // Handle play/pause state changes.
    {
        let is_playing = is_playing.clone();
        use_effect(move || {
            if is_playing() {
                native_audio_command(serde_json::json!({ "type": "play" }));
            } else {
                native_audio_command(serde_json::json!({ "type": "pause" }));
            }
        });
    }

    // Handle repeat mode changes.
    {
        let repeat_mode = repeat_mode.clone();
        use_effect(move || {
            let _ = repeat_mode();
            native_audio_command(serde_json::json!({
                "type": "loop",
                // Repeat behavior is handled in ended-event logic.
                "enabled": false,
            }));
        });
    }

    // Handle volume changes.
    {
        let volume = volume.clone();
        use_effect(move || {
            native_audio_command(serde_json::json!({
                "type": "volume",
                "value": volume().clamp(0.0, 1.0),
            }));
        });
    }

    // Persist a bookmark when playback pauses.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut last_bookmark = last_bookmark.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let playback_position = playback_position.clone();
        use_effect(move || {
            if is_playing() {
                return;
            }

            let Some(song) = now_playing() else {
                return;
            };
            if !can_save_server_bookmark(&song) {
                return;
            }

            let position_ms = (playback_position.peek().max(0.0) * 1000.0).round() as u64;
            if position_ms <= 1500 {
                return;
            }
            if !app_settings.peek().bookmark_auto_save {
                return;
            }

            let should_save = match last_bookmark.peek().clone() {
                Some((id, pos)) => id != song.id || position_ms.abs_diff(pos) >= 2000,
                None => true,
            };

            if should_save {
                let servers_snapshot = servers.peek().clone();
                let bookmark_limit = app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                let song_id = song.id.clone();
                let server_id = song.server_id.clone();
                spawn(async move {
                    last_bookmark.set(Some((song_id.clone(), position_ms)));
                    if let Some(server) =
                        servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                    {
                        let client = NavidromeClient::new(server);
                        let _ = client
                            .create_bookmark_with_limit(
                                &song_id,
                                position_ms,
                                None,
                                Some(bookmark_limit),
                            )
                            .await;
                    }
                });
            }
        });
    }

    rsx! {}
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

    spawn(async move {
        let mut songs = Vec::new();
        if let Some(seed) = seed_song {
            if let Some(server) = active_servers
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned()
            {
                let client = NavidromeClient::new(server);
                if let Ok(similar) = client.get_similar_songs(&seed.id, 50).await {
                    songs.extend(similar);
                }
            }
        }

        if songs.is_empty() {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server.clone());
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
        }

        if songs.is_empty() {
            return;
        }

        let len = songs.len();
        for i in (1..len).rev() {
            let j = (js_sys::Math::random() * ((i + 1) as f64)) as usize;
            songs.swap(i, j);
        }
        songs.truncate(50);

        let first = songs.get(0).cloned();
        defer_signal_update(move || {
            queue.set(songs);
            queue_index.set(0);
            now_playing.set(first);
            if let Some(play_state) = play_state {
                is_playing.set(play_state);
            }
        });
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

    spawn(async move {
        let mut songs = Vec::new();
        if let Some(seed) = seed_song {
            if let Some(server) = active_servers
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned()
            {
                let client = NavidromeClient::new(server);
                if let Ok(similar) = client.get_similar_songs(&seed.id, 50).await {
                    songs.extend(similar);
                }
            }
        }

        if songs.is_empty() {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server);
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
        }

        if songs.is_empty() {
            return;
        }

        let mut rng = rand::thread_rng();
        songs.shuffle(&mut rng);
        songs.truncate(50);

        let first = songs.first().cloned();
        queue.set(songs);
        queue_index.set(0);
        now_playing.set(first);
        if let Some(play_state) = play_state {
            is_playing.set(play_state);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn resolve_stream_url(song: &Song, servers: &[ServerConfig]) -> Option<String> {
    if song.server_name == "Radio" {
        return song
            .stream_url
            .clone()
            .filter(|value| !value.trim().is_empty());
    }

    let song_id = song.id.trim();
    if song_id.is_empty() {
        return None;
    }

    servers
        .iter()
        .find(|s| s.id == song.server_id)
        .map(|server| {
            let client = NavidromeClient::new(server.clone());
            client.get_stream_url(song_id)
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_stream_url(song: &Song, servers: &[ServerConfig]) -> Option<String> {
    if song.server_name == "Radio" {
        return song
            .stream_url
            .clone()
            .filter(|value| !value.trim().is_empty());
    }

    let song_id = song.id.trim();
    if song_id.is_empty() {
        return None;
    }

    servers
        .iter()
        .find(|s| s.id == song.server_id)
        .map(|server| {
            let client = NavidromeClient::new(server.clone());
            client.get_stream_url(song_id)
        })
}

fn can_save_server_bookmark(song: &Song) -> bool {
    song.server_name != "Radio" && !song.id.trim().is_empty() && !song.server_id.trim().is_empty()
}

#[cfg(target_arch = "wasm32")]
fn scrobble_song(servers: &[ServerConfig], song: &Song, finished: bool) {
    let server = servers.iter().find(|s| s.id == song.server_id).cloned();
    if let Some(server) = server {
        let song_id = song.id.clone();
        spawn(async move {
            let client = NavidromeClient::new(server);
            let _ = client.scrobble(&song_id, finished).await;
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scrobble_song(servers: &[ServerConfig], song: &Song, finished: bool) {
    let server = servers.iter().find(|s| s.id == song.server_id).cloned();
    if let Some(server) = server {
        let song_id = song.id.clone();
        spawn(async move {
            let client = NavidromeClient::new(server);
            let _ = client.scrobble(&song_id, finished).await;
        });
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let mut seek_request = use_context::<SeekRequestSignal>().0;
    let mut audio_state = use_context::<Signal<AudioState>>();

    let mut last_song_id = use_signal(|| None::<String>);
    let mut last_src = use_signal(|| None::<String>);
    let mut last_bookmark = use_signal(|| None::<(String, u64)>);
    let mut last_song_for_bookmark = use_signal(|| None::<Song>);

    thread_local! {
        static USER_INTERACTED: Cell<bool> = Cell::new(false);
    }
    let has_user_interacted = || USER_INTERACTED.with(|c| c.get());

    // One-time setup: create audio element and attach listeners.
    {
        let servers = servers.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let repeat_mode = repeat_mode.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        let app_settings = app_settings.clone();
        let playback_position = playback_position.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut audio_state = audio_state.clone();

        use_effect(move || {
            let Some(_audio) = get_or_create_audio_element() else {
                return;
            };
            ensure_web_media_session_shortcuts();

            if let Some(doc) = window().and_then(|w| w.document()) {
                let click_cb = Closure::wrap(
                    Box::new(move || USER_INTERACTED.with(|c| c.set(true))) as Box<dyn FnMut()>,
                );
                let key_cb = Closure::wrap(Box::new(move |event: KeyboardEvent| {
                    USER_INTERACTED.with(|c| c.set(true));
                    if let Some(action) = shortcut_action_from_key(&event) {
                        event.prevent_default();
                        match action {
                            "next" => click_player_control_button("next-btn"),
                            "previous" => click_player_control_button("prev-btn"),
                            "toggle_play" => click_player_control_button("play-pause-btn"),
                            _ => {}
                        }
                    }
                }) as Box<dyn FnMut(KeyboardEvent)>);
                let touch_cb = Closure::wrap(
                    Box::new(move || USER_INTERACTED.with(|c| c.set(true))) as Box<dyn FnMut()>,
                );
                let _ = doc
                    .add_event_listener_with_callback("click", click_cb.as_ref().unchecked_ref());
                let _ = doc
                    .add_event_listener_with_callback("keydown", key_cb.as_ref().unchecked_ref());
                let _ = doc.add_event_listener_with_callback(
                    "touchstart",
                    touch_cb.as_ref().unchecked_ref(),
                );
                click_cb.forget();
                key_cb.forget();
                touch_cb.forget();
            }

            let mut current_time_signal = audio_state.peek().current_time;
            let mut duration_signal = audio_state.peek().duration;
            let mut playback_error_signal = audio_state.peek().playback_error;
            let mut playback_pos = playback_position.clone();
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let repeat_mode = repeat_mode.clone();
            let shuffle_enabled = shuffle_enabled.clone();
            let servers = servers.clone();

            audio_state.write().is_initialized.set(true);

            spawn(async move {
                let mut last_emit = 0.0f64;
                let mut last_duration = -1.0f64;
                let mut ended_for_song: Option<String> = None;
                let mut repeat_one_replayed_song: Option<String> = None;
                let mut paused_streak: u8 = 0;
                let mut playing_streak: u8 = 0;

                loop {
                    gloo_timers::future::TimeoutFuture::new(200).await;

                    let Some(audio) = get_or_create_audio_element() else {
                        continue;
                    };

                    let time = audio.current_time();
                    if (time - last_emit).abs() >= 0.2 {
                        last_emit = time;
                        current_time_signal.set(time);
                        playback_pos.set(time);
                    }

                    let dur = audio.duration();
                    if !dur.is_nan() && (dur - last_duration).abs() > 0.5 {
                        last_duration = dur;
                        duration_signal.set(dur);
                    }
                    let paused = audio.paused();

                    if !paused && app_settings.peek().bookmark_auto_save {
                        if let Some(song) = now_playing.peek().clone() {
                            if can_save_server_bookmark(&song) {
                                let position_ms = (time * 1000.0).round().max(0.0) as u64;
                                if position_ms > 1500 {
                                    let should_save = match last_bookmark.peek().clone() {
                                        Some((id, pos)) => {
                                            id != song.id || position_ms.abs_diff(pos) >= 15_000
                                        }
                                        None => true,
                                    };
                                    if should_save {
                                        last_bookmark.set(Some((song.id.clone(), position_ms)));
                                        let servers_snapshot = servers.peek().clone();
                                        let bookmark_limit =
                                            app_settings.peek().bookmark_limit.clamp(1, 5000)
                                                as usize;
                                        let song_id = song.id.clone();
                                        let server_id = song.server_id.clone();
                                        spawn(async move {
                                            if let Some(server) = servers_snapshot
                                                .iter()
                                                .find(|s| s.id == server_id)
                                                .cloned()
                                            {
                                                let client = NavidromeClient::new(server);
                                                let _ = client
                                                    .create_bookmark_with_limit(
                                                        &song_id,
                                                        position_ms,
                                                        None,
                                                        Some(bookmark_limit),
                                                    )
                                                    .await;
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }

                    let current_song = { now_playing.read().clone() };
                    if current_song.is_some() {
                        // Keep UI play/pause signals synced when playback is controlled
                        // outside app buttons (browser media controls, hardware keys, etc.).
                        if paused {
                            paused_streak = paused_streak.saturating_add(1);
                            playing_streak = 0;
                        } else {
                            playing_streak = playing_streak.saturating_add(1);
                            paused_streak = 0;
                        }

                        if *is_playing.peek() && paused_streak >= 2 && !audio.ended() {
                            is_playing.set(false);
                        } else if !*is_playing.peek() && playing_streak >= 2 {
                            is_playing.set(true);
                        }

                        if let Some(message) =
                            web_playback_error_message(&audio, current_song.as_ref())
                        {
                            if playback_error_signal.peek().as_ref() != Some(&message) {
                                playback_error_signal.set(Some(message));
                            }
                        } else if playback_error_signal.peek().is_some() {
                            let has_started = time > 0.0 || (!dur.is_nan() && dur > 0.0) || !paused;
                            if has_started {
                                playback_error_signal.set(None);
                            }
                        }
                    } else {
                        paused_streak = 0;
                        playing_streak = 0;
                        if *is_playing.peek() {
                            is_playing.set(false);
                        }
                        if playback_error_signal.peek().is_some() {
                            playback_error_signal.set(None);
                        }
                    }

                    if audio.ended() {
                        let current_id = current_song.as_ref().map(|s| s.id.clone());
                        if ended_for_song == current_id {
                            continue;
                        }
                        ended_for_song = current_id.clone();

                        let queue_snapshot = { queue.read().clone() };
                        let idx = { *queue_index.read() };
                        let repeat = { *repeat_mode.read() };
                        let shuffle = { *shuffle_enabled.read() };
                        let servers_snapshot = { servers.read().clone() };

                        if repeat != RepeatMode::One {
                            repeat_one_replayed_song = None;
                        }

                        if let Some(song) = current_song.clone() {
                            scrobble_song(&servers_snapshot, &song, true);
                        }

                        if repeat == RepeatMode::One {
                            if let Some(song_id) = current_id.clone() {
                                if repeat_one_replayed_song.as_ref() != Some(&song_id) {
                                    repeat_one_replayed_song = Some(song_id);
                                    audio.set_current_time(0.0);
                                    if *is_playing.read() {
                                        let _ = audio.play();
                                    }
                                } else {
                                    repeat_one_replayed_song = None;
                                    is_playing.set(false);
                                }
                            } else {
                                is_playing.set(false);
                            }
                            continue;
                        }

                        let len = queue_snapshot.len();
                        if len == 0 {
                            is_playing.set(false);
                            continue;
                        }

                        let should_shuffle = repeat == RepeatMode::Off && shuffle;
                        if should_shuffle {
                            if idx < len.saturating_sub(1) {
                                if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                    queue_index.set(idx + 1);
                                    now_playing.set(Some(song));
                                }
                            } else {
                                spawn_shuffle_queue(
                                    servers_snapshot,
                                    queue.clone(),
                                    queue_index.clone(),
                                    now_playing.clone(),
                                    is_playing.clone(),
                                    current_song,
                                    Some(true),
                                );
                            }
                            continue;
                        }

                        if idx < len.saturating_sub(1) {
                            if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                queue_index.set(idx + 1);
                                now_playing.set(Some(song));
                            }
                        } else if repeat == RepeatMode::All {
                            if let Some(song) = queue_snapshot.get(0).cloned() {
                                queue_index.set(0);
                                now_playing.set(Some(song));
                            }
                        } else {
                            is_playing.set(false);
                        }
                    } else {
                        ended_for_song = None;
                    }
                }
            });
        });
    }

    // Keep queue_index aligned when now_playing changes and the song is in the queue.
    {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let song = now_playing();
            let queue_list = queue();
            if let Some(song) = song {
                if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
                    if pos != queue_index() {
                        defer_signal_update(move || {
                            queue_index.set(pos);
                        });
                    }
                }
            }
        });
    }

    // Update audio source and track changes for bookmarks/scrobbles.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let volume = volume.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut seek_request = seek_request.clone();
        let mut last_song_id = last_song_id.clone();
        let mut last_src = last_src.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_song_for_bookmark = last_song_for_bookmark.clone();
        use_effect(move || {
            let song = now_playing();
            let song_id = song.as_ref().map(|s| s.id.clone());
            let previous_song = last_song_for_bookmark.peek().clone();

            if let Some(prev) = previous_song {
                if Some(prev.id.clone()) != song_id {
                    let position_ms = get_or_create_audio_element()
                        .map(|a| a.current_time())
                        .unwrap_or(0.0)
                        .mul_add(1000.0, 0.0)
                        .round()
                        .max(0.0) as u64;
                    if position_ms > 1500 && can_save_server_bookmark(&prev) {
                        if app_settings.peek().bookmark_auto_save {
                            let servers_snapshot = servers.peek().clone();
                            let bookmark_limit =
                                app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                            let song_id = prev.id.clone();
                            let server_id = prev.server_id.clone();
                            spawn(async move {
                                last_bookmark.set(Some((song_id.clone(), position_ms)));
                                if let Some(server) =
                                    servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                                {
                                    let client = NavidromeClient::new(server);
                                    let _ = client
                                        .create_bookmark_with_limit(
                                            &song_id,
                                            position_ms,
                                            None,
                                            Some(bookmark_limit),
                                        )
                                        .await;
                                }
                            });
                        }
                    }
                }
            }

            let last_id = last_song_id.peek().clone();
            if song_id != last_id {
                last_song_id.set(song_id.clone());
                last_song_for_bookmark.set(song.clone());
            }

            let Some(song) = song else {
                if let Some(audio) = get_or_create_audio_element() {
                    let _ = audio.pause();
                    audio.set_src("");
                    let _ = audio.remove_attribute("src");
                    audio.load();
                }
                last_src.set(None);
                is_playing.set(false);
                audio_state.write().playback_error.set(None);
                return;
            };

            let servers_snapshot = servers.peek().clone();
            if let Some(url) = resolve_stream_url(&song, &servers_snapshot) {
                if Some(url.clone()) != *last_src.peek() {
                    last_src.set(Some(url.clone()));
                    audio_state.write().playback_error.set(None);
                    if let Some(audio) = get_or_create_audio_element() {
                        audio.set_src(&url);
                        audio.set_volume(volume.peek().clamp(0.0, 1.0));

                        if let Some((target_id, target_pos)) = seek_request.peek().clone() {
                            if target_id == song.id {
                                audio.set_current_time(target_pos);
                                let mut playback_position = playback_position.clone();
                                let mut audio_state = audio_state.clone();
                                defer_signal_update(move || {
                                    playback_position.set(target_pos);
                                    audio_state.write().current_time.set(target_pos);
                                    seek_request.set(None);
                                });
                            }
                        }

                        let was_playing = *is_playing.peek();
                        if has_user_interacted() && was_playing {
                            let _ = audio.play();
                        } else {
                            let _ = audio.pause();
                            is_playing.set(false);
                        }
                    }
                }

                scrobble_song(&servers_snapshot, &song, false);
            } else if let Some(audio) = get_or_create_audio_element() {
                audio.set_src("");
                last_src.set(None);
                is_playing.set(false);
                let message = if song.server_name == "Radio" {
                    let station_name = song
                        .album
                        .clone()
                        .or_else(|| song.artist.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| "this station".to_string());
                    format!("No station found: \"{station_name}\" has no playable stream URL.")
                } else {
                    "Unable to load this audio source.".to_string()
                };
                audio_state.write().playback_error.set(Some(message));
            }
        });
    }

    // Handle play/pause state changes.
    {
        let mut is_playing = is_playing.clone();
        use_effect(move || {
            let playing = is_playing();
            if let Some(audio) = get_or_create_audio_element() {
                if playing {
                    if has_user_interacted() {
                        if audio.paused() {
                            let _ = audio.play();
                        }
                    } else {
                        is_playing.set(false);
                    }
                } else if !audio.paused() {
                    let _ = audio.pause();
                }
            }
        });
    }

    // Handle repeat mode changes.
    {
        let repeat_mode = repeat_mode.clone();
        use_effect(move || {
            let _ = repeat_mode();
            if let Some(audio) = get_or_create_audio_element() {
                // Repeat behavior is handled in ended-event logic.
                audio.set_loop(false);
            }
        });
    }

    // Handle volume changes.
    {
        let volume = volume.clone();
        use_effect(move || {
            let vol = volume().clamp(0.0, 1.0);
            if let Some(audio) = get_or_create_audio_element() {
                audio.set_volume(vol);
            }
        });
    }

    // Persist a server bookmark when playback stops/pauses.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut last_bookmark = last_bookmark.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        use_effect(move || {
            let playing = is_playing();
            if playing {
                return;
            }

            let Some(song) = now_playing() else {
                return;
            };
            if !can_save_server_bookmark(&song) {
                return;
            }

            let position_ms = get_or_create_audio_element()
                .map(|a| a.current_time())
                .unwrap_or(0.0)
                .mul_add(1000.0, 0.0)
                .round()
                .max(0.0) as u64;

            if position_ms <= 1500 {
                return;
            }
            if !app_settings.peek().bookmark_auto_save {
                return;
            }

            let should_save = match last_bookmark.peek().clone() {
                Some((id, pos)) => id != song.id || position_ms.abs_diff(pos) >= 2000,
                None => true,
            };

            if should_save {
                let servers_snapshot = servers.peek().clone();
                let bookmark_limit = app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                let song_id = song.id.clone();
                let server_id = song.server_id.clone();
                spawn(async move {
                    last_bookmark.set(Some((song_id.clone(), position_ms)));
                    if let Some(server) =
                        servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                    {
                        let client = NavidromeClient::new(server);
                        let _ = client
                            .create_bookmark_with_limit(
                                &song_id,
                                position_ms,
                                None,
                                Some(bookmark_limit),
                            )
                            .await;
                    }
                });
            }
        });
    }

    rsx! {}
}

/// Seek to a specific position in the current track.
#[cfg(target_arch = "wasm32")]
pub fn seek_to(position: f64) {
    if let Some(audio) = get_or_create_audio_element() {
        audio.set_current_time(position);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn seek_to(position: f64) {
    native_audio_command(serde_json::json!({
        "type": "seek",
        "position": position.max(0.0),
    }));
}

/// Get the current playback position.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn get_current_time() -> f64 {
    get_or_create_audio_element()
        .map(|a| a.current_time())
        .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_current_time() -> f64 {
    0.0
}

/// Get the current track duration.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn get_duration() -> f64 {
    get_or_create_audio_element()
        .map(|a| {
            let d = a.duration();
            if d.is_nan() {
                0.0
            } else {
                d
            }
        })
        .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_duration() -> f64 {
    0.0
}

/// Helper function to play a song and keep queue/now_playing aligned.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn play_song(
    song: Song,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut is_playing: Signal<bool>,
) {
    let queue_list = queue.read().clone();
    if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
        queue_index.set(pos);
        now_playing.set(Some(song));
    } else {
        queue.set(vec![song.clone()]);
        queue_index.set(0);
        now_playing.set(Some(song));
    }
    is_playing.set(true);
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn play_song(
    song: Song,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut is_playing: Signal<bool>,
) {
    let queue_list = queue.read().clone();
    if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
        queue_index.set(pos);
        now_playing.set(Some(song));
    } else {
        queue.set(vec![song.clone()]);
        queue_index.set(0);
        now_playing.set(Some(song));
    }
    is_playing.set(true);
}
