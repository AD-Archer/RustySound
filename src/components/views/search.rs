use crate::api::*;
use crate::components::views::home::{AlbumCard, SongRow};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;
use std::collections::HashSet;

#[cfg(not(target_arch = "wasm32"))]
async fn search_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn search_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

#[component]
pub fn SearchView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let mut search_query = use_signal(String::new);
    let debounced_query = use_signal(String::new);
    let search_results = use_signal(|| None::<SearchResult>);
    let is_searching = use_signal(|| false);
    let debounce_generation = use_signal(|| 0u64);
    let search_generation = use_signal(|| 0u64);

    // Debounce typing to avoid firing search requests on every keystroke.
    {
        let mut debounced_query = debounced_query.clone();
        let mut search_results = search_results.clone();
        let mut is_searching = is_searching.clone();
        let mut debounce_generation = debounce_generation.clone();
        let mut search_generation = search_generation.clone();
        use_effect(move || {
            let raw_query = search_query();
            let query = raw_query.trim().to_string();
            debounce_generation.with_mut(|value| *value = value.saturating_add(1));
            let generation = *debounce_generation.peek();

            if query.is_empty() {
                search_generation.with_mut(|value| *value = value.saturating_add(1));
                debounced_query.set(String::new());
                search_results.set(None);
                is_searching.set(false);
                return;
            }

            if query.len() < 2 {
                search_generation.with_mut(|value| *value = value.saturating_add(1));
                debounced_query.set(String::new());
                search_results.set(None);
                is_searching.set(false);
                return;
            }

            let mut debounced_query = debounced_query.clone();
            let debounce_generation = debounce_generation.clone();
            spawn(async move {
                search_delay_ms(220).await;
                if *debounce_generation.peek() != generation {
                    return;
                }
                debounced_query.set(query);
            });
        });
    }

    // Execute search for the debounced query and drop stale responses.
    {
        let servers = servers.clone();
        let debounced_query = debounced_query.clone();
        let mut search_results = search_results.clone();
        let mut is_searching = is_searching.clone();
        let mut search_generation = search_generation.clone();
        use_effect(move || {
            let query = debounced_query().trim().to_string();
            if query.is_empty() {
                return;
            }

            let active_servers: Vec<ServerConfig> =
                servers().into_iter().filter(|s| s.active).collect();
            if active_servers.is_empty() {
                search_results.set(Some(SearchResult::default()));
                is_searching.set(false);
                return;
            }

            search_generation.with_mut(|value| *value = value.saturating_add(1));
            let generation = *search_generation.peek();
            is_searching.set(true);

            spawn(async move {
                let mut combined = SearchResult::default();

                // Keep result counts bounded so scoring doesn't block UI.
                for server in active_servers {
                    let client = NavidromeClient::new(server);
                    if let Ok(result) = client.search(&query, 24, 48, 96).await {
                        combined.artists.extend(result.artists);
                        combined.albums.extend(result.albums);
                        combined.songs.extend(result.songs);
                    }
                }

                combined = dedupe_search_results(combined);
                combined = filter_and_score_results(combined, &query);

                if *search_generation.peek() != generation {
                    return;
                }

                search_results.set(Some(combined));
                is_searching.set(false);
            });
        });
    }

    let results = search_results();
    let searching = is_searching();

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header gap-4",
                h1 { class: "page-title", "Search" }

                // Search input
                div { class: "relative max-w-2xl",
                    Icon {
                        name: "search".to_string(),
                        class: "absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-zinc-400".to_string(),
                    }
                    input {
                        class: "w-full pl-12 pr-4 py-4 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                        placeholder: "Search songs, albums, artists...",
                        value: search_query,
                        oninput: move |e| {
                            let value = e.value();
                            search_query.set(value);
                        },
                    }
                }
            }

            if searching {
                div { class: "flex items-center justify-center py-20",
                    Icon {
                        name: "loader".to_string(),
                        class: "w-8 h-8 text-zinc-500".to_string(),
                    }
                }
            } else if let Some(results) = results {
                // Clone to owned vectors for iteration in RSX
                {
                    let artists: Vec<Artist> = results.artists.iter().take(6).cloned().collect();
                    let albums: Vec<Album> = results.albums.iter().take(6).cloned().collect();
                    let songs: Vec<Song> = results.songs.iter().take(20).cloned().collect();
                    let has_artists = !artists.is_empty();
                    let has_albums = !albums.is_empty();
                    let has_songs = !songs.is_empty();
                    let no_results = !has_artists && !has_albums && !has_songs;

                    rsx! {
                        // Artists
                        if has_artists {
                            section { class: "mb-8",
                                h2 { class: "text-xl font-semibold text-white mb-4", "Artists" }
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
                                    for artist in artists {
                                        ArtistCard {
                                            key: "{artist.id}-{artist.server_id}",
                                            artist: artist.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let artist_id = artist.id.clone();
                                                let artist_server_id = artist.server_id.clone();
                                                move |_| {
                                                    navigation

                        // Albums

                        // Songs






                                                        .navigate_to(AppView::ArtistDetailView {
                                                            artist_id: artist_id.clone(),
                                                            server_id: artist_server_id.clone(),
                                                        })
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }

                        if has_albums {
                            section { class: "mb-8",
                                h2 { class: "text-xl font-semibold text-white mb-4", "Albums" }
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
                                    for album in albums {
                                        AlbumCard {
                                            key: "{album.id}-{album.server_id}",
                                            album: album.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let album_id = album.id.clone();
                                                let album_server_id = album.server_id.clone();
                                                move |_| {
                                                    navigation
                                                        .navigate_to(
                                                            AppView::AlbumDetailView {
                                                                album_id: album_id.clone(),
                                                                server_id: album_server_id.clone(),
                                                            },
                                                        )
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }

                        if has_songs {
                            section {
                                h2 { class: "text-xl font-semibold text-white mb-4", "Songs" }
                                div { class: "space-y-1",
                                    for (index , song) in songs.iter().enumerate() {
                                        SongRow {
                                            key: "{song.id}-{song.server_id}",
                                            song: song.clone(),
                                            index: index + 1,
                                            show_download: true,
                                            onclick: {
                                                let song = song.clone();
                                                move |_| {
                                                    now_playing.set(Some(song.clone()));
                                                    is_playing.set(true);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }

                        if no_results {
                            div { class: "flex flex-col items-center justify-center py-20",
                                Icon {
                                    name: "search".to_string(),
                                    class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                }
                                p { class: "text-zinc-400", "No results found" }
                            }
                        }
                    }
                }
            } else {
                // Empty state
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon {
                        name: "search".to_string(),
                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                    }
                    p { class: "text-zinc-400", "Search your entire music library" }
                }
            }
        }
    }
}

#[component]
pub fn ArtistCard(artist: Artist, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();

    let cover_url = servers()
        .iter()
        .find(|s| s.id == artist.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            artist
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    let initials: String = artist
        .name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();

    rsx! {
        button { class: "group text-center", onclick: move |e| onclick.call(e),
            // Artist image
            div { class: "aspect-square rounded-full bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow mx-auto",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800 text-2xl font-bold text-zinc-500",
                                "{initials}"
                            }
                        },
                    }
                }
                // Play overlay
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                    div { class: "w-12 h-12 rounded-full bg-emerald-500 flex items-center justify-center shadow-xl transform scale-90 group-hover:scale-100 transition-transform",
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 text-white ml-0.5".to_string(),
                        }
                    }
                }
            }
            // Artist info
            p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors",
                "{artist.name}"
            }
            p { class: "text-xs text-zinc-400", "{artist.album_count} albums" }
        }
    }
}

/// Tokenize and normalize search query
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn dedupe_search_results(mut results: SearchResult) -> SearchResult {
    let mut artist_seen = HashSet::new();
    results
        .artists
        .retain(|artist| artist_seen.insert(format!("{}::{}", artist.server_id, artist.id)));

    let mut album_seen = HashSet::new();
    results
        .albums
        .retain(|album| album_seen.insert(format!("{}::{}", album.server_id, album.id)));

    let mut song_seen = HashSet::new();
    results
        .songs
        .retain(|song| song_seen.insert(format!("{}::{}", song.server_id, song.id)));

    results
}

/// Calculate fuzzy match score for a field against query tokens
fn calculate_score(field: &str, tokens: &[String]) -> i32 {
    let field_lower = field.to_lowercase();
    let mut score = 0;

    for token in tokens {
        // Exact full match (highest score)
        if field_lower == *token {
            score += 100;
        }
        // Starts with token (high score)
        else if field_lower.starts_with(token) {
            score += 80;
        }
        // Contains token as word (medium-high score)
        else if field_lower.split_whitespace().any(|word| word == token) {
            score += 60;
        }
        // Contains token anywhere (medium score)
        else if field_lower.contains(token) {
            score += 40;
        }
    }

    score
}

/// Score songs based on fuzzy matching across title, artist, and album
fn score_song(song: &Song, tokens: &[String]) -> i32 {
    let title_score = calculate_score(&song.title, tokens) * 3; // Title is most important
    let artist_score = song
        .artist
        .as_deref()
        .map_or(0, |artist| calculate_score(artist, tokens))
        * 2; // Artist is second
    let album_score = song
        .album
        .as_deref()
        .map_or(0, |album| calculate_score(album, tokens)); // Album is third

    title_score + artist_score + album_score
}

/// Score albums based on fuzzy matching across name and artist
fn score_album(album: &Album, tokens: &[String]) -> i32 {
    let name_score = calculate_score(&album.name, tokens) * 3;
    let artist_score = calculate_score(&album.artist, tokens) * 2;

    name_score + artist_score
}

/// Score artists based on fuzzy matching on name
fn score_artist(artist: &Artist, tokens: &[String]) -> i32 {
    calculate_score(&artist.name, tokens) * 4 // Artists only have name, so weight it heavily
}

/// Filter and score all search results with fuzzy matching
fn filter_and_score_results(mut results: SearchResult, query: &str) -> SearchResult {
    let tokens = tokenize(query);

    if tokens.is_empty() {
        return results;
    }

    // Score and filter songs
    let mut song_scores: Vec<(Song, i32)> = results
        .songs
        .into_iter()
        .map(|song| {
            let score = score_song(&song, &tokens);
            (song, score)
        })
        .filter(|(_, score)| *score > 0) // Only keep songs with positive scores
        .collect();

    song_scores.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by score descending
    results.songs = song_scores.into_iter().map(|(song, _)| song).collect();

    // Score and filter albums
    let mut album_scores: Vec<(Album, i32)> = results
        .albums
        .into_iter()
        .map(|album| {
            let score = score_album(&album, &tokens);
            (album, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    album_scores.sort_by(|a, b| b.1.cmp(&a.1));
    results.albums = album_scores.into_iter().map(|(album, _)| album).collect();

    // Score and filter artists
    let mut artist_scores: Vec<(Artist, i32)> = results
        .artists
        .into_iter()
        .map(|artist| {
            let score = score_artist(&artist, &tokens);
            (artist, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    artist_scores.sort_by(|a, b| b.1.cmp(&a.1));
    results.artists = artist_scores
        .into_iter()
        .map(|(artist, _)| artist)
        .collect();

    results
}
