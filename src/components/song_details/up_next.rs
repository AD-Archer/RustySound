//! Up Next on the stage, plus the queue actions the queue page reuses.
//!
//! Upcoming rows drag by their handle and swipe for Remove / Add to…
//! (`queue_rows.rs`); played rows swipe but don't reorder (they're history).

use super::actions::{is_live_song, same_song, use_player, PlayerCtx};
use super::queue_rows::{use_queue_gestures, QueueUndoToast, REMOVE_FROM_QUEUE};
use super::types::SongDetailsController;
use crate::api::Song;
use crate::components::audio_manager::generate_queue_extension_from_seed;
use crate::components::ui::{
    Button, ButtonVariant, EmptyState, IconButton, IconButtonSize, InsetList, Menu, MenuGap,
    MenuItem, RowGestureList, RowSwipe, TrackRow,
};
use crate::components::{AddIntent, AddMenuController, AppView, Navigation};
use dioxus::prelude::*;

/// Rows shown in the panel; the queue page shows everything.
const PANEL_LIMIT: usize = 100;
/// How many already-played songs the panel shows above the current one.
const HISTORY_LIMIT: usize = 50;
/// Songs "Quick Create" adds.
const QUICK_CREATE_COUNT: usize = 45;

/// Appends ~45 songs similar to `seed` (skipping ones already queued).
pub(crate) fn spawn_quick_create(player: PlayerCtx, seed: Song, mut busy: Signal<bool>) {
    if *busy.peek() {
        return;
    }
    busy.set(true);
    let servers = player.servers.peek().clone();
    let existing = player.queue.peek().clone();
    let mut queue = player.queue;
    spawn(async move {
        let additions =
            generate_queue_extension_from_seed(servers, seed.clone(), existing, QUICK_CREATE_COUNT)
                .await;
        queue.with_mut(|items| {
            if items.is_empty() {
                items.push(seed.clone());
            }
            for candidate in additions {
                if !items.iter().any(|entry| same_song(entry, &candidate)) {
                    items.push(candidate);
                }
            }
        });
        busy.set(false);
    });
}

/// Opens the add-to sheet for the whole queue ("Save as Playlist").
pub(crate) fn save_queue_as_playlist(player: PlayerCtx, mut add_menu: AddMenuController) {
    let songs = player.queue.peek().clone();
    if songs.is_empty() {
        return;
    }
    let label = format!("Current Queue ({} songs)", songs.len());
    add_menu.open(AddIntent::from_songs(label, songs));
}

