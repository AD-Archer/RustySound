//! The queue page (`/queue`): the whole queue, windowed (queues can run past
//! 1,200 songs). Add Songs is the one visible action; the "…" menu holds
//! Quick Create, Save as Playlist, Restore Previous Queue and Clear. Rows
//! reorder by their handle and swipe for Remove / Add to… at any time
//! (`song_details::queue_rows`), so there is no Edit mode.

use crate::api::*;
use crate::components::song_details::queue_rows::{
    use_queue_gestures, QueueUndoToast, REMOVE_FROM_QUEUE,
};
use crate::components::song_details::up_next::{save_queue_as_playlist, spawn_quick_create};
use crate::components::ui::{
    use_layout, Button, ButtonSize, EmptyState, IconButton, Menu, MenuGap, MenuItem,
    RowGestureList, RowSwipe, SectionHeader, TrackRow, VirtualList,
};
use crate::components::views::library::add_songs::{AddSongsSheet, AddTarget};
use crate::components::{
    use_player, AddMenuController, AppView, Navigation, PageMotion, PlayerCtx,
    PreviewPlaybackSignal, SeekRequestSignal,
};
use crate::db::{load_temporary_queue_snapshots, TemporaryQueueSnapshot};
use crate::diagnostics::{log_perf, PerfTimer};
use crate::format::total;
use dioxus::prelude::*;

/// A TrackRow with art is exactly this tall.
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

/// The song's own server first (no third-party call when it has the lyrics),
/// then LRCLIB.
async fn prefetch_lyrics(songs: Vec<Song>, servers: Vec<ServerConfig>) {
    let started = PerfTimer::now();
    let providers = vec!["server".to_string(), "lrclib".to_string()];
    let mut count = 0usize;
    for song in songs.into_iter().filter(|s| !s.title.trim().is_empty()) {
        let query = LyricsQuery::from_song_on(&song, &servers);
        let _ = fetch_lyrics_with_fallback(&query, &providers, 4).await;
        count += 1;
    }
    log_perf("queue.lyrics_prefetch", started, &format!("prefetched={count}"));
}

#[component]
pub fn QueueView() -> Element {
    let mut player = use_player();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let seek_request = use_context::<SeekRequestSignal>().0;
    let preview = use_context::<PreviewPlaybackSignal>().0;
    let layout = use_layout();
    let undo = use_queue_gestures("queue");
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

    // Warm lyrics for the first few songs whenever the head of the queue changes.
    use_effect(move || {
        let head: Vec<Song> = player.queue.read().iter().take(LYRICS_PREFETCH).cloned().collect();
        let signature = head
            .iter()
            .map(Song::key)
            .collect::<Vec<_>>()
            .join("|");
        if signature.is_empty() || *prefetch_signature.peek() == signature {
            return;
        }
        prefetch_signature.set(signature);
        spawn(prefetch_lyrics(head, player.servers.peek().clone()));
    });

    // Open on the playing song: once, when the page mounts, and only when it
    // isn't already in view near the top. Later song changes don't move the page.
    use_hook(move || {
        let current = *player.queue_index.peek();
        if current > 3 {
            let _ = document::eval(&format!(
                "requestAnimationFrame(() => requestAnimationFrame(() => {{\
                   const main = document.querySelector('.rs-main');\
                   const list = document.querySelector('.rs-player-queue [data-testid=\"queue-list\"]');\
                   if (!main || !list) return;\
                   const top = list.getBoundingClientRect().top - main.getBoundingClientRect().top + main.scrollTop;\
                   main.scrollTop = Math.max(0, top + {current} * {ROW_HEIGHT} - main.clientHeight * 0.25);\
                 }}));"
            ));
        }
    });

    let (count, seconds) = {
        let queue = player.queue.read();
        (queue.len(), queue.iter().map(|s| s.duration as u64).sum::<u64>())
    };
    let current = (player.queue_index)();
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
        let playing = index == current;
        rsx! {
            TrackRow {
                song,
                index,
                playing,
                live: playing && (player.is_playing)(),
                handle: true,
                swipe: RowSwipe::Edit,
                remove_label: REMOVE_FROM_QUEUE.to_string(),
                on_play: move |_| player.play_index(index),
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
                                    navigation.navigate_with(AppView::HomeView {}, PageMotion::Back);
                                }
                            },
                        }
                    }
                    div {
                        h1 { class: "rs-large-title", "Queue" }
                        p { class: "rs-player-queue__sub",
                            if count == 1 { "1 song" } else { "{count} songs" }
                            if seconds > 0 {
                                " · {total(seconds)}"
                            }
                            if busy() {
                                " · adding similar songs…"
                            }
                        }
                    }
                }
                div { class: "rs-player-queue__tools",
                    // One visible action; reordering and removing happen on
                    // the rows.
                    Button {
                        label: "Add Songs".to_string(),
                        icon: "plus".to_string(),
                        size: ButtonSize::Small,
                        testid: "queue-add".to_string(),
                        onclick: move |_| adding.set(true),
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
                SectionHeader { title: "Playing Order".to_string() }
                RowGestureList {
                    id: "queue",
                    reorder: (count > 1).then(|| (0, count - 1)),
                    VirtualList {
                        item_count: count,
                        row_height: ROW_HEIGHT,
                        render_row,
                        class: "rs-list".to_string(),
                        testid: "queue-list".to_string(),
                    }
                }
            }
        }
        if adding() {
            AddSongsSheet {
                target: AddTarget::Queue,
                songs: player.queue,
                on_close: move |_| adding.set(false),
            }
        }
        QueueUndoToast { undo }
        if let Some(anchor) = menu_at() {
            Menu {
                label: "Queue options".to_string(),
                anchor,
                testid: "queue-menu".to_string(),
                on_close: move |_| menu_at.set(None),
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
                        player.clear_queue();
                    },
                }
            }
        }
    }
}
