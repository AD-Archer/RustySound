//! The big cover with its reflection, the songs already played waiting to its
//! left and the next ones to its right, rotated Cover Flow style. Tapping a
//! side cover plays that song. On compact, swiping the big cover sideways
//! steps one song (`gestures.js` moves the covers; this sets the data it
//! reads).

use super::actions::{is_live_song, song_key, use_player};
use super::clock::NowPlayingClock;
use super::gestures::song_token;
use super::source::{source_page, SourcePage};
use super::types::SongDetailsController;
use crate::api::Song;
use crate::components::ui::Artwork;
use crate::components::Navigation;
use dioxus::prelude::*;

/// How far a press may move and still count as a tap on the big cover.
const TAP_SLOP_PX: f64 = 10.0;

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
    // Another device plays: this device's queue isn't what it plays.
    let remote = try_use_context::<NowPlayingClock>().is_some_and(|clock| (clock.remote)());
    let waiting = if radio || remote { Vec::new() } else { waiting() };
    let played = if radio || remote { Vec::new() } else { played() };
    let title = song.title.clone();
    let source = if radio { None } else { source_page(&song) };
    let controller = use_context::<SongDetailsController>();
    let navigation = use_context::<Navigation>();
    let mut press_at = use_signal(|| None::<(f64, f64)>);
    let open_source = move |page: Option<SourcePage>| {
        if let Some(page) = page {
            let mut controller = controller;
            controller.close();
            navigation.navigate_to(page.view);
        }
    };
    // Cover swipe (compact, gestures.js): which entry it starts on and which
    // ways it can go. Off for radio and on wide.
    let token = song_token((player.queue_index)(), &song_key(&song));
    let swipe = !wide && !radio && !remote;
    let (has_prev, has_next) = (!played.is_empty(), !waiting.is_empty());
    // Compact side covers load the big cover's size, so the one a swipe
    // brings to the middle is already the cached image.
    let side_px = if wide { 240 } else { 320 };

    rsx! {
        div {
            class: "rs-player-flow",
            class: if wide { "rs-player-flow--wide" },
            role: "group",
            aria_label: "Now playing, played and up next",
            "data-no-edge-swipe": "true",
            "data-swipe-off": if !swipe { Some("true") } else { None },
            "data-song-token": "{token}",
            "data-has-prev": if has_prev { "true" } else { "false" },
            "data-has-next": if has_next { "true" } else { "false" },
            // Farthest first on each side, so nearer covers paint over them.
            for (step, (index, prev)) in played.into_iter().enumerate().rev() {
                button {
                    key: "{index}:{prev.server_id}:{prev.id}",
                    r#type: "button",
                    class: "rs-player-flow__side rs-player-flow__side--prev",
                    class: if step == 0 { "rs-player-flow__side--near" },
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
                        size_px: side_px,
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
                    class: if step == 0 { "rs-player-flow__side--near" },
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
                        size_px: side_px,
                        fill: true,
                        hero: true,
                    }
                }
            }
            div {
                class: "rs-player-flow__cover",
                class: if source.is_some() { "rs-player-flow__cover--link" },
                role: if source.is_some() { "button" },
                tabindex: if source.is_some() { "0" },
                aria_label: source.as_ref().map(|p| format!("{} for {title}", p.verb)),
                "data-testid": "now-playing-cover",
                // Only a tap opens the page: gestures.js swallows a finished swipe's
                // click, and a press that moved (a vertical drag with a mouse) is no tap.
                onpointerdown: move |evt: PointerEvent| {
                    let p = evt.client_coordinates();
                    press_at.set(Some((p.x, p.y)));
                },
                onclick: {
                    let source = source.clone();
                    move |evt: MouseEvent| {
                        let p = evt.client_coordinates();
                        let tapped = press_at
                            .take()
                            .map(|(x, y)| (p.x - x).hypot(p.y - y) <= TAP_SLOP_PX)
                            .unwrap_or(true);
                        if tapped {
                            open_source(source.clone());
                        }
                    }
                },
                onkeydown: {
                    let source = source.clone();
                    move |evt: KeyboardEvent| {
                        if matches!(evt.key(), Key::Enter) || evt.key() == Key::Character(" ".into()) {
                            evt.prevent_default();
                            open_source(source.clone());
                        }
                    }
                },
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
