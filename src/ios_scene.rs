//! UIScene life-cycle bridge for iOS.
//!
//! Apps built with the iOS 27 SDK must adopt the UIScene life cycle: UIKit
//! traps in `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`
//! ("UIScene life cycle is required for apps built with this SDK") as soon as
//! it creates the first scene of an app whose Info.plist declares no
//! `UIApplicationSceneManifest`.
//!
//! dioxus-mobile 0.7 runs on tao 0.34, which predates scenes: it creates its
//! `UIWindow` from the app delegate with `initWithFrame:` and never gives it a
//! `windowScene`. tao 0.35+ handles scenes itself, but no dioxus 0.7.x release
//! depends on it.
//!
//! The fix has two halves:
//! - `Dioxus.toml` `[ios.raw] info_plist` declares a single-scene manifest
//!   whose delegate class is [`SCENE_DELEGATE_CLASS`] (`scripts/bundle-apple.sh`
//!   writes the same keys with PlistBuddy, for dx versions without `[ios.raw]`).
//! - [`install`] registers that class, and [`use_scene_window`] (called from
//!   the root component) hands tao's `UIWindow` to it. Whichever of the two
//!   happens last (scene connected, window known) moves the window into the
//!   scene. tao's window is not reachable from `UIApplication.windows` while
//!   it has no scene, hence the hand-off through the Dioxus window context.
//!   The delegate also forwards the scene's foreground/background callbacks
//!   to tao's app delegate (UIKit stops calling the app-delegate versions once
//!   scenes are adopted).
//!
//! Remove this module and the manifest once the app moves to a Dioxus release
//! built on tao >= 0.37, which installs its own scene delegate.
#![allow(unexpected_cfgs)] // objc 0.2 macros probe `cfg(feature = "cargo-clippy")`.

use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Protocol, Sel, BOOL, NO};
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
use std::sync::{Mutex, Once};

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

unsafe impl Encode for CGRect {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
    }
}

/// Must match `UISceneDelegateClassName` in `Dioxus.toml` `[ios.raw]`.
pub const SCENE_DELEGATE_CLASS: &str = "RustySoundSceneDelegate";
const WINDOW_IVAR: &str = "_rsWindow";

// Pointers are stored as usize so the statics are Send; every access happens
// on the main thread (UIKit callbacks and the first Dioxus render).
/// The connected `UIWindowScene` (retained), or 0.
static SCENE: Mutex<usize> = Mutex::new(0);
/// tao's windows (retained), attached again whenever a scene (re)connects.
static WINDOWS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

/// Hands the app's tao `UIWindow` to the scene bridge. Call once from the
/// root component.
pub fn use_scene_window() {
    dioxus::prelude::use_hook(|| {
        use dioxus::mobile::tao::platform::ios::WindowExtIOS;
        let ui_window = dioxus::mobile::window().window.ui_window() as *mut Object;
        unsafe { register_window(ui_window) };
    });
}

unsafe fn register_window(window: *mut Object) {
    if window.is_null() {
        return;
    }
    {
        let mut windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
        if windows.contains(&(window as usize)) {
            return;
        }
        let _: *mut Object = msg_send![window, retain];
        windows.push(window as usize);
    }
    let scene = *SCENE.lock().unwrap_or_else(|e| e.into_inner()) as *mut Object;
    if !scene.is_null() {
        attach_window(window, scene);
    }
}

/// Moves `window` into `scene` and makes it key, keeping its visibility:
/// dioxus shows the window itself once the webview has loaded.
unsafe fn attach_window(window: *mut Object, scene: *mut Object) {
    let current: *mut Object = msg_send![window, windowScene];
    if current != scene {
        let () = msg_send![window, setWindowScene: scene];
    }
    // tao sized the window from the window attributes before any scene
    // existed; fill the scene instead.
    let coordinate_space: *mut Object = msg_send![scene, coordinateSpace];
    if !coordinate_space.is_null() {
        let bounds: CGRect = msg_send![coordinate_space, bounds];
        let () = msg_send![window, setFrame: bounds];
        fill_root_view(window);
    }
    let hidden: BOOL = msg_send![window, isHidden];
    if hidden == NO {
        let () = msg_send![window, makeKeyAndVisible];
    } else {
        let () = msg_send![window, makeKeyWindow];
    }
    let delegate: *mut Object = msg_send![scene, delegate];
    if !delegate.is_null() {
        let responds: BOOL = msg_send![delegate, respondsToSelector: sel!(setWindow:)];
        if responds != NO {
            let () = msg_send![delegate, setWindow: window];
        }
    }
}

/// Registers the scene delegate class. Call before `dioxus::launch`.
pub fn install() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| unsafe { register_scene_delegate_class() });
}

