//! Lyrics | Up Next | Info, as a segmented control on the stage.

use super::actions::{is_live_song, song_key};
use super::info::InfoPanel;
use super::lyrics::{use_lyrics_source, LyricsAvailability, LyricsPanel};
use super::types::{Panel, SongDetailsController, SongDetailsTab};
use super::up_next::UpNextPanel;
use crate::api::{LyricsQuery, Song};
use crate::components::ui::Segmented;
use dioxus::prelude::*;

#[component]
pub(super) fn NowPlayingPanels(song: Song, tab: Option<SongDetailsTab>, wide: bool) -> Element {
    let mut controller = use_context::<SongDetailsController>();
    // Song identity + lyrics query; unchanged by star/rating edits, so those
    // never refetch lyrics.
    let base = use_memo(use_reactive!(|song| Some((
        song_key(&song),
        LyricsQuery::from_song(&song)
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

    rsx! {
        div {
            class: "rs-player-panels",
            class: if wide { "rs-player-panels--wide" },
            Segmented {
                options: Panel::ALL.iter().map(|p| p.label().to_string()).collect::<Vec<_>>(),
                selected: panel.index(),
                stage: true,
                label: "Song panels".to_string(),
                testid: "now-playing-panels".to_string(),
                on_select: move |index: usize| controller.set_tab(Panel::ALL[index].tab()),
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
