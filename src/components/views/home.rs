//! The old Home view is gone; `/` renders Discover (`views/discover`).
//! This module only keeps the old import path for the legacy album card and
//! song row that unrebuilt views still use.

pub use super::legacy_cards::{AlbumCard, SongRow};
