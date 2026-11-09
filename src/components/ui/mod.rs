//! The Stage UI kit. One component per file; styles live in
//! `assets/styling/components.css` (classes prefixed `rs-`).
//!
//! Views build from these. To add a component, add a new file here and
//! register it below; don't restyle existing kit classes from a view.
//!
//! Gestures are JavaScript (`*.js` here, each loaded after the shared
//! `gesture_core.js`), so they run in the iOS WKWebView too and cost no
//! Rust work per move: `edge_swipe`, `page_transition`, `row_gestures`
//! (long-press, swipes, reorder, carry), `cover_flow` and `sheet_swipe`.

mod android_shell;
mod artwork;
mod button;
mod collection_actions;
mod cover_flow;
mod edge_swipe;
mod empty_state;
mod eq_curve;
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
mod song_drop;
mod states;
mod status_toast;
mod story;
mod stage;
mod switch;
mod tab_bar;
mod tiles;
mod track_menu;
mod track_menu_host;
mod track_row;
mod undo_toast;
mod vertical_slider;
mod virtual_list;
mod window_drag;

pub use android_shell::AndroidShell;
pub use artwork::Artwork;
pub use button::{Button, ButtonSize, ButtonVariant};
pub use collection_actions::CollectionActions;
pub use cover_flow::{CoverFlow, CoverFlowItem};
pub use edge_swipe::EdgeSwipe;
pub use empty_state::EmptyState;
pub use eq_curve::EqCurve;
pub use env::{cover_art_src, format_clock, use_layout, LayoutSignal};
pub use icon_button::{CloseButton, IconButton, IconButtonSize};
pub use inset_list::{InsetList, ListInset};
pub use list_row::ListRow;
pub use long_press::use_long_press;
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
pub use sheet::{MenuSheet, Sheet};
pub use sidebar::{radio_available, Sidebar};
pub use skeleton::Skeleton;
pub use states::{ErrorState, GridSkeleton, LoadMore, NoServers, PageFoot};
pub use stage::Stage;
pub use status_toast::{StatusToast, StatusToastHost};
pub use story::{Story, StoryCard};
pub use switch::Switch;
pub use tab_bar::TabBar;
pub use tiles::{AlbumTile, ArtistTile};
pub(crate) use track_menu::{go_to_album, go_to_artist};
pub use track_menu::{
    SongChange, TrackMenu, TrackMenuController, TrackMenuRemove, TrackMenuRequest,
};
pub use track_menu_host::TrackMenuHost;
pub use track_row::{SongRow, TrackRow};

pub use undo_toast::UndoToast;

pub use vertical_slider::VerticalSlider;
pub use virtual_list::{use_main_scroll_provider, MainScroll, VirtualList};
pub use window_drag::WindowDragRegion;

// The player bar lives with the rest of the player code; re-exported so the
// kit is one import.
pub use crate::components::player::PlayerBar;
