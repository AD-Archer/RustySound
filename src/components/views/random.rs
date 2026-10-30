//! Random: a shuffle of the library. Random albums sit on a small stage band
//! (Cover Flow, tap the centre cover to open it); random songs below play
//! with "Play All", reshuffle with "Shuffle Again", and grow with "Load More".

use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta, normalize_manual_queue_songs,
};
use crate::components::ui::{
    cover_art_url, use_layout, CoverFlow, CoverFlowItem, EmptyState, InsetList, PillButton,
    PillVariant, Skeleton, Stage, TrackRow,
};
use crate::components::{AppView, IsPlayingSignal, Navigation, ShuffleEnabledSignal};
use dioxus::prelude::*;
use std::collections::HashSet;

const RANDOM_BATCH_PER_SERVER: u32 = 25;
const RANDOM_VISIBLE_BASE: usize = 30;
const RANDOM_VISIBLE_STEP: usize = 25;
const RANDOM_CACHE_TTL_HOURS: u32 = 2;
const RANDOM_MAX_POOL: usize = 500;
const RANDOM_ALBUMS: u32 = 24;

fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn extend_unique(existing: &mut Vec<Song>, incoming: Vec<Song>, max_len: usize) {
    let mut seen: HashSet<String> = existing.iter().map(song_key).collect();
    for song in incoming {
        if existing.len() >= max_len {
            break;
        }
        if seen.insert(song_key(&song)) {
            existing.push(song);
        }
    }
}

fn cache_key(servers: &[ServerConfig]) -> String {
    let mut ids: Vec<&str> = servers.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    format!("view:random:v1:{}:songs", ids.join("|"))
}

/// Fisher-Yates with getrandom (wasm-compatible).
fn shuffle<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let mut bytes = [0u8; 4];
        let _ = getrandom::getrandom(&mut bytes);
        let j = u32::from_le_bytes(bytes) as usize % (i + 1);
        items.swap(i, j);
    }
}

async fn fetch_random_songs(servers: &[ServerConfig], per_server: u32) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    for server in servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client.get_random_songs(per_server).await.unwrap_or_default();
        if fetched.is_empty() {
            fetched = client.get_random_songs(per_server).await.unwrap_or_default();
        }
        songs.append(&mut fetched);
    }
    shuffle(&mut songs);
    let mut unique = Vec::new();
    let cap = (per_server as usize * servers.len().max(1)).max(RANDOM_VISIBLE_BASE);
    extend_unique(&mut unique, songs, cap);
    unique
}

async fn fetch_random_albums(servers: &[ServerConfig]) -> Vec<Album> {
    let per_server = (RANDOM_ALBUMS / servers.len().max(1) as u32).max(8);
    let mut albums = Vec::new();
    for server in servers.iter().cloned() {
        // The API layer caches album lists; a random one must be fresh.
        let _ = crate::cache_service::remove_by_prefix(&format!(
            "api:getAlbumList2:v1:{}:random:",
            server.id
        ));
        if let Ok(mut list) = NavidromeClient::new(server)
            .get_albums("random", per_server, 0)
            .await
        {
            albums.append(&mut list);
        }
    }
    shuffle(&mut albums);
    albums.truncate(RANDOM_ALBUMS as usize);
    albums
}

