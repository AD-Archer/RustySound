//! Up Next on the stage, plus the queue-row pieces the queue page reuses.

use super::actions::{is_live_song, same_song, use_player, PlayerCtx};
use super::types::SongDetailsController;
use crate::api::Song;
use crate::components::audio_manager::generate_queue_extension_from_seed;
use crate::components::ui::{
    Artwork, EmptyState, IconButton, IconButtonSize, InsetList, PillButton, PillVariant, TrackRow,
};
use crate::components::{AddIntent, AddMenuController, AppView, Navigation};
use dioxus::prelude::*;

/// Rows shown in the panel; the queue page shows everything.
const PANEL_LIMIT: usize = 100;
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

/// A queue row in edit mode: move up, move down, remove. Same height as a
/// `TrackRow` with art, so it can share a `VirtualList` row height.
#[component]
pub(crate) fn QueueEditRow(
    song: Song,
    index: usize,
    can_up: bool,
    can_down: bool,
    #[props(default)] playing: bool,
) -> Element {
    let mut player = use_player();
    let subtitle = song.artist.clone().unwrap_or_default();
    rsx! {
        li { "data-index": "{index}",
            div {
                class: "rs-trow rs-trow--art rs-player-qrow",
                class: if playing { "rs-trow--playing" },
                "data-testid": "queue-edit-row",
                Artwork { cover_art: song.cover_art.clone(), server_id: song.server_id.clone(), size_px: 44 }
                div { class: "rs-trow__meta",
                    div { class: "rs-trow__title", "{song.title}" }
                    if !subtitle.is_empty() {
                        div { class: "rs-trow__sub", "{subtitle}" }
                    }
                }
                IconButton {
                    icon: "chevron-up".to_string(),
                    label: "Move {song.title} up".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    disabled: !can_up,
                    onclick: move |_| player.move_entry(index, index - 1),
                }
                IconButton {
                    icon: "chevron-down".to_string(),
                    label: "Move {song.title} down".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    disabled: !can_down,
                    onclick: move |_| player.move_entry(index, index + 1),
                }
                IconButton {
                    icon: "x".to_string(),
                    label: "Remove {song.title} from queue".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    onclick: move |_| player.remove_entry(index),
                }
            }
        }
    }
}

#[component]
pub(super) fn UpNextPanel(song: Song) -> Element {
    let mut player = use_player();
    let add_menu = use_context::<AddMenuController>();
    let navigation = use_context::<Navigation>();
    let mut controller = use_context::<SongDetailsController>();
    let mut editing = use_signal(|| false);
    let busy = use_signal(|| false);

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
    let (upcoming, total) = {
        let queue = player.queue.read();
        let upcoming: Vec<(usize, Song)> = queue
            .iter()
            .enumerate()
            .skip(current + 1)
            .take(PANEL_LIMIT)
            .map(|(i, s)| (i, s.clone()))
            .collect();
        (upcoming, queue.len())
    };
    let remaining = total.saturating_sub(current + 1);
    let editing_now = editing() && !upcoming.is_empty();

    let quick_create = {
        let seed = song.clone();
        move |_| spawn_quick_create(player, seed.clone(), busy)
    };

    rsx! {
        section { class: "rs-player-upnext", "data-testid": "queue-list", aria_label: "Up Next",
            div { class: "rs-player-upnext__head",
                p { class: "rs-player-upnext__count",
                    if remaining == 1 { "1 song" } else { "{remaining} songs" }
                }
                if !upcoming.is_empty() {
                    PillButton {
                        label: if editing_now { "Done".to_string() } else { "Edit".to_string() },
                        variant: PillVariant::Mini,
                        testid: "up-next-edit".to_string(),
                        onclick: move |_| editing.toggle(),
                    }
                    PillButton {
                        label: "Save as Playlist".to_string(),
                        variant: PillVariant::Mini,
                        onclick: move |_| save_queue_as_playlist(player, add_menu.clone()),
                    }
                    PillButton {
                        label: "Clear".to_string(),
                        variant: PillVariant::Mini,
                        testid: "up-next-clear".to_string(),
                        onclick: move |_| {
                            editing.set(false);
                            player.clear_queue();
                        },
                    }
                }
            }
            if upcoming.is_empty() {
                EmptyState {
                    icon: "queue".to_string(),
                    title: "Nothing up next".to_string(),
                    message: "Build a queue of similar songs, with this one as the seed.".to_string(),
                    PillButton {
                        label: if busy() { "Creating Queue…".to_string() } else { "Create Queue From This Song".to_string() },
                        disabled: busy(),
                        onclick: quick_create.clone(),
                    }
                }
            } else {
                InsetList { label: "Up Next".to_string(),
                    for (index, entry) in upcoming.iter().cloned() {
                        if editing_now {
                            QueueEditRow {
                                key: "{index}:{entry.server_id}:{entry.id}",
                                song: entry.clone(),
                                index,
                                can_up: index > current + 1,
                                can_down: index + 1 < total,
                            }
                        } else {
                            TrackRow {
                                key: "{index}:{entry.server_id}:{entry.id}",
                                song: entry.clone(),
                                index,
                                on_play: move |_| player.play_index(index),
                            }
                        }
                    }
                }
                div { class: "rs-player-upnext__foot",
                    if remaining > upcoming.len() {
                        span { "Showing the next {upcoming.len()} of {remaining}." }
                    }
                    PillButton {
                        label: if busy() { "Adding…".to_string() } else { "Quick Create".to_string() },
                        icon: "plus".to_string(),
                        variant: PillVariant::Mini,
                        disabled: busy(),
                        onclick: quick_create,
                    }
                    PillButton {
                        label: "Open Full Queue".to_string(),
                        variant: PillVariant::Mini,
                        onclick: move |_| {
                            controller.close();
                            navigation.navigate_to(AppView::QueueView {});
                        },
                    }
                }
            }
        }
    }
}
