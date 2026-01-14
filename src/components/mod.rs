//! The components module contains all shared pieces of the UI.

mod app;
mod app_view;
mod audio_manager;
mod cached_image;
mod icons;
mod navigation;
mod player;
mod sidebar;
mod views;

use dioxus::prelude::Signal;

#[derive(Clone)]
pub struct VolumeSignal(pub Signal<f64>);

#[derive(Clone)]
#[allow(dead_code)]
pub struct PlaybackPositionSignal(pub Signal<f64>);

pub use app::*;
pub use app_view::{view_label, AppView};
pub use audio_manager::*;
pub use icons::*;
pub use navigation::Navigation;
pub use player::*;
pub use sidebar::*;
// Views are accessed via views::ViewName
