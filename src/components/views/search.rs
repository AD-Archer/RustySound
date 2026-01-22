use crate::api::*;
use crate::components::views::home::{AlbumCard, SongRow};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn SearchView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<SearchResult>);
    let mut is_searching = use_signal(|| false);

    let mut on_search = move |_| {
        let query = search_query().trim().to_string();
        if query.is_empty() {
            return;
        }

        let active_servers: Vec<ServerConfig> =
            servers().into_iter().filter(|s| s.active).collect();
        is_searching.set(true);

        spawn(async move {
            let mut combined = SearchResult::default();

            for server in active_servers {
                let client = NavidromeClient::new(server);
                if let Ok(result) = client.search(&query, 20, 20, 50).await {
                    combined.artists.extend(result.artists);
                    combined.albums.extend(result.albums);
                    combined.songs.extend(result.songs);
                }
            }

            search_results.set(Some(combined));
            is_searching.set(false);
        });
    };

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
                            search_query.set(value.clone());
                            if value.is_empty() {
                                search_results.set(None);
                                is_searching.set(false);
                            } else if value.len() >= 2 {
                                on_search(());
                            }
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






                                                        .navigate_to(


                                                            AppView::ArtistDetail(artist_id.clone(), artist_server_id.clone()),
                                                        )
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
                                                            AppView::AlbumDetail(album_id.clone(), album_server_id.clone()),
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
                                    for (index , song) in songs.into_iter().enumerate() {
                                        SongRow {
                                            key: "{song.id}-{song.server_id}",
                                            song: song.clone(),
                                            index: index + 1,
                                            onclick: move |_| {
                                                now_playing.set(Some(song.clone()));
                                                is_playing.set(true);
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