unsafe fn register_scene_delegate_class() {
    let Some(superclass) = Class::get("UIResponder") else {
        eprintln!("[ios_scene] UIResponder missing; scene delegate not registered");
        return;
    };
    let Some(mut decl) = ClassDecl::new(SCENE_DELEGATE_CLASS, superclass) else {
        // Already registered (e.g. a second call from a test harness).
        return;
    };

    decl.add_ivar::<*mut Object>(WINDOW_IVAR);
    for name in ["UISceneDelegate", "UIWindowSceneDelegate"] {
        if let Some(protocol) = Protocol::get(name) {
            decl.add_protocol(protocol);
        }
    }

    decl.add_method(
        sel!(scene:willConnectToSession:options:),
        scene_will_connect as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object, *mut Object),
    );
    decl.add_method(
        sel!(sceneDidDisconnect:),
        scene_did_disconnect as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(window),
        delegate_window as extern "C" fn(&Object, Sel) -> *mut Object,
    );
    decl.add_method(
        sel!(setWindow:),
        delegate_set_window as extern "C" fn(&mut Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneDidBecomeActive:),
        scene_did_become_active as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneWillResignActive:),
        scene_will_resign_active as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneWillEnterForeground:),
        scene_will_enter_foreground as extern "C" fn(&Object, Sel, *mut Object),
    );
    decl.add_method(
        sel!(sceneDidEnterBackground:),
        scene_did_enter_background as extern "C" fn(&Object, Sel, *mut Object),
    );

    decl.register();
}

/// Records the scene and moves any window already handed over into it.
extern "C" fn scene_will_connect(
    _this: &mut Object,
    _: Sel,
    scene: *mut Object,
    _session: *mut Object,
    _options: *mut Object,
) {
    unsafe {
        let Some(window_scene_cls) = Class::get("UIWindowScene") else {
            return;
        };
        let is_window_scene: BOOL = msg_send![scene, isKindOfClass: window_scene_cls];
        if is_window_scene == NO {
            return;
        }
        {
            let mut current = SCENE.lock().unwrap_or_else(|e| e.into_inner());
            let _: *mut Object = msg_send![scene, retain];
            let previous = std::mem::replace(&mut *current, scene as usize) as *mut Object;
            if !previous.is_null() {
                let () = msg_send![previous, release];
            }
        }
        let windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for window in windows {
            attach_window(window as *mut Object, scene);
        }
    }
}

/// Forgets a scene the system discarded; a reconnect gets the windows back.
extern "C" fn scene_did_disconnect(_: &Object, _: Sel, scene: *mut Object) {
    unsafe {
        let mut current = SCENE.lock().unwrap_or_else(|e| e.into_inner());
        if *current == scene as usize {
            *current = 0;
            let () = msg_send![scene, release];
        }
    }
}

/// Sizes the root view's direct subviews (wry's WKWebView) to the root view.
///
/// Before a scene exists UIKit reports a zero-sized screen, so tao builds a
/// 0x0 window and wry adds its webview at 0x0 with every autoresizing margin
/// flexible. Growing that to the scene size splits the growth evenly between
/// the margins and the size (the webview ends up a third of the width, centred).
/// Once the frame matches the root view, the zero margins absorb nothing and
/// later resizes (rotation) track the window again.
unsafe fn fill_root_view(window: *mut Object) {
    let controller: *mut Object = msg_send![window, rootViewController];
    if controller.is_null() {
        return;
    }
    let root: *mut Object = msg_send![controller, view];
    if root.is_null() {
        return;
    }
    let () = msg_send![root, layoutIfNeeded];
    let bounds: CGRect = msg_send![root, bounds];
    let subviews: *mut Object = msg_send![root, subviews];
    let count: usize = msg_send![subviews, count];
    for index in 0..count {
        let view: *mut Object = msg_send![subviews, objectAtIndex: index];
        let () = msg_send![view, setFrame: bounds];
    }
}

extern "C" fn delegate_window(this: &Object, _: Sel) -> *mut Object {
    unsafe { *this.get_ivar::<*mut Object>(WINDOW_IVAR) }
}

extern "C" fn delegate_set_window(this: &mut Object, _: Sel, window: *mut Object) {
    unsafe {
        let previous = *this.get_ivar::<*mut Object>(WINDOW_IVAR);
        if previous == window {
            return;
        }
        if !window.is_null() {
            let _: *mut Object = msg_send![window, retain];
        }
        this.set_ivar::<*mut Object>(WINDOW_IVAR, window);
        if !previous.is_null() {
            let () = msg_send![previous, release];
        }
    }
}

/// Calls `selector` on tao's app delegate with the shared application, so tao
/// keeps emitting the same Resumed/Suspended events it did before scenes.
unsafe fn forward_to_app_delegate(selector: Sel) {
    let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
    if app.is_null() {
        return;
    }
    let delegate: *mut Object = msg_send![app, delegate];
    if delegate.is_null() {
        return;
    }
    let responds: BOOL = msg_send![delegate, respondsToSelector: selector];
    if responds == NO {
        return;
    }
    let _: *mut Object = msg_send![delegate, performSelector: selector withObject: app];
}

extern "C" fn scene_did_become_active(_: &Object, _: Sel, _scene: *mut Object) {
    unsafe { forward_to_app_delegate(sel!(applicationDidBecomeActive:)) }
}

extern "C" fn scene_will_resign_active(_: &Object, _: Sel, _scene: *mut Object) {
    unsafe { forward_to_app_delegate(sel!(applicationWillResignActive:)) }
}

extern "C" fn scene_will_enter_foreground(_: &Object, _: Sel, _scene: *mut Object) {
    unsafe { forward_to_app_delegate(sel!(applicationWillEnterForeground:)) }
}

extern "C" fn scene_did_enter_background(_: &Object, _: Sel, _scene: *mut Object) {
    unsafe { forward_to_app_delegate(sel!(applicationDidEnterBackground:)) }
}
