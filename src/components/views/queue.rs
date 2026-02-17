use crate::api::models::format_duration;
use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::{
    AddIntent, AddMenuController, AppView, Icon, Navigation, PlaybackPositionSignal,
    PreviewPlaybackSignal, SeekRequestSignal, SongDetailsController,
};
use crate::diagnostics::log_perf;
use dioxus::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

const QUICK_PREVIEW_DURATION_MS: u64 = 12000;
const AUTO_RECOMMENDATION_LIMIT: usize = 25;
const AUTO_RECOMMENDATION_FIRST_SEED_COUNT: usize = 4;
const AUTO_RECOMMENDATION_LAST_SEED_COUNT: usize = 4;
const AUTO_RECOMMENDATION_RECENT_SEED_COUNT: usize = 17;

#[cfg(not(target_arch = "wasm32"))]
async fn quick_preview_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn quick_preview_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn queue_search_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn queue_search_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

async fn prefetch_lrclib_lyrics_for_queue(songs: Vec<Song>, max_songs: usize) {
    if songs.is_empty() || max_songs == 0 {
        return;
    }

    let providers = vec!["lrclib".to_string()];
    let start = Instant::now();
    let mut prefetched = 0usize;

    for song in songs.into_iter().take(max_songs) {
        if song.title.trim().is_empty() {
            continue;
        }
        let query = LyricsQuery::from_song(&song);
        let _ = fetch_lyrics_with_fallback(&query, &providers, 4).await;
        prefetched += 1;
    }

    log_perf(
        "queue.lyrics_prefetch",
        start,
        &format!("prefetched={prefetched}"),
    );
}

