//! Output backend for the web build and the non-Mac desktops (WebView2 is
//! Chromium): `HTMLMediaElement.setSinkId`, applied by the audio deck to both
//! of its elements.
//!
//! Browsers only name and number their speakers once the page holds a media
//! permission (Chromium and WebKit list one nameless output before that), and
//! a list holding only "System Default" offers nothing. So the row shows when
//! `setSinkId` exists and there is something to pick: listed speakers, or
//! the browser's own chooser (`selectAudioOutput`, Firefox). Plugging a
//! device in or out (`devicechange`) reads the list again.

use super::{output_choices, AudioOutput, OutputDevice, OutputPicker};
use dioxus::prelude::*;

/// What the page supports: [setSinkId, selectAudioOutput].
const DETECT_JS: &str = r#"
const media = typeof HTMLMediaElement !== "undefined" && typeof HTMLMediaElement.prototype.setSinkId === "function";
const ask = !!(navigator.mediaDevices && typeof navigator.mediaDevices.selectAudioOutput === "function");
return [media, ask];
"#;

/// Every device as [kind, id, label].
const ENUMERATE_JS: &str = r#"
if (!navigator.mediaDevices || !navigator.mediaDevices.enumerateDevices) return [];
try {
  const all = await navigator.mediaDevices.enumerateDevices();
  return all.map((d) => [d.kind || "", d.deviceId || "", d.label || ""]);
} catch (_) {
  return [];
}
"#;

/// Sends once per `devicechange`; never resolves, so its channel stays open.
const DEVICE_CHANGE_JS: &str = r#"
if (navigator.mediaDevices && navigator.mediaDevices.addEventListener) {
  navigator.mediaDevices.addEventListener("devicechange", () => dioxus.send(true));
}
await new Promise(() => {});
"#;

pub(super) fn start(output: AudioOutput) {
    output.spawn(async move {
        let Ok((media, ask)) = document::eval(DETECT_JS).join::<(bool, bool)>().await else {
            return;
        };
        if !media {
            return;
        }
        let mut can_ask = output.can_ask;
        can_ask.set(ask);
        read_devices(output).await;
        let mut changes = document::eval(DEVICE_CHANGE_JS);
        while changes.recv::<bool>().await.is_ok() {
            read_devices(output).await;
        }
    });
}

/// Reads the device list and shows or hides the row to match. A chosen
/// device that went away falls back to the default (as the browser does).
async fn read_devices(output: AudioOutput) {
    let raw = document::eval(ENUMERATE_JS)
        .join::<Vec<(String, String, String)>>()
        .await
        .unwrap_or_default();
    let devices = output_choices(&raw);
    let (mut list, mut picker, mut selected, mut route) =
        (output.devices, output.picker, output.selected, output.route);
    let usable = devices.len() > 1 || *output.can_ask.peek();
    let chosen = selected.peek().clone();
    if !chosen.is_empty() && !devices.iter().any(|device| device.id == chosen) && !*output.can_ask.peek() {
        selected.set(String::new());
        route.set(None);
    }
    if *list.peek() != devices {
        list.set(devices);
    }
    let next = if usable { OutputPicker::Devices } else { OutputPicker::Hidden };
    if *picker.peek() != next {
        picker.set(next);
    }
}

pub(super) fn refresh_devices(output: AudioOutput) {
    output.spawn(read_devices(output));
}

/// Moves both deck elements (and any later ones) to `device`. On failure
/// (the device went away) the selection stays where it was.
pub(super) fn choose(output: AudioOutput, device: OutputDevice) {
    let id = serde_json::to_string(&device.id).unwrap_or_else(|_| "\"\"".to_string());
    let script = format!(
        r#"
const deck = window.__rsAudioDeck;
if (!deck || !deck.setSinkId) {{ window.__rsSinkId = {id}; return true; }}
try {{ await deck.setSinkId({id}); return true; }} catch (_) {{ return false; }}
"#
    );
    output.spawn(async move {
        let ok = document::eval(&script).join::<bool>().await.unwrap_or(false);
        if ok {
            let (mut selected, mut route) = (output.selected, output.route);
            route.set((!device.id.is_empty()).then(|| device.label.clone()));
            selected.set(device.id);
        }
    });
}

/// `selectAudioOutput()` must start inside the click: the script runs as
/// it is created, while the gesture is still live.
pub(super) fn ask(output: AudioOutput) {
    let chooser = document::eval(
        r#"
try {
  const d = await navigator.mediaDevices.selectAudioOutput();
  return [d.deviceId || "", d.label || ""];
} catch (_) {
  return null;
}
"#,
    );
    output.spawn(async move {
        if let Ok(Some((id, label))) = chooser.join::<Option<(String, String)>>().await {
            choose(output, OutputDevice { id, label });
        }
    });
}

pub(super) fn open_system_picker() {}
