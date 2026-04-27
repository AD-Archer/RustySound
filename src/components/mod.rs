//! The components module contains all shared pieces of the UI.

mod add_to_menu;
mod app;
mod app_view;
mod audio_manager;
mod cached_image;
mod icons;
mod navigation;
mod player;
mod sidebar;
mod song_details;
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
pub struct SidebarOpenSignal(pub Signal<bool>);

#[derive(Clone)]
pub struct PreviewPlaybackSignal(pub Signal<bool>);

#[derive(Clone)]
pub struct HomeRefreshSignal(pub Signal<u64>);

#[derive(Clone)]
pub struct IsPlayingSignal(pub Signal<bool>);

#[derive(Clone)]
pub struct ShuffleEnabledSignal(pub Signal<bool>);

pub use add_to_menu::*;
pub use app::*;
pub use app_view::{AppView, view_instance_key, view_label};
pub use audio_manager::*;
pub use icons::*;
pub use navigation::Navigation;
pub use player::*;
pub use sidebar::*;
pub use song_details::*;
// Views are accessed via views::ViewName
