//! The components module contains all shared components for our app.

mod app;
mod sidebar;
mod player;
mod icons;
mod views;

pub use app::*;
pub use sidebar::*;
pub use player::*;
pub use icons::*;
// Views are accessed via views::ViewName
