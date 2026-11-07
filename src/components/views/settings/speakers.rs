//! Settings › Playback › Speakers: network discovery on or off, the
//! speakers found (pick one to play there, the playing one to come back),
//! and Refresh. Mac and iPhone only; the web build shows nothing.

use super::shared::{ActionRow, Footnote, GroupTitle, SwitchRow};
use crate::components::cast::{CastCtx, CastRequest, CAST_AVAILABLE};
use dioxus::prelude::*;

#[component]
pub(super) fn SpeakersGroup() -> Element {
    let cast = use_context::<CastCtx>();
    // Opening the page refreshes a stale list (and asks for the local
    // network permission the first time).
    use_hook(move || {
        if CAST_AVAILABLE {
            cast.send(CastRequest::Search { fresh_only: true });
        }
    });
    if !CAST_AVAILABLE {
        return rsx! {};
    }
    let discovery = (cast.discovery)();
    let searching = (cast.searching)();
    let speakers = (cast.speakers)();
    let active = (cast.active)();
    let none_found = (cast.searched)() && !searching && speakers.is_empty();

    rsx! {
        GroupTitle { title: "Speakers".to_string() }
        ul { class: "rs-list rs-list--text", "data-testid": "settings-speakers",
            SwitchRow {
                id: "speaker-discovery".to_string(),
                title: "Find Speakers".to_string(),
                subtitle: "Look for Sonos, DLNA and Chromecast speakers on this network.".to_string(),
                checked: discovery,
                on_toggle: move |on| cast.send(CastRequest::SetDiscovery(on)),
            }
            if discovery {
                for speaker in speakers {
                    li { key: "{speaker.id}",
                        button {
                            r#type: "button",
                            class: "rs-row rs-row--compact rs-settings-row rs-settings-link",
                            aria_pressed: if active.as_ref().is_some_and(|a| a.id == speaker.id) { "true" } else { "false" },
                            onclick: {
                                let id = speaker.id.clone();
                                let playing = active.as_ref().is_some_and(|a| a.id == speaker.id);
                                move |_| cast.send(CastRequest::Cast(if playing { None } else { Some(id.clone()) }))
                            },
                            span { class: "rs-row__meta",
                                span { class: "rs-settings-row__title", "{speaker.name}" }
                                span { class: "rs-settings-row__sub", "{speaker.detail}" }
                            }
                            match active.as_ref().filter(|a| a.id == speaker.id) {
                                Some(a) if a.connecting => rsx! { span { class: "rs-row__detail", "Connecting…" } },
                                Some(_) => rsx! { span { class: "rs-row__detail", "Playing" } },
                                None => rsx! {},
                            }
                        }
                    }
                }
                if none_found {
                    li {
                        div { class: "rs-row rs-row--compact rs-settings-row",
                            span { class: "rs-row__meta",
                                span { class: "rs-settings-row__sub", "No speakers found" }
                            }
                        }
                    }
                }
                ActionRow {
                    label: "Refresh".to_string(),
                    icon: "refresh-cw".to_string(),
                    busy: searching,
                    testid: "speakers-refresh".to_string(),
                    onclick: move |_| cast.send(CastRequest::Search { fresh_only: false }),
                }
            }
        }
        Footnote {
            "Speakers fetch music from your server themselves, so it has to be reachable from them. Songs on this device are served to the speaker while RustySound is open."
        }
    }
}
