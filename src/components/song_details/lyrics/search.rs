//! "Search and Pick Lyrics": search the providers by title and choose the
//! exact match for the current song.

use super::LyricsSource;
use crate::api::format_duration;
use crate::components::ui::{InsetList, ListInset, Button, ButtonVariant, Sheet};
use crate::components::Icon;
use dioxus::prelude::*;

#[component]
pub(super) fn LyricsSearchSheet(default_title: String, on_close: EventHandler<()>) -> Element {
    let mut source = use_context::<LyricsSource>();
    let mut input = use_signal({
        let start = source
            .search_title
            .peek()
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| default_title.clone());
        move || start
    });
    let term = (source.search_term)().filter(|t| !t.trim().is_empty());
    let selected = (source.override_query)();

    rsx! {
        Sheet {
            title: "Search Lyrics".to_string(),
            testid: "lyrics-search".to_string(),
            on_close: move |_| on_close.call(()),
            form {
                class: "rs-player-search",
                onsubmit: move |evt| {
                    evt.prevent_default();
                    source.search(input.peek().clone());
                },
                label { class: "rs-field rs-player-search__field",
                    Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    input {
                        r#type: "search",
                        value: "{input}",
                        placeholder: "Song title",
                        aria_label: "Song title to search lyrics for",
                        oninput: move |evt| input.set(evt.value()),
                    }
                }
                div { class: "rs-player-search__actions",
                    button { r#type: "submit", class: "rs-btn rs-btn--filled", "Search" }
                    Button {
                        label: "Use Current Song".to_string(),
                        variant: ButtonVariant::Tinted,
                        onclick: {
                            let default_title = default_title.clone();
                            move |_| {
                                input.set(default_title.clone());
                                source.clear_search();
                            }
                        },
                    }
                }
            }
            if let Some(term) = term {
                p { class: "rs-list-foot", "Pick the best match for \u{201C}{term}\u{201D}" }
                match (source.candidates)() {
                    None => rsx! {
                        div { class: "rs-player-search__state", role: "status",
                            span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                            "Searching…"
                        }
                    },
                    Some(Err(error)) => rsx! {
                        p { class: "rs-player-search__state", "{error}" }
                    },
                    Some(Ok(candidates)) if candidates.is_empty() => rsx! {
                        p { class: "rs-player-search__state", "No lyric matches found." }
                    },
                    Some(Ok(candidates)) => rsx! {
                        InsetList { inset: ListInset::Text, label: "Lyrics matches".to_string(),
                            for (i, candidate) in candidates.into_iter().enumerate() {
                                li { key: "{i}",
                                    button {
                                        r#type: "button",
                                        class: "rs-row rs-row--compact",
                                        aria_pressed: if selected.as_ref() == Some(&candidate.query) { "true" } else { "false" },
                                        onclick: {
                                            let query = candidate.query.clone();
                                            move |_| {
                                                source.pick(query.clone());
                                                on_close.call(());
                                            }
                                        },
                                        span { class: "rs-row__meta",
                                            span { class: "rs-row__title", span { "{candidate.title}" } }
                                            span { class: "rs-row__sub", style: "display:block",
                                                "{candidate.artist}"
                                                if !candidate.album.trim().is_empty() {
                                                    " · {candidate.album}"
                                                }
                                            }
                                        }
                                        span { class: "rs-row__detail",
                                            "{candidate.provider.label()}"
                                            if let Some(duration) = candidate.duration_seconds {
                                                " · {format_duration(duration)}"
                                            }
                                        }
                                        if selected.as_ref() == Some(&candidate.query) {
                                            span { class: "rs-check rs-check--on", aria_hidden: "true",
                                                Icon { name: "check".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
