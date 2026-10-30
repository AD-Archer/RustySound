//! The queue page (`/queue`): the whole queue, windowed (queues can run past
//! 1,200 songs). Add Songs is the one visible action; the "…" menu holds
//! Edit, Quick Create, Save as Playlist, Restore Previous Queue and Clear.

mod add_songs;

use crate::api::*;
use crate::components::song_details::actions::{use_player, PlayerCtx};
use crate::components::song_details::up_next::{
    save_queue_as_playlist, spawn_quick_create, QueueEditRow,
};
use crate::components::ui::{
    use_layout, Button, ButtonSize, EmptyState, IconButton, Menu, MenuGap, MenuItem,
    SectionHeader, TrackRow, VirtualList,
};
use crate::components::{AddMenuController, AppView, Navigation, PreviewPlaybackSignal, SeekRequestSignal};
use crate::db::{load_temporary_queue_snapshots, TemporaryQueueSnapshot};
use crate::diagnostics::{log_perf, PerfTimer};
use add_songs::AddSongsSheet;
use dioxus::prelude::*;

/// TrackRow with art and QueueEditRow are both exactly this tall.
const ROW_HEIGHT: f64 = 58.0;
/// Songs at the head of the queue whose LRCLIB lyrics are warmed up.
const LYRICS_PREFETCH: usize = 6;

fn restore_snapshot(
    mut player: PlayerCtx,
    mut seek_request: Signal<Option<(String, f64)>>,
    mut preview: Signal<bool>,
    snapshot: TemporaryQueueSnapshot,
) {
    if snapshot.queue.is_empty() {
        return;
    }
    let index = snapshot.queue_index.min(snapshot.queue.len() - 1);
    let song = snapshot
        .now_playing
        .or_else(|| snapshot.queue.get(index).cloned());
    let position = snapshot.playback_position.max(0.0);
    player.queue.set(snapshot.queue);
    player.queue_index.set(index);
    player.now_playing.set(song.clone());
    player.position.set(position);
    if let Some(song) = song {
        seek_request.set(Some((song.id.clone(), position)));
    }
    preview.set(false);
    player.is_playing.set(false);
}

async fn prefetch_lyrics(songs: Vec<Song>) {
    let started = PerfTimer::now();
    let providers = vec!["lrclib".to_string()];
    let mut count = 0usize;
    for song in songs.into_iter().filter(|s| !s.title.trim().is_empty()) {
        let _ = fetch_lyrics_with_fallback(&LyricsQuery::from_song(&song), &providers, 4).await;
        count += 1;
    }
    log_perf("queue.lyrics_prefetch", started, &format!("prefetched={count}"));
}

fn total_duration(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    if h > 0 {
        format!("{h} hr {m} min")
    } else {
        format!("{m} min")
    }
}

