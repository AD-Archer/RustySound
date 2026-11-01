//! Output backend for iPhone: AirPlay and Bluetooth through the system
//! route picker.
//!
//! The UI is a web view, so the picker is a native `AVRoutePickerView` kept
//! off to the side in the app's window (nearly transparent, never in the
//! way) and opened by sending its button a tap. That works from any web
//! control (the Output row, the AirPlay glyph) without keeping a native view
//! lined up over moving web content: an overlay would have to follow the
//! sheet's drag, scroll, rotation and Dynamic Type, and would sit above web
//! menus that should cover it.
//!
//! The route name comes from `AVAudioSession.currentRoute`, read again when
//! `AVAudioSessionRouteChangeNotification` fires: the observer only wakes a
//! task, which reads the route and sets the signal.
#![allow(unexpected_cfgs)] // objc 0.2 macros probe `cfg(feature = "cargo-clippy")`.

use super::{route_name, AudioOutput, OutputDevice, OutputPicker, RoutePort};
use dioxus::prelude::*;
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL, NO};
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
use once_cell::sync::Lazy;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Mutex, Once};

#[link(name = "AVKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVAudioSessionRouteChangeNotification: *mut Object;
}

unsafe extern "C" {
    /// The main dispatch queue (`dispatch_get_main_queue()` is a macro for its address).
    static _dispatch_main_q: u8;
    fn dispatch_async_f(queue: *const u8, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

unsafe impl Encode for CGRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

/// `UIControlEventTouchUpInside`.
const TOUCH_UP_INSIDE: usize = 1 << 6;
const PICKER_SIZE: f64 = 44.0;

static ROUTE_CHANGED: Lazy<tokio::sync::Notify> = Lazy::new(tokio::sync::Notify::new);
/// The app's `UIWindow` and the retained picker, as usize so the statics are
/// Send; both are only touched on the main thread.
static WINDOW: Mutex<usize> = Mutex::new(0);
static PICKER: Mutex<usize> = Mutex::new(0);

extern "C" fn route_changed(_: &Object, _: Sel, _: *mut Object) {
    ROUTE_CHANGED.notify_one();
}

unsafe fn ns_to_string(value: *mut Object) -> String {
    if value.is_null() {
        return String::new();
    }
    let utf8: *const c_char = msg_send![value, UTF8String];
    if utf8.is_null() {
        return String::new();
    }
    CStr::from_ptr(utf8).to_string_lossy().into_owned()
}

/// "iPhone", "AirPods Pro", "Living Room" (see `route_name`).
fn current_route_name() -> String {
    unsafe {
        let device: *mut Object = msg_send![class!(UIDevice), currentDevice];
        let model = if device.is_null() {
            String::new()
        } else {
            ns_to_string(msg_send![device, model])
        };
        let session: *mut Object = msg_send![class!(AVAudioSession), sharedInstance];
        if session.is_null() {
            return route_name(&[], &model);
        }
        let route: *mut Object = msg_send![session, currentRoute];
        let outputs: *mut Object = if route.is_null() {
            std::ptr::null_mut()
        } else {
            msg_send![route, outputs]
        };
        let mut ports = Vec::new();
        if !outputs.is_null() {
            let count: usize = msg_send![outputs, count];
            for index in 0..count {
                let port: *mut Object = msg_send![outputs, objectAtIndex: index];
                if port.is_null() {
                    continue;
                }
                ports.push(RoutePort {
                    kind: ns_to_string(msg_send![port, portType]),
                    name: ns_to_string(msg_send![port, portName]),
                });
            }
        }
        route_name(&ports, &model)
    }
}

/// Registers the route-change observer once.
unsafe fn observe_route_changes() {
    static OBSERVE: Once = Once::new();
    OBSERVE.call_once(|| {
        let Some(mut decl) = ClassDecl::new("RustySoundRouteObserver", class!(NSObject)) else {
            return;
        };
        decl.add_method(
            sel!(routeChanged:),
            route_changed as extern "C" fn(&Object, Sel, *mut Object),
        );
        let cls = decl.register();
        let observer: *mut Object = msg_send![cls, new];
        let center: *mut Object = msg_send![class!(NSNotificationCenter), defaultCenter];
        if observer.is_null() || center.is_null() {
            return;
        }
        let _: () = msg_send![center,
            addObserver: observer
            selector: sel!(routeChanged:)
            name: AVAudioSessionRouteChangeNotification
            object: std::ptr::null_mut::<Object>()
        ];
    });
}

pub(super) fn start(output: AudioOutput) {
    use dioxus::mobile::tao::platform::ios::WindowExtIOS;
    let window = dioxus::mobile::window().window.ui_window() as usize;
    *WINDOW.lock().unwrap_or_else(|e| e.into_inner()) = window;

    let (mut picker, mut route) = (output.picker, output.route);
    picker.set(OutputPicker::AirPlay);
    route.set(Some(current_route_name()));
    unsafe { observe_route_changes() };
    output.spawn(async move {
        loop {
            ROUTE_CHANGED.notified().await;
            let name = current_route_name();
            if route.peek().as_deref() != Some(name.as_str()) {
                route.set(Some(name));
            }
        }
    });
}

pub(super) fn refresh_devices(_: AudioOutput) {}

pub(super) fn choose(_: AudioOutput, _: OutputDevice) {}

pub(super) fn ask(_: AudioOutput) {}

/// Opens the AirPlay sheet (on the main thread).
pub(super) fn open_system_picker() {
    extern "C" fn present(_: *mut c_void) {
        unsafe { present_route_picker() };
    }
    unsafe {
        let main: BOOL = msg_send![class!(NSThread), isMainThread];
        if main != NO {
            present_route_picker();
        } else {
            dispatch_async_f(&_dispatch_main_q, std::ptr::null_mut(), present);
        }
    }
}

/// The picker's button: a `UIControl` among its subviews.
unsafe fn find_control(view: *mut Object, depth: u8) -> *mut Object {
    let control = class!(UIControl);
    let subviews: *mut Object = msg_send![view, subviews];
    if subviews.is_null() {
        return std::ptr::null_mut();
    }
    let count: usize = msg_send![subviews, count];
    for index in 0..count {
        let child: *mut Object = msg_send![subviews, objectAtIndex: index];
        let is_control: BOOL = msg_send![child, isKindOfClass: control];
        if is_control != NO {
            return child;
        }
        if depth > 0 {
            let found = find_control(child, depth - 1);
            if !found.is_null() {
                return found;
            }
        }
    }
    std::ptr::null_mut()
}

unsafe fn present_route_picker() {
    let window = *WINDOW.lock().unwrap_or_else(|e| e.into_inner()) as *mut Object;
    let Some(picker_cls) = Class::get("AVRoutePickerView") else {
        return;
    };
    if window.is_null() {
        return;
    }
    let mut stored = PICKER.lock().unwrap_or_else(|e| e.into_inner());
    let mut picker = *stored as *mut Object;
    if picker.is_null() {
        let alloc: *mut Object = msg_send![picker_cls, alloc];
        picker = msg_send![alloc, initWithFrame: CGRect { x: 0.0, y: 0.0, width: PICKER_SIZE, height: PICKER_SIZE }];
        if picker.is_null() {
            return;
        }
        // Practically invisible, and never in the way of a tap.
        let _: () = msg_send![picker, setAlpha: 0.011f64];
        let _: () = msg_send![picker, setUserInteractionEnabled: NO];
        *stored = picker as usize;
    }
    // Bottom centre of the window: where an iPad popover points from.
    let bounds: CGRect = msg_send![window, bounds];
    let frame = CGRect {
        x: (bounds.width - PICKER_SIZE) / 2.0,
        y: bounds.height - PICKER_SIZE * 2.0,
        width: PICKER_SIZE,
        height: PICKER_SIZE,
    };
    let _: () = msg_send![picker, setFrame: frame];
    let superview: *mut Object = msg_send![picker, superview];
    if superview != window {
        let _: () = msg_send![window, addSubview: picker];
    }
    let _: () = msg_send![picker, layoutIfNeeded];
    let button = find_control(picker, 2);
    if !button.is_null() {
        let _: () = msg_send![button, sendActionsForControlEvents: TOUCH_UP_INSIDE];
    }
}
