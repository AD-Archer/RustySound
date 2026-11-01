//! Speakers in the player: the "Speakers" row of Now Playing's "…" menu
//! (beside Output), and the quiet "Playing on Living Room" indicator next
//! to the format badge. Both render nothing where casting isn't available.

use super::{CastCtx, CastRequest, CAST_AVAILABLE, THIS_DEVICE};
use crate::api::Song;
use crate::components::ui::{PlayerMenu, TrackMenuController};
use crate::components::Icon;
use dioxus::prelude::*;

/// An expanding row like Output: this device first, then the speakers
/// found (searching again when the list is over a minute old). Picking one
/// closes the menu through `on_done`. Hidden with discovery off, unless a
/// speaker is playing (so there's a way back).
#[component]
pub fn SpeakersMenu(open: bool, on_toggle: EventHandler<()>, on_done: EventHandler<()>) -> Element {
    let cast = use_context::<CastCtx>();
    let active = (cast.active)();
    if !CAST_AVAILABLE || (!(cast.discovery)() && active.is_none()) {
        return rsx! {};
    }
    let speakers = (cast.speakers)();
    let searching = (cast.searching)();
    let detail = match &active {
        Some(active) if active.connecting => "Connecting…".to_string(),
        Some(active) => active.name.clone(),
        None => THIS_DEVICE.to_string(),
    };
    let active_id = active.as_ref().map(|active| active.id.clone());

    rsx! {
        button {
            r#type: "button",
            class: "rs-menu__item",
            role: "menuitem",
            aria_haspopup: "menu",
            aria_expanded: if open { "true" } else { "false" },
            "data-testid": "track-menu-speakers",
            onclick: move |_| {
                if !open {
                    cast.send(CastRequest::Search { fresh_only: true });
                }
                on_toggle.call(());
            },
            span { class: "rs-menu__label", "Speakers" }
            span { class: "rs-menu__detail", "data-testid": "track-menu-speakers-current", "{detail}" }
            Icon { name: "speaker".to_string(), class: "rs-ic".to_string() }
        }
        if open {
            div {
                class: "rs-menu__choices rs-menu__choices--names",
                role: "group",
                aria_label: "Speakers",
                "data-testid": "speaker-choices",
                button {
                    r#type: "button",
                    class: "rs-menu__choice",
                    role: "menuitemradio",
                    aria_checked: if active_id.is_none() { "true" } else { "false" },
                    "data-testid": "speaker-choice-local",
                    onclick: move |_| {
                        cast.send(CastRequest::Cast(None));
                        on_done.call(());
                    },
                    "{THIS_DEVICE}"
                }
                for speaker in speakers {
                    button {
                        key: "{speaker.id}",
                        r#type: "button",
                        class: "rs-menu__choice",
                        role: "menuitemradio",
                        aria_checked: if active_id.as_deref() == Some(speaker.id.as_str()) { "true" } else { "false" },
                        title: "{speaker.name} · {speaker.detail}",
                        onclick: {
                            let id = speaker.id.clone();
                            move |_| {
                                cast.send(CastRequest::Cast(Some(id.clone())));
                                on_done.call(());
                            }
                        },
                        "{speaker.name}"
                    }
                }
                if searching {
                    span { class: "rs-menu__choice rs-menu__choice--note", aria_live: "polite", "Searching…" }
                } else if (cast.searched)() && (cast.speakers)().is_empty() {
                    span { class: "rs-menu__choice rs-menu__choice--note", "No speakers found" }
                }
            }
        }
    }
}

/// "Playing on Living Room" beside the format badge in Now Playing while a
/// speaker plays; opens the menu on Speakers.
#[component]
pub fn CastIndicator(song: Song) -> Element {
    let cast = use_context::<CastCtx>();
    let mut menu = use_context::<TrackMenuController>();
    let Some(name) = cast.playing_on() else {
        return rsx! {};
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-player-cast",
            aria_label: "Playing on {name}",
            title: "Playing on {name}",
            "data-testid": "cast-indicator",
            onclick: move |evt: MouseEvent| {
                let point = evt.client_coordinates();
                menu.open_for_player(
                    song.clone(),
                    Some((point.x - 130.0, point.y + 12.0)),
                    PlayerMenu::Speakers,
                );
            },
            Icon { name: "speaker".to_string(), class: "rs-ic rs-ic--xs".to_string() }
            span { class: "rs-truncate", "{name}" }
        }
    }
}

/// The playing speaker's name, for the mini player and the wide bar.
pub fn use_playing_on() -> Option<String> {
    try_use_context::<CastCtx>().and_then(|cast| cast.playing_on())
}
