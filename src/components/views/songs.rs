use crate::api::*;
use crate::components::{AppView, Navigation};
use crate::components::Icon;
use dioxus::prelude::*;

#[component]
pub fn SongsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();

    let mut search_query = use_signal(String::new);
    let mut sort_by = use_signal(|| "alphabetical".to_string());
    let mut filter_min_rating = use_signal(|| 0i32);

    let songs = use_resource(move || {
        let servers = servers();
        async move {
            let mut songs = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                // Try to get songs from different sources to get more variety
                if let Ok(server_songs) = client.get_random_songs(200).await {
                    songs.extend(server_songs);
                }
                // Also try to get some recent songs
                if let Ok(recent_albums) = client.get_albums("newest", 20, 0).await {
                    for album in recent_albums {
                        if let Ok((_, album_songs)) = client.get_album(&album.id).await {
                            songs.extend(album_songs);
                        }
                    }
                }
            }
            // Remove duplicates based on id
            let mut seen = std::collections::HashSet::new();
            songs.retain(|song| seen.insert(song.id.clone()));
            songs.truncate(500); // Limit to 500 songs for performance
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
                    div { class: "flex flex-wrap gap-3 justify-center",
                        // Sort by
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-zinc-400", "Sort:" }
                            select {
                                class: "px-3 py-1.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-sm text-white focus:outline-none focus:border-emerald-500/50",
                                value: sort_by,
                                oninput: move |e| sort_by.set(e.value()),
                                option { value: "alphabetical", "A-Z" }
                                option { value: "rating", "Rating" }
                                option { value: "recent", "Recent" }
                                option { value: "duration", "Duration" }
                            }
                        }

                        // Min rating filter
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-zinc-400", "Min Rating:" }
                            select {
                                class: "px-3 py-1.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg text-sm text-white focus:outline-none focus:border-emerald-500/50",
                                value: "{filter_min_rating}",
                                oninput: move |e| {
                                    if let Ok(rating) = e.value().parse::<i32>() {
                                        filter_min_rating.set(rating);
                                    }
                                },
                                option { value: "0", "Any" }
                                option { value: "1", "1+ Stars" }
                                option { value: "2", "2+ Stars" }
                                option { value: "3", "3+ Stars" }
                                option { value: "4", "4+ Stars" }
                                option { value: "5", "5 Stars" }
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
                            "rating" => {
                                filtered
                                    .sort_by(|a, b| {
                                        let a_rating = a.user_rating.unwrap_or(0);
                                        let b_rating = b.user_rating.unwrap_or(0);
                                        b_rating.cmp(&a_rating).then(a.title.cmp(&b.title))
                                    });
                            }
                            "recent" => {
                                filtered.sort_by(|a, b| a.title.cmp(&b.title));
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
    let queue = use_context::<Signal<Vec<Song>>>();
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

    let on_add_queue = {
        let mut queue = queue.clone();
        let song = song.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            queue.with_mut(|q| q.push(song.clone()));
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
                    p { class: "text-sm text-zinc-400 truncate",
                        "{song.album.clone().unwrap_or_default()}"
                    }
                } else {
                    p { class: "text-sm text-zinc-400 truncate",
                        "{song.album.clone().unwrap_or_default()}"
                    }
                }
            }
            // Rating
            div { class: "flex items-center gap-1",
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
            // Duration and actions
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
