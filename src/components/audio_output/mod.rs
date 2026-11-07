//! Audio output: where the sound goes (AirPlay or Bluetooth on iPhone, the
//! Mac's output device, a browser's speakers) and how to change it. One model
//! for every platform, `AudioOutput`, filled by one backend per platform:
//!
//! * iPhone (`ios.rs`): the AVAudioSession route, kept current by the
//!   route-change notification; changed in the system AirPlay sheet
//!   (`AVRoutePickerView`).
//! * Mac app (`macos.rs`): the default output device from CoreAudio, kept
//!   current by a property listener. WKWebView has `setSinkId`, but it lists
//!   no speakers without microphone permission, so the change happens in
//!   System Settings › Sound.
//! * Web and other desktops (`sink.rs`): `HTMLMediaElement.setSinkId` where
//!   it exists (Chromium), applied to both audio deck elements; the row is
//!   hidden elsewhere.
//!
//! Nothing here runs per frame: route names change on platform events, and
//! the device list is read when the Output row opens.

use dioxus::core::{current_scope_id, Runtime};
use dioxus::prelude::*;

#[cfg(target_os = "ios")]
#[path = "ios.rs"]
mod backend;
#[cfg(all(feature = "desktop", target_os = "macos"))]
#[path = "macos.rs"]
mod backend;
#[cfg(not(any(target_os = "ios", all(feature = "desktop", target_os = "macos"))))]
#[path = "sink.rs"]
mod backend;

/// How this platform changes the output.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OutputPicker {
    /// No way to choose (or not detected yet): the row is hidden.
    Hidden,
    /// iPhone: the system AirPlay / Bluetooth sheet.
    AirPlay,
    /// Mac app: System Settings › Sound.
    SoundSettings,
    /// A list of devices the page can switch between (`setSinkId`).
    #[cfg_attr(any(target_os = "ios", all(feature = "desktop", target_os = "macos")), allow(dead_code))]
    Devices,
}

/// One output the page can switch to. `id` "" is the system default.
#[derive(Clone, PartialEq, Debug)]
pub struct OutputDevice {
    pub id: String,
    pub label: String,
}

/// Context handle (provided by the shell, `use_audio_output_provider`).
#[derive(Clone, Copy, PartialEq)]
pub struct AudioOutput {
    pub picker: Signal<OutputPicker>,
    /// The current output's name ("iPhone", "AirPods Pro", "Living Room").
    pub route: Signal<Option<String>>,
    /// `Devices` only: what the Output row lists (system default first).
    pub devices: Signal<Vec<OutputDevice>>,
    /// `Devices` only: the chosen device id ("" = system default).
    pub selected: Signal<String>,
    /// `Devices` only: the browser can ask for another device itself
    /// (`navigator.mediaDevices.selectAudioOutput`, Firefox).
    pub can_ask: Signal<bool>,
    /// A network speaker playing instead of this device ("Living Room",
    /// set by `components::cast`): named wherever the route is.
    pub remote: Signal<Option<String>>,
    /// The shell's scope: backend tasks outlive the menu that started them.
    scope: ScopeId,
}

impl AudioOutput {
    fn spawn(&self, task: impl std::future::Future<Output = ()> + 'static) {
        Runtime::current().spawn(self.scope, task);
    }

    /// Name for the Output row's detail text.
    pub fn route_label(&self) -> String {
        (self.remote)()
            .or_else(|| (self.route)())
            .unwrap_or_else(|| SYSTEM_DEFAULT.to_string())
    }

    /// Reads the device list again (`Devices`; when the row opens).
    pub fn refresh(&self) {
        backend::refresh_devices(*self);
    }

    /// Sends both audio elements to `device` (`Devices`).
    pub fn choose(&self, device: OutputDevice) {
        backend::choose(*self, device);
    }

    /// `Devices` with `can_ask`: the browser's own device chooser. Call from
    /// the click itself (it needs the user gesture).
    pub fn ask(&self) {
        backend::ask(*self);
    }

    /// Opens the system's picker: the AirPlay sheet on iPhone, Sound
    /// settings on the Mac.
    pub fn open_system_picker(&self) {
        backend::open_system_picker();
    }
}

/// Creates the output model and starts this platform's backend. Call once,
/// from the shell.
pub fn use_audio_output_provider() -> AudioOutput {
    use_context_provider(|| {
        let output = AudioOutput {
            picker: Signal::new(OutputPicker::Hidden),
            route: Signal::new(None),
            devices: Signal::new(Vec::new()),
            selected: Signal::new(String::new()),
            can_ask: Signal::new(false),
            remote: Signal::new(None),
            scope: current_scope_id(),
        };
        backend::start(output);
        output
    })
}

pub const SYSTEM_DEFAULT: &str = "System Default";

/// One output port of an iPhone audio route: AVAudioSession's `portType`
/// (the constant's string value) and `portName`.
#[derive(Clone, PartialEq, Debug)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub struct RoutePort {
    pub kind: String,
    pub name: String,
}

