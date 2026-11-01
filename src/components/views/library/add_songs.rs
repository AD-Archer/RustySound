//! "Add Songs", for a playlist and for the queue: search the library, or
//! pick from recommendations seeded by the list being added to (its first
//! and last songs, and the song added last). Quick Listen previews a song
//! for 30 seconds on its own player, so the queue is never touched.

use super::shared::use_debounced;
use crate::api::{Capabilities, MusicClient, ServerConfig, Song};
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::ui::{
    Button, ButtonVariant, IconButton, IconButtonSize, InsetList, SearchField, SectionHeader,
    Sheet, Skeleton, SongRow,
};
use crate::components::{quick_listen_id, use_player, use_servers, use_setting, Preview, QueuePosition};
use dioxus::prelude::*;
use std::collections::HashSet;

const RECOMMENDATION_LIMIT: usize = 25;
/// Recommendations per seed: the song added last, then the list's first and last.
const RECENT_SEED_COUNT: usize = 17;
const EDGE_SEED_COUNT: usize = 4;
/// Search results at most, across servers.
const SEARCH_LIMIT: usize = 60;
const SEARCH_DEBOUNCE_MS: u64 = 220;

/// Where the sheet adds songs.
#[derive(Clone, PartialEq)]
pub enum AddTarget {
    /// A playlist on `server` (searched alone); `on_add` saves each song.
    Playlist {
        server: Option<ServerConfig>,
        on_add: EventHandler<Song>,
    },
    /// The queue (every active server is searched): songs go after the song
    /// that plays next, leaving Play Next's slot alone.
    Queue,
}

impl AddTarget {
    fn sheet_testid(&self) -> &'static str {
        match self {
            Self::Playlist { .. } => "playlist-add-sheet",
            Self::Queue => "queue-add-songs",
        }
    }

    fn added_label(&self) -> &'static str {
        match self {
            Self::Playlist { .. } => "Added",
            Self::Queue => "In Queue",
        }
    }
}

/// Songs matching `query` on `servers` (cached for a few hours).
async fn search_candidates(servers: Vec<ServerConfig>, query: String) -> Vec<Song> {
    if query.chars().count() < 2 || servers.is_empty() {
        return Vec::new();
    }
    let mut ids: Vec<&str> = servers.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    let cache_key = format!("search:add_songs:v1:{}:{}", ids.join("|"), query.to_lowercase());
    if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
        return cached;
    }
    let mut results = Vec::<Song>::new();
    'servers: for server in servers {
        let Ok(found) = MusicClient::new(server.clone()).search(&query, 0, 0, 25).await else {
            continue;
        };
        for mut song in found.songs {
            if song.server_id.trim().is_empty() {
                song.server_id = server.id.clone();
            }
            if song.server_name.trim().is_empty() {
                song.server_name = server.name.clone();
            }
            if !results.iter().any(|seen| seen.same_as(&song)) {
                results.push(song);
            }
            if results.len() >= SEARCH_LIMIT {
                break 'servers;
            }
        }
    }
    let _ = cache_put_json(cache_key, &results, Some(4));
    results
}

/// Songs like `seed` on its own server, a few times `count` (the caller
/// skips the ones it already has); random songs when the server knows none.
async fn similar_to(servers: &[ServerConfig], seed: &Song, count: usize) -> Vec<Song> {
    let Some(client) = MusicClient::for_source(servers, &seed.server_id) else {
        return Vec::new();
    };
    let lookup = (count as u32).saturating_mul(4);
    let mut similar = client.get_similar_songs(&seed.id, lookup).await.unwrap_or_default();
    if similar.is_empty() {
        if let MusicClient::Subsonic(subsonic) = &client {
            similar = subsonic.get_similar_songs2(&seed.id, lookup).await.unwrap_or_default();
        }
    }
    if similar.is_empty() {
        similar = client
            .get_random_songs((count as u32).saturating_mul(6).max(24))
            .await
            .unwrap_or_default();
    }
    similar.retain(|song| !song.same_as(seed));
    similar
}

/// Similar songs to the song added last and the list's first and last songs,
/// minus what the list holds and what was dismissed, topped up with random
/// songs.
async fn recommendations(
    servers: Vec<ServerConfig>,
    list: Vec<Song>,
    recent: Option<Song>,
    dismissed: HashSet<String>,
) -> Vec<Song> {
    let mut seeds: Vec<(Song, usize)> = Vec::new();
    if let Some(seed) = recent {
        seeds.push((seed, RECENT_SEED_COUNT));
    }
    if let Some(seed) = list.first().cloned() {
        seeds.push((seed, EDGE_SEED_COUNT));
    }
    if let Some(seed) = list.last().cloned() {
        seeds.push((seed, EDGE_SEED_COUNT));
    }
    let mut excluded: HashSet<String> = list.iter().map(Song::key).collect();
    excluded.extend(dismissed);
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for (seed, count) in seeds {
        if !used.insert(seed.key()) {
            continue;
        }
        let mut taken = 0;
        for song in similar_to(&servers, &seed, count).await {
            if taken >= count || out.len() >= RECOMMENDATION_LIMIT {
                break;
            }
            if excluded.insert(song.key()) {
                out.push(song);
                taken += 1;
            }
        }
    }
    for server in servers {
        if out.len() >= RECOMMENDATION_LIMIT {
            break;
        }
        let random = MusicClient::new(server).get_random_songs(40).await.unwrap_or_default();
        for song in random {
            if out.len() >= RECOMMENDATION_LIMIT {
                break;
            }
            if excluded.insert(song.key()) {
                out.push(song);
            }
        }
    }
    out
}

