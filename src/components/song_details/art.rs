//! The big cover with its reflection, the songs already played waiting to its
//! left and the next ones to its right, rotated Cover Flow style. Tapping a
//! side cover plays that song.

use super::actions::{is_live_song, use_player};
use crate::api::Song;
use crate::components::ui::Artwork;
use dioxus::prelude::*;

/// How many covers wait on each side of the current one.
const WAITING: usize = 3;

#[component]
pub(super) fn NowPlayingArt(song: Song, wide: bool) -> Element {
    let player = use_player();
    // Re-runs on queue changes only; the component re-renders only when the
    // waiting covers actually change.
    let waiting = use_memo(move || {
        let queue = player.queue.read();
        let index = *player.queue_index.read();
        queue
            .iter()
            .enumerate()
            .skip(index + 1)
            .take(WAITING)
            .map(|(i, s)| (i, s.clone()))
            .collect::<Vec<_>>()
    });
    // Nearest first, like `waiting`.
    let played = use_memo(move || {
        let queue = player.queue.read();
        let index = *player.queue_index.read();
        queue
            .iter()
            .enumerate()
            .take(index)
            .rev()
            .take(WAITING)
            .map(|(i, s)| (i, s.clone()))
            .collect::<Vec<_>>()
    });
    let radio = is_live_song(&song);
    let waiting = if radio { Vec::new() } else { waiting() };
    let played = if radio { Vec::new() } else { played() };
    let title = song.title.clone();

    rsx! {
        div {
            class: "rs-player-flow",
            class: if wide { "rs-player-flow--wide" },
            role: "group",
            aria_label: "Now playing, played and up next",
            // Farthest first on each side, so nearer covers paint over them.
            for (step, (index, prev)) in played.into_iter().enumerate().rev() {
                button {
                    key: "{index}:{prev.server_id}:{prev.id}",
                    r#type: "button",
                    class: "rs-player-flow__side rs-player-flow__side--prev",
                    style: "--i:{step}",
                    aria_label: "Go back to {prev.title}",
                    title: "{prev.title}",
                    "data-testid": "now-playing-prev-cover",
                    onpointerdown: move |evt| evt.stop_propagation(),
                    onclick: move |_| {
                        let mut player = player;
                        player.play_index(index);
                    },
                    Artwork {
                        cover_art: prev.cover_art.clone(),
                        server_id: prev.server_id.clone(),
                        size_px: 240,
                        fill: true,
                        hero: true,
                    }
                }
            }
            for (step, (index, next)) in waiting.into_iter().enumerate().rev() {
                button {
                    key: "{index}:{next.server_id}:{next.id}",
                    r#type: "button",
                    class: "rs-player-flow__side",
                    style: "--i:{step}",
                    aria_label: "Skip to {next.title}",
                    title: "{next.title}",
                    "data-testid": "now-playing-next-cover",
                    onpointerdown: move |evt| evt.stop_propagation(),
                    onclick: move |_| {
                        let mut player = player;
                        player.play_index(index);
                    },
                    Artwork {
                        cover_art: next.cover_art.clone(),
                        server_id: next.server_id.clone(),
                        size_px: 240,
                        fill: true,
                        hero: true,
                    }
                }
            }
            div { class: "rs-player-flow__cover",
                Artwork {
                    cover_art: song.cover_art.clone(),
                    server_id: song.server_id.clone(),
                    size_px: if wide { 420 } else { 320 },
                    fill: true,
                    hero: true,
                    eager: true,
                    alt: "{title} cover",
                    placeholder: if radio { "radio".to_string() } else { "music".to_string() },
                }
            }
        }
    }
}