/// The name of an iPhone route, the way Control Center says it: the device
/// ("iPhone") for its own speaker, the port name for AirPods or an AirPlay
/// speaker, and "A & B" / "A & 2 more" when audio goes to several.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn route_name(outputs: &[RoutePort], device: &str) -> String {
    let device = if device.trim().is_empty() { "iPhone" } else { device.trim() };
    let mut names: Vec<String> = Vec::new();
    for port in outputs {
        let name = port_name(port, device);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    match names.len() {
        0 => device.to_string(),
        1 => names.remove(0),
        2 => format!("{} & {}", names[0], names[1]),
        n => format!("{} & {} more", names[0], n - 1),
    }
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
fn port_name(port: &RoutePort, device: &str) -> String {
    let name = port.name.trim();
    match port.kind.as_str() {
        // The phone itself: its speaker or the earpiece.
        "Speaker" | "Receiver" => device.to_string(),
        "Headphones" => "Headphones".to_string(),
        _ if !name.is_empty() => name.to_string(),
        "AirPlay" => "AirPlay".to_string(),
        "BluetoothA2DPOutput" | "BluetoothLE" | "BluetoothHFP" => "Bluetooth".to_string(),
        "HDMIOutput" => "HDMI".to_string(),
        "CarAudio" => "CarPlay".to_string(),
        "USBAudio" => "USB Audio".to_string(),
        "LineOut" => "Line Out".to_string(),
        _ => device.to_string(),
    }
}

/// A Mac output device's name ("MacBook Pro Speakers"); "Mac" when CoreAudio
/// gives none.
#[cfg_attr(not(all(feature = "desktop", target_os = "macos")), allow(dead_code))]
pub fn mac_device_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "Mac".to_string()
    } else {
        name.to_string()
    }
}

/// The Output row's list from `enumerateDevices()` (`[kind, id, label]`):
/// the system default first, then each other speaker once. Ids the browser
/// hides (no permission) can't be chosen and are left out; unnamed ones are
/// numbered.
#[cfg_attr(any(target_os = "ios", all(feature = "desktop", target_os = "macos")), allow(dead_code))]
pub fn output_choices(raw: &[(String, String, String)]) -> Vec<OutputDevice> {
    let mut list = vec![OutputDevice {
        id: String::new(),
        label: SYSTEM_DEFAULT.to_string(),
    }];
    for (kind, id, label) in raw {
        let id = id.trim();
        // "default" and "communications" are aliases of real devices.
        if kind != "audiooutput" || id.is_empty() || id == "default" || id == "communications" {
            continue;
        }
        if list.iter().any(|device| device.id == id) {
            continue;
        }
        let label = label.trim();
        let label = if label.is_empty() {
            format!("Speaker {}", list.len())
        } else {
            label.to_string()
        };
        list.push(OutputDevice {
            id: id.to_string(),
            label,
        });
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(kind: &str, name: &str) -> RoutePort {
        RoutePort {
            kind: kind.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn the_phone_speaker_is_named_after_the_device() {
        assert_eq!(route_name(&[port("Speaker", "Speaker")], "iPhone"), "iPhone");
        assert_eq!(route_name(&[port("Receiver", "Receiver")], "iPad"), "iPad");
        assert_eq!(route_name(&[], ""), "iPhone");
    }

    #[test]
    fn other_ports_use_their_own_name() {
        assert_eq!(route_name(&[port("BluetoothA2DPOutput", "AirPods Pro")], "iPhone"), "AirPods Pro");
        assert_eq!(route_name(&[port("AirPlay", " Living Room ")], "iPhone"), "Living Room");
        assert_eq!(route_name(&[port("Headphones", "Headphones")], "iPhone"), "Headphones");
        assert_eq!(route_name(&[port("CarAudio", "")], "iPhone"), "CarPlay");
        assert_eq!(route_name(&[port("Mystery", "")], "iPhone"), "iPhone");
    }

    #[test]
    fn several_outputs_are_joined() {
        let two = [port("AirPlay", "Living Room"), port("AirPlay", "Kitchen")];
        assert_eq!(route_name(&two, "iPhone"), "Living Room & Kitchen");
        let three = [port("AirPlay", "Living Room"), port("AirPlay", "Kitchen"), port("AirPlay", "Den")];
        assert_eq!(route_name(&three, "iPhone"), "Living Room & 2 more");
        let same = [port("AirPlay", "Living Room"), port("AirPlay", "Living Room")];
        assert_eq!(route_name(&same, "iPhone"), "Living Room");
    }

    #[test]
    fn mac_names_fall_back() {
        assert_eq!(mac_device_name(" MacBook Pro Speakers "), "MacBook Pro Speakers");
        assert_eq!(mac_device_name(""), "Mac");
    }

    #[test]
    fn choices_start_with_the_default_and_skip_aliases() {
        let raw = |kind: &str, id: &str, label: &str| (kind.to_string(), id.to_string(), label.to_string());
        let list = output_choices(&[
            raw("audioinput", "mic", "Microphone"),
            raw("audiooutput", "default", "Default - Speakers"),
            raw("audiooutput", "communications", "Communications"),
            raw("audiooutput", "a1", "Speakers"),
            raw("audiooutput", "a1", "Speakers"),
            raw("audiooutput", "b2", ""),
            raw("audiooutput", "", "Hidden"),
        ]);
        let labels: Vec<&str> = list.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["System Default", "Speakers", "Speaker 2"]);
        assert_eq!(list[0].id, "");
        assert_eq!(list[2].id, "b2");
    }
}
