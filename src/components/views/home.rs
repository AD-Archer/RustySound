use crate::api::*;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;
use futures_util::future::join_all;

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();

    // Fetch recent albums from all active servers
    let recent_albums = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server);
                client.get_albums("newest", 12, 0).await.unwrap_or_default()
            });
            let mut albums: Vec<Album> = join_all(tasks).await.into_iter().flatten().collect();
            albums.truncate(12);
            albums
        }
    });

    // Fetch most played albums
    let most_played_albums = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server);
                client
                    .get_albums("frequent", 12, 0)
                    .await
                    .unwrap_or_default()
            });
            let mut albums: Vec<Album> = join_all(tasks).await.into_iter().flatten().collect();
            albums.truncate(12);
            albums
        }
    });

    // Fetch recently played albums
    let recently_played_albums = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server);
                client.get_albums("recent", 12, 0).await.unwrap_or_default()
            });
            let mut albums: Vec<Album> = join_all(tasks).await.into_iter().flatten().collect();
            albums.truncate(12);
            albums
        }
    });

    // Fetch random albums
    let random_albums = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server);
                client.get_albums("random", 12, 0).await.unwrap_or_default()
            });
            let mut albums: Vec<Album> = join_all(tasks).await.into_iter().flatten().collect();
            albums.truncate(12);
            albums
        }
    });

    // Fetch genres from albums
    let genres = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let mut genre_set = std::collections::HashSet::new();
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server);
                client
                    .get_albums("alphabeticalByName", 80, 0)
                    .await
                    .unwrap_or_default()
            });

            for album in join_all(tasks).await.into_iter().flatten() {
                if let Some(genre) = album.genre {
                    genre_set.insert(genre);
                }
            }

            let mut genres: Vec<String> = genre_set.into_iter().collect();
            genres.sort();
            genres.truncate(12);
            genres
        }
    });

    // Fetch quick picks (random songs)
    let quick_picks = use_resource(move || {
        let active_servers = servers()
            .into_iter()
            .filter(|s| s.active)
            .collect::<Vec<_>>();
        async move {
            let mut songs = Vec::new();
            let tasks = active_servers.into_iter().map(|server| async move {
                let client = NavidromeClient::new(server.clone());
                if let Ok(artists) = client.get_artists().await {
                    if let Some(artist) = artists
                        .iter()
                        .filter(|a| !a.name.trim().is_empty())
                        .max_by_key(|a| a.album_count)
                    {
                        if let Ok(top) = client.get_top_songs(&artist.name, 24).await {
                            return top;
                        }
                    }
                }

                client.get_random_songs(24).await.unwrap_or_default()
            });

            songs.extend(join_all(tasks).await.into_iter().flatten());

            songs.truncate(24);
            songs
        }
    });

    let has_servers = servers().iter().any(|s| s.active);

    rsx! {
        div { class: "space-y-8 max-w-none",
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
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Settings)
                        },
                        "Add Server"
                    }
                }
            } else {
                // Quick play cards
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-3 mb-8",
                    QuickPlayCard {
                        title: "Random Mix".to_string(),
                        gradient: "from-purple-600 to-indigo-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Random)
                        },
                    }
                    QuickPlayCard {
                        title: "All Songs".to_string(),
                        gradient: "from-sky-600 to-cyan-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Songs)
                        },
                    }
                    QuickPlayCard {
                        title: "Favorites".to_string(),
                        gradient: "from-rose-600 to-pink-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Favorites)
                        },
                    }
                    QuickPlayCard {
                        title: "Radio Stations".to_string(),
                        gradient: "from-emerald-600 to-teal-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Radio)
                        },
                    }
                    QuickPlayCard {
                        title: "All Albums".to_string(),
                        gradient: "from-amber-600 to-orange-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Albums(None))
                        },
                    }
                    QuickPlayCard {
                        title: "Playlists".to_string(),
                        gradient: "from-amber-600 to-orange-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Playlists)
                        },
                    }
                }

                // Genres
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Genres" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Albums(None))
                            },
                            "See all"
                        }
                    }

                    {
                        match genres() {
                            Some(genres_list) if !genres_list.is_empty() => rsx! {
                                div { class: "grid grid-cols-2 gap-3 max-h-48 overflow-y-auto",
                                    for genre in genres_list {
                                        GenreCard {
                                            genre: genre.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let genre_name = genre.clone();
                                                move |_| {
                                                    navigation.navigate_to(AppView::Albums(Some(genre_name.clone())));
                                                }
                                            },
                                        }
                                    }
                                }
                            },
                            _ => rsx! {
                                div { class: "flex items-center justify-center py-8",
                                    Icon {
                                        name: "loader".to_string(),
                                        class: "w-8 h-8 text-zinc-500".to_string(),
                                    }
                                }
                            },
                        }
                    }
                }

                // Recently added albums
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Recently Added" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Albums(None))
                            },
                            "See all"
                        }
                    }

                    {
                        match recent_albums() {
                            Some(albums) => rsx! {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
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
                                                            AppView::AlbumDetail(album_id.clone(), album_server_id.clone()),
                                                        )
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

                // Most played albums
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Most Played" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Albums(None))
                            },
                            "See all"
                        }
                    }

                    {
                        match most_played_albums() {
                            Some(albums) => rsx! {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
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
                                                            AppView::AlbumDetail(album_id.clone(), album_server_id.clone()),
                                                        )
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

                // Recently played albums
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Recently Played" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Albums(None))
                            },
                            "See all"
                        }
                    }

                    {
                        match recently_played_albums() {
                            Some(albums) => rsx! {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
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
                                                            AppView::AlbumDetail(album_id.clone(), album_server_id.clone()),
                                                        )
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

                // Random albums
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Random" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Albums(None))
                            },
                            "See all"
                        }
                    }

                    {
                        match random_albums() {
                            Some(albums) => rsx! {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6 gap-4 overflow-x-hidden",
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
                                                            AppView::AlbumDetail(album_id.clone(), album_server_id.clone()),
                                                        )
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

                // Quick picks (random songs)
                section {
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Quick Picks" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::Songs)
                            },
                            "See all"
                        }
                    }

                    {
                        match quick_picks() {
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
fn GenreCard(genre: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "flex items-center gap-3 p-4 rounded-xl bg-zinc-800/50 hover:bg-zinc-800 transition-colors text-left group flex-shrink-0",
            onclick: move |e| onclick.call(e),
            div { class: "w-12 h-12 rounded-lg bg-gradient-to-br from-blue-600 to-purple-600 flex items-center justify-center shadow-lg",
                Icon {
                    name: "music".to_string(),
                    class: "w-5 h-5 text-white".to_string(),
                }
            }
            span { class: "font-medium text-white group-hover:text-emerald-400 transition-colors",
                "{genre}"
            }
        }
    }
}

