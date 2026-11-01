//! Sleep Timer and Playback Speed rows for the track menu, shown when Now
//! Playing opens it for the playing song (`TrackMenuController::open_for_player`),
//! under a quiet "Also playing on Mac" line while another player is playing.

use crate::components::{
    set_playback_speed, speed_label, Icon, PlaybackSync, SleepChoice, SleepTimer,
    PLAYBACK_SPEEDS, SLEEP_CHOICES,
};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Which of the player rows the menu opens with, if any.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlayerMenu {
    /// Both rows closed.
    Closed,
    Sleep,
    Speed,
}

/// Two expanding rows, like "Rate": Sleep Timer (15/30/45 min, 1 hour, End
/// of Track, Off) and Playback Speed (0.75×–2×). Picking a choice closes the
/// menu through `on_done`. `radio`: speed is fixed at 1× for live streams.
#[component]
pub fn PlayerOptions(open: PlayerMenu, radio: bool, on_done: EventHandler<()>) -> Element {
    let timer = use_context::<SleepTimer>();
    let settings = use_context::<Signal<AppSettings>>();
    let expanded = use_signal(|| open);
    let sleep_open = expanded() == PlayerMenu::Sleep;
    let speed_open = expanded() == PlayerMenu::Speed && !radio;
    let mode = (timer.mode)();
    let minutes = (timer.minutes)();
    let sleep_detail = timer.label().unwrap_or_else(|| "Off".to_string());
    let rate = if radio { 1.0 } else { settings.read().playback_rate };
    let toggle = move |row: PlayerMenu| {
        let mut expanded = expanded;
        let next = if *expanded.peek() == row { PlayerMenu::Closed } else { row };
        expanded.set(next);
    };
    // Another player of this user (polled while Now Playing is open).
    let elsewhere = (use_context::<PlaybackSync>().elsewhere)();

    rsx! {
        if let Some(other) = elsewhere {
            div {
                class: "rs-menu__item rs-menu__item--note",
                role: "menuitem",
                aria_disabled: "true",
                tabindex: "-1",
                "data-testid": "playing-elsewhere",
                span { class: "rs-menu__label", "Also playing on {other.device}" }
                span { class: "rs-menu__detail rs-menu__label", "{other.title}" }
                Icon { name: "headphones".to_string(), class: "rs-ic".to_string() }
            }
        }
        button {
            r#type: "button",
            class: "rs-menu__item",
            role: "menuitem",
            aria_haspopup: "menu",
            aria_expanded: if sleep_open { "true" } else { "false" },
            "data-testid": "track-menu-sleep",
            onclick: move |_| toggle(PlayerMenu::Sleep),
            span { class: "rs-menu__label", "Sleep Timer" }
            span { class: "rs-menu__detail", "{sleep_detail}" }
            Icon { name: "moon".to_string(), class: "rs-ic".to_string() }
        }
        if sleep_open {
            div {
                class: "rs-menu__choices",
                role: "group",
                aria_label: "Sleep Timer",
                "data-testid": "sleep-choices",
                for choice in SLEEP_CHOICES {
                    button {
                        key: "{choice.testid()}",
                        r#type: "button",
                        class: "rs-menu__choice",
                        role: "menuitemradio",
                        aria_checked: if mode.matches(choice, minutes) { "true" } else { "false" },
                        "data-testid": choice.testid(),
                        onclick: move |_| {
                            timer.set(choice);
                            on_done.call(());
                        },
                        "{choice.label()}"
                    }
                }
            }
        }
        button {
            r#type: "button",
            class: "rs-menu__item",
            role: "menuitem",
            aria_haspopup: "menu",
            aria_expanded: if speed_open { "true" } else { "false" },
            disabled: radio,
            "data-testid": "track-menu-speed",
            onclick: move |_| toggle(PlayerMenu::Speed),
            span { class: "rs-menu__label", "Playback Speed" }
            span { class: "rs-menu__detail", "{speed_label(rate)}" }
            Icon { name: "speed".to_string(), class: "rs-ic".to_string() }
        }
        if speed_open {
            div {
                class: "rs-menu__choices",
                role: "group",
                aria_label: "Playback Speed",
                "data-testid": "speed-choices",
                for speed in PLAYBACK_SPEEDS {
                    button {
                        key: "{speed}",
                        r#type: "button",
                        class: "rs-menu__choice",
                        role: "menuitemradio",
                        aria_checked: if speed == rate { "true" } else { "false" },
                        "data-testid": "speed-choice-{speed}",
                        onclick: move |_| {
                            set_playback_speed(settings, speed);
                            on_done.call(());
                        },
                        "{speed_label(speed)}"
                    }
                }
            }
        }
    }
}
