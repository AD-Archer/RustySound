use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    let has_servers = !active_servers.is_empty();
    
    let active_servers_for_albums = active_servers.clone();
    let active_servers_for_songs = active_servers.clone();
    
    // Fetch recent albums from all active servers
    let recent_albums = use_resource(move || {
        let servers = active_servers_for_albums.clone();
        async move {
            let mut albums = Vec::new();
            for server in servers {
                let client = NavidromeClient::new(server);
                if let Ok(server_albums) = client.get_albums("newest", 8, 0).await {
                    albums.extend(server_albums);
                }
            }
            albums.truncate(12);
            albums
        }
    });
    
    // Fetch random songs
    let random_songs = use_resource(move || {
        let servers = active_servers_for_songs.clone();
        async move {
            let mut songs = Vec::new();
            for server in servers {
                let client = NavidromeClient::new(server);
                if let Ok(server_songs) = client.get_random_songs(5).await {
                    songs.extend(server_songs);
                }
            }
            songs
        }
    });
    
    rsx! {
        div { class: "space-y-8",
            // Welcome header
            header { class: "mb-8",
                h1 { class: "text-3xl font-bold text-white mb-2", "Good evening" }
                p { class: "text-zinc-400",
                    if has_servers {
                        "Welcome back. Here's what's new in your library."
                    } else {
                        "Connect a Navidrome server to get started."
                    }
                }
            }

            if !has_servers {
                // Empty state - no servers
                div { class: "flex flex-col items-center justify-center py-20",
                    div { class: "w-20 h-20 rounded-2xl bg-zinc-800/50 flex items-center justify-center mb-6",
                        Icon {
                            name: "server".to_string(),
                            class: "w-10 h-10 text-zinc-500".to_string(),
                        }
                    }
                    h2 { class: "text-xl font-semibold text-white mb-2", "No servers connected" }
                    p { class: "text-zinc-400 text-center max-w-md mb-6",
                        "Add your Navidrome server to start streaming your music collection."
                    }
                    button {
                        class: "px-6 py-3 bg-emerald-500 hover:bg-emerald-400 text-white font-medium rounded-xl transition-colors",
                        onclick: move |_| current_view.set(AppView::Settings),
                        "Add Server"
                    }
                }
            } else {
                // Quick play cards
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3 mb-8",
                    QuickPlayCard {
                        title: "Random Mix".to_string(),
                        gradient: "from-purple-600 to-indigo-600".to_string(),
                        onclick: move |_| current_view.set(AppView::Random),
                    }
                    QuickPlayCard {
                        title: "Favorites".to_string(),
                        gradient: "from-rose-600 to-pink-600".to_string(),
                        onclick: move |_| current_view.set(AppView::Favorites),
                    }
                    QuickPlayCard {
                        title: "Radio Stations".to_string(),
                        gradient: "from-emerald-600 to-teal-600".to_string(),
                        onclick: move |_| current_view.set(AppView::Radio),
                    }
                    QuickPlayCard {
                        title: "All Albums".to_string(),
                        gradient: "from-amber-600 to-orange-600".to_string(),
                        onclick: move |_| current_view.set(AppView::Albums),
                    }
                }

                // Recently added albums
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Recently Added" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: move |_| current_view.set(AppView::Albums),
                            "See all"
                        }
                    }

                    {
                        match recent_albums() {
                            Some(albums) => rsx! {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4",
                                    for album in albums {
                                        AlbumCard {
                                            album: album.clone(),
                                            onclick: move |_| {
                                                current_view.set(AppView::AlbumDetail(album.id.clone(), album.server_id.clone()))
                                            },
                                        }
                                    }
                                }
                            },
                            None => rsx! {
                                div { class: "flex items-center justify-center py-12",
                                    Icon {
                                        name: "loader".to_string(),
                                        class: "w-8 h-8 text-zinc-500".to_string(),
                                    }
                                }
                            },
                        }
                    }
                }

                // Quick picks (random songs)
                section {
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Quick Picks" }
                    }

                    {
                        match random_songs() {
                            Some(songs) => rsx! {
                                div { class: "space-y-1",
                                    for (index , song) in songs.iter().enumerate() {
                                        SongRow {
                                            song: song.clone(),
                                            index: index + 1,
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
                            },
                            None => rsx! {
                                div { class: "flex items-center justify-center py-12",
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
    }
}

#[component]
fn QuickPlayCard(title: String, gradient: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "flex items-center gap-3 p-4 rounded-xl bg-zinc-800/50 hover:bg-zinc-800 transition-colors text-left group",
            onclick: move |e| onclick.call(e),
            div { class: "w-12 h-12 rounded-lg bg-gradient-to-br {gradient} flex items-center justify-center shadow-lg",
                Icon {
                    name: "play".to_string(),
                    class: "w-5 h-5 text-white".to_string(),
                }
            }
            span { class: "font-medium text-white group-hover:text-emerald-400 transition-colors",
                "{title}"
            }
        }
    }
}

#[component]
pub fn AlbumCard(album: Album, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    
    let cover_url = servers().iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 300))
        });
    
    rsx! {
        button { class: "group text-left", onclick: move |e| onclick.call(e),
            // Album cover
            div { class: "aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-12 h-12 text-zinc-500".to_string(),
                                }
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
            // Album info
            p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors",
                "{album.name}"
            }
            p { class: "text-xs text-zinc-400 truncate", "{album.artist}" }
        }
    }
}

#[component]
pub fn SongRow(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    
    let cover_url = servers().iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
        });
    
    rsx! {
        button {
            class: "w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group",
            onclick: move |e| onclick.call(e),
            // Index
            span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{index}" }
            span { class: "w-6 text-sm text-white hidden group-hover:block",
                Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
            }
            // Cover
            div { class: "w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                            }
                        },
                    }
                }
            }
            // Song info
            div { class: "flex-1 min-w-0 text-left",
                p { class: "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                    "{song.title}"
                }
                p { class: "text-xs text-zinc-400 truncate",
                    "{song.artist.clone().unwrap_or_default()}"
                }
            }
            // Album
            div { class: "hidden md:block flex-1 min-w-0",
                p { class: "text-sm text-zinc-400 truncate",
                    "{song.album.clone().unwrap_or_default()}"
                }
            }
            // Duration
            span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
        }
    }
}
