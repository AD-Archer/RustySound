use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::diagnostics::log_perf;
use dioxus::prelude::*;
use std::time::Instant;

async fn fetch_albums_for_servers(
    active_servers: &[ServerConfig],
    album_type: &str,
    limit: u32,
) -> Vec<Album> {
    let mut albums = Vec::<Album>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_albums(album_type, limit, 0)
            .await
            .unwrap_or_default();

        // Retry once to smooth transient issues (notably on mobile/webview clients).
        if fetched.is_empty() {
            home_fetch_yield().await;
            fetched = client
                .get_albums(album_type, limit, 0)
                .await
                .unwrap_or_default();
        }

        albums.append(&mut fetched);
        if albums.len() >= limit as usize {
            break;
        }
        home_fetch_yield().await;
    }
    albums.truncate(limit as usize);
    albums
}

fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn dedupe_songs(songs: Vec<Song>, limit: usize) -> Vec<Song> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in songs {
        let key = song_key(&song);
        if seen.insert(key) {
            output.push(song);
        }
        if output.len() >= limit {
            break;
        }
    }
    output
}

async fn prefetch_lrclib_lyrics_for_songs(songs: Vec<Song>, limit: usize) {
    if limit == 0 || songs.is_empty() {
        return;
    }

    let providers = vec!["lrclib".to_string()];
    let mut seen = std::collections::HashSet::<String>::new();
    let mut prefetched = 0usize;
    let start = Instant::now();

    for song in songs {
        let key = song_key(&song);
        if !seen.insert(key) {
            continue;
        }
        if song.title.trim().is_empty() {
            continue;
        }

        let query = LyricsQuery::from_song(&song);
        let _ = fetch_lyrics_with_fallback(&query, &providers, 4).await;
        prefetched += 1;
        if prefetched >= limit {
            break;
        }
        home_fetch_yield().await;
    }

    log_perf(
        "home.lyrics_prefetch",
        start,
        &format!("prefetched={prefetched}"),
    );
}

async fn fetch_random_songs_for_servers(active_servers: &[ServerConfig], limit: u32) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client.get_random_songs(limit).await.unwrap_or_default();
        if fetched.is_empty() {
            home_fetch_yield().await;
            fetched = client.get_random_songs(limit).await.unwrap_or_default();
        }
        songs.append(&mut fetched);
        if songs.len() >= limit as usize {
            break;
        }
        home_fetch_yield().await;
    }
    dedupe_songs(songs, limit as usize)
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_native_activity_songs_for_servers(
    active_servers: &[ServerConfig],
    sort: NativeSongSortField,
    limit: u32,
) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    let end = limit.saturating_sub(1) as usize;

    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_native_songs(sort, NativeSortOrder::Desc, 0, end)
            .await
            .unwrap_or_default();

        // Retry once for intermittent native API auth/network hiccups.
        if fetched.is_empty() {
            home_fetch_yield().await;
            fetched = client
                .get_native_songs(sort, NativeSortOrder::Desc, 0, end)
                .await
                .unwrap_or_default();
        }

        songs.append(&mut fetched);
        if songs.len() >= limit as usize {
            break;
        }
        home_fetch_yield().await;
    }

    dedupe_songs(songs, limit as usize)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_native_activity_songs_for_servers(
    active_servers: &[ServerConfig],
    _sort: NativeSongSortField,
    limit: u32,
) -> Vec<Song> {
    // Native API endpoints are expensive/unstable on web clients.
    // Use regular Subsonic random songs for fast, reliable home loading.
    fetch_random_songs_for_servers(active_servers, limit).await
}

async fn fetch_similar_songs_for_seeds(
    active_servers: &[ServerConfig],
    seeds: &[Song],
    per_seed: u32,
    total_limit: usize,
) -> Vec<Song> {
    if seeds.is_empty() || per_seed == 0 {
        return Vec::new();
    }

    let seed_keys = seeds.iter().map(song_key).collect::<std::collections::HashSet<_>>();
    let mut similar = Vec::<Song>::new();

    for seed in seeds.iter().take(8).cloned() {
        let Some(server) = active_servers.iter().find(|s| s.id == seed.server_id).cloned() else {
            continue;
        };

        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_similar_songs2(&seed.id, per_seed)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            fetched = client
                .get_similar_songs(&seed.id, per_seed)
                .await
                .unwrap_or_default();
        }
        similar.append(&mut fetched);
        home_fetch_yield().await;
    }

    similar.retain(|song| !seed_keys.contains(&song_key(song)));

    dedupe_songs(similar, total_limit)
}

