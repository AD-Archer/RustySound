use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::Icon;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta, normalize_manual_queue_songs,
};
use crate::components::views::home::SongRow;
use dioxus::prelude::*;
use std::collections::HashSet;

const RANDOM_BATCH_PER_SERVER: u32 = 25;
const RANDOM_VISIBLE_BASE: usize = 30;
const RANDOM_VISIBLE_STEP: usize = 25;
const RANDOM_CACHE_TTL_HOURS: u32 = 2;
const RANDOM_MAX_POOL: usize = 500;

fn random_song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn dedupe_random_songs(songs: Vec<Song>, limit: usize) -> Vec<Song> {
    let mut seen = HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in songs {
        let key = random_song_key(&song);
        if seen.insert(key) {
            output.push(song);
        }
        if output.len() >= limit {
            break;
        }
    }
    output
}

fn extend_unique_songs(existing: &mut Vec<Song>, incoming: Vec<Song>, max_len: usize) {
    let mut seen: HashSet<String> = existing.iter().map(random_song_key).collect();
    for song in incoming {
        let key = random_song_key(&song);
        if seen.insert(key) {
            existing.push(song);
        }
        if existing.len() >= max_len {
            break;
        }
    }
}

fn random_cache_key(active_servers: &[ServerConfig]) -> String {
    let mut ids: Vec<String> = active_servers
        .iter()
        .map(|server| server.id.clone())
        .collect();
    ids.sort();
    format!("view:random:v1:{}:songs", ids.join("|"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn random_yield() {
    tokio::task::yield_now().await;
}

#[cfg(target_arch = "wasm32")]
async fn random_yield() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

async fn fetch_random_songs_for_servers(
    active_servers: &[ServerConfig],
    per_server_limit: u32,
) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_random_songs(per_server_limit)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            random_yield().await;
            fetched = client
                .get_random_songs(per_server_limit)
                .await
                .unwrap_or_default();
        }
        songs.append(&mut fetched);
        random_yield().await;
    }

    shuffle_songs(&mut songs);
    let cap = (per_server_limit as usize).saturating_mul(active_servers.len().max(1));
    dedupe_random_songs(songs, cap.max(RANDOM_VISIBLE_BASE))
}

