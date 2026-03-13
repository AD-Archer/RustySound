use crate::api::*;
use crate::components::{
    ios_audio_log_snapshot, ios_diag_log, AddIntent, AddMenuController, AppView, HomeFeedState,
    Icon, Navigation,
};
use crate::db::AppSettings;
use crate::offline_audio::{is_song_downloaded, prefetch_song_audio};
use dioxus::prelude::*;

const HOME_SECTION_BASE_COUNT: usize = 9;
const HOME_SECTION_LOAD_STEP: usize = 6;
const HOME_LOADING_FORCE_UNBLOCK_MS: u64 = 12_000;

fn loading_progress_percent(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * 100.0).round() as u32
}

#[component]
fn LoadingProgressBar(progress: f32, stage: String) -> Element {
    let percent = loading_progress_percent(progress);
    rsx! {
        div { class: "w-full space-y-2",
            div { class: "flex items-center justify-between gap-3 text-xs text-zinc-500",
                p { class: "truncate text-left", "{stage}" }
                p { class: "shrink-0 font-medium text-zinc-400", "{percent}%" }
            }
            div { class: "h-2 overflow-hidden rounded-full bg-zinc-800/80",
                div {
                    class: "h-full rounded-full bg-gradient-to-r from-emerald-500 via-emerald-400 to-teal-300 transition-[width] duration-500 ease-out",
                    style: format!("width: {percent}%;"),
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn home_loading_log_poll_sleep() {
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
}

#[cfg(target_arch = "wasm32")]
async fn home_loading_log_poll_sleep() {
    gloo_timers::future::TimeoutFuture::new(350).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn home_now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn home_now_millis() -> u64 {
    js_sys::Date::now().max(0.0).round() as u64
}

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let home_feed = use_context::<HomeFeedState>();
    let recent_albums = home_feed.recent_albums;
    let most_played_albums = home_feed.most_played_albums;
    let recently_played_songs = home_feed.recently_played_songs;
    let most_played_songs = home_feed.most_played_songs;
    let random_songs = home_feed.random_songs;
    let quick_picks = home_feed.quick_picks;
    let home_loading_progress = home_feed.progress;
    let home_loading_status = home_feed.status;
    let mut ios_loading_log_lines = use_signal(Vec::<String>::new);
    let mut ios_loading_log_poll_generation = use_signal(|| 0u64);
    let mut home_loading_started_at_ms = use_signal(|| None::<u64>);
    let mut home_loading_elapsed_ms = use_signal(|| 0u64);
    let mut home_loading_force_unblocked = use_signal(|| false);
    let mut most_played_album_visible = use_signal(|| HOME_SECTION_BASE_COUNT);
    let mut most_played_song_visible = use_signal(|| HOME_SECTION_BASE_COUNT);
    let mut last_played_song_visible = use_signal(|| HOME_SECTION_BASE_COUNT);
    let mut random_song_visible = use_signal(|| HOME_SECTION_BASE_COUNT);

    use_effect(move || {
        ios_diag_log("home.view.mount", "HomeView mounted");
    });

    let has_servers = servers().iter().any(|s| s.active);
    let is_home_album_loading =
        has_servers && (recent_albums().is_none() || most_played_albums().is_none());
    let show_home_album_overlay = is_home_album_loading && !home_loading_force_unblocked();
    let show_ios_loading_logs = cfg!(all(not(target_arch = "wasm32"), target_os = "ios"));

    use_effect(move || {
        let recent_album_count = recent_albums().as_ref().map(Vec::len);
        let most_played_album_count = most_played_albums().as_ref().map(Vec::len);
        let recent_song_count = recently_played_songs().as_ref().map(Vec::len);
        let most_played_song_count = most_played_songs().as_ref().map(Vec::len);
        let random_song_count = random_songs().as_ref().map(Vec::len);
        let quick_pick_count = quick_picks().as_ref().map(Vec::len);

        ios_diag_log(
            "home.view.state",
            &format!(
                "servers_active={} overlay={} recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
                has_servers,
                show_home_album_overlay,
                recent_album_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                most_played_album_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                recent_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                most_played_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                random_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                quick_pick_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
            ),
        );
    });

    use_effect(move || {
        if !is_home_album_loading {
            home_loading_started_at_ms.set(None);
            home_loading_elapsed_ms.set(0);
            home_loading_force_unblocked.set(false);
            return;
        }

        if home_loading_started_at_ms().is_some() {
            return;
        }

        let started_at = home_now_millis();
        home_loading_started_at_ms.set(Some(started_at));
        home_loading_elapsed_ms.set(0);
        home_loading_force_unblocked.set(false);
        ios_diag_log(
            "home.view.load",
            &format!("album loading overlay shown at={started_at}"),
        );
    });

    use_effect(move || {
        if !cfg!(all(not(target_arch = "wasm32"), target_os = "ios")) {
            ios_loading_log_lines.set(Vec::new());
            return;
        }

        if !is_home_album_loading {
            ios_loading_log_lines.set(Vec::new());
            ios_loading_log_poll_generation
                .with_mut(|generation| *generation = generation.saturating_add(1));
            return;
        }

        ios_loading_log_poll_generation
            .with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *ios_loading_log_poll_generation.peek();
        let mut ios_loading_log_lines = ios_loading_log_lines.clone();
        let ios_loading_log_poll_generation = ios_loading_log_poll_generation.clone();
        let mut home_loading_started_at_ms = home_loading_started_at_ms.clone();
        let mut home_loading_elapsed_ms = home_loading_elapsed_ms.clone();
        let mut home_loading_force_unblocked = home_loading_force_unblocked.clone();
        spawn(async move {
            loop {
                ios_loading_log_lines.set(ios_audio_log_snapshot(16));
                let started_at_ms = match home_loading_started_at_ms() {
                    Some(value) => value,
                    None => {
                        let now = home_now_millis();
                        home_loading_started_at_ms.set(Some(now));
                        now
                    }
                };
                let elapsed_ms = home_now_millis().saturating_sub(started_at_ms);
                home_loading_elapsed_ms.set(elapsed_ms);

                if !home_loading_force_unblocked() && elapsed_ms >= HOME_LOADING_FORCE_UNBLOCK_MS {
                    home_loading_force_unblocked.set(true);
                    ios_diag_log(
                        "home.view.load",
                        &format!(
                            "force dismiss album loading overlay after {elapsed_ms}ms timeout"
                        ),
                    );
                }
                home_loading_log_poll_sleep().await;
                if *ios_loading_log_poll_generation.peek() != generation {
                    break;
                }
            }
        });
    });

    let ios_loading_logs_preview = ios_loading_log_lines();
    let home_loading_elapsed_ms = home_loading_elapsed_ms();
    let home_loading_progress_value = home_loading_progress();
    let home_loading_status_text =
        home_loading_status().unwrap_or_else(|| "Loading Home feed".to_string());
    let home_loading_recent_count_value = recent_albums().as_ref().map(Vec::len);
    let home_loading_most_played_count_value = most_played_albums().as_ref().map(Vec::len);
    let home_loading_error_text = None::<String>;
    let on_unblock_home_loading = {
        let mut home_loading_force_unblocked = home_loading_force_unblocked.clone();
        move |_| {
            home_loading_force_unblocked.set(true);
            ios_diag_log("home.view.load", "manual dismiss of album loading overlay");
        }
    };
    rsx! {
        div { class: "space-y-8 max-w-none",
            if show_home_album_overlay {
                div { class: "fixed inset-0 z-[210] bg-zinc-950/95 backdrop-blur-sm overflow-y-auto px-6 py-8 flex items-center justify-center",
                    div { class: "w-full max-w-lg text-center space-y-4 rounded-2xl border border-zinc-700/70 bg-zinc-950/95 px-5 py-5 shadow-2xl",
                        div { class: "flex items-center justify-center",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-10 h-10 text-emerald-400 animate-spin".to_string(),
                            }
                        }
                        h2 { class: "text-xl font-semibold text-white", "Loading Home" }
                        p { class: "text-sm text-zinc-400",
                            "Fetching albums and preparing your home feed."
                        }
                        LoadingProgressBar {
                            progress: home_loading_progress_value,
                            stage: home_loading_status_text,
                        }
                        div { class: "grid grid-cols-2 gap-3 text-left",
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500", "Recent Albums" }
                                p { class: "text-sm font-medium text-white",
                                    match home_loading_recent_count_value {
                                        Some(count) => format!("{count}"),
                                        None => "Pending".to_string(),
                                    }
                                }
                            }
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500", "Most Played Albums" }
                                p { class: "text-sm font-medium text-white",
                                    match home_loading_most_played_count_value {
                                        Some(count) => format!("{count}"),
                                        None => "Pending".to_string(),
                                    }
                                }
                            }
                        }
                        p { class: "text-xs text-zinc-500",
                            "Elapsed: {home_loading_elapsed_ms} ms"
                        }
                        if let Some(error_text) = home_loading_error_text {
                            p { class: "text-xs text-amber-300", "{error_text}" }
                        }
                        button {
                            class: "mt-1 px-3 py-2 rounded-lg border border-zinc-600 text-zinc-200 hover:text-white hover:border-zinc-400 transition-colors text-sm",
                            onclick: on_unblock_home_loading,
                            "Continue without blocking"
                        }
                        if show_ios_loading_logs && !ios_loading_logs_preview.is_empty() {
                            div { class: "mt-3 text-left rounded-lg border border-zinc-700/70 bg-zinc-900/70 p-2 max-h-72 overflow-y-auto",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500 mb-1", "iOS Loading Log" }
                                for line in ios_loading_logs_preview.iter() {
                                    p { class: "text-[11px] leading-tight text-zinc-300 font-mono break-all", "{line}" }
                                }
                            }
                        }
                    }
                }
            }

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
                        title: "Downloads".to_string(),
                        gradient: "from-indigo-500 to-blue-600".to_string(),
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::DownloadsView {})
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
                            Some(albums) => {
                                let visible = most_played_album_visible().min(albums.len());
                                let display: Vec<Album> =
                                    albums.iter().take(visible).cloned().collect();
                                rsx! {
                                    div { class: "overflow-x-auto",
                                        div { class: "flex gap-4 pb-2 min-w-min",
                                            for album in display {
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
                                            if albums.len() > visible {
                                                LoadMoreStripCard {
                                                    label: "Load 6 more".to_string(),
                                                    onclick: move |_| {
                                                        most_played_album_visible
                                                            .with_mut(|count| {
                                                                *count = count.saturating_add(HOME_SECTION_LOAD_STEP);
                                                            });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
                            Some(songs) => {
                                let visible = most_played_song_visible().min(songs.len());
                                let display: Vec<Song> =
                                    songs.iter().take(visible).cloned().collect();
                                rsx! {
                                    div { class: "overflow-x-auto",
                                        div { class: "flex gap-4 pb-2 min-w-min",
                                            for (index , song) in display.iter().enumerate() {
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
                                            if songs.len() > visible {
                                                LoadMoreStripCard {
                                                    label: "Load 6 more".to_string(),
                                                    onclick: move |_| {
                                                        most_played_song_visible
                                                            .with_mut(|count| {
                                                                *count = count.saturating_add(HOME_SECTION_LOAD_STEP);
                                                            });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
                            Some(songs) => {
                                let visible = last_played_song_visible().min(songs.len());
                                let display: Vec<Song> =
                                    songs.iter().take(visible).cloned().collect();
                                rsx! {
                                    div { class: "overflow-x-auto",
                                        div { class: "flex gap-4 pb-2 min-w-min",
                                            for (index , song) in display.iter().enumerate() {
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
                                            if songs.len() > visible {
                                                LoadMoreStripCard {
                                                    label: "Load 6 more".to_string(),
                                                    onclick: move |_| {
                                                        last_played_song_visible
                                                            .with_mut(|count| {
                                                                *count = count.saturating_add(HOME_SECTION_LOAD_STEP);
                                                            });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
                                move |_| nav.navigate_to(AppView::RandomView {})
                            },
                            "See all"
                        }
                    }

                    {
                        match random_songs() {
                            Some(songs) => {
                                let visible = random_song_visible().min(songs.len());
                                let display: Vec<Song> = songs.iter().take(visible).cloned().collect();
                                rsx! {
                                    div { class: "overflow-x-auto",
                                        div { class: "flex gap-4 pb-2 min-w-min",
                                            for (index , song) in display.iter().enumerate() {
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
                                            if songs.len() > visible {
                                                LoadMoreStripCard {
                                                    label: "Load 6 more".to_string(),
                                                    onclick: move |_| {
                                                        random_song_visible
                                                            .with_mut(|count| {
                                                                *count = count.saturating_add(HOME_SECTION_LOAD_STEP);
                                                            });
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
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
fn LoadMoreStripCard(label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "flex-shrink-0 w-32 aspect-square rounded-xl border border-dashed border-zinc-700 bg-zinc-900/30 hover:border-emerald-500/70 hover:bg-emerald-500/10 text-zinc-300 hover:text-white transition-colors flex flex-col items-center justify-center gap-2",
            onclick: move |evt| onclick.call(evt),
            Icon { name: "next".to_string(), class: "w-5 h-5".to_string() }
            span { class: "text-xs font-medium text-center px-2", "{label}" }
        }
    }
}

#[component]
fn SongCard(song: Song, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
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

    let album_id = song.album_id.clone();
    let server_id = song.server_id.clone();

    let on_album_click_artist = {
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
            if album_id.is_some() {
                button {
                    class: "text-xs text-zinc-400 truncate max-w-full text-left hover:text-emerald-400 transition-colors",
                    onclick: on_album_click_artist,
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
pub fn SongRow(
    song: Song,
    index: usize,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] show_download: bool,
    #[props(default = true)] show_duration: bool,
    #[props(default)] show_favorite_indicator: bool,
    #[props(default)] show_duration_in_menu: bool,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(|| song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let download_busy = use_signal(|| false);
    let mut show_mobile_actions = use_signal(|| false);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);

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

    let on_album_click_artist = {
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

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                add_menu.open(AddIntent::from_song(song.clone()));
            }
        }
    };

    let make_on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let current_rating = current_rating.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut current_rating = current_rating.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let normalized = new_rating.min(5);
                current_rating.set(normalized);
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
        }
    };

    let make_on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let queue = queue.clone();
        let is_favorited = is_favorited.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut queue = queue.clone();
            let mut is_favorited = is_favorited.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
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
        }
    };

    let make_on_download_song = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let song = song.clone();
        let download_busy = download_busy.clone();
        let downloaded = downloaded.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let app_settings = app_settings.clone();
            let song = song.clone();
            let mut download_busy = download_busy.clone();
            let mut downloaded = downloaded.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if download_busy() || downloaded() {
                    return;
                }
                let servers_snapshot = servers();
                if servers_snapshot.is_empty() {
                    return;
                }
                let mut settings_snapshot = app_settings();
                settings_snapshot.downloads_enabled = true;
                download_busy.set(true);
                let song = song.clone();
                spawn(async move {
                    if prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot)
                        .await
                        .is_ok()
                    {
                        downloaded.set(true);
                    }
                    download_busy.set(false);
                });
            }
        }
    };

    rsx! {
        div {
            class: "relative w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer",
            onclick: move |e| {
                show_mobile_actions.set(false);
                onclick.call(e);
            },
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
                if album_id.is_some() {
                    button {
                        class: "text-xs text-zinc-400 truncate hover:text-emerald-400 transition-colors text-left",
                        onclick: on_album_click_artist,
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
            div { class: "flex items-center gap-2 md:gap-3 relative",
                button {
                    class: "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: make_on_open_menu(),
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                if show_download {
                    if downloaded() {
                        span { class: "hidden md:inline-flex text-emerald-400", title: "Downloaded",
                            Icon {
                                name: "check".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    } else {
                        button {
                            class: if download_busy() { "hidden md:inline-flex p-2 rounded-lg text-zinc-500 cursor-not-allowed" } else { "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors" },
                            aria_label: "Download song",
                            disabled: download_busy(),
                            onclick: make_on_download_song(),
                            Icon {
                                name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    }
                }
                if current_rating() > 0 {
                    div { class: "hidden md:flex items-center gap-1 text-amber-400",
                        for i in 1..=5 {
                            Icon {
                                name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
                button {
                    class: if is_favorited() { "hidden md:inline-flex p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "hidden md:inline-flex p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                if show_favorite_indicator {
                    span {
                        class: if is_favorited() { "text-emerald-400" } else { "text-zinc-500" },
                        title: if is_favorited() { "Favorited" } else { "Not favorited" },
                        Icon {
                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                    }
                } else if show_duration {
                    span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
                }
                button {
                    class: "md:hidden p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors",
                    aria_label: "Song actions",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_mobile_actions.set(!show_mobile_actions());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                if show_mobile_actions() {
                    div {
                        class: "md:hidden absolute right-0 top-10 z-20 w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_open_menu(),
                            Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                            "Add To..."
                        }
                        if show_download {
                            if downloaded() {
                                div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                                    Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                    "Downloaded"
                                }
                            } else {
                                button {
                                    class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                                    disabled: download_busy(),
                                    onclick: make_on_download_song(),
                                    Icon {
                                        name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                    if download_busy() {
                                        "Downloading..."
                                    } else {
                                        "Download"
                                    }
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if is_favorited() {
                                "Unfavorite"
                            } else {
                                "Favorite"
                            }
                        }
                        div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500", "Rating" }
                        div { class: "flex items-center gap-1 px-2 pb-1",
                            for i in 1..=5 {
                                button {
                                    class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                    onclick: make_on_set_rating(i as u32),
                                    Icon {
                                        name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                        class: "w-3.5 h-3.5".to_string(),
                                    }
                                }
                            }
                        }
                        if show_duration_in_menu {
                            div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500", "Length" }
                            p { class: "px-2.5 pb-2 text-xs text-zinc-300", "{format_duration(song.duration)}" }
                        }
                    }
                }
            }
        }
    }
}
