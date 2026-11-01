//! The player's buttons. Each one calls `PlayerCtx` (`song_details/actions.rs`),
//! the same methods Now Playing's transport uses.

use crate::api::Song;
use crate::components::ui::env::is_radio_song;
use crate::components::{use_player, AddIntent, AddMenuController, Icon};
use dioxus::prelude::*;

/// Visual size of a transport control.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum ControlSize {
    /// 34px buttons in the wide player bar.
    #[default]
    Bar,
    /// 44px touch targets in the compact mini player.
    Mini,
}

/// Play/Pause (`play-pause-btn`).
#[component]
pub(super) fn PlayPauseButton(#[props(default)] size: ControlSize) -> Element {
    let mut player = use_player();
    let playing = (player.is_playing)();

    rsx! {
        button {
            id: "play-pause-btn",
            r#type: "button",
            class: "rs-iconbtn rs-iconbtn--play",
            aria_label: if playing { "Pause" } else { "Play" },
            title: if playing { "Pause" } else { "Play" },
            onclick: move |_| player.toggle_play(),
            Icon {
                name: if playing { "pause".to_string() } else { "play".to_string() },
                class: match size {
                    ControlSize::Bar => "rs-ic rs-ic--lg".to_string(),
                    ControlSize::Mini => "rs-ic rs-ic--md".to_string(),
                },
            }
        }
    }
}

/// Previous (`prev-btn`): restart, or go back a song on a quick second press
/// (the rule lives in `skip_back.rs`). MediaSession, the macOS Playback menu
/// and keyboard shortcuts press it by id.
#[component]
pub(super) fn PrevButton(
    /// Keep `prev-btn` in the DOM for MediaSession without showing it.
    #[props(default)]
    hidden: bool,
) -> Element {
    let mut player = use_player();
    let is_radio = player.now_playing.read().as_ref().map(is_radio_song).unwrap_or(false);

    rsx! {
        button {
            id: "prev-btn",
            r#type: "button",
            disabled: is_radio,
            class: if hidden { "rs-sr-only" } else { "rs-iconbtn" },
            tabindex: if hidden { Some("-1") } else { None },
            aria_hidden: if hidden { Some("true") } else { None },
            aria_label: "Previous",
            title: "Previous",
            onclick: move |_| player.previous("player.prev"),
            Icon { name: "prev".to_string(), class: "rs-ic rs-ic--md".to_string() }
        }
    }
}

/// Next (`next-btn`).
#[component]
pub(super) fn NextButton(#[props(default)] size: ControlSize) -> Element {
    let mut player = use_player();
    let is_radio = player.now_playing.read().as_ref().map(is_radio_song).unwrap_or(false);

    rsx! {
        button {
            id: "next-btn",
            r#type: "button",
            disabled: is_radio,
            class: "rs-iconbtn",
            aria_label: "Next",
            title: "Next",
            onclick: move |_| player.next("player.next"),
            Icon {
                name: "next".to_string(),
                class: match size {
                    ControlSize::Bar => "rs-ic rs-ic--md".to_string(),
                    ControlSize::Mini => "rs-ic rs-ic--lg".to_string(),
                },
            }
        }
    }
}

/// Add the current song to the queue or a playlist (`add-menu-btn`).
#[component]
pub(super) fn AddToMenuButton() -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let mut add_menu = use_context::<AddMenuController>();
    let has_song = now_playing.read().as_ref().is_some_and(|song| !is_radio_song(song));

    rsx! {
        button {
            id: "add-menu-btn",
            r#type: "button",
            disabled: !has_song,
            class: "rs-iconbtn rs-iconbtn--quiet",
            aria_label: "Add to queue or playlist",
            title: "Add to queue or playlist",
            onclick: move |_| {
                if let Some(song) = now_playing.peek().clone().filter(|song| !is_radio_song(song)) {
                    add_menu.open(AddIntent::from_song(song));
                }
            },
            Icon {
                name: "playlist".to_string(),
                class: "rs-ic rs-ic--sm".to_string(),
            }
        }
    }
}

/// Buttons other parts of the app press by id while their visible controls
/// sit in the bar's playback menu: the macOS Playback menu presses
/// `shuffle-btn` and `repeat-btn`, and `rating-btn` opens the menu at the
/// rating. Kept in the DOM, out of sight and out of the tab order.
#[component]
pub(super) fn PressTargets(on_rate: EventHandler<()>) -> Element {
    let mut player = use_player();
    rsx! {
        button {
            id: "shuffle-btn",
            r#type: "button",
            class: "rs-sr-only",
            tabindex: "-1",
            aria_hidden: "true",
            onclick: move |_| player.toggle_shuffle(),
            "Shuffle"
        }
        button {
            id: "repeat-btn",
            r#type: "button",
            class: "rs-sr-only",
            tabindex: "-1",
            aria_hidden: "true",
            onclick: move |_| player.cycle_repeat(),
            "Repeat"
        }
        button {
            id: "rating-btn",
            r#type: "button",
            class: "rs-sr-only",
            tabindex: "-1",
            aria_hidden: "true",
            onclick: move |_| on_rate.call(()),
            "Rate"
        }
    }
}
