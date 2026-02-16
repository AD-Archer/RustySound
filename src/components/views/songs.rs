use crate::api::*;
use crate::components::Icon;
use crate::components::{AddIntent, AddMenuController, AppView, Navigation, SongDetailsController};
use dioxus::prelude::*;

#[component]
pub fn SongsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();

    let mut search_query = use_signal(String::new);
    let mut sort_by = use_signal(|| "last_played".to_string());
    let mut filter_min_rating = use_signal(|| 0i32);
    let limit = use_signal(|| 30u32);

    let songs = use_resource(move || {
        let servers = servers();
        let limit = limit();
        let query_snapshot = search_query();
        async move {
            let mut songs = Vec::new();
            let active_servers: Vec<ServerConfig> =
                servers.into_iter().filter(|s| s.active).collect();
            if query_snapshot.trim().is_empty() {
                // Lazy load a small batch by default
                let per_server =
                    ((limit as usize).max(10) / active_servers.len().max(1)).max(10) as u32;
                for server in active_servers {
                    let client = NavidromeClient::new(server);
                    if let Ok(server_songs) = client.get_random_songs(per_server).await {
                        songs.extend(server_songs);
                    }
                }
            } else {
                // Search mode - query backend directly
                for server in active_servers {
                    let client = NavidromeClient::new(server);
                    if let Ok(results) = client
                        .search(&query_snapshot, 0, 0, limit as u32 + 20)
                        .await
                    {
                        songs.extend(results.songs);
                    }
                }
            }
            // Remove duplicates based on id
            let mut seen = std::collections::HashSet::new();
            songs.retain(|song| seen.insert(song.id.clone()));
            songs.truncate(limit as usize);
            songs
        }
    });

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header gap-4",
                h1 { class: "page-title", "Songs" }

                div { class: "flex flex-col gap-3 md:flex-row md:items-center md:justify-between",
                    div { class: "relative w-full md:max-w-xs",
                        Icon {
                            name: "search".to_string(),
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-sm text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "Search songs",
                            value: search_query,
                            oninput: move |e| {
                                let value = e.value();
                                if value.is_empty() || value.len() >= 2 {
                                    search_query.set(value);
                                }
                            },
                        }
                    }

                    // Filters
                    div { class: "flex flex-wrap gap-2 justify-center items-center",
                        // Sort by
                        div { class: "flex items-center gap-1",
                            span { class: "text-xs text-zinc-400 whitespace-nowrap",
                                "Sort:"
                            }
                            select {
                                class: "px-2 py-1 bg-zinc-800/50 border border-zinc-700/50 rounded text-xs text-white focus:outline-none focus:border-emerald-500/50 min-w-0",
                                value: sort_by,
                                oninput: move |e| sort_by.set(e.value()),
                                option { value: "last_played", "Last Played" }
                                option { value: "alphabetical", "A-Z" }
                                option { value: "rating", "Rating" }
                                option { value: "duration", "Duration" }
                            }
                        }

                        // Min rating filter
                        div { class: "flex items-center gap-1",
                            span { class: "text-xs text-zinc-400 whitespace-nowrap",
                                "Min:"
                            }
                            select {
                                class: "px-2 py-1 bg-zinc-800/50 border border-zinc-700/50 rounded text-xs text-white focus:outline-none focus:border-emerald-500/50 min-w-0",
                                value: "{filter_min_rating}",
                                oninput: move |e| {
                                    if let Ok(rating) = e.value().parse::<i32>() {
                                        filter_min_rating.set(rating);
                                    }
                                },
                                option { value: "0", "Any" }
                                option { value: "1", "1+" }
                                option { value: "2", "2+" }
                                option { value: "3", "3+" }
                                option { value: "4", "4+" }
                                option { value: "5", "5" }
                            }
                        }
                    }
                }
            }

            {

                // First filter by search query

                // Filter by genre

                // Filter by minimum rating

                // Sort the results
                // For now, sort by title since we don't have date info
                // Alphabetical

                match songs() {
                    Some(songs) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let sort_option = sort_by();
                        let min_rating = filter_min_rating();
                        let mut filtered: Vec<Song> = if query.is_empty() {
                            songs.clone()
                        } else {
                            songs
                                .iter()
                                .filter(|song| {
                                    let title = song.title.to_lowercase();
                                    let artist = song
                                        .artist
                                        .clone()
                                        .unwrap_or_default()
                                        .to_lowercase();
                                    let album = song
                                        .album
                                        .clone()
                                        .unwrap_or_default()
                                        .to_lowercase();
                                    title.contains(&query) || artist.contains(&query)
                                        || album.contains(&query)
                                })
                                .cloned()
                                .collect()
                        };
                        if min_rating > 0 {
                            filtered
                                .retain(|song| {
                                    song.user_rating.unwrap_or(0) >= min_rating as u32
                                });
                        }
                        match sort_option.as_str() {
                            // Keep server order as-is to approximate last played/random order
                            "last_played" => {}
                            "rating" => {
                                filtered
                                    .sort_by(|a, b| {
                                        let a_rating = a.user_rating.unwrap_or(0);
                                        let b_rating = b.user_rating.unwrap_or(0);
                                        b_rating.cmp(&a_rating).then(a.title.cmp(&b.title))
                                    });
                            }
                            "duration" => {
                                filtered.sort_by(|a, b| b.duration.cmp(&a.duration));
                            }
                            _ => {
                                filtered.sort_by(|a, b| a.title.cmp(&b.title));
                            }
                        }
                        let has_query = !query.is_empty() || min_rating > 0;
                        rsx! {
                            if filtered.is_empty() {
                                div { class: "flex flex-col items-center justify-center py-20",
                                    Icon {
                                        name: "music".to_string(),
                                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                    }
                                    if has_query {
                                        p { class: "text-zinc-300", "No songs match \"{raw_query}\"" }
                                    } else {
                                        p { class: "text-zinc-400", "No songs found" }
                                    }
                                }
                            } else {
                                div { class: "space-y-1",
                                    for (index , song) in filtered.iter().enumerate() {
                                        SongRowWithRating {
                                            song: song.clone(),
                                            index: index + 1,
                                            onclick: {
                                                let songs = filtered.clone();
                                                let mut now_playing = now_playing.clone();
                                                let mut is_playing = is_playing.clone();
                                                let mut queue = queue.clone();
                                                let mut queue_index = queue_index.clone();
                                                let song = song.clone();
                                                move |_| {
                                                    queue.set(songs.clone());
                                                    queue_index.set(index);
                                                    now_playing.set(Some(song.clone()));
                                                    is_playing.set(true);
                                                }
                                            },
                                        }
                                    }
                                }
                                button {
                                    class: "w-full mt-4 py-3 rounded-xl bg-zinc-800/60 hover:bg-zinc-800 text-zinc-200 text-sm font-medium transition-colors",
                                    onclick: {
                                        let mut limit = limit.clone();
                                        move |_| limit.set(limit() + 30)
                                    },
                                    "View more"
                                }
                            }
                        }
                    }
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
fn SongRowWithRating(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let rating = song.user_rating.unwrap_or(0).min(5);

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
    let is_favorited = use_signal(|| song.starred.is_some());

    let on_album_click_cover = {
        let song = song.clone();
        let mut song_details = song_details.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            song_details.open(song.clone());
        }
    };

    let on_album_click_text = {
        let album_id = album_id.clone();
        let server_id = server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id_val,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let on_open_menu = {
        let mut add_menu = add_menu.clone();
        let song = song.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };

    let on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        move |new_rating: i32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let _ = client.set_rating(&song_id, new_rating as u32).await;
                }
            });
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
                    aria_label: "Open song menu",
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
                p { class: "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors max-w-full",
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
            // Favorite + Rating
            div { class: "flex items-center gap-2",
                button {
                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: on_toggle_favorite,
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                // Hide star ratings on mobile to leave space for titles
                div { class: "hidden sm:flex items-center gap-1",
                    for i in 1..=5 {
                        button {
                            class: "w-4 h-4",
                            onclick: {
                                let on_set_rating = on_set_rating.clone();
                                let rating_value = i as i32;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    on_set_rating(rating_value);
                                }
                            },
                            Icon {
                                name: if i <= rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-4 h-4 text-amber-400 hover:text-amber-300 transition-colors".to_string(),
                            }
                        }
                    }
                }
            }
            // Duration and actions
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: on_open_menu,
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
