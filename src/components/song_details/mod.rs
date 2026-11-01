//! Now Playing (the song-details sheet), its Lyrics / Up Next / Info panels,
//! and the queue actions the queue page shares.
//!
//! Module map:
//! - `overlay`   the sheet (compact) or large overlay (wide); one tree only
//! - `art`       big cover with reflection, played covers left, up-next right
//! - `meta`      title, artist links, star, "…", rating stars, volume
//! - `transport` glossy scrubber, times, format badge, transport buttons
//! - `panels`    Lyrics | Up Next | Info segmented control
//! - `strip`     compact focused view's header strip
//! - `gestures`  compact panel focus swipe and cover swipe (`gestures.js`)
//! - `lyrics/*`  lyrics source, lines, search sheet, Shot share mode
//! - `up_next`   Up Next panel, quick create, save queue
//! - `queue_rows` drag to reorder, swipe actions and Undo for queue rows
//! - `info`      details list, related songs
//! - `actions`   playback/queue/library actions (`PlayerCtx`)
//! - `types`     `SongDetailsController` and friends

pub(crate) mod actions;
mod art;
mod gestures;
mod info;
mod lyrics;
mod meta;
mod overlay;
mod panels;
mod strip;
mod transport;
mod types;
pub(crate) mod queue_rows;
pub(crate) mod up_next;

pub use overlay::SongDetailsOverlay;
pub use types::{SongDetailsController, SongDetailsState, SongDetailsTab};