#[component]
pub fn RandomView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;

    let refresh_counter = use_signal(|| 0u64);
    let shuffled_songs = use_signal(Vec::<Song>::new);
    let visible_count = use_signal(|| RANDOM_VISIBLE_BASE);
    let loading = use_signal(|| false);

    {
        let servers = servers.clone();
        let mut shuffled_songs = shuffled_songs.clone();
        let mut visible_count = visible_count.clone();
        let mut loading = loading.clone();
        let refresh_counter = refresh_counter.clone();
        use_effect(move || {
            let active_servers: Vec<ServerConfig> = servers()
                .into_iter()
                .filter(|server| server.active)
                .collect();
            let refresh_nonce = refresh_counter();
            let cache_key = random_cache_key(&active_servers);

            if active_servers.is_empty() {
                shuffled_songs.set(Vec::new());
                visible_count.set(RANDOM_VISIBLE_BASE);
                loading.set(false);
                return;
            }

            if refresh_nonce == 0 {
                if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
                    shuffled_songs.set(cached);
                }
            }

            let mut shuffled_songs = shuffled_songs.clone();
            let mut visible_count = visible_count.clone();
            let mut loading = loading.clone();
            loading.set(true);
            spawn(async move {
                let fetched =
                    fetch_random_songs_for_servers(&active_servers, RANDOM_BATCH_PER_SERVER).await;
                if !fetched.is_empty() {
                    let new_visible = RANDOM_VISIBLE_BASE.min(fetched.len());
                    shuffled_songs.set(fetched.clone());
                    visible_count.set(new_visible);
                    let _ = cache_put_json(cache_key, &fetched, Some(RANDOM_CACHE_TTL_HOURS));
                }
                loading.set(false);
            });
        });
    }

    let on_play_all = {
        let shuffled_songs = shuffled_songs.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        move |_| {
            let songs = shuffled_songs();
            if !songs.is_empty() {
                let songs = assign_collection_queue_meta(
                    songs,
                    QueueSourceKind::RandomMix,
                    "random_mix::play_all".to_string(),
                );
                queue.set(songs.clone());
                queue_index.set(0);
                now_playing.set(Some(songs[0].clone()));
                is_playing.set(true);
                if shuffle_enabled() {
                    let _ = apply_collection_shuffle_mode(
                        queue.clone(),
                        queue_index.clone(),
                        now_playing.clone(),
                        true,
                    );
                }
            }
        }
    };

    let on_refresh = {
        let mut refresh_counter = refresh_counter.clone();
        move |_: MouseEvent| {
            refresh_counter.with_mut(|value| *value = value.saturating_add(1));
        }
    };

    let on_load_more = {
        let servers = servers.clone();
        let shuffled_songs = shuffled_songs.clone();
        let mut visible_count = visible_count.clone();
        let mut loading = loading.clone();
        move |_: MouseEvent| {
            let current_len = shuffled_songs().len();
            let current_visible = visible_count().min(current_len);
            if current_visible < current_len {
                visible_count.set((current_visible + RANDOM_VISIBLE_STEP).min(current_len));
                return;
            }

            if loading() {
                return;
            }

            let active_servers: Vec<ServerConfig> = servers()
                .into_iter()
                .filter(|server| server.active)
                .collect();
            if active_servers.is_empty() {
                return;
            }

            let starting_visible = visible_count();
            loading.set(true);
            let mut shuffled_songs = shuffled_songs.clone();
            let mut visible_count = visible_count.clone();
            let mut loading = loading.clone();
            spawn(async move {
                let fetched =
                    fetch_random_songs_for_servers(&active_servers, RANDOM_BATCH_PER_SERVER).await;
                if !fetched.is_empty() {
                    shuffled_songs.with_mut(|existing| {
                        extend_unique_songs(existing, fetched, RANDOM_MAX_POOL);
                    });
                    let merged = shuffled_songs();
                    let new_len = merged.len();
                    visible_count.set((starting_visible + RANDOM_VISIBLE_STEP).min(new_len));
                    let _ = cache_put_json(
                        random_cache_key(&active_servers),
                        &merged,
                        Some(RANDOM_CACHE_TTL_HOURS),
                    );
                }
                loading.set(false);
            });
        }
    };

    let songs = shuffled_songs();
    let visible = visible_count().min(songs.len());
    let display: Vec<Song> = songs.iter().take(visible).cloned().collect();
    let has_active_servers = servers().iter().any(|server| server.active);

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Random Mix" }
                    p { class: "page-subtitle", "A random selection from your library" }
                }
                div { class: "flex flex-wrap gap-6",
                    button {
                        class: "px-6 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                        onclick: on_play_all,
                        Icon {
                            name: "play".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Play All"
                    }
                    button {
                        class: "px-6 py-2 rounded-xl bg-zinc-800 text-zinc-200 border border-zinc-700/60 hover:border-zinc-500 transition-colors flex items-center gap-2",
                        onclick: on_refresh,
                        Icon {
                            name: "refresh-cw".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Refresh"
                    }
                }
            }

            if !display.is_empty() {
                div { class: "space-y-1",
                    for (index , song) in display.iter().enumerate() {
                        SongRow {
                            song: song.clone(),
                            index: index + 1,
                            show_download: true,
                            onclick: {
                                let song = song.clone();
                                let servers = servers.clone();
                                let shuffle_enabled = shuffle_enabled.clone();
                                move |_| {
                                    let single_queue =
                                        normalize_manual_queue_songs(vec![song.clone()]);
                                    queue.set(single_queue.clone());
                                    queue_index.set(0);
                                    now_playing.set(single_queue.first().cloned());
                                    is_playing.set(true);
                                    if shuffle_enabled() {
                                        let seed_song = song.clone();
                                        let servers_snapshot = servers();
                                        let mut queue = queue.clone();
                                        spawn(async move {
                                            let mut similar = if let Some(server) = servers_snapshot
                                                .iter()
                                                .find(|entry| entry.id == seed_song.server_id)
                                                .cloned()
                                            {
                                                let client = NavidromeClient::new(server);
                                                let mut similar =
                                                    client.get_similar_songs(&seed_song.id, 40)
                                                        .await
                                                        .unwrap_or_default();
                                                if similar.is_empty() {
                                                    similar = client
                                                        .get_similar_songs2(&seed_song.id, 40)
                                                        .await
                                                        .unwrap_or_default();
                                                }
                                                if similar.is_empty() {
                                                    similar =
                                                        client.get_random_songs(40).await.unwrap_or_default();
                                                }
                                                similar
                                            } else {
                                                Vec::new()
                                            };

                                            if similar.is_empty() {
                                                return;
                                            }

                                            let seed_key =
                                                format!("{}::{}", seed_song.server_id, seed_song.id);
                                            let mut seen = HashSet::<String>::new();
                                            seen.insert(seed_key);
                                            similar.retain(|candidate| {
                                                seen.insert(format!(
                                                    "{}::{}",
                                                    candidate.server_id, candidate.id
                                                ))
                                            });
                                            let similar =
                                                normalize_manual_queue_songs(similar.into_iter().take(40).collect());

                                            queue.with_mut(|items| {
                                                let queue_is_seed_single = items.len() == 1
                                                    && items
                                                        .first()
                                                        .map(|entry| {
                                                            entry.id == seed_song.id
                                                                && entry.server_id
                                                                    == seed_song.server_id
                                                        })
                                                        .unwrap_or(false);
                                                if !queue_is_seed_single {
                                                    return;
                                                }
                                                items.extend(similar);
                                            });
                                        });
                                    }
                                }
                            },
                        }
                    }
                }
                if has_active_servers {
                    div { class: "flex justify-center pt-3",
                        button {
                            class: "px-4 py-2 rounded-xl bg-zinc-800/70 border border-zinc-700 text-zinc-200 hover:text-white hover:border-emerald-500/60 transition-colors text-sm font-medium",
                            disabled: loading(),
                            onclick: on_load_more,
                            if loading() {
                                "Loading..."
                            } else if visible < songs.len() {
                                "Load more"
                            } else {
                                "Load more random"
                            }
                        }
                    }
                }
            } else if loading() {
                div { class: "flex items-center justify-center py-20",
                    Icon {
                        name: "loader".to_string(),
                        class: "w-8 h-8 text-zinc-500".to_string(),
                    }
                }
            } else if !has_active_servers {
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon {
                        name: "shuffle".to_string(),
                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                    }
                    h2 { class: "text-xl font-semibold text-white mb-2", "No songs available" }
                    p { class: "text-zinc-400", "Connect a server with music to get random picks" }
                }
            } else {
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon {
                        name: "shuffle".to_string(),
                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                    }
                    h2 { class: "text-xl font-semibold text-white mb-2", "No songs available" }
                    p { class: "text-zinc-400", "Try refreshing to load a new random mix" }
                }
            }
        }
    }
}

// Fisher-Yates shuffle using getrandom (wasm-compatible)
fn shuffle_songs(songs: &mut Vec<Song>) {
    let len = songs.len();
    if len <= 1 {
        return;
    }

    for i in (1..len).rev() {
        let mut bytes = [0u8; 4];
        let _ = getrandom::getrandom(&mut bytes);
        let j = u32::from_le_bytes(bytes) as usize % (i + 1);
        songs.swap(i, j);
    }
}
