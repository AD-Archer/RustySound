//! "Add Songs" for the queue page: search, plus recommendations (first,
//! last and recently added songs as seeds) with a 12-second Preview.

use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::song_details::actions::{same_song, song_key, use_player, PlayerCtx};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{Artwork, IconButton, IconButtonSize, InsetList, SectionHeader, Sheet, Skeleton};
use crate::components::{Icon, PreviewPlaybackSignal, SeekRequestSignal};
use crate::diagnostics::{log_perf, PerfTimer};
use dioxus::prelude::*;
use std::collections::HashSet;

const PREVIEW_MS: u64 = 12_000;
const RECOMMENDATION_LIMIT: usize = 25;
const FIRST_SEED_COUNT: usize = 4;
const LAST_SEED_COUNT: usize = 4;
const RECENT_SEED_COUNT: usize = 17;
const SEARCH_DEBOUNCE_MS: u64 = 220;

/// Inserts `song` two places after the current one (once). Starts the queue
/// (paused) when it was empty. Returns true when it was added.
fn enqueue_after_current(mut player: PlayerCtx, song: Song) -> bool {
    if player.queue.peek().iter().any(|s| same_song(s, &song)) {
        return false;
    }
    let was_empty = player.queue.peek().is_empty();
    let at = if was_empty {
        0
    } else {
        (*player.queue_index.peek() + 2).min(player.queue.peek().len())
    };
    player.queue.with_mut(|items| items.insert(at, song.clone()));
    if was_empty && player.now_playing.peek().is_none() {
        player.queue_index.set(at);
        player.now_playing.set(Some(song));
        player.is_playing.set(false);
    }
    true
}

async fn search_candidates(servers: Vec<ServerConfig>, query: String) -> Vec<Song> {
    let started = PerfTimer::now();
    let query = query.trim().to_string();
    if query.len() < 2 {
        return Vec::new();
    }
    let mut targets: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    if targets.is_empty() {
        targets = servers;
    }
    let mut ids: Vec<String> = targets.iter().map(|s| s.id.clone()).collect();
    ids.sort();
    let cache_key = format!("search:queue_add:v1:{}:{}", ids.join("|"), query.to_lowercase());
    if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
        return cached;
    }
    let mut results = Vec::<Song>::new();
    'servers: for server in targets {
        let client = NavidromeClient::new(server.clone());
        let Ok(found) = client.search(&query, 0, 0, 20).await else {
            continue;
        };
        for mut song in found.songs {
            if song.server_id.trim().is_empty() {
                song.server_id = server.id.clone();
            }
            if song.server_name.trim().is_empty() {
                song.server_name = server.name.clone();
            }
            if !results.iter().any(|seen| same_song(seen, &song)) {
                results.push(song);
            }
            if results.len() >= 60 {
                break 'servers;
            }
        }
    }
    log_perf("queue.search.total", started, &format!("query={query} results={}", results.len()));
    let _ = cache_put_json(cache_key, &results, Some(4));
    results
}

async fn similar_to(servers: &[ServerConfig], seed: &Song, count: usize) -> Vec<Song> {
    let Some(server) = servers.iter().find(|s| s.id == seed.server_id).cloned() else {
        return Vec::new();
    };
    let client = NavidromeClient::new(server);
    let lookup = (count as u32).saturating_mul(4).max(count as u32);
    let mut similar = client.get_similar_songs(&seed.id, lookup).await.unwrap_or_default();
    if similar.is_empty() {
        similar = client.get_similar_songs2(&seed.id, lookup).await.unwrap_or_default();
    }
    if similar.is_empty() {
        similar = client
            .get_random_songs((count as u32).saturating_mul(6).max(24))
            .await
            .unwrap_or_default();
    }
    let seed_key = song_key(seed);
    let mut seen = HashSet::new();
    similar
        .into_iter()
        .filter(|s| {
            let key = song_key(s);
            key != seed_key && seen.insert(key)
        })
        .take(count)
        .collect()
}