#[component]
fn SongCard(song: Song, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let rating = song.user_rating.unwrap_or(0).min(5);
    let is_favorited = use_signal(|| song.starred.is_some());

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 120))
        });

    let artist_id = song.artist_id.clone();

    let make_on_add_queue = {
        let queue = queue.clone();
        let song = song.clone();
        move || {
            let mut queue = queue.clone();
            let song = song.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                queue.with_mut(|q| q.push(song.clone()));
            }
        }
    };

    let on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut is_favorited = is_favorited.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let should_star = !is_favorited();
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                let servers_snapshot = servers();
                if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&song_id, "song").await
                    } else {
                        client.unstar(&song_id, "song").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                        now_playing.with_mut(|current| {
                            if let Some(ref mut s) = current {
                                if s.id == song_id {
                                    s.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                        queue.with_mut(|items| {
                            for s in items.iter_mut() {
                                if s.id == song_id {
                                    s.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                    }
                }
            });
        }
    };

    rsx! {
        div {
            class: "group text-left cursor-pointer flex-shrink-0 w-48",
            onclick: move |e| onclick.call(e),
            // Cover
            div { class: "aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon { name: "music".to_string(), class: "w-8 h-8 text-zinc-500".to_string() }
                            }
                        },
                    }
                }
                button {
                    class: "absolute top-2 right-2 p-2 rounded-full bg-zinc-950/70 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: make_on_add_queue(),
                    Icon {
                        name: "plus".to_string(),
                        class: "w-3 h-3".to_string(),
                    }
                }
                // Play overlay
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                    div { class: "w-10 h-10 rounded-full bg-emerald-500 flex items-center justify-center shadow-xl transform scale-90 group-hover:scale-100 transition-transform",
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 text-white ml-0.5".to_string(),
                        }
                    }
                }
            }
            // Song info
            p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors max-w-full",
                "{song.title}"
            }
            if artist_id.is_some() {
                p { class: "text-xs text-zinc-400 truncate max-w-full",
                    "{song.artist.clone().unwrap_or_default()}"
                }
            } else {
                p { class: "text-xs text-zinc-400 truncate max-w-full",
                    "{song.artist.clone().unwrap_or_default()}"
                }
            }
            if rating > 0 {
                div { class: "mt-2 flex items-center gap-1 text-amber-400",
                    for i in 1..=5 {
                        Icon {
                            name: if i <= rating { "star-filled".to_string() } else { "star".to_string() },
                            class: "w-3.5 h-3.5".to_string(),
                        }
                    }
                }
            }
            div { class: "mt-2 flex items-center gap-3",
                button {
                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: on_toggle_favorite,
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                button {
                    class: "p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors",
                    aria_label: "Add to queue",
                    onclick: make_on_add_queue(),
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            }
        }
    }
}

