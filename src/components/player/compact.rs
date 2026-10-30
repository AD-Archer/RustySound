//! Compact mini player: the floating black bar above the tab bar.

use super::controls::{ControlSize, NextButton, PlayPauseButton, PrevButton};
use crate::api::Song;
use crate::components::ui::env::is_radio_song;
use crate::components::ui::{Artwork, MiniProgressLine};
use crate::components::SongDetailsController;
use dioxus::prelude::*;

/// Art, title/artist, play/pause and next, with a thin progress line.
/// Tapping the art or text opens Now Playing (the song details overlay).
#[component]
pub(super) fn CompactPlayer() -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let song_details = use_context::<SongDetailsController>();

    let Some(song) = now_playing() else {
        return rsx! {};
    };
    let radio = is_radio_song(&song);
    let title = if radio && song.title.trim().is_empty() {
        "Internet Radio".to_string()
    } else {
        song.title.clone()
    };
    let subtitle = if radio {
        song.album
            .clone()
            .or_else(|| song.artist.clone())
            .unwrap_or_else(|| "Internet Radio".to_string())
    } else {
        song.artist.clone().unwrap_or_default()
    };

    rsx! {
        div { class: "rs-mini", role: "region", aria_label: "Now playing", "data-testid": "mini-player",
            button {
                r#type: "button",
                class: "rs-mini__open",
                // Named by its visible text, with the action read first.
                onclick: {
                    let song = song.clone();
                    let mut song_details = song_details.clone();
                    move |_| song_details.open(song.clone())
                },
                Artwork {
                    cover_art: song.cover_art.clone(),
                    server_id: song.server_id.clone(),
                    size_px: 40,
                    eager: true,
                    placeholder: if radio { "radio".to_string() } else { "music".to_string() },
                }
                span { class: "rs-sr-only", "Open Now Playing: " }
                span { class: "rs-mini__meta",
                    span { class: "rs-mini__title", style: "display:block", "{title}" }
                    if !subtitle.is_empty() {
                        span { class: "rs-mini__sub", style: "display:block", "{subtitle}" }
                    }
                }
            }
            PlayPauseButton { size: ControlSize::Mini }
            NextButton { size: ControlSize::Mini }
            PrevButton { hidden: true }
            MiniProgressLine {}
        }
    }
}
