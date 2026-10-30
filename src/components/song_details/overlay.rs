//! Now Playing: a full-screen stage sheet (compact) or a large stage overlay
//! (wide). Only the tree for the current layout is mounted.
//!
//! Perf: nothing in this file reads the playback position or the queue. The
//! root re-renders when the sheet opens/closes, the song changes or the
//! layout flips. Position readers are the kit leaves inside `transport.rs`
//! and the active-line memo in `lyrics/lines.rs`; queue readers are the art
//! strip and Up Next.

use super::actions::is_live_song;
use super::art::NowPlayingArt;
use super::meta::{NowPlayingExtras, NowPlayingMeta};
use super::panels::NowPlayingPanels;
use super::transport::{NowPlayingScrubber, NowPlayingTransport};
use super::types::{SongDetailsController, SongDetailsTab};
use crate::api::Song;
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{use_layout, IconButton};
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

/// Drag distance (px) past which releasing the sheet dismisses it.
const DISMISS_PX: f64 = 110.0;
/// Matches the slide-down animation in player.css.
const CLOSE_ANIMATION_MS: u64 = 240;

#[component]
pub fn SongDetailsOverlay(controller: SongDetailsController) -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let layout = use_layout();
    let closing = use_signal(|| false);

    let state = controller.current();
    if !state.is_open {
        return rsx! {};
    }
    let Some(song) = now_playing().or(state.song.clone()) else {
        return rsx! {};
    };
    let tab = state.tab_chosen.then_some(state.active_tab);

    if layout.is_compact() {
        rsx! {
            CompactNowPlaying { song, tab, controller, closing }
        }
    } else {
        rsx! {
            WideNowPlaying { song, tab, controller, closing }
        }
    }
}

/// "Playing from" line above the cover.
fn context_label(song: &Song) -> String {
    if is_live_song(song) {
        return "Internet Radio".to_string();
    }
    song.album
        .clone()
        .filter(|album| !album.trim().is_empty())
        .unwrap_or_else(|| "Now Playing".to_string())
}

fn dismiss(mut controller: SongDetailsController, mut closing: Signal<bool>) {
    if *closing.peek() {
        return;
    }
    closing.set(true);
    spawn(async move {
        sleep_ms(CLOSE_ANIMATION_MS).await;
        controller.close();
        closing.set(false);
    });
}

/// Close chevron, context label, and a shortcut to the full queue page.
#[component]
fn SheetBar(label: String, controller: SongDetailsController, closing: Signal<bool>) -> Element {
    let navigation = use_context::<Navigation>();
    rsx! {
        div { class: "rs-player-np__bar",
            IconButton {
                icon: "chevron-down".to_string(),
                label: "Close Now Playing".to_string(),
                quiet: true,
                testid: "now-playing-close".to_string(),
                onclick: move |_| dismiss(controller.clone(), closing),
            }
            span { class: "rs-player-np__ctx", "{label}" }
            IconButton {
                icon: "queue".to_string(),
                label: "Open Queue".to_string(),
                quiet: true,
                onclick: {
                    let mut controller = controller.clone();
                    move |_| {
                        controller.close();
                        navigation.navigate_to(AppView::QueueView {});
                    }
                },
            }
        }
    }
}

#[component]
fn CompactNowPlaying(
    song: Song,
    tab: Option<SongDetailsTab>,
    controller: SongDetailsController,
    closing: Signal<bool>,
) -> Element {
    // Swipe-down to dismiss: (start y, start x) while a drag is armed, and
    // the current downward offset. Only written while dragging.
    let mut drag_start = use_signal(|| None::<(f64, f64)>);
    let mut drag_dy = use_signal(|| 0.0f64);
    let dy = drag_dy();
    let style = if dy > 0.0 {
        format!("transform:translateY({dy:.0}px);transition:none")
    } else {
        String::new()
    };
    let label = context_label(&song);

    rsx! {
        div {
            class: "rs-stage rs-player-np rs-player-np--compact",
            class: if closing() { "rs-player-np--closing" },
            style: "{style}",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Now Playing",
            tabindex: "-1",
            "data-testid": "now-playing",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: {
                let controller = controller.clone();
                move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.prevent_default();
                        dismiss(controller.clone(), closing);
                    }
                }
            },
            onpointermove: move |evt| {
                let Some((y0, x0)) = *drag_start.peek() else {
                    return;
                };
                let point = evt.client_coordinates();
                let (dy, dx) = (point.y - y0, (point.x - x0).abs());
                if dy > 6.0 && dy > dx {
                    drag_dy.set(dy);
                } else if *drag_dy.peek() != 0.0 && dy <= 0.0 {
                    drag_dy.set(0.0);
                }
            },
            onpointerup: {
                let controller = controller.clone();
                move |_| {
                    if drag_start.peek().is_none() {
                        return;
                    }
                    drag_start.set(None);
                    let travelled = *drag_dy.peek();
                    drag_dy.set(0.0);
                    if travelled > DISMISS_PX {
                        dismiss(controller.clone(), closing);
                    }
                }
            },
            onpointercancel: move |_| {
                drag_start.set(None);
                drag_dy.set(0.0);
            },

            div {
                class: "rs-player-np__grip",
                onpointerdown: move |evt| {
                    let point = evt.client_coordinates();
                    drag_start.set(Some((point.y, point.x)));
                },
                div { class: "rs-player-np__grab", aria_hidden: "true" }
                SheetBar { label, controller: controller.clone(), closing }
                NowPlayingArt { song: song.clone(), wide: false }
            }
            NowPlayingMeta { song: song.clone() }
            NowPlayingScrubber { song: song.clone() }
            NowPlayingTransport { song: song.clone() }
            NowPlayingPanels { song: song.clone(), tab, wide: false }
        }
    }
}

#[component]
fn WideNowPlaying(
    song: Song,
    tab: Option<SongDetailsTab>,
    controller: SongDetailsController,
    closing: Signal<bool>,
) -> Element {
    let label = context_label(&song);
    rsx! {
        div {
            class: "rs-player-np-scrim",
            class: if closing() { "rs-player-np-scrim--closing" },
            aria_hidden: "true",
            onclick: {
                let controller = controller.clone();
                move |_| dismiss(controller.clone(), closing)
            },
        }
        div {
            class: "rs-stage rs-player-np rs-player-np--wide",
            class: if closing() { "rs-player-np--closing" },
            role: "dialog",
            aria_modal: "true",
            aria_label: "Now Playing",
            tabindex: "-1",
            "data-testid": "now-playing",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: {
                let controller = controller.clone();
                move |evt: KeyboardEvent| {
                    if evt.key() == Key::Escape {
                        evt.prevent_default();
                        dismiss(controller.clone(), closing);
                    }
                }
            },
            SheetBar { label, controller: controller.clone(), closing }
            div { class: "rs-player-np__cols",
                div { class: "rs-player-np__left",
                    NowPlayingArt { song: song.clone(), wide: true }
                    NowPlayingMeta { song: song.clone() }
                    NowPlayingScrubber { song: song.clone() }
                    NowPlayingTransport { song: song.clone() }
                    NowPlayingExtras { song: song.clone() }
                }
                div { class: "rs-player-np__right",
                    NowPlayingPanels { song: song.clone(), tab, wide: true }
                }
            }
        }
    }
}
