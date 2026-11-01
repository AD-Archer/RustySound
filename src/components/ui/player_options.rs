//! Output, Sleep Timer, Playback Speed, Equalizer and Ambient Mode rows for the track menu, shown when
//! Now Playing opens it for the playing song
//! (`TrackMenuController::open_for_player`), under a quiet "Also playing on
//! Mac" line while another player is playing. With a sync server, Output also
//! lists the user's other devices ("Play on", `remote_sync::PlayOnChoices`).

use crate::components::remote_sync::PlayOnChoices;
use crate::components::{
    set_playback_speed, speed_label, AppView, AudioOutput, Icon, Navigation, OutputPicker,
    PlaybackSync, RemoteSync, SleepTimer, SongDetailsController, EQ_AVAILABLE, PLAYBACK_SPEEDS,
    SLEEP_CHOICES,
};
use crate::components::ui::use_layout;
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Which of the player rows the menu opens with, if any.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PlayerMenu {
    /// All rows closed.
    Closed,
    Output,
    Sleep,
    Speed,
}

/// Expanding rows, like "Rate": Output (the current device; see
/// `audio_output`), Sleep Timer (15/30/45 min, 1 hour, End of Track, Off)
/// and Playback Speed (0.75×–2×). Picking a choice closes the menu through
/// `on_done`. Equalizer shows the active preset and opens its page (closing
/// Now Playing). `radio`: speed is fixed at 1× for live streams. On iPhone,
/// Output opens the AirPlay sheet instead of expanding.
#[component]
pub fn PlayerOptions(open: PlayerMenu, radio: bool, on_done: EventHandler<()>) -> Element {
    let timer = use_context::<SleepTimer>();
    let settings = use_context::<Signal<AppSettings>>();
    let output = use_context::<AudioOutput>();
    let expanded = use_signal(|| open);
    let picker = (output.picker)();
    // A sync server's other devices: the row expands into "Play on" too
    // (on iPhone, under "AirPlay & Bluetooth…").
    let remote = use_context::<RemoteSync>();
    let play_on = !remote.play_on().is_empty();
    let playing_elsewhere = remote.active_elsewhere().map(|device| device.info.name);
    let output_open = expanded() == PlayerMenu::Output && (picker != OutputPicker::AirPlay || play_on);
    let expands = picker != OutputPicker::AirPlay || play_on;
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
    let eq_summary = settings.read().equalizer.summary();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let show_shortcut = !use_layout().is_compact();

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
        if picker != OutputPicker::Hidden || play_on {
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                aria_haspopup: if expands { "menu" },
                aria_expanded: if expands { if output_open { "true" } else { "false" } },
                "data-testid": "track-menu-output",
                onclick: move |_| match picker {
                    OutputPicker::AirPlay if !play_on => {
                        output.open_system_picker();
                        on_done.call(());
                    }
                    OutputPicker::Devices => {
                        if !output_open {
                            output.refresh();
                        }
                        toggle(PlayerMenu::Output);
                    }
                    _ => toggle(PlayerMenu::Output),
                },
                span { class: "rs-menu__label", "Output" }
                span { class: "rs-menu__detail", "data-testid": "track-menu-output-route",
                    "{playing_elsewhere.clone().unwrap_or_else(|| output.route_label())}"
                }
                Icon {
                    name: if picker == OutputPicker::AirPlay { "airplay".to_string() } else { "volume-2".to_string() },
                    class: "rs-ic".to_string(),
                }
            }
        }
        if output_open {
            div {
                class: "rs-menu__choices rs-menu__choices--names",
                role: "group",
                aria_label: "Output",
                "data-testid": "output-choices",
                if picker == OutputPicker::AirPlay {
                    button {
                        r#type: "button",
                        class: "rs-menu__choice",
                        role: "menuitem",
                        "data-testid": "output-airplay",
                        onclick: move |_| {
                            output.open_system_picker();
                            on_done.call(());
                        },
                        "AirPlay & Bluetooth…"
                    }
                } else if picker == OutputPicker::SoundSettings {
                    button {
                        r#type: "button",
                        class: "rs-menu__choice",
                        role: "menuitem",
                        "data-testid": "output-sound-settings",
                        onclick: move |_| {
                            output.open_system_picker();
                            on_done.call(());
                        },
                        "Sound Settings…"
                    }
                } else {
                    for (index, device) in (output.devices)().into_iter().enumerate() {
                        button {
                            key: "{device.id}",
                            r#type: "button",
                            class: "rs-menu__choice",
                            role: "menuitemradio",
                            aria_checked: if *output.selected.read() == device.id { "true" } else { "false" },
                            title: "{device.label}",
                            "data-testid": "output-choice-{index}",
                            onclick: {
                                let device = device.clone();
                                move |_| {
                                    output.choose(device.clone());
                                    on_done.call(());
                                }
                            },
                            "{device.label}"
                        }
                    }
                    if (output.can_ask)() {
                        button {
                            r#type: "button",
                            class: "rs-menu__choice",
                            role: "menuitem",
                            "data-testid": "output-ask",
                            onclick: move |_| {
                                output.ask();
                                on_done.call(());
                            },
                            "Other Device…"
                        }
                    }
                }
                PlayOnChoices { on_done }
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
        if EQ_AVAILABLE {
            button {
                r#type: "button",
                class: "rs-menu__item",
                role: "menuitem",
                "data-testid": "track-menu-equalizer",
                onclick: move |_| {
                    let mut song_details = song_details;
                    on_done.call(());
                    song_details.close();
                    navigation.navigate_to(AppView::EqualizerView {});
                },
                span { class: "rs-menu__label", "Equalizer" }
                span { class: "rs-menu__detail", "data-testid": "track-menu-equalizer-preset", "{eq_summary}" }
                Icon { name: "sliders".to_string(), class: "rs-ic".to_string() }
            }
        }
        // Ambient mode (components/ambient). The A key opens it too while
        // Now Playing has focus; the hint shows where there's a keyboard.
        button {
            r#type: "button",
            class: "rs-menu__item",
            role: "menuitem",
            "aria-keyshortcuts": "A",
            "data-testid": "track-menu-ambient",
            onclick: move |_| {
                let mut song_details = song_details;
                on_done.call(());
                song_details.set_ambient(true);
            },
            span { class: "rs-menu__label", "Ambient Mode" }
            if show_shortcut {
                span { class: "rs-menu__detail", aria_hidden: "true", "A" }
            }
            Icon { name: "maximize".to_string(), class: "rs-ic".to_string() }
        }
    }
}
