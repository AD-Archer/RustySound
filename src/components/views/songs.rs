use crate::api::*;
use crate::components::Icon;
use crate::components::{AddIntent, AddMenuController, AppView, Navigation};
use chrono::{DateTime, NaiveDateTime};
use dioxus::prelude::*;
use futures_util::future::join_all;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

async fn fetch_songs_from_album_order(
    active_servers: &[ServerConfig],
    album_type: &str,
    desired_song_count: u32,
) -> Vec<Song> {
    if active_servers.is_empty() {
        return Vec::new();
    }

    let per_server_song_target =
        ((desired_song_count as usize).max(30) / active_servers.len()).max(20);
    let per_server_album_count = ((per_server_song_target / 8).clamp(4, 12)) as u32;

    let tasks = active_servers.iter().cloned().map(|server| {
        let album_type = album_type.to_string();
        async move {
            let client = NavidromeClient::new(server.clone());
            let albums = client
                .get_albums(&album_type, per_server_album_count, 0)
                .await
                .unwrap_or_default();
            let album_ids: Vec<String> = albums
                .into_iter()
                .take(per_server_album_count as usize)
                .map(|album| album.id)
                .collect();

            let album_tasks = album_ids.into_iter().map({
                let server = server.clone();
                move |album_id| {
                    let server = server.clone();
                    async move {
                        let client = NavidromeClient::new(server);
                        match client.get_album(&album_id).await {
                            Ok((_album, songs)) => songs,
                            Err(_) => Vec::new(),
                        }
                    }
                }
            });

            let mut songs: Vec<Song> = join_all(album_tasks).await.into_iter().flatten().collect();
            songs.truncate(per_server_song_target);
            songs
        }
    });

    let mut songs: Vec<Song> = join_all(tasks).await.into_iter().flatten().collect();
    let mut seen = HashSet::new();
    songs.retain(|song| seen.insert(song.id.clone()));
    songs
}