#[cfg(not(target_arch = "wasm32"))]
async fn build_quick_picks_mix(
    active_servers: &[ServerConfig],
    most_played_songs: &[Song],
    limit: usize,
) -> Vec<Song> {
    if limit == 0 {
        return Vec::new();
    }

    let anchors = dedupe_songs(most_played_songs.to_vec(), 8);
    let similar = fetch_similar_songs_for_seeds(active_servers, &anchors, 4, limit * 3).await;
    let random =
        fetch_random_songs_for_servers(active_servers, (limit as u32).saturating_mul(2)).await;

    let mut anchor_iter = anchors.into_iter();
    let mut similar_iter = similar.into_iter();
    let mut random_iter = random.into_iter();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut mixed = Vec::<Song>::new();

    loop {
        let mut progressed = false;

        if let Some(song) = anchor_iter.next() {
            progressed = true;
            let key = song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit {
            break;
        }

        if let Some(song) = similar_iter.next() {
            progressed = true;
            let key = song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit {
            break;
        }

        if let Some(song) = random_iter.next() {
            progressed = true;
            let key = song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit || !progressed {
            break;
        }
    }

    mixed.truncate(limit);
    mixed
}

#[cfg(target_arch = "wasm32")]
async fn build_quick_picks_mix(
    active_servers: &[ServerConfig],
    _most_played_songs: &[Song],
    limit: usize,
) -> Vec<Song> {
    if limit == 0 {
        return Vec::new();
    }
    fetch_random_songs_for_servers(active_servers, limit as u32).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn home_fetch_yield() {
    tokio::task::yield_now().await;
}

#[cfg(target_arch = "wasm32")]
async fn home_fetch_yield() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let recent_albums = use_signal(|| None::<Vec<Album>>);
    let most_played_albums = use_signal(|| None::<Vec<Album>>);
    let recently_played_songs = use_signal(|| None::<Vec<Song>>);
    let most_played_songs = use_signal(|| None::<Vec<Song>>);
    let random_songs = use_signal(|| None::<Vec<Song>>);
    let quick_picks = use_signal(|| None::<Vec<Song>>);
    let load_generation = use_signal(|| 0u64);

    {
        let servers = servers.clone();
        let mut recent_albums = recent_albums.clone();
        let mut most_played_albums = most_played_albums.clone();
        let mut recently_played_songs = recently_played_songs.clone();
        let mut most_played_songs = most_played_songs.clone();
        let mut random_songs = random_songs.clone();
        let mut quick_picks = quick_picks.clone();
        let mut load_generation = load_generation.clone();

        use_effect(move || {
            let active_servers: Vec<ServerConfig> =
                servers().into_iter().filter(|s| s.active).collect();
            let mut cache_server_ids: Vec<String> = active_servers
                .iter()
                .map(|server| server.id.clone())
                .collect();
            cache_server_ids.sort();
            let cache_prefix = format!("view:home:v1:{}", cache_server_ids.join("|"));
            let recent_cache_key = format!("{cache_prefix}:recent_albums");
            let most_played_album_cache_key = format!("{cache_prefix}:most_played_albums");
            let recent_song_cache_key = format!("{cache_prefix}:recent_songs");
            let most_played_song_cache_key = format!("{cache_prefix}:most_played_songs");
            let random_song_cache_key = format!("{cache_prefix}:random_songs");
            let quick_pick_cache_key = format!("{cache_prefix}:quick_picks");

            load_generation.with_mut(|value| *value = value.saturating_add(1));
            let generation = *load_generation.peek();

            recent_albums.set(cache_get_json::<Vec<Album>>(&recent_cache_key));
            most_played_albums.set(cache_get_json::<Vec<Album>>(&most_played_album_cache_key));
            recently_played_songs.set(cache_get_json::<Vec<Song>>(&recent_song_cache_key));
            most_played_songs.set(cache_get_json::<Vec<Song>>(&most_played_song_cache_key));
            random_songs.set(cache_get_json::<Vec<Song>>(&random_song_cache_key));
            quick_picks.set(cache_get_json::<Vec<Song>>(&quick_pick_cache_key));

            if active_servers.is_empty() {
                recent_albums.set(Some(Vec::new()));
                most_played_albums.set(Some(Vec::new()));
                recently_played_songs.set(Some(Vec::new()));
                most_played_songs.set(Some(Vec::new()));
                random_songs.set(Some(Vec::new()));
                quick_picks.set(Some(Vec::new()));
                return;
            }

            spawn(async move {
                let total_start = Instant::now();

                let recent_start = Instant::now();
                let recent = fetch_albums_for_servers(&active_servers, "newest", 12).await;
                let _ = cache_put_json(recent_cache_key.clone(), &recent, Some(6));
                log_perf(
                    "home.recent_albums",
                    recent_start,
                    &format!("count={}", recent.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                recent_albums.set(Some(recent));
                home_fetch_yield().await;

                let most_played_start = Instant::now();
                let most_played = fetch_albums_for_servers(&active_servers, "frequent", 12).await;
                let _ = cache_put_json(most_played_album_cache_key.clone(), &most_played, Some(6));
                log_perf(
                    "home.most_played_albums",
                    most_played_start,
                    &format!("count={}", most_played.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                most_played_albums.set(Some(most_played));
                home_fetch_yield().await;

                let recent_played_start = Instant::now();
                let recent_played = fetch_native_activity_songs_for_servers(
                    &active_servers,
                    NativeSongSortField::PlayDate,
                    12,
                )
                .await;
                let _ = cache_put_json(recent_song_cache_key.clone(), &recent_played, Some(3));
                log_perf(
                    "home.recently_played_songs",
                    recent_played_start,
                    &format!("count={}", recent_played.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                recently_played_songs.set(Some(recent_played.clone()));
                home_fetch_yield().await;

                let most_played_song_start = Instant::now();
                let most_played_song_items = fetch_native_activity_songs_for_servers(
                    &active_servers,
                    NativeSongSortField::PlayCount,
                    12,
                )
                .await;
                let _ = cache_put_json(
                    most_played_song_cache_key.clone(),
                    &most_played_song_items,
                    Some(6),
                );
                log_perf(
                    "home.most_played_songs",
                    most_played_song_start,
                    &format!("count={}", most_played_song_items.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                most_played_songs.set(Some(most_played_song_items.clone()));
                home_fetch_yield().await;

                let random_start = Instant::now();
                let random_song_items = fetch_random_songs_for_servers(&active_servers, 16).await;
                let _ = cache_put_json(random_song_cache_key.clone(), &random_song_items, Some(2));
                log_perf(
                    "home.random_songs",
                    random_start,
                    &format!("count={}", random_song_items.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                random_songs.set(Some(random_song_items));
                home_fetch_yield().await;

                let quick_pick_start = Instant::now();
                let mut quick =
                    build_quick_picks_mix(&active_servers, &most_played_song_items, 12).await;
                if quick.is_empty() {
                    quick = fetch_random_songs_for_servers(&active_servers, 12).await;
                }
                let _ = cache_put_json(quick_pick_cache_key.clone(), &quick, Some(3));
                log_perf(
                    "home.quick_picks",
                    quick_pick_start,
                    &format!("count={}", quick.len()),
                );
                if *load_generation.peek() != generation {
                    return;
                }
                quick_picks.set(Some(quick));

                log_perf(
                    "home.total",
                    total_start,
                    &format!("servers={}", active_servers.len()),
                );

                let mut lyrics_seeds = most_played_song_items.clone();
                lyrics_seeds.extend(recent_played.into_iter());
                spawn(async move {
                    prefetch_lrclib_lyrics_for_songs(lyrics_seeds, 8).await;
                });
            });
        });
    }

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
                            move |_| nav.navigate_to(AppView::SettingsView {})
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
                            move |_| nav.navigate_to(AppView::RandomView {})
                        },
                    }
                    QuickPlayCard {
                        title: "All Songs".to_string(),
                        gradient: "from-sky-600 to-cyan-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::SongsView {})
                        },
                    }
                    QuickPlayCard {
                        title: "Favorites".to_string(),
                        gradient: "from-rose-600 to-pink-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::FavoritesView {})
                        },
                    }
                    QuickPlayCard {
                        title: "Bookmarks".to_string(),
                        gradient: "from-indigo-500 to-blue-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::BookmarksView {})
                        },
                    }
                    QuickPlayCard {
                        title: "Radio Stations".to_string(),
                        gradient: "from-emerald-600 to-teal-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::RadioView {})
                        },
                    }
                    QuickPlayCard {
                        title: "All Albums".to_string(),
                        gradient: "from-amber-600 to-orange-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Albums {})
                        },
                    }
                    QuickPlayCard {
                        title: "Playlists".to_string(),
                        gradient: "from-amber-600 to-orange-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::PlaylistsView {})
                        },
                    }
                    QuickPlayCard {
                        title: "Artists".to_string(),
                        gradient: "from-purple-600 to-indigo-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::ArtistsView {})
                        },
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
                                move |_| nav.navigate_to(AppView::Albums {})
                            },
                            "See all"
                        }
                    }

                    {
                        match recent_albums() {
                            Some(albums) => rsx! {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for album in albums {
                                            div { class: "w-32 flex-shrink-0",
                                                AlbumCard {
                                                    album: album.clone(),
                                                    onclick: {
                                                        let navigation = navigation.clone();
                                                        let album_id = album.id.clone();
                                                        let album_server_id = album.server_id.clone();
                                                        move |_| {
                                                            navigation
                                                                .navigate_to(AppView::AlbumDetailView {
                                                                    album_id: album_id.clone(),
                                                                    server_id: album_server_id.clone(),
                                                                })
                                                        }
                                                    },
                                                }
                                            }
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
                                move |_| nav.navigate_to(AppView::Albums {})
                            },
                            "See all"
                        }
                    }

                    {
                        match most_played_albums() {
                            Some(albums) => rsx! {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for album in albums {
                                            div { class: "w-32 flex-shrink-0",
                                                AlbumCard {
                                                    album: album.clone(),
                                                    onclick: {
                                                        let navigation = navigation.clone();
                                                        let album_id = album.id.clone();
                                                        let album_server_id = album.server_id.clone();
                                                        move |_| {
                                                            navigation
                                                                .navigate_to(AppView::AlbumDetailView {
                                                                    album_id: album_id.clone(),
                                                                    server_id: album_server_id.clone(),
                                                                })
                                                        }
                                                    },
                                                }
                                            }
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

                // Most played songs
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Most Played Songs" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::SongsView {})
                            },
                            "See all"
                        }
                    }

                    {
                        match most_played_songs() {
                            Some(songs) => rsx! {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for (index , song) in songs.iter().enumerate() {
                                            SongCard {
                                                song: song.clone(),
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

                // Last played songs
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Last Played Songs" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::SongsView {})
                            },
                            "See all"
                        }
                    }

                    {
                        match recently_played_songs() {
                            Some(songs) => rsx! {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for (index , song) in songs.iter().enumerate() {
                                            SongCard {
                                                song: song.clone(),
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

                // Random songs
                section { class: "mb-8",
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Random Songs" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::SongsView {})
                            },
                            "See all"
                        }
                    }

                    {
                        match random_songs() {
                            Some(songs) => rsx! {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for (index , song) in songs.iter().enumerate() {
                                            SongCard {
                                                song: song.clone(),
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

                // Quick picks (mixed: most played + similar + random)
                section {
                    div { class: "flex items-center justify-between mb-4",
                        h2 { class: "text-xl font-semibold text-white", "Quick Picks" }
                        button {
                            class: "text-sm text-zinc-400 hover:text-white transition-colors",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::SongsView {})
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
fn SongCard(song: Song, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
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

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                add_menu.open(AddIntent::from_song(song.clone()));
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
            class: "group text-left cursor-pointer flex-shrink-0 w-32",
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
                    onclick: make_on_open_menu(),
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
                    onclick: make_on_open_menu(),
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
    let add_menu = use_context::<AddMenuController>();

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

    let on_open_menu = {
        let mut add_menu = add_menu.clone();
        let album = album.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            add_menu.open(AddIntent::from_album(&album));
        }
    };

    let on_artist_click = {
        let artist_id = album.artist_id.clone();
        let server_id = album.server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(artist_id) = artist_id.clone() {
                navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    rsx! {
        div {
            class: "group text-left cursor-pointer w-full max-w-48 overflow-hidden relative",
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
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/80 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10",
                    aria_label: "Add album to queue",
                    onclick: on_open_menu,
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
    let add_menu = use_context::<AddMenuController>();
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
