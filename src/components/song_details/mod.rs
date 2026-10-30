//! Now Playing (the song-details sheet), its Lyrics / Up Next / Info panels,
//! and the queue actions the queue page shares.
//!
//! Module map:
//! - `overlay`   the sheet (compact) or large overlay (wide); one tree only
//! - `art`       big cover with reflection, up-next covers waiting to its right
//! - `meta`      title, artist links, star, "…", rating stars, volume
//! - `transport` glossy scrubber, times, format badge, transport buttons
//! - `panels`    Lyrics | Up Next | Info segmented control
//! - `lyrics/*`  lyrics source, lines, search sheet, Shot share mode
//! - `up_next`   Up Next panel, queue edit rows, quick create, save queue
//! - `info`      details list, related songs
//! - `actions`   playback/queue/library actions (`PlayerCtx`)
//! - `types`     `SongDetailsController` and friends

pub(crate) mod actions;
mod art;
mod info;
mod lyrics;
mod meta;
mod overlay;
mod panels;
mod transport;
mod types;
pub(crate) mod up_next;

pub use overlay::SongDetailsOverlay;
pub use types::{SongDetailsController, SongDetailsState, SongDetailsTab};