#[component]
pub fn SongsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();

    let mut search_query = use_signal(String::new);
    let mut sort_by = use_signal(|| "last_played".to_string());
    let mut sort_order = use_signal(|| "desc".to_string());
    let mut filter_min_rating = use_signal(|| 0i32);
    let rating_overrides = use_signal(HashMap::<String, u32>::new);
    let limit = use_signal(|| 30u32);

    let songs = use_resource(move || {
        let servers = servers();
        let limit = limit();
        let sort_option_snapshot = sort_by();
        let query_snapshot = search_query();
        async move {
            let mut songs = Vec::new();
            let active_servers: Vec<ServerConfig> =
                servers.into_iter().filter(|s| s.active).collect();
            let candidate_limit = limit.saturating_mul(3).clamp(45, 240);

            if query_snapshot.trim().is_empty() {
                match sort_option_snapshot.as_str() {
                    // Pull from server-side "recent" ordering first so last played is meaningful.
                    "last_played" => {
                        songs = fetch_songs_from_album_order(
                            &active_servers,
                            "recent",
                            candidate_limit,
                        )
                        .await;
                    }
                    // Pull from server-side "frequent" ordering first for most played mode.
                    "most_played" => {
                        songs = fetch_songs_from_album_order(
                            &active_servers,
                            "frequent",
                            candidate_limit,
                        )
                        .await;
                    }
                    // Use backend "highest" ordering as the best available source for ratings.
                    "rating" => {
                        songs = fetch_songs_from_album_order(
                            &active_servers,
                            "highest",
                            candidate_limit,
                        )
                        .await;
                    }
                    // Use backend alphabetical albums as a deterministic source for A-Z mode.
                    "alphabetical" => {
                        songs = fetch_songs_from_album_order(
                            &active_servers,
                            "alphabeticalByName",
                            candidate_limit,
                        )
                        .await;
                    }
                    _ => {}
                }

                if songs.is_empty() {
                    // Fallback: random pool when backend source is empty/unsupported.
                    let per_server =
                        ((candidate_limit as usize) / active_servers.len().max(1)).max(20) as u32;
                    for server in active_servers {
                        let client = NavidromeClient::new(server);
                        if let Ok(server_songs) = client.get_random_songs(per_server).await {
                            songs.extend(server_songs);
                        }
                    }
                }
            } else {
                // Search mode - query backend directly
                for server in active_servers {
                    let client = NavidromeClient::new(server);
                    if let Ok(results) = client
                        .search(&query_snapshot, 0, 0, candidate_limit + 20)
                        .await
                    {
                        songs.extend(results.songs);
                    }
                }
            }
            // Remove duplicates based on id
            let mut seen = HashSet::new();
            songs.retain(|song| seen.insert(song.id.clone()));
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
                                option { value: "most_played", "Most Played" }
                                option { value: "alphabetical", "Alphabetical" }
                                option { value: "rating", "Rating" }
                                option { value: "duration", "Duration" }
                            }
                        }

                        // Sort order
                        div { class: "flex items-center gap-1",
                            span { class: "text-xs text-zinc-400 whitespace-nowrap", "Order:" }
                            select {
                                class: "px-2 py-1 bg-zinc-800/50 border border-zinc-700/50 rounded text-xs text-white focus:outline-none focus:border-emerald-500/50 min-w-0",
                                value: sort_order,
                                oninput: move |e| sort_order.set(e.value()),
                                option { value: "desc", "Descending" }
                                option { value: "asc", "Ascending" }
                            }
                        }

                        if sort_by() == "rating" {
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
            }
            {
                match songs() {
                    Some(songs) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let sort_option = sort_by();
                        let sort_ascending = sort_order() == "asc";
                        let rating_filter_active = sort_option == "rating";
                        let rating_snapshot = rating_overrides();
                        let min_rating = filter_min_rating();
                        let display_limit = limit() as usize;
                        let source_song_count = songs.len();
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
                        if rating_filter_active && min_rating > 0 {
                            filtered
                                .retain(|song| {
                                    effective_song_rating(song, &rating_snapshot) >= min_rating as u32
                                });
                        }
                        match sort_option.as_str() {
                            "last_played" => {
                                filtered.sort_by(|a, b| {
                                    compare_song_last_played(a, b, sort_ascending)
                                });
                            }
                            "most_played" => {
                                filtered.sort_by(|a, b| {
                                    compare_song_most_played(a, b, sort_ascending)
                                });
                            }
                            "rating" => {
                                filtered
                                    .sort_by(|a, b| {
                                        let a_rating = effective_song_rating(a, &rating_snapshot);
                                        let b_rating = effective_song_rating(b, &rating_snapshot);
                                        if sort_ascending {
                                            a_rating.cmp(&b_rating)
                                        } else {
                                            b_rating.cmp(&a_rating)
                                        }
                                        .then_with(|| {
                                            compare_song_title(a, b, sort_ascending)
                                        })
                                    });
                            }
                            "duration" => {
                                filtered.sort_by(|a, b| {
                                    if sort_ascending {
                                        a.duration.cmp(&b.duration)
                                    } else {
                                        b.duration.cmp(&a.duration)
                                    }
                                    .then_with(|| {
                                        compare_song_title(a, b, sort_ascending)
                                    })
                                });
                            }
                            _ => {
                                filtered
                                    .sort_by(|a, b| compare_song_title(a, b, sort_ascending));
                            }
                        }
                        let has_query = !query.is_empty() || (rating_filter_active && min_rating > 0);
                        let has_more = filtered.len() > display_limit || source_song_count >= display_limit;
                        filtered.truncate(display_limit);
                        let on_rating_changed = {
                            let mut rating_overrides = rating_overrides.clone();
                            let mut queue = queue.clone();
                            move |(song_id, new_rating): (String, u32)| {
                                let normalized = new_rating.min(5);
                                rating_overrides.with_mut(|overrides| {
                                    if normalized == 0 {
                                        overrides.remove(&song_id);
                                    } else {
                                        overrides.insert(song_id.clone(), normalized);
                                    }
                                });
                                queue.with_mut(|items| {
                                    for song in items.iter_mut() {
                                        if song.id == song_id {
                                            song.user_rating = if normalized == 0 {
                                                None
                                            } else {
                                                Some(normalized)
                                            };
                                        }
                                    }
                                });
                            }
                        };
                        rsx! {
                            if filtered.is_empty() {
                                div { class: "flex flex-col items-center justify-center py-20",
                                    Icon {
                                        name: "music".to_string(),
                                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                    }
                                    if has_query {
                                        if raw_query.is_empty() {
                                            p { class: "text-zinc-300", "No songs match the selected filters" }
                                        } else {
                                            p { class: "text-zinc-300", "No songs match \"{raw_query}\"" }
                                        }
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
                                            effective_rating: effective_song_rating(song, &rating_snapshot),
                                            on_rating_changed: on_rating_changed.clone(),
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
                                if has_more {
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

fn parse_played_timestamp(value: &str) -> Option<i64> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }

    None
}

fn effective_song_rating(song: &Song, overrides: &HashMap<String, u32>) -> u32 {
    overrides
        .get(&song.id)
        .copied()
        .unwrap_or(song.user_rating.unwrap_or(0))
        .min(5)
}

fn compare_song_title(left: &Song, right: &Song, ascending: bool) -> Ordering {
    let left_title = left.title.to_lowercase();
    let right_title = right.title.to_lowercase();
    if ascending {
        left_title.cmp(&right_title)
    } else {
        right_title.cmp(&left_title)
    }
}

fn compare_optional_i64(left: Option<i64>, right: Option<i64>, ascending: bool) -> Ordering {
    match (left, right) {
        (Some(l), Some(r)) => {
            if ascending {
                l.cmp(&r)
            } else {
                r.cmp(&l)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_optional_u32(left: Option<u32>, right: Option<u32>, ascending: bool) -> Ordering {
    match (left, right) {
        (Some(l), Some(r)) => {
            if ascending {
                l.cmp(&r)
            } else {
                r.cmp(&l)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_song_last_played(left: &Song, right: &Song, ascending: bool) -> Ordering {
    let left_played = left.played.as_deref().and_then(parse_played_timestamp);
    let right_played = right.played.as_deref().and_then(parse_played_timestamp);

    compare_optional_i64(left_played, right_played, ascending)
        .then_with(|| compare_optional_u32(left.play_count, right.play_count, ascending))
        .then_with(|| compare_song_title(left, right, ascending))
}

fn compare_song_most_played(left: &Song, right: &Song, ascending: bool) -> Ordering {
    let left_played = left.played.as_deref().and_then(parse_played_timestamp);
    let right_played = right.played.as_deref().and_then(parse_played_timestamp);

    compare_optional_u32(left.play_count, right.play_count, ascending)
        .then_with(|| compare_optional_i64(left_played, right_played, ascending))
        .then_with(|| compare_song_title(left, right, ascending))
}

#[component]
fn SongRowWithRating(
    song: Song,
    index: usize,
    effective_rating: u32,
    onclick: EventHandler<MouseEvent>,
    on_rating_changed: EventHandler<(String, u32)>,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();

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
        let on_rating_changed = on_rating_changed.clone();
        move |new_rating: u32| {
            let normalized = new_rating.min(5);
            on_rating_changed.call((song_id.clone(), normalized));
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let _ = client.set_rating(&song_id, normalized).await;
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
                                let rating_value = i as u32;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    on_set_rating(rating_value);
                                }
                            },
                            Icon {
                                name: if i <= effective_rating {
                                    "star-filled".to_string()
                                } else {
                                    "star".to_string()
                                },
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
