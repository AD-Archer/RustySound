//! Add-to-menu overlay and queue/playlist insertion workflows.

use crate::api::*;
use crate::components::{
    AppView, Icon, Navigation, PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal,
};
use dioxus::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;

// Intent and controller types that drive the add overlay.
include!("intent.rs");
// Async helpers that resolve a target and gather similar-song suggestions.
include!("song_resolver.rs");
// The overlay component split into setup, actions, and view sections.
include!("overlay.rs");