async fn recommendations(
    servers: Vec<ServerConfig>,
    queue: Vec<Song>,
    recent: Option<Song>,
    dismissed: HashSet<String>,
) -> Vec<Song> {
    let mut seeds = Vec::<(Song, usize)>::new();
    if let Some(seed) = queue.first().cloned() {
        seeds.push((seed, FIRST_SEED_COUNT));
    }
    if let Some(seed) = queue.last().cloned() {
        seeds.push((seed, LAST_SEED_COUNT));
    }
    if let Some(seed) = recent {
        seeds.push((seed, RECENT_SEED_COUNT));
    }
    let mut excluded: HashSet<String> = queue.iter().map(song_key).collect();
    excluded.extend(dismissed);
    let mut used = HashSet::new();
    let mut out = Vec::<Song>::new();
    for (seed, count) in seeds {
        if !used.insert(song_key(&seed)) {
            continue;
        }
        for candidate in similar_to(&servers, &seed, count).await {
            if excluded.insert(song_key(&candidate)) {
                out.push(candidate);
                if out.len() >= RECOMMENDATION_LIMIT {
                    return out;
                }
            }
        }
    }
    let mut random_servers: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    if random_servers.is_empty() {
        random_servers = servers;
    }
    for server in random_servers {
        let client = NavidromeClient::new(server);
        for candidate in client.get_random_songs(30).await.unwrap_or_default() {
            if excluded.insert(song_key(&candidate)) {
                out.push(candidate);
                if out.len() >= RECOMMENDATION_LIMIT {
                    return out;
                }
            }
        }
    }
    out
}

/// 12-second preview of a song, then back to what was playing.
#[derive(Clone, Copy)]
struct Preview {
    session: Signal<u64>,
    song_key: Signal<Option<String>>,
    seek_request: Signal<Option<(String, f64)>>,
    flag: Signal<bool>,
}

impl Preview {
    fn start(self, mut player: PlayerCtx, song: Song) {
        let (mut seek_request, mut preview_flag) = (self.seek_request, self.flag);
        let (mut session, mut key) = (self.session, self.song_key);
        let saved_index = *player.queue_index.peek();
        let saved_song = player.now_playing.peek().clone();
        let saved_playing = *player.is_playing.peek();
        let saved_position = *player.position.peek();
        let saved_seek = seek_request.peek().clone().or_else(|| {
            saved_song.as_ref().map(|s| (s.id.clone(), saved_position.max(0.0)))
        });

        session += 1;
        let this_session = *session.peek();
        key.set(Some(song_key(&song)));
        preview_flag.set(true);
        player.position.set(0.0);
        seek_request.set(Some((song.id.clone(), 0.0)));
        player.now_playing.set(Some(song));
        player.is_playing.set(true);

        spawn(async move {
            sleep_ms(PREVIEW_MS).await;
            if *session.peek() != this_session {
                return;
            }
            player.queue_index.set(saved_index);
            player.now_playing.set(saved_song);
            player.is_playing.set(saved_playing);
            player.position.set(saved_position.max(0.0));
            seek_request.set(saved_seek);
            key.set(None);
            preview_flag.set(false);
        });
    }
}

