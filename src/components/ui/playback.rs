//! Leaf components that read the playback position.
//!
//! Position ticks 4-5 times a second. Only these tiny components subscribe to
//! it, so a tick re-renders a time label or a track fill and nothing else.
//! They take the inner `current_time` / `duration` signals once (peek) and
//! never subscribe to the outer `AudioState` signal, which some controllers
//! write on every tick.

use crate::api::Song;
use crate::components::ui::env::{format_clock, is_radio_song};
use crate::components::ui::{ProgressTrack, TrackVariant};
use crate::components::{seek_to, AudioState, PlaybackPositionSignal};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
struct PlaybackClock {
    current: Signal<f64>,
    duration: Signal<f64>,
}

fn use_playback_clock() -> PlaybackClock {
    let audio = use_context::<Signal<AudioState>>();
    use_hook(move || {
        let state = audio.peek();
        PlaybackClock {
            current: state.current_time,
            duration: state.duration,
        }
    })
}

fn effective_duration(reported: f64, song: Option<&Song>) -> f64 {
    if reported.is_finite() && reported > 0.0 {
        reported
    } else {
        song.map(|s| s.duration as f64).unwrap_or(0.0)
    }
}

/// The scrubbable playback track for the current song (seek on release).
/// * `variant`: Stage (default) or Page trough.
#[component]
pub fn PlaybackProgress(
    #[props(default)] variant: TrackVariant,
    #[props(default)] class: String,
) -> Element {
    let clock = use_playback_clock();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let position_signal = use_context::<PlaybackPositionSignal>().0;
    let audio = use_context::<Signal<AudioState>>();

    let song = now_playing.read();
    let live = song.as_ref().map(is_radio_song).unwrap_or(false);
    let duration = effective_duration((clock.duration)(), song.as_ref());
    let has_song = song.is_some();
    drop(song);
    let current = (clock.current)();
    let value_text = format!("{} of {}", format_clock(current), format_clock(duration));

    rsx! {
        ProgressTrack {
            value: current,
            max: duration,
            live,
            disabled: !has_song,
            variant,
            class,
            label: "Position".to_string(),
            value_text,
            testid: "progress".to_string(),
            on_commit: move |seconds: f64| {
                let mut position = position_signal;
                position.set(seconds);
                let mut current = audio.peek().current_time;
                current.set(seconds);
                seek_to(seconds);
            },
        }
    }
}

/// Elapsed time, or remaining time with a leading minus (`remaining`).
/// Shows LIVE for radio.
#[component]
pub fn PlaybackTime(
    #[props(default)] remaining: bool,
    #[props(default)] class: String,
) -> Element {
    let clock = use_playback_clock();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let song = now_playing.read();
    if song.as_ref().map(is_radio_song).unwrap_or(false) {
        return rsx! {
            span { class: "rs-time {class}", class: if remaining { "rs-time--end" }, "LIVE" }
        };
    }
    let duration = effective_duration((clock.duration)(), song.as_ref());
    drop(song);
    let current = (clock.current)().max(0.0);
    let text = if remaining {
        format!("\u{2212}{}", format_clock((duration - current).max(0.0)))
    } else {
        format_clock(current)
    };
    rsx! {
        span {
            class: "rs-time {class}",
            class: if remaining { "rs-time--end" },
            "{text}"
        }
    }
}

/// The 2px progress line along the bottom of the mini player.
#[component]
pub fn MiniProgressLine() -> Element {
    let clock = use_playback_clock();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let song = now_playing.read();
    let duration = effective_duration((clock.duration)(), song.as_ref());
    let live = song.as_ref().map(is_radio_song).unwrap_or(false);
    drop(song);
    let percent = if live {
        100.0
    } else if duration > 0.0 {
        ((clock.current)() / duration * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let scale = percent / 100.0;
    rsx! {
        div { class: "rs-mini__line", aria_hidden: "true",
            i { style: "transform:scaleX({scale:.4})" }
        }
    }
}
