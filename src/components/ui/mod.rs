//! The Stage UI kit. One component per file; styles live in
//! `assets/styling/components.css` (classes prefixed `rs-`).
//!
//! Phase-2 views build from these. To add a component, add a new file here
//! and register it below; don't restyle existing kit classes from a view.

// The kit is consumed by phase-2 views; until they land, parts are unused.
#![allow(dead_code, unused_imports)]

mod artwork;
mod artwork_mosaic;
mod button;
mod cover_flow;
mod edge_swipe;
mod empty_state;
pub mod env;
mod icon_button;
mod inset_list;
mod list_row;
mod long_press;
mod menu;
mod page_transition;
mod playback;
mod player_options;
mod preview_ring;
mod progress_track;
mod quick_listen;
mod row_gestures;
mod search_field;
mod section_header;
mod segmented;
mod sheet;
mod sidebar;
mod skeleton;
mod status_toast;
mod stage;
mod switch;
mod tab_bar;
mod track_menu;
mod track_menu_host;
mod track_row;
mod undo_toast;
mod virtual_list;
mod window_drag;

pub use artwork::Artwork;
pub use artwork_mosaic::ArtworkMosaic;
pub use button::{Button, ButtonSize, ButtonVariant};
pub use cover_flow::{CoverFlow, CoverFlowItem};
pub use edge_swipe::EdgeSwipe;
pub use empty_state::EmptyState;
pub use env::{
    cover_art_src, cover_art_url, format_clock, use_layout, Layout, LayoutSignal, COMPACT_MAX_WIDTH,
};
pub use icon_button::{IconButton, IconButtonSize};
pub use inset_list::{InsetList, ListInset};
pub use list_row::ListRow;
pub use long_press::{use_long_press, LongPress};
pub use menu::{Menu, MenuAction, MenuGap, MenuItem};
pub use page_transition::PageTransition;
pub use playback::{MiniProgressLine, PlaybackProgress, PlaybackTime};
pub use player_options::{PlayerMenu, PlayerOptions};
pub use preview_ring::PreviewRing;
pub use progress_track::{ProgressTrack, TrackVariant};
pub use quick_listen::QuickListenCapsule;
pub use row_gestures::{
    use_row_gestures, RowGesture, RowGestureList, RowGestureListId, RowRemoveRequest, RowSwipe,
};
pub use search_field::SearchField;
pub use section_header::SectionHeader;
pub use segmented::Segmented;
pub use sheet::Sheet;
pub use sidebar::{radio_available, Sidebar};
pub use skeleton::Skeleton;
pub use stage::Stage;
pub use status_toast::{StatusToast, StatusToastHost};
pub use switch::Switch;
pub use tab_bar::TabBar;
pub use track_menu::{
    SongChange, TrackMenu, TrackMenuController, TrackMenuRemove, TrackMenuRequest,
};
pub use track_menu_host::TrackMenuHost;
pub use track_row::TrackRow;
pub use undo_toast::{UndoToast, UNDO_TOAST_MS};
pub use virtual_list::{use_main_scroll_provider, MainScroll, VirtualList};
pub use window_drag::WindowDragRegion;

// The player bar lives with the rest of the player code; re-exported so the
// kit is one import.
pub use crate::components::player::PlayerBar;
