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
use crate::offline_audio::{
    cached_audio_url, is_song_downloaded, prefetch_song_audio_with_origin, DownloadOrigin,
};

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
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::ffi::CStr;
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
use std::os::raw::c_char;
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
    pub is_transport_loading: Signal<bool>,
    pub transport_loading_label: Signal<Option<String>>,
    #[allow(dead_code)]
    pub is_initialized: Signal<bool>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            current_time: Signal::new(0.0),
            duration: Signal::new(0.0),
            playback_error: Signal::new(None),
            is_transport_loading: Signal::new(false),
            transport_loading_label: Signal::new(None),
            is_initialized: Signal::new(false),
        }
    }
}

/// Writes only the inner signals, and only when they change. (Taking
/// `audio_state.write()` here notified every reader of the outer AudioState
/// signal on each 200/250 ms poll tick, even when nothing changed.)
fn set_transport_loading(audio_state: Signal<AudioState>, loading: bool, message: Option<&str>) {
    let (mut loading_signal, mut label_signal) = {
        let state = audio_state.peek();
        (state.is_transport_loading, state.transport_loading_label)
    };
    let next_label = if loading {
        message.map(|value| value.to_string())
    } else {
        None
    };
    if *loading_signal.peek() != loading {
        loading_signal.set(loading);
    }
    if *label_signal.peek() != next_label {
        label_signal.set(next_label);
    }
}

/// The page's audio deck (`audio_deck.js`): two `<audio>` elements, for
/// gapless starts and crossfades. Installed on first use.
#[cfg(target_arch = "wasm32")]
fn web_audio_deck() -> Option<wasm_bindgen::JsValue> {
    let win = window()?;
    let mut factory = js_sys::Reflect::get(&win, &"__rsAudioDeck".into()).ok()?;
    if !factory.is_object() {
        let _ = js_sys::eval(include_str!("audio_deck.js"));
        factory = js_sys::Reflect::get(&win, &"__rsAudioDeck".into()).ok()?;
    }
    let install = js_sys::Reflect::get(&factory, &"install".into())
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    install.call1(&factory, &"rustysound-audio".into()).ok()
}

/// Calls `deck.<method>(...args)`.
#[cfg(target_arch = "wasm32")]
fn web_deck_call(method: &str, args: &[wasm_bindgen::JsValue]) -> Option<wasm_bindgen::JsValue> {
    let deck = web_audio_deck()?;
    let function = js_sys::Reflect::get(&deck, &method.into())
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let list = args.iter().cloned().collect::<js_sys::Array>();
    function.apply(&deck, &list).ok()
}

/// The current audio element (the deck swaps which one that is on a gapless
/// handover, and moves the `rustysound-audio` id with it).
#[cfg(target_arch = "wasm32")]
pub fn get_or_create_audio_element() -> Option<HtmlAudioElement> {
    let document = window()?.document()?;
    if let Some(existing) = document.get_element_by_id("rustysound-audio") {
        return existing.dyn_into::<HtmlAudioElement>().ok();
    }
    web_audio_deck()?;
    document
        .get_element_by_id("rustysound-audio")?
        .dyn_into::<HtmlAudioElement>()
        .ok()
}

/// Plays `src` on the current element (or switches to the standby one when
/// it already holds `src`) and returns the element now current.
#[cfg(target_arch = "wasm32")]
fn web_deck_load(src: &str) -> Option<HtmlAudioElement> {
    web_deck_call("load", &[src.into()])?
        .dyn_into::<HtmlAudioElement>()
        .ok()
}

/// Arms the standby element with the next song, or disarms it (`None`).
#[cfg(target_arch = "wasm32")]
fn web_deck_arm(arm: Option<&DeckArm>) {
    let Some(arm) = arm else {
        let _ = web_deck_call("disarm", &[]);
        return;
    };
    let payload = serde_json::json!({
        "src": arm.src,
        "from": arm.from_id,
        "to": arm.song_id,
        "volume": arm.volume,
        "fadeMs": arm.fade_ms,
    });
    if let Ok(value) = js_sys::JSON::parse(&payload.to_string()) {
        let _ = web_deck_call("arm", &[value]);
    }
}