/// The sheet.
/// * `target`: a playlist or the queue.
/// * `songs`: the list as it stands (it seeds the recommendations, and its
///   songs show as already added).
#[component]
pub fn AddSongsSheet(target: AddTarget, songs: ReadSignal<Vec<Song>>, on_close: EventHandler<()>) -> Element {
    let all_servers = use_servers();
    let player = use_player();
    let preview = use_context::<Preview>();
    let offline = use_setting(|s| s.offline_mode);
    let mut search = use_signal(String::new);
    let query = use_debounced(search, SEARCH_DEBOUNCE_MS);
    let mut dismissed = use_signal(HashSet::<String>::new);
    let mut added = use_signal(HashSet::<String>::new);
    let mut last_added = use_signal(|| None::<Song>);
    let mut nonce = use_signal(|| 0u32);

    // The playlist's server, or every active server for the queue.
    let target_sig = use_signal(|| target.clone());
    let servers = use_memo(move || match &*target_sig.read() {
        AddTarget::Playlist { server, .. } => server.iter().cloned().collect::<Vec<_>>(),
        AddTarget::Queue => all_servers.read().iter().filter(|s| s.active).cloned().collect(),
    });

    let results = use_resource(move || {
        let text = query();
        let servers = servers();
        async move { search_candidates(servers, text).await }
    });
    let recs = use_resource(move || {
        let _ = nonce();
        let recent = last_added();
        // Dismissing hides a row in place, and adding marks it; neither refetches.
        let ignore = dismissed.peek().clone();
        let list = songs.peek().clone();
        let servers = servers();
        async move { recommendations(servers, list, recent, ignore).await }
    });

    let add = {
        let target = target.clone();
        move |song: Song| {
            let key = song.key();
            if added.peek().contains(&key) || songs.peek().iter().any(|s| s.same_as(&song)) {
                return;
            }
            match &target {
                AddTarget::Playlist { on_add, .. } => on_add.call(song.clone()),
                AddTarget::Queue => {
                    player.enqueue(vec![song.clone()], QueuePosition::AfterNext);
                }
            }
            added.write().insert(key);
            last_added.set(Some(song));
        }
    };

    let typed = query.read().chars().count() >= 2;
    let present: HashSet<String> = songs.read().iter().map(Song::key).chain(added()).collect();
    let previewing = (preview.current)();
    let hidden = dismissed();
    let offline_now = offline();
    let added_label = target.added_label();

    let row = move |song: Song, can_dismiss: bool| {
        let key = song.key();
        let is_added = present.contains(&key);
        let listening = previewing.as_deref() == Some(quick_listen_id(&song).as_str());
        let can_listen =
            !offline_now && Capabilities::for_source(&all_servers.read(), &song.server_id).quick_listen;
        let mut add = add.clone();
        rsx! {
            SongRow {
                key: "{key}",
                song: song.clone(),
                class: "rs-library-cand".to_string(),
                trailing: rsx! {
                    if can_listen {
                        button {
                            r#type: "button",
                            class: "rs-btn rs-btn--plain",
                            title: "Play 30 seconds without changing your queue",
                            aria_pressed: if listening { "true" } else { "false" },
                            "data-testid": "add-songs-quick-listen",
                            onclick: {
                                let song = song.clone();
                                move |_| preview.quick_listen(song.clone(), &all_servers.peek())
                            },
                            if listening { "Stop" } else { "Quick Listen" }
                        }
                    }
                    Button {
                        label: if is_added { added_label.to_string() } else { "Add".to_string() },
                        icon: if is_added { "check".to_string() } else { "plus".to_string() },
                        variant: ButtonVariant::Plain,
                        disabled: is_added,
                        testid: "add-songs-add".to_string(),
                        onclick: {
                            let song = song.clone();
                            move |_| add(song.clone())
                        },
                    }
                    if can_dismiss {
                        IconButton {
                            icon: "x".to_string(),
                            label: format!("Dismiss {}", song.title),
                            quiet: true,
                            size: IconButtonSize::Small,
                            onclick: move |_| {
                                dismissed.write().insert(key.clone());
                            },
                        }
                    }
                },
            }
        }
    };

    rsx! {
        Sheet {
            title: "Add Songs".to_string(),
            on_close: move |_| on_close.call(()),
            testid: target.sheet_testid().to_string(),
            SearchField {
                value: search(),
                placeholder: "Search your library".to_string(),
                testid: "add-songs-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }
            if typed {
                SectionHeader { title: "Search Results".to_string() }
                match &*results.read() {
                    None => rsx! { Skeleton { rows: 4 } },
                    Some(list) if list.is_empty() => rsx! {
                        p { class: "rs-list-foot", "No songs match “{query}”." }
                    },
                    Some(list) => rsx! {
                        InsetList { label: "Search results".to_string(),
                            for song in list.iter().cloned() {
                                {row(song, false)}
                            }
                        }
                    },
                }
            } else {
                SectionHeader {
                    title: "Recommended".to_string(),
                    action: "Refresh".to_string(),
                    on_action: move |_| nonce += 1,
                }
                match &*recs.read() {
                    None => rsx! { Skeleton { rows: 5 } },
                    Some(list) if list.is_empty() => rsx! {
                        p { class: "rs-list-foot", "No suggestions right now. Try searching instead." }
                    },
                    Some(list) => rsx! {
                        InsetList { label: "Recommended songs".to_string(),
                            for song in list.iter().filter(|s| !hidden.contains(&s.key())).cloned() {
                                {row(song, true)}
                            }
                        }
                        p { class: "rs-list-foot", "From the first, last and most recently added songs." }
                    },
                }
            }
        }
    }
}
