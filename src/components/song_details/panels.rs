//! Lyrics | Up Next | Info, as a segmented control on the stage.

use super::actions::{is_live_song, song_key, use_player};
use super::info::InfoPanel;
use super::lyrics::{use_lyrics_source, LyricsAvailability, LyricsPanel};
use super::types::{NowPlayingFocus, Panel, SongDetailsController, SongDetailsTab};
use super::up_next::UpNextPanel;
use crate::api::{LyricsQuery, Song};
use crate::components::ui::{IconButton, IconButtonSize, Segmented};
use dioxus::prelude::*;

#[component]
pub(super) fn NowPlayingPanels(song: Song, tab: Option<SongDetailsTab>, wide: bool) -> Element {
    let mut controller = use_context::<SongDetailsController>();
    // Song identity + lyrics query; unchanged by star/rating edits, so those
    // never refetch lyrics.
    // The query carries the song's server so the server provider can ask it.
    let player = use_player();
    let base = use_memo(use_reactive!(|song| Some((
        song_key(&song),
        LyricsQuery::from_song_on(&song, &player.servers.read())
    ))));
    let source = use_lyrics_source(base);
    use_context_provider(|| source);

    let live = is_live_song(&song);
    let automatic = match source.availability() {
        LyricsAvailability::Missing if live => Panel::Info,
        LyricsAvailability::Missing => Panel::UpNext,
        LyricsAvailability::Loading | LyricsAvailability::Available => Panel::Lyrics,
    };
    let panel = tab.map(Panel::from_tab).unwrap_or(automatic);
    // Compact only: the focused view. Tapping the active segment again
    // toggles it, and so does the chevron (the path without gestures).
    let NowPlayingFocus(mut focus) = use_context::<NowPlayingFocus>();
    let focused = !wide && focus();

    rsx! {
        div {
            class: "rs-player-panels",
            class: if wide { "rs-player-panels--wide" },
            div { class: "rs-player-panels__head",
                Segmented {
                    options: Panel::ALL.iter().map(|p| p.label().to_string()).collect::<Vec<_>>(),
                    selected: panel.index(),
                    stage: true,
                    label: "Song panels".to_string(),
                    testid: "now-playing-panels".to_string(),
                    on_select: move |index: usize| {
                        if !wide && index == panel.index() {
                            focus.toggle();
                        } else {
                            controller.set_tab(Panel::ALL[index].tab());
                        }
                    },
                }
                if !wide {
                    IconButton {
                        icon: if focused { "chevron-down".to_string() } else { "chevron-up".to_string() },
                        label: if focused { "Show full player".to_string() } else { "Expand panel".to_string() },
                        quiet: true,
                        size: IconButtonSize::Small,
                        testid: "now-playing-expand".to_string(),
                        class: "rs-player-panels__expand".to_string(),
                        onclick: move |_| focus.toggle(),
                    }
                }
            }
            div { class: "rs-player-panel",
                match panel {
                    Panel::Lyrics => rsx! { LyricsPanel { song: song.clone() } },
                    Panel::UpNext => rsx! { UpNextPanel { song: song.clone() } },
                    Panel::Info => rsx! { InfoPanel { song: song.clone(), wide } },
                }
            }
        }
    }
}
