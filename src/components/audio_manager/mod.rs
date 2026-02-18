//! Audio Manager - Handles audio playback outside of the component render cycle.
//! Keeps audio side-effects isolated and defers signal writes to avoid borrow loops.
#![cfg_attr(
    all(not(target_arch = "wasm32"), target_os = "ios"),
    allow(unexpected_cfgs)
)]

// Shared imports, state types, and browser-only utility helpers.
include!("shared_types_and_web_helpers.rs");
// Desktop-webview bridge bootstrap script for native (non-wasm) targets.
include!("native_bridge_bootstrap.rs");
// Native Windows backend implementation.
include!("native_windows_backend.rs");
// Core iOS player backend implementation.
include!("native_ios_backend.rs");
// iOS remote control and now-playing integration helpers.
include!("native_ios_remote.rs");
// Cross-platform native bridge command/snapshot runtime helpers.
include!("native_bridge_runtime.rs");
// Native (non-wasm) audio controller component.
include!("controller_native.rs");
// Shared queue/shuffle/stream/scrobble helpers.
include!("queue_and_stream_helpers.rs");
// Web (wasm) audio controller component.
include!("controller_web.rs");
// Public playback utility API.
include!("playback_api.rs");
