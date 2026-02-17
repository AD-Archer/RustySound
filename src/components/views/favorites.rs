use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::views::home::{AlbumCard, SongRow};
use crate::components::views::search::ArtistCard;
use crate::components::{AppView, Icon, Navigation};
use crate::diagnostics::log_perf;
use dioxus::prelude::*;
use std::collections::HashSet;
use std::time::Instant;

const FAVORITES_INITIAL_LIMIT: usize = 100;
const FAVORITES_SERVER_SONG_CAP: usize = 300;

#[component]
pub fn FavoritesView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let mut active_tab = use_signal(|| "songs".to_string());
    let mut display_limit = use_signal(|| FAVORITES_INITIAL_LIMIT);

    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();

    let favorites = use_resource(move || {
        let servers = active_servers.clone();
        async move {
            let total_start = Instant::now();
            let server_count = servers.len();
            let mut cache_server_ids: Vec<String> =
                servers.iter().map(|server| server.id.clone()).collect();
            cache_server_ids.sort();
            let cache_key = format!("view:favorites:v1:{}", cache_server_ids.join("|"));

            if let Some(cached) = cache_get_json::<(Vec<Artist>, Vec<Album>, Vec<Song>)>(&cache_key)
            {
                log_perf(
                    "favorites.cache_hit",
                    total_start,
                    &format!("servers={} songs={}", server_count, cached.2.len()),
                );
                return cached;
            }

            let mut artists = Vec::new();
            let mut albums = Vec::new();
            let mut songs = Vec::new();
            let mut seen_song_keys = HashSet::<String>::new();

            for server in servers {
                let server_start = Instant::now();
                let server_name = server.name.clone();
                let client = NavidromeClient::new(server);
                if let Ok((a, al, s)) = client.get_starred().await {
                    let mut per_server_songs = 0usize;
                    artists.extend(a);
                    albums.extend(al);
                    for song in s {
                        let key = format!("{}::{}", song.server_id, song.id);
                        if seen_song_keys.insert(key) {
                            songs.push(song);
                            per_server_songs += 1;
                        }
                        if per_server_songs >= FAVORITES_SERVER_SONG_CAP {
                            break;
                        }
                    }
                    log_perf(
                        "favorites.server",
                        server_start,
                        &format!(
                            "server={server_name} total_artists={} total_albums={} total_songs={}",
                            artists.len(),
                            albums.len(),
                            songs.len()
                        ),
                    );
                }
            }

            log_perf(
                "favorites.total",
                total_start,
                &format!(
                    "servers={} artists={} albums={} songs={}",
                    server_count,
                    artists.len(),
                    albums.len(),
                    songs.len()
                ),
            );

            let payload = (artists, albums, songs);
            let _ = cache_put_json(cache_key, &payload, Some(12));
            payload
        }
    });

    let tab = active_tab();

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header gap-4",
                h1 { class: "page-title", "Favorites" }

                // Tabs
                div { class: "flex flex-wrap gap-2",
                    button {
                        class: if tab == "songs" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| {
                            active_tab.set("songs".to_string());
                            display_limit.set(FAVORITES_INITIAL_LIMIT);
                        },
                        "Songs"
                    }
                    button {
                        class: if tab == "albums" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| {
                            active_tab.set("albums".to_string());
                            display_limit.set(FAVORITES_INITIAL_LIMIT);
                        },
                        "Albums"
                    }
                    button {
                        class: if tab == "artists" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| {
                            active_tab.set("artists".to_string());
                            display_limit.set(FAVORITES_INITIAL_LIMIT);
                        },
                        "Artists"
                    }
                }
            }

            {
                match favorites() {
                    Some((artists, albums, songs)) => rsx! {
                        {
                            match tab.as_str() {
                                "songs" => rsx! {
                                    if songs.is_empty() {
                                        EmptyFavorites { item_type: "songs".to_string() }
                                    } else {
                                        {
                                            let limit = display_limit();
                                            let display: Vec<Song> = songs.iter().take(limit).cloned().collect();
                                            rsx! {
                                                div { class: "space-y-1",
                                                    for (index , song) in display.iter().enumerate() {
                                                        SongRow {
                                                            song: song.clone(),
                                                            index: index + 1,
                                                            onclick: {
                                                                let song = song.clone();
                                                                let songs_for_queue = songs.clone();
                                                                move |_| {
                                                                    queue.set(songs_for_queue.clone());
                                                                    queue_index.set(index);
                                                                    now_playing.set(Some(song.clone()));
                                                                    is_playing.set(true);
                                                                }
                                                            },
                                                        }
                                                    }
                                                    if songs.len() > limit {
                                                        div { class: "pt-2 flex justify-center",
                                                            button {
                                                                class: "px-4 py-2 rounded-xl bg-zinc-800/60 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors text-sm",
                                                                onclick: move |_| display_limit.set(display_limit().saturating_add(FAVORITES_INITIAL_LIMIT)),
                                                                "Show more ({songs.len() - limit} remaining)"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                "albums" => rsx! {
                                    if albums.is_empty() {
                                        EmptyFavorites { item_type: "albums".to_string() }
                                    } else {
                                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 overflow-x-hidden",
                                            for album in albums {
                                                AlbumCard {
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
                                },
                                "artists" => rsx! {
                                    if artists.is_empty() {
                                        EmptyFavorites { item_type: "artists".to_string() }
                                    } else {
                                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6",
                                            for artist in artists {
                                                ArtistCard {
                                                    artist: artist.clone(),
                                                    onclick: {
                                                        let navigation = navigation.clone();
                                                        let artist_id = artist.id.clone();
                                                        let artist_server_id = artist.server_id.clone();
                                                    move |_| {
                                                        navigation
                                                            .navigate_to(
                                                                AppView::ArtistDetailView {
                                                                    artist_id: artist_id.clone(),
                                                                    server_id: artist_server_id.clone(),
                                                                },
                                                            )
                                                    }
                                                },
                                            }
                                        }
                                        }
                                    }
                                },
                                _ => rsx! {},
                            }
                        }
                    },
                    None => rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-8 h-8 text-zinc-500".to_string(),
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn EmptyFavorites(item_type: String) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-20",
            Icon {
                name: "heart".to_string(),
                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
            }
            h2 { class: "text-xl font-semibold text-white mb-2", "No favorite {item_type}" }
            p { class: "text-zinc-400", "Star {item_type} in your Navidrome server to see them here" }
        }
    }
}
