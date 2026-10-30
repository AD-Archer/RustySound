//! The glossy scrubber with times and the format badge, and the transport
//! row (shuffle, previous, play/pause, next, repeat).
//!
//! The player bar owns the ids MediaSession presses (`play-pause-btn`,
//! `prev-btn`, `next-btn`, `shuffle-btn`, `repeat-btn`). Those stay mounted
//! while Now Playing is open, so the sheet's buttons use their own ids
//! (`np-…`) and call the same actions (`actions.rs`), keeping every id unique.

use super::actions::{format_badge, is_live_song, use_player};
use crate::api::Song;
use crate::components::ui::{IconButton, IconButtonSize, PlaybackProgress, PlaybackTime, TrackVariant};
use crate::db::RepeatMode;
use dioxus::prelude::*;

/// Position track and times. Only the kit leaves inside read the position.
#[component]
pub(super) fn NowPlayingScrubber(song: Song) -> Element {
    let live = is_live_song(&song);
    let badge = format_badge(&song);
    rsx! {
        div { class: "rs-player-scrub",
            PlaybackProgress { variant: TrackVariant::Stage }
            div { class: "rs-player-scrub__times",
                if live {
                    span { class: "rs-player-badge rs-player-badge--live", "LIVE" }
                } else {
                    PlaybackTime { class: "rs-player-scrub__time".to_string() }
                    if let Some(badge) = badge {
                        span { class: "rs-player-badge", title: "Format", "{badge}" }
                    }
                    PlaybackTime { remaining: true, class: "rs-player-scrub__time".to_string() }
                }
            }
        }
    }
}

#[component]
pub(super) fn NowPlayingTransport(song: Song) -> Element {
    let mut player = use_player();
    let playing = (player.is_playing)();
    let shuffle = (player.shuffle)();
    let repeat = (player.repeat)();
    let live = is_live_song(&song);

    rsx! {
        div { class: "rs-player-transport", role: "group", aria_label: "Transport",
            IconButton {
                icon: "shuffle".to_string(),
                label: "Shuffle".to_string(),
                pressed: shuffle,
                quiet: true,
                disabled: live,
                id: "np-shuffle-btn".to_string(),
                testid: "now-playing-shuffle".to_string(),
                class: "rs-player-transport__side".to_string(),
                onclick: move |_| player.toggle_shuffle(),
            }
            IconButton {
                icon: "prev".to_string(),
                label: "Previous".to_string(),
                size: IconButtonSize::Large,
                disabled: live,
                id: "np-prev-btn".to_string(),
                testid: "now-playing-prev".to_string(),
                icon_class: "rs-ic rs-ic--xl".to_string(),
                onclick: move |_| player.previous(),
            }
            IconButton {
                icon: if playing { "pause".to_string() } else { "play".to_string() },
                label: if playing { "Pause".to_string() } else { "Play".to_string() },
                size: IconButtonSize::Large,
                id: "np-play-pause-btn".to_string(),
                testid: "now-playing-play-pause".to_string(),
                icon_class: "rs-ic rs-ic--xxl".to_string(),
                class: "rs-player-transport__play".to_string(),
                onclick: move |_| player.toggle_play(),
            }
            IconButton {
                icon: "next".to_string(),
                label: "Next".to_string(),
                size: IconButtonSize::Large,
                disabled: live,
                id: "np-next-btn".to_string(),
                testid: "now-playing-next".to_string(),
                icon_class: "rs-ic rs-ic--xl".to_string(),
                onclick: move |_| player.next(),
            }
            IconButton {
                icon: if repeat == RepeatMode::One { "repeat-1".to_string() } else { "repeat".to_string() },
                label: match repeat {
                    RepeatMode::Off => "Repeat: off".to_string(),
                    RepeatMode::All => "Repeat: all".to_string(),
                    RepeatMode::One => "Repeat: one".to_string(),
                },
                pressed: repeat != RepeatMode::Off,
                quiet: true,
                disabled: live,
                id: "np-repeat-btn".to_string(),
                testid: "now-playing-repeat".to_string(),
                class: "rs-player-transport__side".to_string(),
                onclick: move |_| player.cycle_repeat(),
            }
        }
    }
}