#[component]
pub fn QueueView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let add_menu = use_context::<AddMenuController>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let seek_request = use_context::<SeekRequestSignal>().0;
    let preview_playback = use_context::<PreviewPlaybackSignal>().0;
    let add_song_panel_open = use_signal(|| false);
    let mut queue_search = use_signal(String::new);
    let queue_search_debounced = use_signal(String::new);
    let queue_search_generation = use_signal(|| 0u64);
    let recently_added_seed = use_signal(|| None::<Song>);
    let dismissed_recommendations = use_signal(HashSet::<String>::new);
    let recommendation_refresh_nonce = use_signal(|| 0u64);
    let preview_session = use_signal(|| 0u64);
    let preview_song_key = use_signal(|| None::<String>);
    let lyrics_prefetch_signature = use_signal(String::new);

    let current_index = queue_index();
    let songs: Vec<Song> = queue().into_iter().collect();
    let queue_len = songs.len();
    let current_song = now_playing();

    {
        let queue = queue.clone();
        let mut lyrics_prefetch_signature = lyrics_prefetch_signature.clone();
        use_effect(move || {
            let snapshot = queue();
            let signature = snapshot
                .iter()
                .take(6)
                .map(song_identity_key)
                .collect::<Vec<_>>()
                .join("|");

            if signature.is_empty() || signature == lyrics_prefetch_signature() {
                return;
            }
            lyrics_prefetch_signature.set(signature);

            let seeds: Vec<Song> = snapshot.into_iter().take(6).collect();
            spawn(async move {
                prefetch_lrclib_lyrics_for_queue(seeds, 6).await;
            });
        });
    }

    {
        let mut queue_search_debounced = queue_search_debounced.clone();
        let mut queue_search_generation = queue_search_generation.clone();
        use_effect(move || {
            let query = queue_search().trim().to_string();
            queue_search_generation.with_mut(|value| *value = value.saturating_add(1));
            let generation = *queue_search_generation.peek();

            if query.len() < 2 {
                queue_search_debounced.set(String::new());
                return;
            }

            let mut queue_search_debounced = queue_search_debounced.clone();
            let queue_search_generation = queue_search_generation.clone();
            spawn(async move {
                queue_search_delay_ms(220).await;
                if *queue_search_generation.peek() != generation {
                    return;
                }
                queue_search_debounced.set(query);
            });
        });
    }

    let on_preview_song = Rc::new({
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let playback_position = playback_position.clone();
        let seek_request = seek_request.clone();
        let preview_playback = preview_playback.clone();
        let preview_session = preview_session.clone();
        let preview_song_key = preview_song_key.clone();
        move |song: Song| {
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let mut playback_position = playback_position.clone();
            let mut seek_request = seek_request.clone();
            let mut preview_playback = preview_playback.clone();
            let mut preview_session = preview_session.clone();
            let mut preview_song_key = preview_song_key.clone();
            let saved_queue = queue();
            let saved_queue_index = queue_index();
            let saved_now_playing = now_playing();
            let saved_is_playing = is_playing();
            let saved_playback_position = playback_position();
            let saved_seek_request = seek_request();
            let saved_seek = saved_seek_request.or_else(|| {
                saved_now_playing
                    .as_ref()
                    .map(|current| (current.id.clone(), saved_playback_position.max(0.0)))
            });

            preview_session.with_mut(|session| *session = session.saturating_add(1));
            let session = preview_session();
            preview_song_key.set(Some(song_identity_key(&song)));
            preview_playback.set(true);

            queue.set(vec![song.clone()]);
            queue_index.set(0);
            playback_position.set(0.0);
            seek_request.set(Some((song.id.clone(), 0.0)));
            now_playing.set(Some(song));
            is_playing.set(true);

            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let mut playback_position = playback_position.clone();
            let mut seek_request = seek_request.clone();
            let preview_session = preview_session.clone();
            let mut preview_song_key = preview_song_key.clone();
            spawn(async move {
                quick_preview_delay_ms(QUICK_PREVIEW_DURATION_MS).await;
                if preview_session() != session {
                    return;
                }
                queue.set(saved_queue);
                queue_index.set(saved_queue_index);
                now_playing.set(saved_now_playing);
                is_playing.set(saved_is_playing);
                playback_position.set(saved_playback_position.max(0.0));
                seek_request.set(saved_seek);
                preview_song_key.set(None);
                preview_playback.set(false);
            });
        }
    });

    let auto_recommendations = {
        let servers = servers.clone();
        let queue = queue.clone();
        let add_song_panel_open = add_song_panel_open.clone();
        let recently_added_seed = recently_added_seed.clone();
        let dismissed_recommendations = dismissed_recommendations.clone();
        let recommendation_refresh_nonce = recommendation_refresh_nonce.clone();
        use_resource(move || {
            let is_open = add_song_panel_open();
            let servers_snapshot = servers();
            let queue_snapshot = queue();
            let recent_seed = recently_added_seed();
            let dismissed_keys = dismissed_recommendations();
            let _refresh = recommendation_refresh_nonce();
            async move {
                if !is_open {
                    return Vec::new();
                }
                build_queue_add_recommendations(
                    servers_snapshot,
                    queue_snapshot,
                    recent_seed,
                    dismissed_keys,
                )
                .await
            }
        })
    };

    let add_song_results = {
        let servers = servers.clone();
        let add_song_panel_open = add_song_panel_open.clone();
        use_resource(move || {
            let query = queue_search_debounced();
            let is_open = add_song_panel_open();
            let servers_snapshot = servers();
            async move {
                if !is_open {
                    return Vec::new();
                }
                search_queue_add_candidates(servers_snapshot, query).await
            }
        })
    };

    let on_clear = move |_| {
        let current = now_playing();
        if let Some(song) = current {
            queue.set(vec![song]);
            queue_index.set(0);
        } else {
            queue.set(Vec::new());
            queue_index.set(0);
            is_playing.set(false);
        }
    };

    let on_toggle_add_song_panel = {
        let mut add_song_panel_open = add_song_panel_open.clone();
        let mut queue_search = queue_search.clone();
        let mut dismissed_recommendations = dismissed_recommendations.clone();
        move |_| {
            let next_state = !add_song_panel_open();
            add_song_panel_open.set(next_state);
            dismissed_recommendations.set(HashSet::new());
            if !next_state {
                queue_search.set(String::new());
            }
        }
    };

    let on_save_queue_to_playlist = {
        let queue = queue.clone();
        let mut add_menu = add_menu.clone();
        move |_| {
            let queue_snapshot = queue();
            if queue_snapshot.is_empty() {
                return;
            }
            let label = format!("Current Queue ({} songs)", queue_snapshot.len());
            add_menu.open(AddIntent::from_songs(label, queue_snapshot));
        }
    };

    let on_refresh_recommendations = {
        let mut recommendation_refresh_nonce = recommendation_refresh_nonce.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            recommendation_refresh_nonce.set(recommendation_refresh_nonce().saturating_add(1));
        }
    };

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Play Queue" }
                    p { class: "page-subtitle",
                        "{songs.len()} songs • {format_duration(songs.iter().map(|s| s.duration).sum())}"
                    }
                }

                div { class: "flex items-center gap-2",
                    button {
                        class: if add_song_panel_open() {
                            "px-4 py-2 rounded-xl bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 hover:text-white transition-colors flex items-center gap-2"
                        } else {
                            "px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors flex items-center gap-2"
                        },
                        onclick: on_toggle_add_song_panel,
                        Icon {
                            name: "plus".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        if add_song_panel_open() {
                            "Close Add Songs"
                        } else {
                            "Add Songs"
                        }
                    }
                    if !songs.is_empty() {
                        button {
                            class: "px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors flex items-center gap-2",
                            onclick: on_save_queue_to_playlist,
                            Icon {
                                name: "playlist".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Save Queue"
                        }
                    }
                    if !songs.is_empty() {
                        button {
                            class: "px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors flex items-center gap-2",
                            onclick: on_clear,
                            Icon {
                                name: "trash".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Clear Queue"
                        }
                    }
                }
            }

            if add_song_panel_open() {
                div { class: "rounded-2xl border border-zinc-700/40 bg-zinc-900/40 p-4 space-y-3",
                    p { class: "text-xs uppercase tracking-wider text-zinc-500", "Add Songs To Queue" }
                    input {
                        class: "w-full px-3 py-2 rounded-lg bg-zinc-950/70 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                        placeholder: "Search songs",
                        value: queue_search,
                        oninput: move |evt| queue_search.set(evt.value()),
                    }
                    div { class: "rounded-xl border border-zinc-800/70 bg-zinc-950/30 p-3 space-y-2",
                        div { class: "flex items-center justify-between",
                            p { class: "text-xs uppercase tracking-wide text-zinc-500", "Recommended" }
                            p { class: "text-xs text-zinc-600", "first + last + recent (up to 25)" }
                        }
                        match auto_recommendations() {
                            None => rsx! {
                                div { class: "py-2 flex items-center gap-2 text-zinc-500 text-sm",
                                    Icon { name: "loader".to_string(), class: "w-4 h-4 animate-spin".to_string() }
                                    "Finding recommendations..."
                                }
                            },
                            Some(recommendations) => {
                                if recommendations.is_empty() {
                                    rsx! {
                                        p { class: "text-sm text-zinc-500", "No recommendations yet. Add a song to shape suggestions." }
                                    }
                                } else {
                                    rsx! {
                                        div { class: "space-y-2 max-h-72 overflow-y-auto pr-1",
                                            for result in recommendations {
                                                {
                                                    let already_queued = songs.iter().any(|queued_song| {
                                                        same_song_identity(queued_song, &result)
                                                    });
                                                    let cover_url = servers()
                                                        .iter()
                                                        .find(|server| server.id == result.server_id)
                                                        .and_then(|server| {
                                                            let client = NavidromeClient::new(server.clone());
                                                            result
                                                                .cover_art
                                                                .as_ref()
                                                                .map(|cover| client.get_cover_art_url(cover, 80))
                                                        });
                                                    rsx! {
                                                        div {
                                                            key: "{result.server_id}:{result.id}:recommended",
                                                            class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors",
                                                            {
                                                                if let Some(url) = cover_url {
                                                                    rsx! {
                                                                        img {
                                                                            class: "w-10 h-10 rounded object-cover border border-zinc-800/80",
                                                                            src: "{url}",
                                                                        }
                                                                    }
                                                                } else {
                                                                    rsx! {
                                                                        div { class: "w-10 h-10 rounded bg-zinc-800 flex items-center justify-center border border-zinc-800/80",
                                                                            Icon {
                                                                                name: "music".to_string(),
                                                                                class: "w-4 h-4 text-zinc-500".to_string(),
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div { class: "min-w-0 flex-1",
                                                                p { class: "text-sm text-white truncate", "{result.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate",
                                                                    "{result.artist.clone().unwrap_or_default()} • {result.album.clone().unwrap_or_default()}"
                                                                }
                                                            }
                                                            div { class: "flex items-center gap-2",
                                                                button {
                                                                    class: if preview_song_key()
                                                                        == Some(song_identity_key(&result))
                                                                    {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs"
                                                                    },
                                                                    title: "Play a short preview, then return to your current song",
                                                                    disabled: preview_song_key()
                                                                        == Some(song_identity_key(&result)),
                                                                    onclick: {
                                                                        let song = result.clone();
                                                                        let on_preview_song = on_preview_song.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            on_preview_song(song.clone());
                                                                        }
                                                                    },
                                                                    if preview_song_key()
                                                                        == Some(song_identity_key(&result))
                                                                    {
                                                                        "Previewing..."
                                                                    } else {
                                                                        "Preview"
                                                                    }
                                                                }
                                                                button {
                                                                    class: if already_queued {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-xs"
                                                                    },
                                                                    disabled: already_queued,
                                                                    onclick: {
                                                                        let queue = queue.clone();
                                                                        let queue_index = queue_index.clone();
                                                                        let now_playing = now_playing.clone();
                                                                        let is_playing = is_playing.clone();
                                                                        let mut recently_added_seed = recently_added_seed.clone();
                                                                        let song = result.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            if enqueue_song_to_queue(
                                                                                queue.clone(),
                                                                                queue_index.clone(),
                                                                                now_playing.clone(),
                                                                                is_playing.clone(),
                                                                                song.clone(),
                                                                            ) {
                                                                                recently_added_seed.set(Some(song.clone()));
                                                                            }
                                                                        }
                                                                    },
                                                                    if already_queued {
                                                                        "In Queue"
                                                                    } else {
                                                                        "Add"
                                                                    }
                                                                }
                                                                button {
                                                                    class: "w-7 h-7 rounded-full border border-zinc-700 text-zinc-500 hover:text-zinc-200 hover:border-zinc-500 transition-colors flex items-center justify-center",
                                                                    title: "Dismiss recommendation",
                                                                    onclick: {
                                                                        let mut dismissed_recommendations = dismissed_recommendations.clone();
                                                                        let result_key = song_identity_key(&result);
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            dismissed_recommendations.with_mut(|dismissed| {
                                                                                dismissed.insert(result_key.clone());
                                                                            });
                                                                        }
                                                                    },
                                                                    Icon { name: "x".to_string(), class: "w-3 h-3".to_string() }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "pt-1 flex justify-end",
                            button {
                                class: "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                onclick: on_refresh_recommendations,
                                "Refresh recommendations"
                            }
                        }
                    }
                    if queue_search().trim().len() < 2 {
                        p { class: "text-sm text-zinc-500", "Type at least 2 characters to search for additional songs." }
                    } else {
                        p { class: "text-xs uppercase tracking-wide text-zinc-500", "Search Results" }
                        match add_song_results() {
                            None => rsx! {
                                div { class: "py-3 flex items-center gap-2 text-zinc-500 text-sm",
                                    Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                    "Searching..."
                                }
                            },
                            Some(results) => {
                                if results.is_empty() {
                                    rsx! {
                                        p { class: "text-sm text-zinc-500", "No songs found." }
                                    }
                                } else {
                                    rsx! {
                                        div { class: "space-y-2 max-h-72 overflow-y-auto pr-1",
                                            for result in results {
                                                {
                                                    let already_queued = songs.iter().any(|queued_song| {
                                                        same_song_identity(queued_song, &result)
                                                    });
                                                    let cover_url = servers()
                                                        .iter()
                                                        .find(|server| server.id == result.server_id)
                                                        .and_then(|server| {
                                                            let client = NavidromeClient::new(server.clone());
                                                            result
                                                                .cover_art
                                                                .as_ref()
                                                                .map(|cover| client.get_cover_art_url(cover, 80))
                                                        });
                                                    rsx! {
                                                        div {
                                                            key: "{result.server_id}:{result.id}:search",
                                                            class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors",
                                                            {
                                                                if let Some(url) = cover_url {
                                                                    rsx! {
                                                                        img {
                                                                            class: "w-10 h-10 rounded object-cover border border-zinc-800/80",
                                                                            src: "{url}",
                                                                        }
                                                                    }
                                                                } else {
                                                                    rsx! {
                                                                        div { class: "w-10 h-10 rounded bg-zinc-800 flex items-center justify-center border border-zinc-800/80",
                                                                            Icon {
                                                                                name: "music".to_string(),
                                                                                class: "w-4 h-4 text-zinc-500".to_string(),
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div { class: "min-w-0 flex-1",
                                                                p { class: "text-sm text-white truncate", "{result.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate",
                                                                    "{result.artist.clone().unwrap_or_default()} • {result.album.clone().unwrap_or_default()}"
                                                                }
                                                            }
                                                            div { class: "flex items-center gap-2",
                                                                button {
                                                                    class: if preview_song_key()
                                                                        == Some(song_identity_key(&result))
                                                                    {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs"
                                                                    },
                                                                    title: "Play a short preview, then return to your current song",
                                                                    disabled: preview_song_key()
                                                                        == Some(song_identity_key(&result)),
                                                                    onclick: {
                                                                        let song = result.clone();
                                                                        let on_preview_song = on_preview_song.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            on_preview_song(song.clone());
                                                                        }
                                                                    },
                                                                    if preview_song_key()
                                                                        == Some(song_identity_key(&result))
                                                                    {
                                                                        "Previewing..."
                                                                    } else {
                                                                        "Preview"
                                                                    }
                                                                }
                                                                button {
                                                                    class: if already_queued {
                                                                        "px-3 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                                    } else {
                                                                        "px-3 py-1 rounded-lg border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-xs"
                                                                    },
                                                                    disabled: already_queued,
                                                                    onclick: {
                                                                        let queue = queue.clone();
                                                                        let queue_index = queue_index.clone();
                                                                        let now_playing = now_playing.clone();
                                                                        let is_playing = is_playing.clone();
                                                                        let mut recently_added_seed = recently_added_seed.clone();
                                                                        let song = result.clone();
                                                                        move |evt: MouseEvent| {
                                                                            evt.stop_propagation();
                                                                            if enqueue_song_to_queue(
                                                                                queue.clone(),
                                                                                queue_index.clone(),
                                                                                now_playing.clone(),
                                                                                is_playing.clone(),
                                                                                song.clone(),
                                                                            ) {
                                                                                recently_added_seed.set(Some(song.clone()));
                                                                            }
                                                                        }
                                                                    },
                                                                    if already_queued {
                                                                        "In Queue"
                                                                    } else {
                                                                        "Add"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if songs.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon {
                        name: "queue".to_string(),
                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                    }
                    p { class: "text-zinc-400", "Your queue is empty" }
                    p { class: "text-zinc-500 text-sm mt-2",
                        "Use Add Songs above to build a queue."
                    }
                }
            } else {
                div { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 overflow-hidden",
                    // Current Song Section
                    if let Some(ref current) = current_song {
                        {
                            let current_cover = servers()
                                .iter()
                                .find(|s| s.id == current.server_id)
                                .and_then(|server| {
                                    let client = NavidromeClient::new(server.clone());
                                    current.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                });
                            rsx! {
                                div { class: "p-4 bg-emerald-500/10 border-b border-zinc-700/50",
                                    p { class: "text-xs font-semibold text-emerald-400 uppercase tracking-wider mb-2",
                                        "Now Playing"
                                    }
                                    div { class: "flex items-center justify-between group",
                                        div { class: "flex items-center gap-4",
                                            if current.album_id.is_some() {
                                                button {
                                                    class: "w-12 h-12 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden",
                                                    aria_label: "Open song menu",
                                                    onclick: {
                                                        let song = current.clone();
                                                        let mut song_details = song_details.clone();
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            song_details.open(song.clone());
                                                        }
                                                    },
                                                    {
                                                        match current_cover.clone() {
                                                            Some(url) => rsx! {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: "{current.title}",
                                                                    class: "w-full h-full object-cover",
                                                                    loading: "lazy",
                                                                }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full bg-zinc-700 flex items-center justify-center",
                                                                    Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            } else {
                                                div { class: "w-12 h-12 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden",
                                                    {
                                                        match current_cover {
                                                            Some(url) => rsx! {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: "{current.title}",
                                                                    class: "w-full h-full object-cover",
                                                                    loading: "lazy",
                                                                }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full bg-zinc-700 flex items-center justify-center",
                                                                    Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }



                                            div {
                                                p { class: "font-medium text-white", "{current.title}" }
                                                if current.artist_id.is_some() {
                                                    button {
                                                        class: "text-sm text-zinc-400 hover:text-emerald-400 transition-colors text-left",
                                                        onclick: {
                                                            let artist_id = current.artist_id.clone();
                                                            let server_id = current.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(artist_id) = artist_id.clone() {
                                                                    navigation
                                                                        .navigate_to(AppView::ArtistDetailView {
                                                                            artist_id,
                                                                            server_id: server_id.clone(),
                                                                        });
                                                                }
                                                            }
                                                        },
                                                        "{current.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-sm text-zinc-400",
                                                        "{current.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "text-sm text-zinc-500 font-mono", "{format_duration(current.duration)}" }
                                    }
                                }
                            }
                        }
                    }

                    // Queue List
                    div { class: "divide-y divide-zinc-800/50",
                        for (idx , song) in songs.into_iter().enumerate() {
                            {
                                let is_current = idx == current_index;
                                let song_id = song.id.clone();
                                let row_class = if is_current {
                                    "p-3 bg-emerald-500/5 flex items-center justify-between group cursor-pointer select-none ios-drag-lock"
                                } else {
                                    "p-3 hover:bg-zinc-700/30 transition-colors flex items-center justify-between group cursor-pointer select-none ios-drag-lock"
                                };
                                let can_move_up = idx > 0;
                                let can_move_down = idx + 1 < queue_len;
                                let cover_url = servers()
                                    .iter()
                                    .find(|s| s.id == song.server_id)
                                    .and_then(|server| {
                                        let client = NavidromeClient::new(server.clone());
                                        song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                    });
                                rsx! {
                                    div {
                                        key: "{song_id}-{idx}",
                                        class: "{row_class}",
                                        onclick: move |_| {
                                            if !is_current {
                                                queue_index.set(idx);
                                                now_playing.set(Some(song.clone()));
                                                is_playing.set(true);
                                            }
                                        },



                                        div { class: "flex items-center gap-4 overflow-hidden",
                                            div { class: "w-8 text-center text-sm flex-shrink-0",
                                                if is_current {
                                                    Icon {
                                                        name: "play".to_string(),
                                                        class: "w-4 h-4 text-emerald-400 mx-auto".to_string(),
                                                    }
                                                } else {
                                                    span { class: "text-zinc-500", "{idx + 1}" }
                                                }
                                            }
                                            if song.album_id.is_some() {
                                                button {
                                                    class: "w-12 h-12 rounded-lg bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    aria_label: "Open song menu",
                                                    onclick: {
                                                        let song = song.clone();
                                                        let mut song_details = song_details.clone();
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            song_details.open(song.clone());
                                                        }
                                                    },
                                                    {
                                                        match cover_url.clone() {
                                                            Some(url) => rsx! {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: "{song.title}",
                                                                    class: "w-full h-full object-cover",
                                                                    loading: "lazy",
                                                                }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            } else {
                                                div { class: "w-12 h-12 rounded-lg bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    {
                                                        match cover_url {
                                                            Some(url) => rsx! {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: "{song.title}",
                                                                    class: "w-full h-full object-cover",
                                                                    loading: "lazy",
                                                                }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }

                                            div { class: "min-w-0",
                                                p { class: if is_current { "text-emerald-400 font-medium truncate" } else { "text-zinc-300 truncate group-hover:text-white" },
                                                    "{song.title}"
                                                }
                                                if song.artist_id.is_some() {
                                                    button {
                                                        class: "text-xs text-zinc-500 truncate hover:text-emerald-400 transition-colors",
                                                        onclick: {
                                                            let artist_id = song.artist_id.clone();
                                                            let server_id = song.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(artist_id) = artist_id.clone() {
                                                                    navigation
                                                                        .navigate_to(AppView::ArtistDetailView {
                                                                            artist_id,
                                                                            server_id: server_id.clone(),
                                                                        });
                                                                }
                                                            }
                                                        },
                                                        "{song.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                                if song.album_id.is_some() {
                                                    button {
                                                        class: "text-xs text-zinc-600 truncate hover:text-emerald-400 transition-colors hidden sm:block",
                                                        onclick: {
                                                            let album_id = song.album_id.clone();
                                                            let server_id = song.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(album_id) = album_id.clone() {
                                                                    navigation
                                                                        .navigate_to(AppView::AlbumDetailView {
                                                                            album_id,
                                                                            server_id: server_id.clone(),
                                                                        });
                                                                }
                                                            }
                                                        },
                                                        "{song.album.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-xs text-zinc-600 truncate hidden sm:block",
                                                        "{song.album.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "flex items-center gap-4",
                                            span { class: "text-sm text-zinc-600 font-mono group-hover:hidden",
                                                "{format_duration(song.duration)}"
                                            }

                                            div { class: "flex flex-col gap-1",
                                                button {
                                                    r#type: "button",
                                                    class: if can_move_up {
                                                        "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                                    } else {
                                                        "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                                    },
                                                    title: "Move up",
                                                    disabled: !can_move_up,
                                                    onclick: {
                                                        let queue = queue.clone();
                                                        let queue_index = queue_index.clone();
                                                        let now_playing = now_playing.clone();
                                                        let source_index = idx;
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            if !can_move_up {
                                                                return;
                                                            }
                                                            reorder_queue_entry(
                                                                queue.clone(),
                                                                queue_index.clone(),
                                                                now_playing.clone(),
                                                                source_index,
                                                                source_index.saturating_sub(1),
                                                            );
                                                        }
                                                    },
                                                    Icon { name: "chevron-up".to_string(), class: "w-3.5 h-3.5".to_string() }
                                                }
                                                button {
                                                    r#type: "button",
                                                    class: if can_move_down {
                                                        "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                                    } else {
                                                        "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                                    },
                                                    title: "Move down",
                                                    disabled: !can_move_down,
                                                    onclick: {
                                                        let queue = queue.clone();
                                                        let queue_index = queue_index.clone();
                                                        let now_playing = now_playing.clone();
                                                        let source_index = idx;
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            if !can_move_down {
                                                                return;
                                                            }
                                                            reorder_queue_entry(
                                                                queue.clone(),
                                                                queue_index.clone(),
                                                                now_playing.clone(),
                                                                source_index,
                                                                source_index.saturating_add(1),
                                                            );
                                                        }
                                                    },
                                                    Icon { name: "chevron-down".to_string(), class: "w-3.5 h-3.5".to_string() }
                                                }
                                            }

                                            button {
                                                class: "p-2 text-zinc-500 hover:text-red-400 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    remove_queue_entry(
                                                        queue.clone(),
                                                        queue_index.clone(),
                                                        now_playing.clone(),
                                                        is_playing.clone(),
                                                        idx,
                                                    );
                                                },
                                                Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn adjusted_queue_index_after_reorder(
    current_index: usize,
    source_index: usize,
    target_index: usize,
) -> usize {
    if source_index == current_index {
        target_index
    } else if source_index < current_index && target_index >= current_index {
        current_index.saturating_sub(1)
    } else if source_index > current_index && target_index <= current_index {
        current_index.saturating_add(1)
    } else {
        current_index
    }
}

fn reorder_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    source_index: usize,
    target_index: usize,
) {
    let current_index = queue_index();
    let mut reordered = false;
    let mut next_index = current_index;

    queue.with_mut(|items| {
        if items.len() < 2
            || source_index >= items.len()
            || target_index >= items.len()
            || source_index == target_index
        {
            return;
        }

        let moved_song = items.remove(source_index);
        let insert_index = target_index;
        items.insert(insert_index, moved_song);
        next_index = adjusted_queue_index_after_reorder(current_index, source_index, insert_index);
        reordered = true;
    });

    if !reordered {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        return;
    }

    let clamped_index = next_index.min(updated_queue.len().saturating_sub(1));
    queue_index.set(clamped_index);
    if now_playing().is_some() {
        now_playing.set(updated_queue.get(clamped_index).cloned());
    }
}

fn remove_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    remove_index: usize,
) {
    let had_now_playing = now_playing().is_some();
    let was_playing = is_playing();
    let current_index = queue_index();
    let mut removed = false;

    queue.with_mut(|items| {
        if remove_index >= items.len() {
            return;
        }
        items.remove(remove_index);
        removed = true;
    });

    if !removed {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        is_playing.set(false);
        return;
    }

    let mut next_index = current_index.min(updated_queue.len().saturating_sub(1));
    if remove_index < current_index {
        next_index = current_index.saturating_sub(1);
    } else if remove_index == current_index {
        next_index = remove_index.min(updated_queue.len().saturating_sub(1));
    }

    queue_index.set(next_index);
    if had_now_playing {
        now_playing.set(updated_queue.get(next_index).cloned());
        is_playing.set(was_playing);
    }
}

fn enqueue_song_to_queue(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    song: Song,
) -> bool {
    let was_empty = queue().is_empty();
    let mut inserted_index = None;
    let mut inserted_new_song = false;

    let insert_anchor = if queue().is_empty() {
        0
    } else {
        queue_index().saturating_add(2)
    };

    queue.with_mut(|items| {
        if let Some(existing_index) = items
            .iter()
            .position(|entry| same_song_identity(entry, &song))
        {
            inserted_index = Some(existing_index);
            return;
        }
        let insert_index = insert_anchor.min(items.len());
        items.insert(insert_index, song.clone());
        inserted_index = Some(insert_index);
        inserted_new_song = true;
    });

    if was_empty && now_playing().is_none() {
        if let Some(index) = inserted_index {
            let updated_queue = queue();
            if let Some(first_song) = updated_queue.get(index).cloned() {
                queue_index.set(index);
                now_playing.set(Some(first_song));
                is_playing.set(false);
            }
        }
    }
    inserted_new_song
}

async fn search_queue_add_candidates(servers: Vec<ServerConfig>, query: String) -> Vec<Song> {
    let total_start = Instant::now();
    let normalized_query = query.trim().to_string();
    if normalized_query.len() < 2 {
        return Vec::new();
    }

    let mut servers_to_search: Vec<ServerConfig> = servers
        .iter()
        .filter(|server| server.active)
        .cloned()
        .collect();
    if servers_to_search.is_empty() {
        servers_to_search = servers;
    }

    let mut cache_server_ids: Vec<String> =
        servers_to_search.iter().map(|server| server.id.clone()).collect();
    cache_server_ids.sort();
    let cache_key = format!(
        "search:queue_add:v1:{}:{}",
        cache_server_ids.join("|"),
        normalized_query.to_lowercase()
    );
    if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
        log_perf(
            "queue.search.cache_hit",
            total_start,
            &format!("query={normalized_query} results={}", cached.len()),
        );
        return cached;
    }

    let mut results = Vec::<Song>::new();
    for server in servers_to_search {
        let server_start = Instant::now();
        let server_name = server.name.clone();
        let client = NavidromeClient::new(server.clone());
        let Ok(search) = client.search(&normalized_query, 0, 0, 20).await else {
            log_perf(
                "queue.search.server",
                server_start,
                &format!("server={server_name} query={normalized_query} status=error"),
            );
            continue;
        };
        for mut song in search.songs {
            if song.server_id.trim().is_empty() {
                song.server_id = server.id.clone();
            }
            if song.server_name.trim().is_empty() {
                song.server_name = server.name.clone();
            }
            if results
                .iter()
                .any(|existing| same_song_identity(existing, &song))
            {
                continue;
            }
            results.push(song);
            if results.len() >= 60 {
                log_perf(
                    "queue.search.server",
                    server_start,
                    &format!(
                        "server={server_name} query={normalized_query} status=ok results_so_far={}",
                        results.len()
                    ),
                );
                log_perf(
                    "queue.search.total",
                    total_start,
                    &format!("query={normalized_query} results={}", results.len()),
                );
                let _ = cache_put_json(cache_key, &results, Some(4));
                return results;
            }
        }
        log_perf(
            "queue.search.server",
            server_start,
            &format!(
                "server={server_name} query={normalized_query} status=ok results_so_far={}",
                results.len()
            ),
        );
    }

    log_perf(
        "queue.search.total",
        total_start,
        &format!("query={normalized_query} results={}", results.len()),
    );
    let _ = cache_put_json(cache_key, &results, Some(4));

    results
}

fn same_song_identity(left: &Song, right: &Song) -> bool {
    left.id == right.id && left.server_id == right.server_id
}

fn song_identity_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

async fn fetch_similar_queue_candidates(
    servers: &[ServerConfig],
    seed: &Song,
    count: usize,
) -> Vec<Song> {
    if count == 0 {
        return Vec::new();
    }

    let Some(server) = servers
        .iter()
        .find(|server| server.id == seed.server_id)
        .cloned()
    else {
        return Vec::new();
    };

    let client = NavidromeClient::new(server);
    let lookup_count = (count as u32).saturating_mul(4).max(count as u32);
    let mut similar = client
        .get_similar_songs(&seed.id, lookup_count)
        .await
        .unwrap_or_default();

    if similar.is_empty() {
        similar = client
            .get_similar_songs2(&seed.id, lookup_count)
            .await
            .unwrap_or_default();
    }

    if similar.is_empty() {
        similar = client
            .get_random_songs((count as u32).saturating_mul(6).max(24))
            .await
            .unwrap_or_default();
    }

    let seed_key = song_identity_key(seed);
    let mut dedup = HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in similar {
        let key = song_identity_key(&song);
        if key == seed_key || !dedup.insert(key) {
            continue;
        }
        output.push(song);
        if output.len() >= count {
            break;
        }
    }
    output
}

async fn build_queue_add_recommendations(
    servers: Vec<ServerConfig>,
    queue_snapshot: Vec<Song>,
    recent_seed: Option<Song>,
    dismissed_keys: HashSet<String>,
) -> Vec<Song> {
    let first_seed = queue_snapshot.first().cloned();
    let last_seed = queue_snapshot.last().cloned();

    let mut seed_specs = Vec::<(Song, usize)>::new();
    if let Some(seed) = first_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_FIRST_SEED_COUNT));
    }
    if let Some(seed) = last_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_LAST_SEED_COUNT));
    }
    if let Some(seed) = recent_seed {
        seed_specs.push((seed, AUTO_RECOMMENDATION_RECENT_SEED_COUNT));
    }

    let mut excluded = HashSet::<String>::new();
    for song in &queue_snapshot {
        excluded.insert(song_identity_key(song));
    }
    for key in dismissed_keys {
        excluded.insert(key);
    }

    let mut suggestions = Vec::<Song>::new();
    let mut used_seed_keys = HashSet::<String>::new();
    for (seed, count) in seed_specs {
        let seed_key = song_identity_key(&seed);
        if !used_seed_keys.insert(seed_key) {
            continue;
        }
        for candidate in fetch_similar_queue_candidates(&servers, &seed, count).await {
            let candidate_key = song_identity_key(&candidate);
            if excluded.insert(candidate_key) {
                suggestions.push(candidate);
                if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
                    return suggestions;
                }
            }
        }
    }

    if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
        return suggestions;
    }

    let mut random_servers: Vec<ServerConfig> = servers
        .iter()
        .filter(|server| server.active)
        .cloned()
        .collect();
    if random_servers.is_empty() {
        random_servers = servers;
    }

    for server in random_servers {
        let client = NavidromeClient::new(server);
        let random = client.get_random_songs(30).await.unwrap_or_default();
        for candidate in random {
            let candidate_key = song_identity_key(&candidate);
            if excluded.insert(candidate_key) {
                suggestions.push(candidate);
                if suggestions.len() >= AUTO_RECOMMENDATION_LIMIT {
                    return suggestions;
                }
            }
        }
    }

    suggestions
}
