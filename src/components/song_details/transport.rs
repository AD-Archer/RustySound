//! The glossy scrubber with times and the format badge, and the transport
//! row (shuffle, previous, play/pause, next, repeat).
//!
//! The player bar owns the ids MediaSession presses (`play-pause-btn`,
//! `prev-btn`, `next-btn`, `shuffle-btn`, `repeat-btn`). Those stay mounted
//! while Now Playing is open, so the sheet's buttons use their own ids
//! (`np-…`) and call the same actions (`actions.rs`), keeping every id unique.

use super::actions::{format_badge, is_live_song, use_player};
use super::clock::NowPlayingClock;
use crate::api::Song;
use crate::components::remote_sync::protocol::Op;
use crate::components::remote_sync::RemoteSync;
use crate::components::ui::env::format_clock;
use crate::components::ui::{
    IconButton, IconButtonSize, PlaybackProgress, PlaybackTime, PlayerMenu, ProgressTrack,
    TrackMenuController, TrackVariant,
};
use crate::components::{Icon, SleepTimer};
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
                    span { class: "rs-player-scrub__mid",
                        span { class: "rs-player-badge rs-player-badge--live", "LIVE" }
                        crate::components::cast::CastIndicator { song: song.clone() }
                        SleepIndicator { song: song.clone() }
                    }
                } else {
                    PlaybackTime { class: "rs-player-scrub__time".to_string() }
                    span { class: "rs-player-scrub__mid",
                        if let Some(badge) = badge {
                            span { class: "rs-player-badge", title: "Format", "{badge}" }
                        }
                        crate::components::cast::CastIndicator { song: song.clone() }
                        SleepIndicator { song: song.clone() }
                    }
                    PlaybackTime { remaining: true, class: "rs-player-scrub__time".to_string() }
                }
            }
        }
    }
}

/// Moon and time left while the sleep timer runs, next to the format badge.
/// Opens the menu on the sleep choices. Only this leaf reads the countdown,
/// which changes once a minute (each second in the last one).
#[component]
fn SleepIndicator(song: Song) -> Element {
    let timer = use_context::<SleepTimer>();
    let mut menu = use_context::<TrackMenuController>();
    let Some(label) = timer.label() else {
        return rsx! {};
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-player-sleep",
            aria_label: "Sleep timer: {label}",
            title: "Sleep Timer",
            "data-testid": "sleep-indicator",
            onclick: move |evt: MouseEvent| {
                let point = evt.client_coordinates();
                menu.open_for_player(
                    song.clone(),
                    Some((point.x - 130.0, point.y + 12.0)),
                    PlayerMenu::Sleep,
                );
            },
            Icon { name: "moon".to_string(), class: "rs-ic rs-ic--xs".to_string() }
            span { "{label}" }
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
                onclick: move |_| player.previous("now-playing.prev"),
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
                onclick: move |_| player.next("now-playing.next"),
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

/// Another device's position, on the same glossy track: dragging it seeks
/// that device. The middle says where it plays.
#[component]
pub(super) fn RemoteScrubber(song: Song) -> Element {
    let clock = use_context::<NowPlayingClock>();
    let sync = use_context::<RemoteSync>();
    let caption = sync.view().map(|view| view.caption()).unwrap_or_default();
    let duration = (song.duration as f64).max(0.0);
    let current = (clock.time)().clamp(0.0, duration);
    let can_seek = clock.target.read().is_some() && duration > 0.0;
    let now = format_clock(current);
    let left = format_clock((duration - current).max(0.0));
    rsx! {
        div { class: "rs-player-scrub",
            ProgressTrack {
                value: current,
                max: duration,
                disabled: !can_seek,
                variant: TrackVariant::Stage,
                label: "Position".to_string(),
                value_text: format!("{now} of {}", format_clock(duration)),
                testid: "remote-progress".to_string(),
                on_commit: move |seconds: f64| clock.seek(seconds),
            }
            div { class: "rs-player-scrub__times",
                span { class: "rs-time rs-player-scrub__time", "{now}" }
                span { class: "rs-player-scrub__mid",
                    span { class: "rs-player-badge rs-player-badge--remote", "data-testid": "now-playing-remote-caption",
                        Icon { name: "headphones".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                        "{caption}"
                    }
                }
                span { class: "rs-time rs-time--end rs-player-scrub__time", "\u{2212}{left}" }
            }
        }
    }
}

/// Previous, play/pause and next for another device. Shuffle and repeat
/// belong to that device's queue, so their places stay empty.
#[component]
pub(super) fn RemoteTransport() -> Element {
    let clock = use_context::<NowPlayingClock>();
    let sync = use_context::<RemoteSync>();
    let target = (clock.target)();
    let playing = (clock.playing)();
    let send = {
        let target = target.clone();
        move |op: Op| {
            let target = target.clone();
            move |_| {
                if let Some(target) = target.as_deref() {
                    sync.send(target, op.clone());
                }
            }
        }
    };
    let toggle = {
        let target = target.clone();
        move |_| {
            if let Some(target) = target.as_deref() {
                sync.toggle_remote(target);
            }
        }
    };
    let off = target.is_none();
    rsx! {
        div { class: "rs-player-transport", role: "group", aria_label: "Transport",
            span { class: "rs-iconbtn rs-player-transport__side", aria_hidden: "true" }
            IconButton {
                icon: "prev".to_string(),
                label: "Previous".to_string(),
                size: IconButtonSize::Large,
                disabled: off,
                testid: "now-playing-prev".to_string(),
                icon_class: "rs-ic rs-ic--xl".to_string(),
                onclick: send(Op::Prev),
            }
            IconButton {
                icon: if playing { "pause".to_string() } else { "play".to_string() },
                label: if playing { "Pause".to_string() } else { "Play".to_string() },
                size: IconButtonSize::Large,
                disabled: off,
                testid: "now-playing-play-pause".to_string(),
                icon_class: "rs-ic rs-ic--xxl".to_string(),
                class: "rs-player-transport__play".to_string(),
                onclick: toggle,
            }
            IconButton {
                icon: "next".to_string(),
                label: "Next".to_string(),
                size: IconButtonSize::Large,
                disabled: off,
                testid: "now-playing-next".to_string(),
                icon_class: "rs-ic rs-ic--xl".to_string(),
                onclick: send(Op::Next),
            }
            span { class: "rs-iconbtn rs-player-transport__side", aria_hidden: "true" }
        }
    }
}
