//! The components module contains all shared pieces of the UI.

mod add_to_menu;
mod app;
mod app_view;
mod audio_manager;
pub mod audio_output;
mod icons;
mod navigation;
pub mod offline_sync;
pub mod playback_sync;
pub mod profiles;
pub mod playlist_sync;
mod player;
mod settings_store;
mod share;
mod song_details;
pub mod ui;
mod views;

use dioxus::prelude::Signal;

#[derive(Clone)]
pub struct VolumeSignal(pub Signal<f64>);

#[derive(Clone)]
#[allow(dead_code)]
pub struct PlaybackPositionSignal(pub Signal<f64>);

#[derive(Clone)]
pub struct SeekRequestSignal(pub Signal<Option<(String, f64)>>);

#[derive(Clone)]
pub struct PreviewPlaybackSignal(pub Signal<bool>);

#[derive(Clone)]
pub struct IsPlayingSignal(pub Signal<bool>);

#[derive(Clone)]
pub struct ShuffleEnabledSignal(pub Signal<bool>);

pub use add_to_menu::*;
pub use app::*;
pub use app_view::AppView;
pub use audio_manager::*;
pub use audio_output::{AudioOutput, OutputPicker};
pub use settings_store::{update_cache_settings, update_settings, use_servers, use_setting};
pub use share::{can_share, share_link, ShareTarget};
pub use icons::*;
pub use navigation::{Navigation, PageMotion};
pub use playback_sync::{note_track_finished, PlaybackSync};
pub use playlist_sync::{PlaylistSync, SyncStatus};
pub use song_details::*;
// The playback API views start music with (`PlayerCtx::play`, `enqueue`…).
#[allow(unused_imports)]
pub(crate) use song_details::actions::{use_player, PlayerCtx, QueuePosition};
// Views are accessed via views::ViewName
