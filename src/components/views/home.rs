use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    
    // Fetch recent albums from all active servers
    let recent_albums = use_resource(move || {
        let active_servers = servers().into_iter().filter(|s| s.active).collect::<Vec<_>>();
        async move {
            let mut albums = Vec::new();
            for server in active_servers {
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
        let active_servers = servers().into_iter().filter(|s| s.active).collect::<Vec<_>>();
        async move {
            let mut songs = Vec::new();
            for server in active_servers {
                let client = NavidromeClient::new(server);
                if let Ok(server_songs) = client.get_random_songs(5).await {
                    songs.extend(server_songs);
                }
            }
            songs
        }
    });
    
    let has_servers = servers().iter().any(|s| s.active);
    
    rsx! {
        div { class: "space-y-8",
            // Welcome header
            header { class: "page-header",
                h1 { class: "page-title", "Good evening" }
                p { class: "page-subtitle",
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
    let current_view = use_context::<Signal<AppView>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    
    let cover_url = servers().iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 300))
        });

    let on_add_album = {
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let album_id = album_id.clone();
            let server = servers()
                .iter()
                .find(|s| s.id == server_id)
                .map(|s| s.clone());
            if let Some(server) = server {
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    if let Ok((_, songs)) = client.get_album(&album_id).await {
                        queue.with_mut(|q| q.extend(songs));
                    }
                });
            }
        }
    };

    let on_artist_click = {
        let artist_id = album.artist_id.clone();
        let server_id = album.server_id.clone();
        let mut current_view = current_view.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(artist_id) = artist_id.clone() {
                current_view.set(AppView::ArtistDetail(artist_id, server_id.clone()));
            }
        }
    };
    
    rsx! {
        div {
            class: "group text-left cursor-pointer",
            onclick: move |e| onclick.call(e),
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
                button {
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/70 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add album to queue",
                    onclick: on_add_album,
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
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
            if album.artist_id.is_some() {
                button {
                    class: "text-xs text-zinc-400 truncate hover:text-emerald-400 transition-colors",
                    onclick: on_artist_click,
                    "{album.artist}"
                }
            } else {
                p { class: "text-xs text-zinc-400 truncate", "{album.artist}" }
            }
        }
    }
}

#[component]
pub fn SongRow(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let current_view = use_context::<Signal<AppView>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    
    let cover_url = servers().iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
        });

    let album_id = song.album_id.clone();
    let artist_id = song.artist_id.clone();
    let server_id = song.server_id.clone();

    let on_album_click_cover = {
        let album_id = &album_id;
        let server_id = &server_id;
        let current_view = current_view.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(ref album_id_val) = album_id {
                current_view.set(AppView::AlbumDetail((*album_id_val).clone(), (*server_id).clone()));
            }
        }
    };

    let on_album_click_text = {
        let album_id = &album_id;
        let server_id = &server_id;
        let current_view = current_view.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(ref album_id_val) = album_id {
                current_view.set(AppView::AlbumDetail((*album_id_val).clone(), (*server_id).clone()));
            }
        }
    };

    let on_artist_click = {
        let artist_id = artist_id.clone();
        let server_id = server_id.clone();
        let mut current_view = current_view.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(artist_id) = artist_id.clone() {
                current_view.set(AppView::ArtistDetail(artist_id, server_id.clone()));
            }
        }
    };

    let on_add_queue = {
        let mut queue = queue.clone();
        let song = song.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            queue.with_mut(|q| q.push(song.clone()));
        }
    };
    
    rsx! {
        div {
            class: "w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer",
            onclick: move |e| onclick.call(e),
            // Index
            span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{index}" }
            span { class: "w-6 text-sm text-white hidden group-hover:block",
                Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
            }
            // Cover
            if album_id.is_some() {
                button {
                    class: "w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                    aria_label: "Open album",
                    onclick: on_album_click_cover,
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
            } else {
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
            }
            // Song info
            div { class: "flex-1 min-w-0 text-left",
                p { class: "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                    "{song.title}"
                }
                if artist_id.is_some() {
                    button {
                        class: "text-xs text-zinc-400 truncate hover:text-emerald-400 transition-colors",
                        onclick: on_artist_click,
                        "{song.artist.clone().unwrap_or_default()}"
                    }
                } else {
                    p { class: "text-xs text-zinc-400 truncate",
                        "{song.artist.clone().unwrap_or_default()}"
                    }
                }
            }
            // Album
            div { class: "hidden md:block flex-1 min-w-0",
                if album_id.is_some() {
                    button {
                        class: "text-sm text-zinc-400 truncate hover:text-emerald-400 transition-colors",
                        onclick: on_album_click_text,
                        "{song.album.clone().unwrap_or_default()}"
                    }
                } else {
                    p { class: "text-sm text-zinc-400 truncate",
                        "{song.album.clone().unwrap_or_default()}"
                    }
                }
            }
            // Duration
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: on_add_queue,
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
            }
        }
    }
}