#[component]
pub(super) fn AddSongsSheet(on_close: EventHandler<()>) -> Element {
    let player = use_player();
    let mut query = use_signal(String::new);
    let mut debounced = use_signal(String::new);
    let mut generation = use_signal(|| 0u64);
    let mut recent_seed = use_signal(|| None::<Song>);
    let mut dismissed = use_signal(HashSet::<String>::new);
    let mut refresh = use_signal(|| 0u64);
    let preview = Preview {
        session: use_signal(|| 0u64),
        song_key: use_signal(|| None::<String>),
        seek_request: use_context::<SeekRequestSignal>().0,
        flag: use_context::<PreviewPlaybackSignal>().0,
    };

    use_effect(move || {
        let text = query().trim().to_string();
        generation += 1;
        let current = *generation.peek();
        if text.len() < 2 {
            if !debounced.peek().is_empty() {
                debounced.set(String::new());
            }
            return;
        }
        spawn(async move {
            sleep_ms(SEARCH_DEBOUNCE_MS).await;
            if *generation.peek() == current {
                debounced.set(text);
            }
        });
    });

    let recommended = use_resource(move || {
        let servers = player.servers.read().clone();
        let queue = player.queue.read().clone();
        let recent = recent_seed();
        let skip = dismissed();
        let _ = refresh();
        async move { recommendations(servers, queue, recent, skip).await }
    });
    let results = use_resource(move || {
        let text = debounced();
        let servers = player.servers.read().clone();
        async move { search_candidates(servers, text).await }
    });

    let queued_keys: HashSet<String> = player.queue.read().iter().map(song_key).collect();
    let previewing = (preview.song_key)();
    let typed = query().trim().len() >= 2;

    let row = |song: Song, can_dismiss: bool| {
        let key = song_key(&song);
        let dismiss_key = key.clone();
        let queued = queued_keys.contains(&key);
        let is_previewing = previewing.as_deref() == Some(key.as_str());
        rsx! {
            li { key: "{key}",
                div { class: "rs-trow rs-trow--art rs-player-addrow",
                    Artwork { cover_art: song.cover_art.clone(), server_id: song.server_id.clone(), size_px: 44 }
                    div { class: "rs-trow__meta",
                        div { class: "rs-trow__title", "{song.title}" }
                        div { class: "rs-trow__sub", "{song.artist.clone().unwrap_or_default()}" }
                    }
                    button {
                        r#type: "button",
                        class: "rs-btn rs-btn--plain",
                        title: "Play 12 seconds, then return to your song",
                        disabled: is_previewing,
                        onclick: {
                            let song = song.clone();
                            move |_| preview.start(player, song.clone())
                        },
                        if is_previewing { "Previewing…" } else { "Preview" }
                    }
                    button {
                        r#type: "button",
                        class: "rs-btn rs-btn--plain",
                        disabled: queued,
                        onclick: {
                            let song = song.clone();
                            move |_| {
                                if enqueue_after_current(player, song.clone()) {
                                    recent_seed.set(Some(song.clone()));
                                }
                            }
                        },
                        if queued { "In Queue" } else { "Add" }
                    }
                    if can_dismiss {
                        IconButton {
                            icon: "x".to_string(),
                            label: "Dismiss {song.title}".to_string(),
                            quiet: true,
                            size: IconButtonSize::Small,
                            onclick: move |_| {
                                dismissed.with_mut(|set| {
                                    set.insert(dismiss_key.clone());
                                });
                            },
                        }
                    }
                }
            }
        }
    };

    rsx! {
        Sheet {
            title: "Add Songs".to_string(),
            testid: "queue-add-songs".to_string(),
            on_close: move |_| on_close.call(()),
            label { class: "rs-field rs-player-search__field",
                Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                input {
                    r#type: "search",
                    placeholder: "Search songs",
                    aria_label: "Search songs to add",
                    value: "{query}",
                    oninput: move |evt| query.set(evt.value()),
                }
            }
            if typed {
                SectionHeader { title: "Search Results".to_string() }
                match results() {
                    None => rsx! { Skeleton { rows: 3 } },
                    Some(list) if list.is_empty() => rsx! { p { class: "rs-list-foot", "No songs found." } },
                    Some(list) => rsx! {
                        InsetList { label: "Search results".to_string(),
                            for song in list {
                                {row(song, false)}
                            }
                        }
                    },
                }
            } else {
                p { class: "rs-list-foot", "Type at least 2 characters to search." }
            }
            SectionHeader {
                title: "Recommended".to_string(),
                action: "Refresh".to_string(),
                on_action: move |_| refresh += 1,
            }
            match recommended() {
                None => rsx! { Skeleton { rows: 4 } },
                Some(list) if list.is_empty() => rsx! {
                    p { class: "rs-list-foot", "No recommendations yet. Add a song to shape them." }
                },
                Some(list) => rsx! {
                    InsetList { label: "Recommended songs".to_string(),
                        for song in list {
                            {row(song, true)}
                        }
                    }
                    p { class: "rs-list-foot", "From the first, last and most recently added songs." }
                },
            }
        }
    }
}
