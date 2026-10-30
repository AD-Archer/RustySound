//! The Stage UI kit. One component per file; styles live in
//! `assets/styling/components.css` (classes prefixed `rs-`).
//!
//! Phase-2 views build from these. To add a component, add a new file here
//! and register it below; don't restyle existing kit classes from a view.

// The kit is consumed by phase-2 views; until they land, parts are unused.
#![allow(dead_code, unused_imports)]

mod action_menu;
mod artwork;
mod artwork_mosaic;
mod cover_flow;
mod empty_state;
pub mod env;
mod icon_button;
mod inset_list;
mod list_row;
mod pill_button;
mod playback;
mod progress_track;
mod search_field;
mod section_header;
mod segmented;
mod sheet;
mod sidebar;
mod skeleton;
mod stage;
mod tab_bar;
mod track_menu;
mod track_menu_host;
mod track_row;
mod virtual_list;

pub use action_menu::{ActionMenu, ActionMenuGap, ActionMenuItem};
pub use artwork::Artwork;
pub use artwork_mosaic::ArtworkMosaic;
pub use cover_flow::{CoverFlow, CoverFlowItem};
pub use empty_state::EmptyState;
pub use env::{
    cover_art_url, format_clock, use_layout, Layout, LayoutSignal, COMPACT_MAX_WIDTH,
};
pub use icon_button::{IconButton, IconButtonSize};
pub use inset_list::{InsetList, ListInset};
pub use list_row::ListRow;
pub use pill_button::{PillButton, PillVariant};
pub use playback::{MiniProgressLine, PlaybackProgress, PlaybackTime};
pub use progress_track::{ProgressTrack, TrackVariant};
pub use search_field::SearchField;
pub use section_header::SectionHeader;
pub use segmented::Segmented;
pub use sheet::Sheet;
pub use sidebar::Sidebar;
pub use skeleton::Skeleton;
pub use stage::Stage;
pub use tab_bar::TabBar;
pub use track_menu::{SongChange, TrackMenu, TrackMenuController, TrackMenuRequest};
pub use track_menu_host::TrackMenuHost;
pub use track_row::TrackRow;
pub use virtual_list::{MainScroll, VirtualList};

// The player bar lives with the rest of the player code; re-exported so the
// kit is one import.
pub use crate::components::player::PlayerBar;
