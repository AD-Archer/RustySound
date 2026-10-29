//! The player: a strip of the stage.
//!
//! `PlayerBar` renders the compact mini player (floating above the tab bar)
//! or the wide bar (bottom of the window), never both. Button ids that
//! MediaSession and the native bridges press (`play-pause-btn`, `prev-btn`,
//! `next-btn`, `shuffle-btn`, `repeat-btn`, `rating-btn`, `add-menu-btn`) live
//! in `controls.rs`. On compact layouts `prev-btn` stays mounted but hidden;
//! shuffle, repeat, rating, volume and the add menu are reached through Now
//! Playing (tap the mini player).
//!
//! Perf: nothing here reads the playback position. Time labels and track
//! fills are the leaf components in `ui::playback`.

use crate::api::Song;
use crate::components::ui::use_layout;
use crate::components::AudioState;
use dioxus::prelude::*;

mod compact;
mod controls;
mod wide;

use compact::CompactPlayer;
use wide::WidePlayer;

/// The player for the current layout. No props.
/// Compact renders only while a song is loaded; wide always renders.
#[component]
pub fn PlayerBar() -> Element {
    let layout = use_layout();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let has_song = now_playing.read().is_some();

    rsx! {
        PlaybackErrorNote {}
        if layout.is_compact() {
            if has_song {
                CompactPlayer {}
            }
        } else {
            WidePlayer {}
        }
    }
}

/// Transient playback error, floating above the player.
#[component]
fn PlaybackErrorNote() -> Element {
    let audio = use_context::<Signal<AudioState>>();
    let error = use_hook(move || audio.peek().playback_error);
    let message = error();
    rsx! {
        if let Some(message) = message {
            div { class: "rs-player-error", role: "status", "{message}" }
        }
    }
}
