//! Song-details overlay, panels, and shared helpers.

use crate::api::{
    fetch_lyrics_with_fallback, format_duration, normalize_lyrics_provider_order,
    search_lyrics_candidates, LyricLine, LyricsQuery, LyricsResult, LyricsSearchCandidate,
    NavidromeClient, ServerConfig, Song,
};
use crate::components::{
    seek_to, spawn_shuffle_queue, AddIntent, AddMenuController, AppView, AudioState, Icon,
    Navigation, PlaybackPositionSignal, SidebarOpenSignal, VolumeSignal,
};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;

// Tab/state/controller definitions shared by all song-details panels.
include!("types.rs");
// Main overlay component split into setup and view chunks.
include!("overlay.rs");
// Desktop/mobile details pane with metadata and transport controls.
include!("details_panel.rs");
// Up-next queue panel controls.
include!("queue_panel.rs");
// Related songs recommendation panel.
include!("related_panel.rs");
// Compact lyrics preview strip for details view.
include!("mini_lyrics_strip.rs");
// Full lyrics panel with sync, search, and candidate selection.
include!("lyrics_panel.rs");
// Shared helper functions used across song-details sections.
include!("helpers.rs");
