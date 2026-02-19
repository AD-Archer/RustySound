// Shared imports, state primitives, and browser-specific helper utilities.
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::*;
#[cfg(not(target_arch = "wasm32"))]
use crate::api::*;
#[cfg(target_arch = "wasm32")]
use crate::components::{
    PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal, VolumeSignal,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{
    PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal, VolumeSignal,
};
#[cfg(target_arch = "wasm32")]
use crate::db::{AppSettings, RepeatMode};
#[cfg(not(target_arch = "wasm32"))]
use crate::db::{AppSettings, RepeatMode};
#[cfg(not(target_arch = "wasm32"))]
use crate::offline_audio::{cached_audio_url, prefetch_song_audio};

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
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use std::cell::RefCell;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::collections::VecDeque;
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use std::collections::VecDeque;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::ptr;
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::sync::{Mutex, Once};
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlAudioElement, HtmlElement, KeyboardEvent};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::core::{IInspectable, HSTRING};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Media::Core::MediaSource;
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Media::MediaPlaybackType;
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Media::Playback::{MediaPlaybackState, MediaPlayer};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Media::{
    MediaPlaybackStatus, SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs,
};
#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
use windows::Storage::Streams::RandomAccessStreamReference;

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
fn web_try_play(audio: &HtmlAudioElement) {
    if let Ok(promise) = audio.play() {
        spawn(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    }
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

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_diag_enabled() -> bool {
    static IOS_AUDIO_DEBUG_ENABLED: Lazy<bool> = Lazy::new(|| {
        if cfg!(debug_assertions) {
            return true;
        }
        std::env::var("RUSTYSOUND_IOS_AUDIO_DEBUG")
            .map(|raw| {
                let normalized = raw.trim().to_ascii_lowercase();
                !(normalized.is_empty() || normalized == "0" || normalized == "false")
            })
            .unwrap_or(false)
    });
    *IOS_AUDIO_DEBUG_ENABLED
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_diag_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_diag_log(tag: &str, message: &str) {
    if !ios_diag_enabled() {
        return;
    }
    let ts = ios_diag_now_ms();
    let line = format!("[ios-audio][{ts}][{tag}] {message}");
    eprintln!("{line}");
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[derive(Clone, Debug)]
struct IosPlaybackPlanItem {
    song_id: String,
    src: Option<String>,
    meta: NativeTrackMetadata,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[derive(Clone, Debug)]
struct IosPlaybackPlan {
    items: Vec<IosPlaybackPlanItem>,
    index: usize,
    repeat_mode: RepeatMode,
    shuffle: bool,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl Default for IosPlaybackPlan {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: 0,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_PLAYBACK_PLAN: Lazy<Mutex<IosPlaybackPlan>> =
    Lazy::new(|| Mutex::new(IosPlaybackPlan::default()));

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_update_playback_plan(
    items: Vec<IosPlaybackPlanItem>,
    index: usize,
    repeat_mode: RepeatMode,
    shuffle: bool,
) {
    if let Ok(mut plan) = IOS_PLAYBACK_PLAN.lock() {
        plan.items = items;
        plan.index = index.min(plan.items.len().saturating_sub(1));
        plan.repeat_mode = repeat_mode;
        plan.shuffle = shuffle;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_plan_sync_current_song(song_id: Option<&str>) {
    let Some(song_id) = song_id else {
        return;
    };
    if let Ok(mut plan) = IOS_PLAYBACK_PLAN.lock() {
        if let Some(index) = plan.items.iter().position(|item| item.song_id == song_id) {
            plan.index = index;
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_plan_take_transition(action: &str) -> Option<IosPlaybackPlanItem> {
    let mut plan = IOS_PLAYBACK_PLAN.lock().ok()?;
    let len = plan.items.len();
    if len == 0 {
        return None;
    }

    let current = plan.index.min(len.saturating_sub(1));
    let target = match action {
        "next" | "ended" => {
            if current + 1 < len {
                Some(current + 1)
            } else if plan.repeat_mode == RepeatMode::All {
                Some(0)
            } else {
                None
            }
        }
        "previous" => {
            if current > 0 {
                Some(current - 1)
            } else if plan.repeat_mode == RepeatMode::All {
                Some(len.saturating_sub(1))
            } else {
                None
            }
        }
        _ => None,
    }?;

    let item = plan.items.get(target)?.clone();
    if item.src.is_none() {
        return None;
    }

    plan.index = target;
    Some(item)
}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
fn ios_diag_now_ms() -> u128 {
    0
}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
fn ios_diag_log(_tag: &str, _message: &str) {}

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