#[component]
pub(super) fn UpNextPanel(song: Song) -> Element {
    let mut player = use_player();
    let add_menu = use_context::<AddMenuController>();
    let navigation = use_context::<Navigation>();
    let mut controller = use_context::<SongDetailsController>();
    let undo = use_queue_gestures("upnext");
    let busy = use_signal(|| false);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut confirm_clear = use_signal(|| false);

    if is_live_song(&song) {
        return rsx! {
            section { class: "rs-player-upnext", "data-testid": "queue-list",
                EmptyState {
                    icon: "radio".to_string(),
                    title: "Live radio".to_string(),
                    message: "Up Next and queue editing come back when you play a regular track.".to_string(),
                }
            }
        };
    }

    let current = (player.queue_index)();
    let live = (player.is_playing)();
    let (played, now, upcoming, total) = {
        let queue = player.queue.read();
        let played: Vec<(usize, Song)> = queue
            .iter()
            .enumerate()
            .take(current)
            .skip(current.saturating_sub(HISTORY_LIMIT))
            .map(|(i, s)| (i, s.clone()))
            .collect();
        let now = queue.get(current).cloned();
        let upcoming: Vec<(usize, Song)> = queue
            .iter()
            .enumerate()
            .skip(current + 1)
            .take(PANEL_LIMIT)
            .map(|(i, s)| (i, s.clone()))
            .collect();
        (played, now, upcoming, queue.len())
    };
    let remaining = total.saturating_sub(current + 1);
    // The handle moves rows within what's still to come (as many as shown).
    let reorder = (upcoming.len() > 1).then(|| (current + 1, current + upcoming.len()));

    use_effect(move || {
        let _ = (player.queue_index)();
        let _ = document::eval(SCROLL_TO_NOW_JS);
    });

    let quick_create = {
        let seed = song.clone();
        move |_| spawn_quick_create(player, seed.clone(), busy)
    };

    let mut open_queue = move || {
        menu_at.set(None);
        controller.close();
        navigation.navigate_to(AppView::QueueView {});
    };

    rsx! {
        section { class: "rs-player-upnext", "data-testid": "queue-list", aria_label: "Up Next",
            div { class: "rs-player-upnext__head",
                p { class: "rs-player-upnext__count",
                    if remaining == 1 { "1 song" } else { "{remaining} songs" }
                    if busy() {
                        " · adding similar songs…"
                    }
                }
                // Reordering and removing are on the rows themselves (handle,
                // swipe, track menu); the rest lives in the "…" menu.
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: "Up Next options".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "up-next-more".to_string(),
                    onclick: move |evt: MouseEvent| {
                        let p = evt.client_coordinates();
                        menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                    },
                }
            }
            if !played.is_empty() {
                p { class: "rs-player-upnext__label", "Played" }
                div { class: "rs-player-upnext__history",
                    RowGestureList { id: "upnext",
                        InsetList { label: "Played".to_string(),
                            for (index, entry) in played.iter().cloned() {
                                TrackRow {
                                    key: "{index}:{entry.server_id}:{entry.id}",
                                    song: entry.clone(),
                                    index,
                                    swipe: RowSwipe::Edit,
                                    remove_label: REMOVE_FROM_QUEUE.to_string(),
                                    on_play: move |_| player.play_index(index),
                                }
                            }
                        }
                    }
                }
            }
            if let Some(entry) = now.clone() {
                p { class: "rs-player-upnext__label", "data-upnext-now": "true", "Now Playing" }
                InsetList { label: "Now Playing".to_string(),
                    TrackRow {
                        key: "{current}:{entry.server_id}:{entry.id}",
                        song: entry.clone(),
                        index: current,
                        playing: true,
                        live,
                        on_play: move |_| player.play_index(current),
                    }
                }
            }
            if !upcoming.is_empty() {
                p { class: "rs-player-upnext__label", "Up Next" }
            }
            if upcoming.is_empty() {
                EmptyState {
                    icon: "queue".to_string(),
                    title: "Nothing up next".to_string(),
                    message: "Build a queue of similar songs, with this one as the seed.".to_string(),
                    Button {
                        label: if busy() { "Creating Queue…".to_string() } else { "Create Queue From This Song".to_string() },
                        disabled: busy(),
                        onclick: quick_create.clone(),
                    }
                }
            } else {
                RowGestureList { id: "upnext", reorder,
                    InsetList { label: "Up Next".to_string(),
                        for (index, entry) in upcoming.iter().cloned() {
                            TrackRow {
                                key: "{index}:{entry.server_id}:{entry.id}",
                                song: entry.clone(),
                                index,
                                handle: reorder.is_some(),
                                swipe: RowSwipe::Edit,
                                remove_label: REMOVE_FROM_QUEUE.to_string(),
                                on_play: move |_| player.play_index(index),
                            }
                        }
                    }
                }
                if remaining > upcoming.len() {
                    div { class: "rs-player-upnext__foot",
                        span { "Showing the next {upcoming.len()} of {remaining}." }
                        Button {
                            label: "See All".to_string(),
                            variant: ButtonVariant::Plain,
                            onclick: move |_| open_queue(),
                        }
                    }
                }
            }
        }
        // Modals opened from the panel: their Escape must not also close Now Playing.
        div { class: "rs-player-modals", onkeydown: move |evt| evt.stop_propagation(),
            QueueUndoToast { undo }
            if let Some(anchor) = menu_at() {
                Menu {
                    label: "Up Next options".to_string(),
                    anchor,
                    testid: "up-next-menu".to_string(),
                    on_close: move |_| menu_at.set(None),
                    MenuItem {
                        label: "Quick Create".to_string(),
                        icon: "plus".to_string(),
                        detail: if busy() { Some("Adding…".to_string()) } else { None },
                        disabled: busy(),
                        testid: "up-next-quick-create".to_string(),
                        onclick: {
                            let quick_create = quick_create.clone();
                            move |evt| {
                                menu_at.set(None);
                                quick_create(evt);
                            }
                        },
                    }
                    if !upcoming.is_empty() {
                        MenuItem {
                            label: "Save as Playlist…".to_string(),
                            icon: "playlist".to_string(),
                            testid: "up-next-save".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                save_queue_as_playlist(player, add_menu.clone());
                            },
                        }
                    }
                    MenuItem {
                        label: "Open Full Queue".to_string(),
                        icon: "queue".to_string(),
                        testid: "up-next-open-queue".to_string(),
                        onclick: move |_| open_queue(),
                    }
                    if !upcoming.is_empty() {
                        MenuGap {}
                        MenuItem {
                            label: "Clear Up Next…".to_string(),
                            icon: "trash".to_string(),
                            danger: true,
                            testid: "up-next-menu-clear".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                confirm_clear.set(true);
                            },
                        }
                    }
                }
            }
            if confirm_clear() {
                Menu {
                    message: "Clear Up Next? The song that's playing keeps playing; everything after it is removed.".to_string(),
                    label: "Clear Up Next".to_string(),
                    testid: "up-next-clear-confirm".to_string(),
                    on_close: move |_| confirm_clear.set(false),
                    MenuItem {
                        label: "Clear Up Next".to_string(),
                        danger: true,
                        testid: "up-next-clear".to_string(),
                        onclick: move |_| {
                            confirm_clear.set(false);
                            player.clear_queue();
                        },
                    }
                }
            }
        }
    }
}

/// Scrolls the Up Next panel so the "Now Playing" label sits at its top. Only
/// the panel's own scroll position changes, never the page around it.
const SCROLL_TO_NOW_JS: &str = r#"
requestAnimationFrame(() => {
  const panel = document.querySelector('.rs-player-upnext[data-testid="queue-list"]');
  const now = panel && panel.querySelector('[data-upnext-now]');
  if (!panel || !now) return;
  panel.scrollTop += now.getBoundingClientRect().top - panel.getBoundingClientRect().top;
});
"#;