#[component]
pub fn RandomView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<ShuffleEnabledSignal>().0;
    let layout = use_layout();

    let mut generation = use_signal(|| 0u64);
    let mut songs = use_signal(Vec::<Song>::new);
    let mut albums = use_signal(|| None::<Vec<Album>>);
    let mut visible = use_signal(|| RANDOM_VISIBLE_BASE);
    let mut loading = use_signal(|| false);

    // Load on mount and on "Shuffle Again" (generation bump).
    use_effect(move || {
        let gen = generation();
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        if active.is_empty() {
            songs.set(Vec::new());
            albums.set(Some(Vec::new()));
            return;
        }
        let key = cache_key(&active);
        if gen == 0 {
            if let Some(cached) = cache_get_json::<Vec<Song>>(&key) {
                songs.set(cached);
            }
        }
        loading.set(true);
        let album_servers = active.clone();
        spawn(async move {
            let picked = fetch_random_albums(&album_servers).await;
            albums.set(Some(picked));
        });
        spawn(async move {
            let fetched = fetch_random_songs(&active, RANDOM_BATCH_PER_SERVER).await;
            if !fetched.is_empty() {
                visible.set(RANDOM_VISIBLE_BASE.min(fetched.len()));
                let _ = cache_put_json(key, &fetched, Some(RANDOM_CACHE_TTL_HOURS));
                songs.set(fetched);
            }
            loading.set(false);
        });
    });

    let play_all = move |_| {
        let list = songs.peek().clone();
        if list.is_empty() {
            return;
        }
        let list = assign_collection_queue_meta(
            list,
            QueueSourceKind::RandomMix,
            "random_mix::play_all".to_string(),
        );
        now_playing.set(list.first().cloned());
        queue.set(list);
        queue_index.set(0);
        is_playing.set(true);
        if *shuffle_enabled.peek() {
            let _ = apply_collection_shuffle_mode(queue, queue_index, now_playing, true);
        }
    };

    let load_more = move |_| {
        let len = songs.peek().len();
        let shown = (*visible.peek()).min(len);
        if shown < len {
            visible.set((shown + RANDOM_VISIBLE_STEP).min(len));
            return;
        }
        if *loading.peek() {
            return;
        }
        let active: Vec<ServerConfig> = servers.peek().iter().filter(|s| s.active).cloned().collect();
        if active.is_empty() {
            return;
        }
        loading.set(true);
        spawn(async move {
            let fetched = fetch_random_songs(&active, RANDOM_BATCH_PER_SERVER).await;
            if !fetched.is_empty() {
                songs.with_mut(|existing| extend_unique(existing, fetched, RANDOM_MAX_POOL));
                let merged = songs.peek().clone();
                visible.set((shown + RANDOM_VISIBLE_STEP).min(merged.len()));
                let _ = cache_put_json(cache_key(&active), &merged, Some(RANDOM_CACHE_TTL_HOURS));
            }
            loading.set(false);
        });
    };

    // A single song plays alone; with shuffle on, similar songs follow it.
    let mut play_one = move |song: Song| {
        let single = normalize_manual_queue_songs(vec![song.clone()]);
        queue.set(single.clone());
        queue_index.set(0);
        now_playing.set(single.first().cloned());
        is_playing.set(true);
        if !*shuffle_enabled.peek() {
            return;
        }
        let server = servers.peek().iter().find(|s| s.id == song.server_id).cloned();
        let Some(server) = server else { return };
        spawn(async move {
            let client = NavidromeClient::new(server);
            let mut similar = client.get_similar_songs(&song.id, 40).await.unwrap_or_default();
            if similar.is_empty() {
                similar = client.get_similar_songs2(&song.id, 40).await.unwrap_or_default();
            }
            if similar.is_empty() {
                similar = client.get_random_songs(40).await.unwrap_or_default();
            }
            let mut seen = HashSet::from([song_key(&song)]);
            similar.retain(|candidate| seen.insert(song_key(candidate)));
            let similar = normalize_manual_queue_songs(similar.into_iter().take(40).collect());
            queue.with_mut(|items| {
                let still_seed = items.len() == 1
                    && items
                        .first()
                        .map(|e| e.id == song.id && e.server_id == song.server_id)
                        .unwrap_or(false);
                if still_seed {
                    items.extend(similar);
                }
            });
        });
    };

    let has_servers = servers.read().iter().any(|s| s.active);
    let list = songs.read().clone();
    let shown = visible().min(list.len());
    let busy = loading();
    let current = now_playing
        .read()
        .as_ref()
        .map(|s| (s.server_id.clone(), s.id.clone()));
    let live = is_playing();

    let flow_items: Option<Vec<CoverFlowItem>> = albums.read().as_ref().map(|albums| {
        let servers = servers.read();
        albums
            .iter()
            .map(|album| CoverFlowItem {
                key: format!("{}:{}", album.server_id, album.id),
                title: album.name.clone(),
                subtitle: album.artist.clone(),
                reason: match (album.year, album.song_count) {
                    (Some(year), n) if n > 0 => format!("{year} · {n} songs"),
                    (Some(year), _) => year.to_string(),
                    (None, n) if n > 0 => format!("{n} songs"),
                    _ => String::new(),
                },
                art: cover_art_url(&servers, &album.server_id, album.cover_art.as_deref(), 300)
                    .into_iter()
                    .collect(),
            })
            .collect()
    });
    let wide = !layout.is_compact();

    rsx! {
        div { class: "rs-view rs-view--flush", "data-testid": "random",
            Stage { class: "rs-personal-band".to_string(), label: "Random albums".to_string(),
                div { class: "rs-personal-band__head",
                    h1 { class: "rs-large-title", "Random" }
                }
                match flow_items {
                    None => rsx! {
                        div { class: "rs-personal-band__empty", span { class: "rs-spinner" } }
                    },
                    Some(items) if items.is_empty() => rsx! {
                        div { class: "rs-personal-band__empty",
                            if has_servers { "No albums came back from the server." } else { "Connect a server to shuffle your library." }
                        }
                    },
                    Some(items) => rsx! {
                        CoverFlow {
                            key: "{generation}",
                            items,
                            initial: 2,
                            cover_px: if wide { 176 } else { 132 },
                            wide,
                            label: "Random albums".to_string(),
                            on_open: move |index: usize| {
                                let album = albums.peek().as_ref().and_then(|a| a.get(index).cloned());
                                if let Some(album) = album {
                                    navigation.navigate_to(AppView::AlbumDetailView {
                                        album_id: album.id,
                                        server_id: album.server_id,
                                    });
                                }
                            },
                        }
                    },
                }
            }

            div { class: "rs-personal-page",
                if !has_servers {
                    EmptyState {
                        icon: "shuffle".to_string(),
                        title: "No servers connected".to_string(),
                        message: "Add a Navidrome server to get random picks from your library.".to_string(),
                        PillButton {
                            label: "Add Server".to_string(),
                            onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                        }
                    }
                } else {
                    div { class: "rs-personal-actions",
                        PillButton {
                            label: "Play All".to_string(),
                            icon: "play".to_string(),
                            disabled: list.is_empty(),
                            testid: "random-play-all".to_string(),
                            onclick: play_all,
                        }
                        PillButton {
                            label: "Shuffle Again".to_string(),
                            icon: "refresh-cw".to_string(),
                            variant: PillVariant::Quiet,
                            disabled: busy,
                            testid: "random-refresh".to_string(),
                            onclick: move |_| generation.with_mut(|g| *g += 1),
                        }
                    }
                    if list.is_empty() && busy {
                        Skeleton { rows: 8 }
                    } else if list.is_empty() {
                        EmptyState {
                            icon: "shuffle".to_string(),
                            title: "No songs came back".to_string(),
                            message: "Your server returned no random songs. Try shuffling again.".to_string(),
                        }
                    } else {
                        InsetList { label: "Random songs".to_string(), testid: "random-songs".to_string(),
                            for (index, song) in list.iter().take(shown).enumerate() {
                                {
                                    let playing = current
                                        .as_ref()
                                        .map(|(server, id)| *server == song.server_id && *id == song.id)
                                        .unwrap_or(false);
                                    let song_for_play = song.clone();
                                    rsx! {
                                        TrackRow {
                                            key: "{song.server_id}:{song.id}",
                                            song: song.clone(),
                                            index,
                                            playing,
                                            live: playing && live,
                                            on_play: move |_| play_one(song_for_play.clone()),
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "rs-personal-foot",
                            PillButton {
                                label: if busy { "Loading…".to_string() } else { "Load More".to_string() },
                                variant: PillVariant::Quiet,
                                disabled: busy,
                                testid: "random-load-more".to_string(),
                                onclick: load_more,
                            }
                        }
                    }
                }
            }
        }
    }
}
