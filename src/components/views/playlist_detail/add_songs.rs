//! "Add Songs" sheet for a playlist: search the server, or pick from
//! recommendations (similar to the playlist's first, last and last-added
//! songs) with a 12-second preview.

use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{
    Artwork, IconButton, IconButtonSize, InsetList, PillButton, PillVariant, SearchField,
    SectionHeader, Sheet, Skeleton,
};
use crate::components::views::library::shared::{song_key, use_debounced};
use crate::components::{PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal};
use dioxus::prelude::*;
use std::collections::HashSet;

const PREVIEW_MS: u64 = 12_000;
const RECOMMENDATION_LIMIT: usize = 25;

async fn search_candidates(server: Option<ServerConfig>, query: String) -> Vec<Song> {
    let query = query.trim().to_string();
    let Some(server) = server.filter(|_| !query.is_empty()) else {
        return Vec::new();
    };
    let cache_key = format!("search:playlist_add:v1:{}:{}", server.id, query.to_lowercase());
    if let Some(cached) = cache_get_json::<Vec<Song>>(&cache_key) {
        return cached;
    }
    let found = NavidromeClient::new(server)
        .search(&query, 0, 0, 25)
        .await
        .map(|r| r.songs)
        .unwrap_or_default();
    let _ = cache_put_json(cache_key, &found, Some(4));
    found
}

async fn similar_to(server: &ServerConfig, seed: &Song, count: usize) -> Vec<Song> {
    let client = NavidromeClient::new(server.clone());
    let lookup = (count as u32 * 4).max(count as u32);
    let mut similar = client.get_similar_songs(&seed.id, lookup).await.unwrap_or_default();
    if similar.is_empty() {
        similar = client.get_similar_songs2(&seed.id, lookup).await.unwrap_or_default();
    }
    if similar.is_empty() {
        similar = client.get_random_songs((count as u32 * 6).max(24)).await.unwrap_or_default();
    }
    similar.truncate(count * 4);
    similar
}

/// Similar songs to the first and last songs and the song added last,
/// minus what's already in the playlist and what was dismissed.
async fn recommendations(
    server: Option<ServerConfig>,
    playlist: Vec<Song>,
    recent: Option<Song>,
    dismissed: HashSet<String>,
) -> Vec<Song> {
    let Some(server) = server else {
        return Vec::new();
    };
    let mut seeds: Vec<(Song, usize)> = Vec::new();
    if let Some(seed) = recent {
        seeds.push((seed, 17));
    }
    if let Some(seed) = playlist.first().cloned() {
        seeds.push((seed, 4));
    }
    if let Some(seed) = playlist.last().cloned() {
        seeds.push((seed, 4));
    }
    let mut excluded: HashSet<String> = playlist.iter().map(song_key).collect();
    excluded.extend(dismissed);
    let mut used = HashSet::new();
    let mut out = Vec::new();
    for (seed, count) in seeds {
        if !used.insert(song_key(&seed)) {
            continue;
        }
        let mut taken = 0;
        for song in similar_to(&server, &seed, count).await {
            if taken >= count || out.len() >= RECOMMENDATION_LIMIT {
                break;
            }
            if excluded.insert(song_key(&song)) {
                out.push(song);
                taken += 1;
            }
        }
    }
    if out.len() < RECOMMENDATION_LIMIT {
        let client = NavidromeClient::new(server);
        for song in client.get_random_songs(40).await.unwrap_or_default() {
            if out.len() >= RECOMMENDATION_LIMIT {
                break;
            }
            if excluded.insert(song_key(&song)) {
                out.push(song);
            }
        }
    }
    out
}

/// Plays `song` for 12 s, then restores what was playing and where.
fn use_preview() -> (Signal<Option<String>>, impl FnMut(Song) + Copy) {
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let position = use_context::<PlaybackPositionSignal>().0;
    let seek = use_context::<SeekRequestSignal>().0;
    let preview_flag = use_context::<PreviewPlaybackSignal>().0;
    let mut session = use_signal(|| 0u64);
    let mut previewing = use_signal(|| None::<String>);

    let start = move |song: Song| {
        let (mut queue_index, mut now_playing, mut is_playing, mut position, mut seek, mut preview_flag) =
            (queue_index, now_playing, is_playing, position, seek, preview_flag);
        let saved_index = *queue_index.peek();
        let saved_song = now_playing.peek().clone();
        let saved_playing = *is_playing.peek();
        let saved_position = *position.peek();
        let saved_seek = seek.peek().clone().or_else(|| {
            saved_song.as_ref().map(|s| (s.id.clone(), saved_position.max(0.0)))
        });
        let id = *session.peek() + 1;
        session.set(id);
        previewing.set(Some(song_key(&song)));
        preview_flag.set(true);
        position.set(0.0);
        seek.set(Some((song.id.clone(), 0.0)));
        now_playing.set(Some(song));
        is_playing.set(true);
        spawn(async move {
            sleep_ms(PREVIEW_MS).await;
            if *session.peek() != id {
                return;
            }
            queue_index.set(saved_index);
            now_playing.set(saved_song);
            is_playing.set(saved_playing);
            position.set(saved_position.max(0.0));
            seek.set(saved_seek);
            previewing.set(None);
            preview_flag.set(false);
        });
    };
    (previewing, start)
}

