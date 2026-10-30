//! The Lyrics panel on the Now Playing stage.
//!
//! Big readable lines; everything else (refresh, search-and-pick, timing
//! offset, the provider settings shortcut, and the Shot share mode) lives in
//! the "…" menu in the panel header.

mod lines;
mod search;
mod shot;
mod shot_card;
mod shot_share;
mod source;

pub(crate) use source::{use_lyrics_source, LyricsAvailability, LyricsSource};

use super::actions::{is_live_song, song_key};
use super::types::SongDetailsController;
use crate::api::{LyricsResult, Song};
use crate::components::ui::{IconButton, IconButtonSize, InsetList, ListInset, ListRow, Sheet};
use crate::components::{AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;
use lines::{PlainLyrics, SyncedLyrics};
use search::LyricsSearchSheet;
use shot::LyricsShot;

const OFFSET_STEP_MS: i32 = 250;
const OFFSET_LIMIT_MS: i32 = 10_000;

pub(crate) fn plain_lines(lyrics: &LyricsResult) -> Vec<String> {
    lyrics
        .plain_lyrics
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn dom_id_for(song: &Song) -> String {
    let raw = song_key(song);
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    format!("rs-lyrics-{cleaned}")
}

#[component]
pub(crate) fn LyricsPanel(song: Song) -> Element {
    let source = use_context::<LyricsSource>();
    let settings = use_context::<Signal<AppSettings>>();
    // Only the two settings this panel shows; other settings writes don't
    // re-render it.
    let shot_enabled = use_memo(move || settings.read().lyrics_screenshot_mode);
    let mut menu_open = use_signal(|| false);
    let mut search_open = use_signal(|| false);
    let mut shot_open = use_signal(|| false);

    let (lyrics, error) = source.shown();
    let live = is_live_song(&song);
    let sync = source.sync_enabled();
    let dom_id = dom_id_for(&song);
    let provider = lyrics.as_ref().map(|l| l.provider.label());

    let body = match lyrics.clone() {
        None if error.is_none() => rsx! {
            div { class: "rs-player-lyrics__state", role: "status",
                span { class: "rs-spinner", aria_hidden: "true" }
                "Loading lyrics…"
            }
        },
        None => rsx! {
            div { class: "rs-player-lyrics__state",
                p { class: "rs-player-lyrics__state-title", "No lyrics found" }
                p { "Search by title and pick the exact match." }
                button {
                    r#type: "button",
                    class: "rs-pill rs-pill--quiet",
                    onclick: move |_| search_open.set(true),
                    "Search Lyrics"
                }
            }
        },
        Some(found) => {
            if sync && !found.synced_lines.is_empty() {
                let key = format!(
                    "{}:{}:{}:{}",
                    dom_id,
                    found.provider.label(),
                    found.synced_lines.len(),
                    found.synced_lines.first().map(|l| l.timestamp_seconds).unwrap_or(0.0)
                );
                rsx! {
                    SyncedLyrics { key: "{key}", lines: found.synced_lines.clone(), live, dom_id: dom_id.clone() }
                }
            } else {
                let lines = plain_lines(&found);
                rsx! {
                    if sync {
                        p { class: "rs-player-lyrics__note", "No synced timing for this result; showing plain lyrics." }
                    }
                    if lines.is_empty() {
                        div { class: "rs-player-lyrics__state", "Lyrics unavailable." }
                    } else {
                        PlainLyrics { lines }
                    }
                }
            }
        }
    };

    rsx! {
        section { class: "rs-player-lyrics", "data-testid": "lyrics", aria_label: "Lyrics",
            div { class: "rs-player-lyrics__head",
                span { class: "rs-player-lyrics__source",
                    if let Some(provider) = provider {
                        "{provider}"
                    }
                    if lyrics.is_some() && error.is_some() {
                        " · last loaded"
                    }
                    if live {
                        " · live: no sync"
                    }
                }
                IconButton {
                    icon: "more".to_string(),
                    label: "Lyrics options".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "lyrics-menu".to_string(),
                    onclick: move |_| menu_open.set(true),
                }
            }
            {body}
        }
        if menu_open() {
            LyricsMenu {
                can_shot: shot_enabled() && lyrics.is_some(),
                on_close: move |_| menu_open.set(false),
                on_search: move |_| {
                    menu_open.set(false);
                    search_open.set(true);
                },
                on_shot: move |_| {
                    menu_open.set(false);
                    shot_open.set(true);
                },
            }
        }
        if search_open() {
            LyricsSearchSheet { default_title: song.title.clone(), on_close: move |_| search_open.set(false) }
        }
        if shot_open() {
            if let Some(found) = lyrics.clone() {
                LyricsShot { song: song.clone(), lyrics: found, sync, on_close: move |_| shot_open.set(false) }
            }
        }
    }
}

/// The lyrics "…" menu.
#[component]
fn LyricsMenu(
    can_shot: bool,
    on_close: EventHandler<()>,
    on_search: EventHandler<()>,
    on_shot: EventHandler<()>,
) -> Element {
    let mut source = use_context::<LyricsSource>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let navigation = use_context::<Navigation>();
    let mut controller = use_context::<SongDetailsController>();
    let offset_ms = settings.read().lyrics_offset_ms;
    let offset_text = if offset_ms == 0 {
        "In sync".to_string()
    } else {
        format!("{:+.2} s", offset_ms as f64 / 1000.0)
    };

    let mut set_offset = move |value: i32| {
        let value = value.clamp(-OFFSET_LIMIT_MS, OFFSET_LIMIT_MS);
        settings.with_mut(|s| s.lyrics_offset_ms = value);
        let snapshot = settings.peek().clone();
        spawn(async move {
            let _ = crate::db::save_settings(snapshot).await;
        });
    };

    rsx! {
        Sheet {
            title: "Lyrics".to_string(),
            testid: "lyrics-options".to_string(),
            on_close: move |_| on_close.call(()),
            InsetList { inset: ListInset::Icon,
                ListRow {
                    title: "Refresh Lyrics".to_string(),
                    icon: "refresh-cw".to_string(),
                    compact: true,
                    onclick: move |_| {
                        source.refresh();
                        on_close.call(());
                    },
                }
                ListRow {
                    title: "Search and Pick Lyrics…".to_string(),
                    icon: "search".to_string(),
                    compact: true,
                    onclick: move |_| on_search.call(()),
                }
                if can_shot {
                    ListRow {
                        title: "Lyrics Shot".to_string(),
                        subtitle: "Share selected lines as an image".to_string(),
                        icon: "eye".to_string(),
                        compact: true,
                        onclick: move |_| on_shot.call(()),
                    }
                }
            }
            InsetList { inset: ListInset::Icon, class: "rs-player-sheet-gap".to_string(),
                ListRow {
                    title: "Timing Offset".to_string(),
                    subtitle: offset_text.clone(),
                    icon: "clock".to_string(),
                    compact: true,
                    trailing: rsx! {
                        div { class: "rs-player-stepper", role: "group", aria_label: "Lyrics timing offset",
                            button {
                                r#type: "button",
                                class: "rs-player-stepper__btn",
                                aria_label: "Show lines a quarter second later",
                                onclick: move |_| set_offset(offset_ms - OFFSET_STEP_MS),
                                "\u{2212}"
                            }
                            button {
                                r#type: "button",
                                class: "rs-player-stepper__btn",
                                aria_label: "Reset lyrics timing",
                                disabled: offset_ms == 0,
                                onclick: move |_| set_offset(0),
                                "0"
                            }
                            button {
                                r#type: "button",
                                class: "rs-player-stepper__btn",
                                aria_label: "Show lines a quarter second earlier",
                                onclick: move |_| set_offset(offset_ms + OFFSET_STEP_MS),
                                "+"
                            }
                        }
                    },
                }
                ListRow {
                    title: "Lyrics Providers…".to_string(),
                    subtitle: "Order, timeout and sync in Settings".to_string(),
                    icon: "settings".to_string(),
                    chevron: true,
                    compact: true,
                    onclick: move |_| {
                        on_close.call(());
                        controller.close();
                        navigation.navigate_to(AppView::SettingsView {});
                    },
                }
            }
            p { class: "rs-list-foot",
                Icon { name: "info".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                " A positive offset shows each line earlier; a negative one, later."
            }
        }
    }
}