#[component]
pub fn AlbumCard(album: Album, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();

    let cover_url = servers()
        .iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
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
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(artist_id) = artist_id.clone() {
                navigation.navigate_to(AppView::ArtistDetail(artist_id, server_id.clone()));
            }
        }
    };

    rsx! {
        div {
            class: "group text-left cursor-pointer w-full max-w-48 overflow-hidden",
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
            p {
                class: "font-medium text-white text-sm group-hover:text-emerald-400 transition-colors truncate",
                title: "{album.name}",
                "{album.name}"
            }
            if album.artist_id.is_some() {
                button {
                    class: "text-xs text-zinc-400 truncate hover:text-emerald-400 transition-colors",
                    title: "{album.artist}",
                    onclick: on_artist_click,
                    "{album.artist}"
                }
            } else {
                p {
                    class: "text-xs text-zinc-400 truncate",
                    title: "{album.artist}",
                    "{album.artist}"
                }
            }
        }
    }
}

#[component]
pub fn SongRow(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let rating = song.user_rating.unwrap_or(0).min(5);
    let is_favorited = use_signal(|| song.starred.is_some());

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 80))
        });

    let album_id = song.album_id.clone();
    let artist_id = song.artist_id.clone();
    let server_id = song.server_id.clone();

    let on_album_click_cover = {
        let album_id = album_id.clone();
        let server_id = server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetail(album_id_val, server_id.clone()));
            }
        }
    };

    let on_album_click_text = {
        let album_id = album_id.clone();
        let server_id = server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetail(album_id_val, server_id.clone()));
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

    let on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let mut queue = queue.clone();
        let mut is_favorited = is_favorited.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let should_star = !is_favorited();
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                let servers_snapshot = servers();
                if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&song_id, "song").await
                    } else {
                        client.unstar(&song_id, "song").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                        queue.with_mut(|items| {
                            for s in items.iter_mut() {
                                if s.id == song_id {
                                    s.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                    }
                }
            });
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
                    p { class: "text-xs text-zinc-400 truncate",
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
                        class: "text-sm text-zinc-400 truncate hover:text-emerald-400 transition-colors text-left w-full",
                        onclick: on_album_click_text,
                        "{song.album.clone().unwrap_or_default()}"
                    }
                } else {
                    p { class: "text-sm text-zinc-400 truncate",
                        "{song.album.clone().unwrap_or_default()}"
                    }
                }
            }
            // Duration and actions
            div { class: "flex items-center gap-3",
                if rating > 0 {
                    div { class: "hidden sm:flex items-center gap-1 text-amber-400",
                        for i in 1..=5 {
                            Icon {
                                name: if i <= rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
                button {
                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: on_toggle_favorite,
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
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