#[component]
fn CandidateRow(
    song: Song,
    previewing: bool,
    added: bool,
    on_preview: EventHandler<()>,
    on_add: EventHandler<()>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    let sub = song.artist.clone().unwrap_or_default();
    rsx! {
        li {
            div { class: "rs-row rs-library-cand",
                Artwork { cover_art: song.cover_art.clone(), server_id: song.server_id.clone(), size_px: 40 }
                span { class: "rs-row__meta",
                    span { class: "rs-row__title", span { "{song.title}" } }
                    span { class: "rs-row__sub", style: "display:block", "{sub}" }
                }
                PillButton {
                    label: if previewing { "Playing".to_string() } else { "Preview".to_string() },
                    icon: "play".to_string(),
                    variant: PillVariant::Mini,
                    disabled: previewing,
                    onclick: move |_| on_preview.call(()),
                }
                PillButton {
                    label: if added { "Added".to_string() } else { "Add".to_string() },
                    icon: if added { "check".to_string() } else { "plus".to_string() },
                    variant: PillVariant::Mini,
                    disabled: added,
                    testid: "playlist-add-song".to_string(),
                    onclick: move |_| on_add.call(()),
                }
                if let Some(dismiss) = on_dismiss {
                    IconButton {
                        icon: "x".to_string(),
                        label: format!("Dismiss {}", song.title),
                        quiet: true,
                        size: IconButtonSize::Small,
                        onclick: move |_| dismiss.call(()),
                    }
                }
            }
        }
    }
}

/// The sheet. `songs` is the playlist as it stands; `on_add` adds one song
/// (the parent talks to the server and reloads).
#[component]
pub fn AddSongsSheet(
    server: Option<ServerConfig>,
    songs: Signal<Vec<Song>>,
    on_add: EventHandler<Song>,
    on_close: EventHandler<()>,
) -> Element {
    let mut search = use_signal(String::new);
    let query = use_debounced(search, 250);
    let mut dismissed = use_signal(HashSet::<String>::new);
    let mut added = use_signal(HashSet::<String>::new);
    let mut last_added = use_signal(|| None::<Song>);
    let mut nonce = use_signal(|| 0u32);
    let (previewing, mut preview) = use_preview();
    let server_sig = use_signal(|| server.clone());

    let results = use_resource(move || {
        let q = query();
        let server = server_sig.peek().clone();
        async move { search_candidates(server, q).await }
    });
    let recs = use_resource(move || {
        let _ = nonce();
        let recent = last_added();
        // Dismissing hides a row in place; it doesn't refetch.
        let ignore = dismissed.peek().clone();
        let current = songs.peek().clone();
        let server = server_sig.peek().clone();
        async move { recommendations(server, current, recent, ignore).await }
    });

    let mut add = move |song: Song| {
        added.write().insert(song_key(&song));
        last_added.set(Some(song.clone()));
        on_add.call(song);
    };

    let searching = !query.read().is_empty();
    let previewing_key = previewing();
    let added_now = added();
    let hidden = dismissed();

    rsx! {
        Sheet {
            title: "Add Songs".to_string(),
            on_close: move |_| on_close.call(()),
            testid: "playlist-add-sheet".to_string(),
            SearchField {
                value: search(),
                placeholder: "Search your library".to_string(),
                testid: "playlist-add-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }
            if searching {
                match &*results.read() {
                    None => rsx! { Skeleton { rows: 4 } },
                    Some(list) if list.is_empty() => rsx! {
                        p { class: "rs-list-foot", "No songs match “{query}”." }
                    },
                    Some(list) => rsx! {
                        InsetList { label: "Search results".to_string(),
                            for song in list.clone() {
                                CandidateRow {
                                    key: "{song_key(&song)}",
                                    previewing: previewing_key.as_deref() == Some(song_key(&song).as_str()),
                                    added: added_now.contains(&song_key(&song)),
                                    on_preview: {
                                        let song = song.clone();
                                        move |_| preview(song.clone())
                                    },
                                    on_add: {
                                        let song = song.clone();
                                        move |_| add(song.clone())
                                    },
                                    song,
                                }
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
                            for song in list.iter().filter(|s| !hidden.contains(&song_key(s))).cloned() {
                                CandidateRow {
                                    key: "{song_key(&song)}",
                                    previewing: previewing_key.as_deref() == Some(song_key(&song).as_str()),
                                    added: added_now.contains(&song_key(&song)),
                                    on_preview: {
                                        let song = song.clone();
                                        move |_| preview(song.clone())
                                    },
                                    on_add: {
                                        let song = song.clone();
                                        move |_| add(song.clone())
                                    },
                                    on_dismiss: {
                                        let key = song_key(&song);
                                        move |_| {
                                            dismissed.write().insert(key.clone());
                                        }
                                    },
                                    song,
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