#[component]
pub fn QueueView() -> Element {
    let mut player = use_player();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let seek_request = use_context::<SeekRequestSignal>().0;
    let preview = use_context::<PreviewPlaybackSignal>().0;
    let layout = use_layout();
    let mut editing = use_signal(|| false);
    let mut adding = use_signal(|| false);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut confirm_clear = use_signal(|| false);
    let busy = use_signal(|| false);
    let mut snapshots = use_signal(|| None::<Vec<TemporaryQueueSnapshot>>);
    let mut prefetch_signature = use_signal(String::new);

    use_hook(move || {
        spawn(async move {
            snapshots.set(Some(load_temporary_queue_snapshots().await.unwrap_or_default()));
        });
    });

    // Warm LRCLIB for the first few songs whenever the head of the queue changes.
    use_effect(move || {
        let head: Vec<Song> = player.queue.read().iter().take(LYRICS_PREFETCH).cloned().collect();
        let signature = head
            .iter()
            .map(|s| format!("{}::{}", s.server_id, s.id))
            .collect::<Vec<_>>()
            .join("|");
        if signature.is_empty() || *prefetch_signature.peek() == signature {
            return;
        }
        prefetch_signature.set(signature);
        spawn(prefetch_lyrics(head));
    });

    let (count, seconds) = {
        let queue = player.queue.read();
        (queue.len(), queue.iter().map(|s| s.duration as u64).sum::<u64>())
    };
    let current = (player.queue_index)();
    let editing_now = editing() && count > 0;
    let latest_snapshot = snapshots().and_then(|list| list.into_iter().next());
    let seed = player
        .now_playing
        .read()
        .clone()
        .or_else(|| player.queue.read().get(current).cloned())
        .or_else(|| player.queue.read().last().cloned());

    let render_row = Callback::new(move |index: usize| {
        let song = player.queue.read().get(index).cloned();
        let Some(song) = song else {
            return rsx! {};
        };
        let current = (player.queue_index)();
        if editing() {
            let len = player.queue.read().len();
            rsx! {
                QueueEditRow {
                    song,
                    index,
                    can_up: index > 0,
                    can_down: index + 1 < len,
                    playing: index == current,
                }
            }
        } else {
            let playing = index == current;
            rsx! {
                TrackRow {
                    song,
                    index,
                    playing,
                    live: playing && (player.is_playing)(),
                    on_play: move |_| player.play_index(index),
                }
            }
        }
    });

    let can_quick_create = !busy() && seed.is_some();
    let quick_create = {
        let seed = seed.clone();
        move |_| {
            menu_at.set(None);
            if let Some(seed) = seed.clone() {
                spawn_quick_create(player, seed, busy);
            }
        }
    };

    rsx! {
        div { class: "rs-view rs-view--narrow rs-player-queue",
            div { class: "rs-page-head",
                div { class: "rs-player-queue__title",
                    if layout.is_compact() {
                        IconButton {
                            icon: "chevron-left".to_string(),
                            label: "Back".to_string(),
                            quiet: true,
                            onclick: move |_| {
                                if navigation.can_go_back() {
                                    navigation.go_back();
                                } else {
                                    navigation.navigate_to(AppView::HomeView {});
                                }
                            },
                        }
                    }
                    div {
                        h1 { class: "rs-large-title", "Queue" }
                        p { class: "rs-player-queue__sub",
                            if count == 1 { "1 song" } else { "{count} songs" }
                            if seconds > 0 {
                                " · {total_duration(seconds)}"
                            }
                            if busy() {
                                " · adding similar songs…"
                            }
                        }
                    }
                }
                div { class: "rs-player-queue__tools",
                    // One visible action: Add Songs, or Done while reordering.
                    if editing_now {
                        Button {
                            label: "Done".to_string(),
                            icon: "check".to_string(),
                            size: ButtonSize::Small,
                            testid: "queue-edit".to_string(),
                            onclick: move |_| editing.set(false),
                        }
                    } else {
                        Button {
                            label: "Add Songs".to_string(),
                            icon: "plus".to_string(),
                            size: ButtonSize::Small,
                            testid: "queue-add".to_string(),
                            onclick: move |_| adding.set(true),
                        }
                    }
                    IconButton {
                        icon: "more-horizontal".to_string(),
                        label: "Queue options".to_string(),
                        testid: "queue-more".to_string(),
                        onclick: move |evt: MouseEvent| {
                            let p = evt.client_coordinates();
                            menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                        },
                    }
                }
            }
            if count == 0 {
                EmptyState {
                    icon: "queue".to_string(),
                    title: "Your queue is empty".to_string(),
                    message: if latest_snapshot.is_some() { "Add songs, or bring back the queue you had before with Restore Previous Queue in the “…” menu.".to_string() } else { "Add songs to start one.".to_string() },
                    testid: "queue-list".to_string(),
                }
            } else {
                SectionHeader { title: if editing_now { "Reorder".to_string() } else { "Playing Order".to_string() } }
                VirtualList {
                    item_count: count,
                    row_height: ROW_HEIGHT,
                    render_row,
                    class: "rs-list".to_string(),
                    testid: "queue-list".to_string(),
                }
            }
        }
        if adding() {
            AddSongsSheet { on_close: move |_| adding.set(false) }
        }
        if let Some(anchor) = menu_at() {
            Menu {
                label: "Queue options".to_string(),
                anchor,
                testid: "queue-menu".to_string(),
                on_close: move |_| menu_at.set(None),
                if count > 0 {
                    MenuItem {
                        label: if editing_now { "Done Editing".to_string() } else { "Edit".to_string() },
                        icon: "edit".to_string(),
                        testid: "queue-menu-edit".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            editing.toggle();
                        },
                    }
                }
                MenuItem {
                    label: "Quick Create".to_string(),
                    icon: "plus".to_string(),
                    detail: if busy() { Some("Adding…".to_string()) } else { None },
                    disabled: !can_quick_create,
                    testid: "queue-quick-create".to_string(),
                    onclick: quick_create,
                }
                if count > 0 {
                    MenuItem {
                        label: "Save as Playlist…".to_string(),
                        icon: "playlist".to_string(),
                        testid: "queue-save".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            save_queue_as_playlist(player, add_menu.clone());
                        },
                    }
                }
                if let Some(snapshot) = latest_snapshot.clone() {
                    MenuItem {
                        label: "Restore Previous Queue".to_string(),
                        icon: "clock".to_string(),
                        testid: "queue-restore".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            editing.set(false);
                            restore_snapshot(player, seek_request, preview, snapshot.clone());
                        },
                    }
                }
                if count > 0 {
                    MenuGap {}
                    MenuItem {
                        label: "Clear Queue…".to_string(),
                        icon: "trash".to_string(),
                        danger: true,
                        testid: "queue-menu-clear".to_string(),
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
                message: "Clear the queue? Everything except the song that's playing is removed.".to_string(),
                label: "Clear the queue".to_string(),
                testid: "queue-clear-confirm".to_string(),
                on_close: move |_| confirm_clear.set(false),
                MenuItem {
                    label: "Clear Queue".to_string(),
                    danger: true,
                    testid: "queue-clear".to_string(),
                    onclick: move |_| {
                        confirm_clear.set(false);
                        editing.set(false);
                        player.clear_queue();
                    },
                }
            }
        }
    }
}
