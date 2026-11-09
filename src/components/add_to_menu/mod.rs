//! Add-to-menu overlay and queue/playlist insertion workflows.

use crate::api::*;
use crate::components::song_details::actions::song_key;
use crate::components::ui::{
    Artwork, CloseButton, SectionHeader, Sheet, Skeleton, StatusToast,
};
use crate::components::{
    quick_listen_id, use_player, use_servers, use_setting, AppView, Icon, Navigation, Preview,
    QueuePosition,
};
use dioxus::prelude::*;
use std::collections::HashSet;

// Intent and controller types that drive the add overlay.
include!("intent.rs");
// Async helpers that resolve a target and gather similar-song suggestions.
include!("song_resolver.rs");
// The overlay component split into setup, actions, and view sections.
include!("overlay.rs");
