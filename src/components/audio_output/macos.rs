//! Output backend for the Mac app. The page plays through `<audio>` in
//! WKWebView, which has `setSinkId` but lists no speakers until the page
//! holds a microphone permission (and has no `selectAudioOutput`), so it
//! can't offer a device list. The row names the default output device
//! (CoreAudio) and opens System Settings › Sound to change it.
//!
//! The name stays current through a CoreAudio property listener: its
//! callback (on a CoreAudio thread) only wakes a task, which reads the new
//! name and sets the signal.

use super::{mac_device_name, AudioOutput, OutputDevice, OutputPicker};
use dioxus::prelude::*;
use once_cell::sync::Lazy;
use std::ffi::{c_char, c_void, CStr};
use std::sync::Once;

const SOUND_SETTINGS_URL: &str = "x-apple.systempreferences:com.apple.Sound-Settings.extension";

#[repr(C)]
struct PropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

const SYSTEM_OBJECT: u32 = 1;
const DEFAULT_OUTPUT_DEVICE: PropertyAddress = PropertyAddress {
    selector: four_cc(b"dOut"),
    scope: four_cc(b"glob"),
    element: 0,
};
const OBJECT_NAME: PropertyAddress = PropertyAddress {
    selector: four_cc(b"lnam"),
    scope: four_cc(b"glob"),
    element: 0,
};
const UTF8: u32 = 0x0800_0100;

type ListenerProc =
    unsafe extern "C" fn(object: u32, count: u32, addresses: *const PropertyAddress, data: *mut c_void) -> i32;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectAddPropertyListener(
        object: u32,
        address: *const PropertyAddress,
        listener: ListenerProc,
        data: *mut c_void,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringGetCString(string: *const c_void, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
    fn CFRelease(object: *const c_void);
}

static DEVICE_CHANGED: Lazy<tokio::sync::Notify> = Lazy::new(tokio::sync::Notify::new);

unsafe extern "C" fn device_changed(_: u32, _: u32, _: *const PropertyAddress, _: *mut c_void) -> i32 {
    DEVICE_CHANGED.notify_one();
    0
}

/// The default output device's name, if CoreAudio gives one.
fn default_output_name() -> Option<String> {
    unsafe {
        let mut device: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &DEFAULT_OUTPUT_DEVICE,
            0,
            std::ptr::null(),
            &mut size,
            &mut device as *mut u32 as *mut c_void,
        );
        if status != 0 || device == 0 {
            return None;
        }
        let mut name: *const c_void = std::ptr::null();
        let mut size = std::mem::size_of::<*const c_void>() as u32;
        let status = AudioObjectGetPropertyData(
            device,
            &OBJECT_NAME,
            0,
            std::ptr::null(),
            &mut size,
            &mut name as *mut *const c_void as *mut c_void,
        );
        if status != 0 || name.is_null() {
            return None;
        }
        let mut buffer = [0 as c_char; 256];
        let ok = CFStringGetCString(name, buffer.as_mut_ptr(), buffer.len() as isize, UTF8);
        CFRelease(name);
        (ok != 0).then(|| CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned())
    }
}

pub(super) fn start(output: AudioOutput) {
    let (mut picker, mut route) = (output.picker, output.route);
    picker.set(OutputPicker::SoundSettings);
    route.set(Some(mac_device_name(&default_output_name().unwrap_or_default())));

    static LISTEN: Once = Once::new();
    LISTEN.call_once(|| unsafe {
        AudioObjectAddPropertyListener(
            SYSTEM_OBJECT,
            &DEFAULT_OUTPUT_DEVICE,
            device_changed,
            std::ptr::null_mut(),
        );
    });
    output.spawn(async move {
        loop {
            DEVICE_CHANGED.notified().await;
            let name = mac_device_name(&default_output_name().unwrap_or_default());
            if route.peek().as_deref() != Some(name.as_str()) {
                route.set(Some(name));
            }
        }
    });
}

pub(super) fn refresh_devices(_: AudioOutput) {}

pub(super) fn choose(_: AudioOutput, _: OutputDevice) {}

pub(super) fn ask(_: AudioOutput) {}

pub(super) fn open_system_picker() {
    // Waited for on a thread, so `open` doesn't linger as a zombie.
    std::thread::spawn(|| {
        let _ = std::process::Command::new("open").arg(SOUND_SETTINGS_URL).status();
    });
}
