//! Android: system Back, edge-to-edge insets and bar colours.
//!
//! The activity (android/MainActivity.kt) draws the web view under the status
//! and navigation bars and exposes `window.RustySoundAndroid`; the script
//! (`android_shell.js`) turns the bar sizes into the `--safe-*` variables,
//! keeps the bar icons readable on the theme's background, and handles Back:
//! an open sheet, menu or Now Playing closes first (as on Escape), then Rust
//! goes back a page, and from the first page the app moves to the background
//! as Home does, so playback and the UI stay as they are.

use crate::components::Navigation;
use dioxus::prelude::*;

const ANDROID_SHELL_JS: &str = include_str!("android_shell.js");

/// Mount once in the app shell, inside the router. Renders nothing.
#[component]
pub fn AndroidShell() -> Element {
    let navigation = use_context::<Navigation>();

    use_hook(move || {
        let mut shell = document::eval(ANDROID_SHELL_JS);
        spawn(async move {
            while let Ok(message) = shell.recv::<String>().await {
                if message == "tv" {
                    // TV: wide layout with the TV home's cards.
                    super::env::ANDROID_TV.store(true, std::sync::atomic::Ordering::Relaxed);
                    navigation.navigate_to(crate::components::AppView::TvHome {});
                    continue;
                }
                if message != "back" {
                    continue;
                }
                if navigation.can_go_back() {
                    navigation.go_back();
                } else {
                    let _ = document::eval("window.RustySoundAndroid.leave()");
                }
            }
        });
    });

    rsx! {}
}