/// The oldest gapless handover the deck hasn't reported yet, as
/// `(finished song id, next song id)`.
#[cfg(target_arch = "wasm32")]
fn web_deck_take_handover() -> Option<(String, String)> {
    let raw = web_deck_call("takeHandover", &[])?.as_string()?;
    parse_handover(&raw)
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

/// Called once, from a capture-phase listener inside the first user gesture.
/// WebKit lifts an element's "user gesture required" restriction the first
/// time `play()` runs on it during a gesture, even without a source, so later
/// programmatic `play()` calls (after the track URL resolves, or on a gapless
/// handover) are allowed. The deck does this for both of its elements, and
/// only touches one that has no source yet, so nothing audible starts.
#[cfg(target_arch = "wasm32")]
fn web_unlock_audio_element() {
    let _ = web_deck_call("unlock", &[]);
}

/// Sticky user activation as the browser sees it (`navigator.userActivation`),
/// used as a second opinion next to the app's own interaction flag.
#[cfg(target_arch = "wasm32")]
fn web_has_sticky_activation() -> bool {
    let Some(win) = window() else {
        return false;
    };
    let Ok(navigator) = js_sys::Reflect::get(&win, &"navigator".into()) else {
        return false;
    };
    js_sys::Reflect::get(&navigator, &"userActivation".into())
        .ok()
        .filter(|value| value.is_object())
        .and_then(|activation| js_sys::Reflect::get(&activation, &"hasBeenActive".into()).ok())
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
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

    // Space, the arrows and the rest are the shortcut registry's
    // (command_palette/shortcuts.rs); only hardware media keys stay here.
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
        std::env::var("RUSTYSOUND_IOS_AUDIO_DEBUG")
            .map(|raw| {
                let normalized = raw.trim().to_ascii_lowercase();
                !(normalized.is_empty()
                    || normalized == "0"
                    || normalized == "false"
                    || normalized == "off"
                    || normalized == "no")
            })
            // Keep iOS audio diagnostics on by default so real-device remote-control
            // regressions (lock-screen/Control Center commands) are observable.
            // Set RUSTYSOUND_IOS_AUDIO_DEBUG=0 to silence.
            .unwrap_or(true)
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
static IOS_DIAG_BUFFER: Lazy<Mutex<VecDeque<String>>> = Lazy::new(|| Mutex::new(VecDeque::new()));

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
const IOS_DIAG_BUFFER_MAX_LINES: usize = 1200;

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_diag_log(tag: &str, message: &str) {
    if !ios_diag_enabled() {
        return;
    }
    let ts = ios_diag_now_ms();
    let line = format!("[ios-audio][{ts}][{tag}] {message}");
    eprintln!("{line}");
    if let Ok(mut buffer) = IOS_DIAG_BUFFER.lock() {
        buffer.push_back(line);
        while buffer.len() > IOS_DIAG_BUFFER_MAX_LINES {
            let _ = buffer.pop_front();
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_audio_log_snapshot(max_lines: usize) -> Vec<String> {
    let requested = max_lines.max(1).min(IOS_DIAG_BUFFER_MAX_LINES);
    let Ok(buffer) = IOS_DIAG_BUFFER.lock() else {
        return Vec::new();
    };
    let start = buffer.len().saturating_sub(requested);
    buffer.iter().skip(start).cloned().collect()
}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
pub fn ios_audio_log_snapshot(_max_lines: usize) -> Vec<String> {
    Vec::new()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_audio_log_clear() {
    if let Ok(mut buffer) = IOS_DIAG_BUFFER.lock() {
        buffer.clear();
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
unsafe fn ios_active_view_controller() -> *mut Object {
    let app_cls = class!(UIApplication);
    let app: *mut Object = msg_send![app_cls, sharedApplication];
    if app.is_null() {
        return ptr::null_mut();
    }

    let mut root: *mut Object = ptr::null_mut();

    let key_window: *mut Object = msg_send![app, keyWindow];
    if !key_window.is_null() {
        root = msg_send![key_window, rootViewController];
    }

    if root.is_null() {
        let windows: *mut Object = msg_send![app, windows];
        if !windows.is_null() {
            let count: usize = msg_send![windows, count];
            for index in 0..count {
                let window: *mut Object = msg_send![windows, objectAtIndex: index];
                if window.is_null() {
                    continue;
                }
                let candidate: *mut Object = msg_send![window, rootViewController];
                if candidate.is_null() {
                    continue;
                }
                let is_key_window: BOOL = msg_send![window, isKeyWindow];
                root = candidate;
                if is_key_window == YES {
                    break;
                }
            }
        }
    }

    if root.is_null() {
        return ptr::null_mut();
    }

    loop {
        let presented: *mut Object = msg_send![root, presentedViewController];
        if presented.is_null() {
            break;
        }
        root = presented;
    }

    root
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_audio_log_export_txt(contents: &str) -> Result<String, String> {
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return Err("No iOS logs to export yet.".to_string());
    }

    let mut export_path = std::env::temp_dir();
    export_path.push(format!("rustysound-ios-audio-{}.txt", ios_diag_now_ms()));
    std::fs::write(&export_path, trimmed)
        .map_err(|error| format!("Failed to write log file: {error}"))?;

    unsafe {
        let root = ios_active_view_controller();
        if root.is_null() {
            return Err("Unable to find an active iOS view controller.".to_string());
        }

        let Some(path_string) = export_path.to_str() else {
            return Err("Export path is not valid UTF-8.".to_string());
        };
        let Some(path_ns) = ns_string(path_string) else {
            return Err("Failed to encode export path.".to_string());
        };

        let url_cls = class!(NSURL);
        let file_url: *mut Object = msg_send![url_cls, fileURLWithPath: path_ns];
        let _: () = msg_send![path_ns, release];
        if file_url.is_null() {
            return Err("Failed to create iOS file URL for export.".to_string());
        }

        let array_cls = class!(NSArray);
        let items: *mut Object = msg_send![array_cls, arrayWithObject: file_url];
        if items.is_null() {
            return Err("Failed to create iOS activity items.".to_string());
        }

        let activity_cls = class!(UIActivityViewController);
        let activity_alloc: *mut Object = msg_send![activity_cls, alloc];
        if activity_alloc.is_null() {
            return Err("Failed to allocate iOS share sheet.".to_string());
        }
        let activity: *mut Object = msg_send![
            activity_alloc,
            initWithActivityItems: items
            applicationActivities: ptr::null_mut::<Object>()
        ];
        if activity.is_null() {
            return Err("Failed to initialize iOS share sheet.".to_string());
        }

        let _: () = msg_send![
            root,
            presentViewController: activity
            animated: YES
            completion: ptr::null_mut::<Object>()
        ];
        let _: () = msg_send![activity, release];
    }

    Ok(export_path.display().to_string())
}

/// Opens the iOS share sheet on an image (PNG or JPEG bytes), as a `UIImage`,
/// so it offers Save Image, Messages, Instagram and the rest. Used by Lyrics
/// Shot and Wrapped instead of `navigator.share`: the page captures the image
/// first, which takes long enough that WebKit no longer counts the tap as a
/// user gesture and refuses the web share. On iPad the sheet is a popover
/// from the middle of the screen (it must be anchored somewhere).
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn ios_share_image(bytes: &[u8]) -> Result<(), String> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    unsafe impl objc::Encode for CGRect {
        fn encode() -> objc::Encoding {
            unsafe { objc::Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
        }
    }

    if bytes.is_empty() {
        return Err("No image to share.".to_string());
    }
    unsafe {
        let root = ios_active_view_controller();
        if root.is_null() {
            return Err("Unable to find an active iOS view controller.".to_string());
        }
        let data: *mut Object = msg_send![
            class!(NSData),
            dataWithBytes: bytes.as_ptr() as *const std::ffi::c_void
            length: bytes.len()
        ];
        if data.is_null() {
            return Err("Failed to copy the image.".to_string());
        }
        let image: *mut Object = msg_send![class!(UIImage), imageWithData: data];
        if image.is_null() {
            return Err("The image could not be read.".to_string());
        }
        let items: *mut Object = msg_send![class!(NSArray), arrayWithObject: image];
        let alloc: *mut Object = msg_send![class!(UIActivityViewController), alloc];
        let activity: *mut Object = msg_send![
            alloc,
            initWithActivityItems: items
            applicationActivities: ptr::null_mut::<Object>()
        ];
        if activity.is_null() {
            return Err("Failed to open the share sheet.".to_string());
        }
        let popover: *mut Object = msg_send![activity, popoverPresentationController];
        if !popover.is_null() {
            let view: *mut Object = msg_send![root, view];
            if !view.is_null() {
                let bounds: CGRect = msg_send![view, bounds];
                let anchor = CGRect {
                    origin: CGPoint { x: bounds.size.width / 2.0, y: bounds.size.height / 2.0 },
                    size: CGSize { width: 1.0, height: 1.0 },
                };
                let _: () = msg_send![popover, setSourceView: view];
                let _: () = msg_send![popover, setSourceRect: anchor];
                let _: () = msg_send![popover, setPermittedArrowDirections: 0usize];
            }
        }
        let _: () = msg_send![
            root,
            presentViewController: activity
            animated: YES
            completion: ptr::null_mut::<Object>()
        ];
        let _: () = msg_send![activity, release];
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
pub fn ios_audio_log_clear() {}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
pub fn ios_audio_log_export_txt(_contents: &str) -> Result<String, String> {
    Err("iOS log export is only available in iOS builds.".to_string())
}

/// One queue entry as the native player (iOS, Android) sees it.
#[cfg(all(not(target_arch = "wasm32"), any(target_os = "ios", target_os = "android")))]
#[derive(Clone, Debug, Serialize)]
struct NativePlanItem {
    song_id: String,
    src: Option<String>,
    meta: NativeTrackMetadata,
    /// Levelling multiplier, applied when the native player moves on by itself.
    gain: f64,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
#[derive(Clone, Debug)]
struct IosPlaybackPlan {
    items: Vec<NativePlanItem>,
    index: usize,
    repeat_mode: RepeatMode,
    shuffle: bool,
    /// Queue the next song's item behind the current one (gapless).
    gapless: bool,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
impl Default for IosPlaybackPlan {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            index: 0,
            repeat_mode: RepeatMode::Off,
            shuffle: false,
            gapless: true,
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
static IOS_PLAYBACK_PLAN: Lazy<Mutex<IosPlaybackPlan>> =
    Lazy::new(|| Mutex::new(IosPlaybackPlan::default()));

/// The queue (the `range` part of it) as the native player sees it: sources,
/// lock-screen metadata and levelling per song, so it can move on while the
/// web view is suspended.
#[cfg(all(not(target_arch = "wasm32"), any(target_os = "ios", target_os = "android")))]
fn native_plan_items(
    queue: &[Song],
    range: impl std::ops::RangeBounds<usize>,
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> Vec<NativePlanItem> {
    let start = match range.start_bound() {
        std::ops::Bound::Included(&i) => i,
        std::ops::Bound::Excluded(&i) => i + 1,
        std::ops::Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        std::ops::Bound::Included(&i) => i + 1,
        std::ops::Bound::Excluded(&i) => i,
        std::ops::Bound::Unbounded => queue.len(),
    }
    .min(queue.len());
    queue
        .iter()
        .enumerate()
        .take(end)
        .skip(start)
        .map(|(index, entry)| NativePlanItem {
            song_id: entry.id.clone(),
            src: resolve_stream_url(entry, servers, settings.offline_mode),
            meta: song_metadata(entry, servers),
            gain: levelling_gain(
                settings,
                index.checked_sub(1).and_then(|i| queue.get(i)),
                entry,
                queue.get(index + 1),
            ),
        })
        .collect()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_update_playback_plan(
    items: Vec<NativePlanItem>,
    index: usize,
    repeat_mode: RepeatMode,
    shuffle: bool,
    gapless: bool,
) {
    if let Ok(mut plan) = IOS_PLAYBACK_PLAN.lock() {
        plan.items = items;
        plan.index = index.min(plan.items.len().saturating_sub(1));
        plan.repeat_mode = repeat_mode;
        plan.shuffle = shuffle;
        plan.gapless = gapless;
    }
    // Re-queue the next song's item to match the new plan.
    let _ = with_ios_player(|player| player.sync_next_item());
}

/// The song AVQueuePlayer should start when the current one ends, if
/// gapless applies (`gapless.rs`: not for repeat-one or radio).
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_plan_peek_next() -> Option<NativePlanItem> {
    let plan = IOS_PLAYBACK_PLAN.lock().ok()?;
    if !plan.gapless {
        return None;
    }
    let len = plan.items.len();
    let current = plan.index.min(len.saturating_sub(1));
    let target = next_index(len, current, plan.repeat_mode)?;
    let item = plan.items.get(target)?;
    if plan.items.get(current)?.meta.is_live || item.meta.is_live || item.src.is_none() {
        return None;
    }
    Some(item.clone())
}

/// Moves the plan on to `song_id` after a gapless advance (the next entry,
/// else the first with that id) and returns its item.
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_plan_advance_to(song_id: &str) -> Option<NativePlanItem> {
    let mut plan = IOS_PLAYBACK_PLAN.lock().ok()?;
    let len = plan.items.len();
    let current = plan.index.min(len.saturating_sub(1));
    let target = next_index(len, current, plan.repeat_mode)
        .filter(|&i| plan.items[i].song_id == song_id)
        .or_else(|| plan.items.iter().position(|item| item.song_id == song_id))?;
    plan.index = target;
    plan.items.get(target).cloned()
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
fn ios_plan_take_transition(action: &str) -> Option<NativePlanItem> {
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
        "previous" => crate::components::player::skip_back::previous_index(
            current,
            len,
            plan.repeat_mode,
        ),
        _ => None,
    }?;

    let item = plan.items.get(target)?.clone();
    if item.src.is_none() {
        return None;
    }

    plan.index = target;
    Some(item)
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_plan_queue_stats() -> Option<(usize, usize)> {
    let plan = IOS_PLAYBACK_PLAN.lock().ok()?;
    let len = plan.items.len();
    if len == 0 {
        return None;
    }
    let index = plan.index.min(len.saturating_sub(1));
    Some((index, len))
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn ios_diag_now_ms() -> u128 {
    0
}

#[cfg(any(target_arch = "wasm32", not(target_os = "ios")))]
pub fn ios_diag_log(_tag: &str, _message: &str) {}

#[cfg(target_arch = "wasm32")]
fn web_sync_media_session_metadata(song: Option<&Song>, servers: &[ServerConfig]) {
    let payload = match song.map(|song| track_labels(song, servers)) {
        Some(labels) => serde_json::json!({
            "title": labels.title,
            "artist": labels.artist,
            "album": labels.album,
            "artwork": labels.artwork,
        }),
        None => serde_json::Value::Null,
    };

    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string());
    let script = format!(
        r#"
(() => {{
  if (!("mediaSession" in navigator)) return false;
  const meta = {payload};
  try {{
    if (!meta || typeof MediaMetadata === "undefined") {{
      navigator.mediaSession.metadata = null;
      return true;
    }}

    const artwork = meta.artwork
      ? [{{ src: meta.artwork, sizes: "512x512", type: "image/png" }}]
      : undefined;

    navigator.mediaSession.metadata = new MediaMetadata({{
      title: meta.title || "",
      artist: meta.artist || "",
      album: meta.album || "",
      artwork,
    }});
    return true;
  }} catch (_err) {{
    return false;
  }}
}})();
"#
    );
    let _ = js_sys::eval(&script);
}

#[cfg(target_arch = "wasm32")]
fn ensure_web_media_session_shortcuts() {
    let _ = js_sys::eval(
        r#"
(() => {
  if (window.__rustysoundWebMediaSessionInit) {
    return true;
  }

  // The deck's current element changes on gapless handovers.
  const factory = window.__rsAudioDeck;
  if (!factory) {
    return false;
  }
  const deck = factory.install("rustysound-audio");
  const audio = () => deck.current();

  if (!("mediaSession" in navigator)) {
    window.__rustysoundWebMediaSessionInit = true;
    return true;
  }

  const clickById = (id) => {
    const element = document.getElementById(id);
    if (element && typeof element.click === "function") {
      element.click();
      return true;
    }
    return false;
  };

  const updatePlaybackState = () => {
    try {
      navigator.mediaSession.playbackState = audio().paused ? "paused" : "playing";
    } catch (_err) {}
  };

  const updatePositionState = () => {
    if (!navigator.mediaSession.setPositionState) return;
    if (!Number.isFinite(audio().duration) || audio().duration <= 0) return;
    try {
      navigator.mediaSession.setPositionState({
        duration: audio().duration,
        playbackRate: audio().playbackRate || 1,
        position: Math.max(0, Math.min(audio().currentTime || 0, audio().duration)),
      });
    } catch (_err) {}
  };

  try {
    navigator.mediaSession.setActionHandler("play", () => {
      if (audio().paused) {
        if (!clickById("play-pause-btn")) {
          audio().play().catch(() => {});
        }
      }
      updatePlaybackState();
    });
  } catch (_err) {}
  try {
    navigator.mediaSession.setActionHandler("pause", () => {
      if (!audio().paused) {
        if (!clickById("play-pause-btn")) {
          audio().pause();
        }
      }
      updatePlaybackState();
    });
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
          audio().currentTime = Math.max(0, details.seekTime);
        } catch (_err) {}
        updatePositionState();
      }
    });
  } catch (_err) {}

  deck.on("play", updatePlaybackState);
  deck.on("pause", updatePlaybackState);
  deck.on("timeupdate", updatePositionState);
  deck.on("durationchange", updatePositionState);
  deck.on("ratechange", updatePositionState);
  updatePlaybackState();
  updatePositionState();

  window.__rustysoundWebMediaSessionInit = true;
  return true;
})();
"#,
    );
}
