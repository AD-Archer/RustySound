//! The components module contains all shared components for our app.

mod app;
mod sidebar;
mod player;
mod icons;
mod views;
mod audio_manager;

use dioxus::prelude::Signal;

#[derive(Clone)]
pub struct VolumeSignal(pub Signal<f64>);

#[derive(Clone)]
#[allow(dead_code)]
pub struct PlaybackPositionSignal(pub Signal<f64>);

pub use app::*;
pub use sidebar::*;
pub use player::*;
pub use icons::*;
pub use audio_manager::*;
// Views are accessed via views::ViewName
