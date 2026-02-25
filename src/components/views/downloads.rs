use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::{AppView, Icon, Navigation};
use crate::db::{save_settings, AppSettings};
use crate::offline_audio::{
    clear_downloads, download_stats, list_active_downloads, list_downloaded_collections,
    list_downloaded_entries, refresh_downloaded_cache, remove_downloaded_album,
    remove_downloaded_collection, remove_downloaded_song, remove_downloaded_songs,
    run_auto_download_pass, ActiveDownloadEntry, DownloadCollectionEntry, DownloadIndexEntry,
};
use chrono::{DateTime, Local, Utc};
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1024.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.2} GB", mb / 1024.0)
    }
}

fn format_updated(ms: u64) -> String {
    let Some(dt_utc) = DateTime::<Utc>::from_timestamp_millis(ms as i64) else {
        return "-".to_string();
    };
    dt_utc
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

#[cfg(not(target_arch = "wasm32"))]
async fn download_poll_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
async fn download_poll_delay_ms(ms: u64) {
    gloo_timers::future::TimeoutFuture::new(ms as u32).await;
}

fn download_song_key(server_id: &str, song_id: &str) -> String {
    format!("{}::{}", server_id.trim(), song_id.trim())
}

fn infer_downloaded_albums(entries: &[DownloadIndexEntry]) -> Vec<DownloadCollectionEntry> {
    let mut map = HashMap::<(String, String), DownloadCollectionEntry>::new();
    for entry in entries {
        let Some(album_name) = entry.album.clone().filter(|name| !name.trim().is_empty()) else {
            continue;
        };
        let album_key = entry
            .album_id
            .clone()
            .unwrap_or_else(|| format!("name:{album_name}"));
        let key = (entry.server_id.clone(), album_key.clone());
        let updated_at_ms = entry.updated_at_ms;
        map.entry(key)
            .and_modify(|collection| {
                collection.song_count = collection.song_count.saturating_add(1);
                if updated_at_ms > collection.updated_at_ms {
                    collection.updated_at_ms = updated_at_ms;
                }
            })
            .or_insert(DownloadCollectionEntry {
                kind: "album".to_string(),
                server_id: entry.server_id.clone(),
                collection_id: album_key,
                name: album_name,
                song_count: 1,
                updated_at_ms,
            });
    }

    let mut values: Vec<DownloadCollectionEntry> = map.into_values().collect();
    values.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    values
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DownloadsTab {
    Songs,
    Albums,
    Playlists,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DownloadSongSort {
    Newest,
    Title,
    Artist,
    Album,
    Size,
}

const DOWNLOADS_POLL_IDLE_MS: u64 = 5000;
const DOWNLOADS_POLL_ACTIVE_MS: u64 = 1400;
const DOWNLOADS_SONG_PAGE_SIZE: usize = 80;
const DOWNLOADS_COLLECTION_PAGE_SIZE: usize = 60;

fn download_song_sort_key(sort: DownloadSongSort) -> &'static str {
    match sort {
        DownloadSongSort::Newest => "newest",
        DownloadSongSort::Title => "title",
        DownloadSongSort::Artist => "artist",
        DownloadSongSort::Album => "album",
        DownloadSongSort::Size => "size",
    }
}

fn parse_download_song_sort(value: &str) -> DownloadSongSort {
    match value {
        "title" => DownloadSongSort::Title,
        "artist" => DownloadSongSort::Artist,
        "album" => DownloadSongSort::Album,
        "size" => DownloadSongSort::Size,
        _ => DownloadSongSort::Newest,
    }
}

fn normalize_download_field(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|item| item.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

fn to_download_song(entry: &DownloadIndexEntry, servers: &[ServerConfig]) -> Song {
    let server_name = servers
        .iter()
        .find(|server| server.id == entry.server_id)
        .map(|server| server.name.clone())
        .or_else(|| {
            entry
                .server_name
                .clone()
                .filter(|name| !name.trim().is_empty())
        })
        .unwrap_or_else(|| "Offline".to_string());

    Song {
        id: entry.song_id.clone(),
        title: entry.title.clone(),
        album: entry.album.clone(),
        album_id: entry.album_id.clone(),
        artist: entry.artist.clone(),
        cover_art: entry
            .cover_art_id
            .clone()
            .or_else(|| entry.album_id.clone()),
        duration: 0,
        server_id: entry.server_id.clone(),
        server_name,
        ..Song::default()
    }
}

#[component]
pub fn DownloadsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let refresh_nonce = use_signal(|| 0u64);
    let action_busy = use_signal(|| false);
    let action_status = use_signal(|| None::<String>);
    let mut search_query = use_signal(String::new);
    let mut active_tab = use_signal(|| DownloadsTab::Songs);
    let selected_song_keys = use_signal(HashSet::<String>::new);
    let mut song_sort = use_signal(|| DownloadSongSort::Newest);
    let mut song_visible_limit = use_signal(|| DOWNLOADS_SONG_PAGE_SIZE);
    let mut album_visible_limit = use_signal(|| DOWNLOADS_COLLECTION_PAGE_SIZE);
    let mut playlist_visible_limit = use_signal(|| DOWNLOADS_COLLECTION_PAGE_SIZE);

    {
        let mut refresh_nonce = refresh_nonce.clone();
        use_effect(move || {
            spawn(async move {
                loop {
                    let wait_ms = if list_active_downloads().is_empty() {
                        DOWNLOADS_POLL_IDLE_MS
                    } else {
                        DOWNLOADS_POLL_ACTIVE_MS
                    };
                    download_poll_delay_ms(wait_ms).await;
                    refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                }
            });
        });
    }

    {
        let active_tab = active_tab.clone();
        let search_query = search_query.clone();
        let song_sort = song_sort.clone();
        let mut song_visible_limit = song_visible_limit.clone();
        let mut album_visible_limit = album_visible_limit.clone();
        let mut playlist_visible_limit = playlist_visible_limit.clone();
        use_effect(move || {
            let _ = active_tab();
            let _ = search_query();
            let _ = song_sort();
            song_visible_limit.set(DOWNLOADS_SONG_PAGE_SIZE);
            album_visible_limit.set(DOWNLOADS_COLLECTION_PAGE_SIZE);
            playlist_visible_limit.set(DOWNLOADS_COLLECTION_PAGE_SIZE);
        });
    }

    #[cfg(target_arch = "wasm32")]
    let native_downloads_supported = false;
    #[cfg(not(target_arch = "wasm32"))]
    let native_downloads_supported = true;

    let _refresh = refresh_nonce();
    let servers_snapshot = servers();
    let settings = app_settings();
    let stats = download_stats();
    let mut all_entries = list_downloaded_entries();
    all_entries.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    let active_downloads: Vec<ActiveDownloadEntry> = list_active_downloads();
    let collections = list_downloaded_collections();
    let downloaded_playlists: Vec<DownloadCollectionEntry> = collections
        .iter()
        .filter(|entry| entry.kind == "playlist")
        .cloned()
        .collect();
    let downloaded_albums: Vec<DownloadCollectionEntry> = {
        let mut from_collections: Vec<DownloadCollectionEntry> = collections
            .iter()
            .filter(|entry| entry.kind == "album")
            .cloned()
            .collect();
        if from_collections.is_empty() {
            from_collections = infer_downloaded_albums(&all_entries);
        }
        from_collections
    };
    let album_cover_ids = all_entries.iter().fold(
        HashMap::<(String, String), String>::new(),
        |mut map, entry| {
            if let Some(cover) = entry
                .cover_art_id
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                if let Some(album_id) = entry
                    .album_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                {
                    map.entry((entry.server_id.clone(), album_id.clone()))
                        .or_insert_with(|| cover.clone());
                }
            }
            map
        },
    );

    let query = search_query().trim().to_ascii_lowercase();
    let mut entries: Vec<DownloadIndexEntry> = if query.is_empty() {
        all_entries.clone()
    } else {
        all_entries
            .into_iter()
            .filter(|entry| {
                let title = entry.title.to_ascii_lowercase();
                let artist = normalize_download_field(&entry.artist);
                let album = normalize_download_field(&entry.album);
                title.contains(&query) || artist.contains(&query) || album.contains(&query)
            })
            .collect()
    };
    match song_sort() {
        DownloadSongSort::Newest => {
            entries.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
        }
        DownloadSongSort::Title => {
            entries.sort_by(|left, right| {
                left.title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase())
            });
        }
        DownloadSongSort::Artist => {
            entries.sort_by(|left, right| {
                normalize_download_field(&left.artist).cmp(&normalize_download_field(&right.artist))
            });
        }
        DownloadSongSort::Album => {
            entries.sort_by(|left, right| {
                normalize_download_field(&left.album).cmp(&normalize_download_field(&right.album))
            });
        }
        DownloadSongSort::Size => {
            entries.sort_by(|left, right| right.size_bytes.cmp(&left.size_bytes));
        }
    }

    let visible_song_count = song_visible_limit().min(entries.len());
    let visible_song_entries: Vec<DownloadIndexEntry> =
        entries.iter().take(visible_song_count).cloned().collect();
    let has_more_song_entries = entries.len() > visible_song_count;

    let mut filtered_albums: Vec<DownloadCollectionEntry> = if query.is_empty() {
        downloaded_albums.clone()
    } else {
        downloaded_albums
            .iter()
            .filter(|album| album.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    };
    filtered_albums.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    let visible_album_count = album_visible_limit().min(filtered_albums.len());
    let visible_albums: Vec<DownloadCollectionEntry> = filtered_albums
        .iter()
        .take(visible_album_count)
        .cloned()
        .collect();
    let has_more_albums = filtered_albums.len() > visible_album_count;

    let mut filtered_playlists: Vec<DownloadCollectionEntry> = if query.is_empty() {
        downloaded_playlists.clone()
    } else {
        downloaded_playlists
            .iter()
            .filter(|playlist| playlist.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    };
    filtered_playlists.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    let visible_playlist_count = playlist_visible_limit().min(filtered_playlists.len());
    let visible_playlists: Vec<DownloadCollectionEntry> = filtered_playlists
        .iter()
        .take(visible_playlist_count)
        .cloned()
        .collect();
    let has_more_playlists = filtered_playlists.len() > visible_playlist_count;

    let selected_visible_song_count = {
        let selected = selected_song_keys();
        visible_song_entries
            .iter()
            .filter(|entry| selected.contains(&download_song_key(&entry.server_id, &entry.song_id)))
            .count()
    };

    let used_size_mb = stats.total_size_bytes as f64 / (1024.0 * 1024.0);
    let size_limit_mb = settings.download_limit_mb.max(1) as f64;
    let size_usage_percent = ((used_size_mb / size_limit_mb) * 100.0).clamp(0.0, 100.0);
    let count_limit = settings.download_limit_count.max(1) as usize;
    let count_usage_percent =
        ((stats.song_count as f64 / count_limit as f64) * 100.0).clamp(0.0, 100.0);
    let size_usage_bar_width = format!("{size_usage_percent:.1}%");
    let count_usage_bar_width = format!("{count_usage_percent:.1}%");

    let on_refresh = {
        let mut refresh_nonce = refresh_nonce.clone();
        move |_| {
            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
        }
    };

    let on_toggle_downloads = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.downloads_enabled = !settings.downloads_enabled;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            spawn(async move {
                let _ = save_settings(settings_clone).await;
            });
        }
    };

    let on_toggle_auto_downloads = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.auto_downloads_enabled = !settings.auto_downloads_enabled;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            spawn(async move {
                let _ = save_settings(settings_clone).await;
            });
        }
    };

    let on_clear_downloads = {
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        let mut selected_song_keys = selected_song_keys.clone();
        move |_| {
            let removed = clear_downloads();
            action_status.set(Some(format!("Removed {removed} downloaded songs.")));
            selected_song_keys.set(HashSet::new());
            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
        }
    };

    let on_toggle_select_all_visible = {
        let mut selected_song_keys = selected_song_keys.clone();
        let visible_song_entries = visible_song_entries.clone();
        move |_| {
            let mut selected = selected_song_keys();
            let all_visible_selected = !visible_song_entries.is_empty()
                && visible_song_entries.iter().all(|entry| {
                    selected.contains(&download_song_key(&entry.server_id, &entry.song_id))
                });
            if all_visible_selected {
                for entry in &visible_song_entries {
                    selected.remove(&download_song_key(&entry.server_id, &entry.song_id));
                }
            } else {
                for entry in &visible_song_entries {
                    selected.insert(download_song_key(&entry.server_id, &entry.song_id));
                }
            }
            selected_song_keys.set(selected);
        }
    };

    let on_clear_selection = {
        let mut selected_song_keys = selected_song_keys.clone();
        move |_| selected_song_keys.set(HashSet::new())
    };

    let on_delete_selected = {
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        let mut selected_song_keys = selected_song_keys.clone();
        let entries = entries.clone();
        move |_| {
            let selected = selected_song_keys();
            let keys: Vec<(String, String)> = entries
                .iter()
                .filter(|entry| {
                    selected.contains(&download_song_key(&entry.server_id, &entry.song_id))
                })
                .map(|entry| (entry.server_id.clone(), entry.song_id.clone()))
                .collect();
            if keys.is_empty() {
                action_status.set(Some("No downloaded songs selected.".to_string()));
                return;
            }

            let removed = remove_downloaded_songs(&keys);
            selected_song_keys.set(HashSet::new());
            action_status.set(Some(format!("Removed {removed} selected download(s).")));
            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
        }
    };

    let on_play_top_result = {
        let entries = entries.clone();
        let servers_snapshot = servers_snapshot.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            let Some(first) = entries.first() else {
                return;
            };
            let song = to_download_song(first, &servers_snapshot);
            queue.set(vec![song.clone()]);
            queue_index.set(0);
            now_playing.set(Some(song));
            is_playing.set(true);
        }
    };

    let on_queue_visible_results = {
        let visible_song_entries = visible_song_entries.clone();
        let servers_snapshot = servers_snapshot.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        move |_| {
            if visible_song_entries.is_empty() {
                return;
            }

            let mut appended: Vec<Song> = visible_song_entries
                .iter()
                .map(|entry| to_download_song(entry, &servers_snapshot))
                .collect();
            if appended.is_empty() {
                return;
            }

            let mut current_queue = queue();
            let was_empty = current_queue.is_empty();
            current_queue.append(&mut appended);
            queue.set(current_queue.clone());
            if was_empty {
                queue_index.set(0);
                now_playing.set(current_queue.first().cloned());
            }
        }
    };

    let on_run_auto = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut action_busy = action_busy.clone();
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        move |_| {
            if action_busy() {
                return;
            }

            let active_servers: Vec<ServerConfig> = servers()
                .into_iter()
                .filter(|server| server.active)
                .collect();
            if active_servers.is_empty() {
                action_status.set(Some("No active servers available.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            action_busy.set(true);
            action_status.set(Some("Running auto-download pass...".to_string()));
            spawn(async move {
                match run_auto_download_pass(&active_servers, &settings_snapshot).await {
                    Ok(report) => {
                        action_status.set(Some(format!(
                            "Auto-download finished: {} new, {} skipped, {} failed, {} purged.",
                            report.downloaded, report.skipped, report.failed, report.purged
                        )));
                    }
                    Err(error) => {
                        action_status.set(Some(format!("Auto-download failed: {error}")));
                    }
                }
                refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                action_busy.set(false);
            });
        }
    };

    let on_refresh_cached_assets = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut action_busy = action_busy.clone();
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        move |_| {
            if action_busy() {
                return;
            }

            let servers_snapshot = servers();
            if servers_snapshot.is_empty() {
                action_status.set(Some("No servers configured.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            action_busy.set(true);
            action_status.set(Some(
                "Refreshing downloaded cache (lyrics + artwork)...".to_string(),
            ));
            spawn(async move {
                match refresh_downloaded_cache(&servers_snapshot, &settings_snapshot).await {
                    Ok(report) => {
                        let missing_suffix = if report.missing_servers > 0 {
                            format!(" ({} missing server mappings)", report.missing_servers)
                        } else {
                            String::new()
                        };
                        action_status.set(Some(format!(
                            "Cache refresh finished: {} scanned, {} lyrics warmed ({} attempted), {} artwork refreshed{}.",
                            report.scanned,
                            report.lyrics_warmed,
                            report.lyrics_attempted,
                            report.artwork_refreshed,
                            missing_suffix
                        )));
                    }
                    Err(error) => {
                        action_status.set(Some(format!("Cache refresh failed: {error}")));
                    }
                }
                refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                action_busy.set(false);
            });
        }
    };

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header",
                div {
                    h1 { class: "page-title", "Downloads" }
                    p { class: "page-subtitle",
                        "Manage offline audio, auto-download, and local playback."
                    }
                }
                button {
                    class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-sm",
                    onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    "Open Download Settings"
                }
            }

            if !native_downloads_supported {
                section { class: "bg-amber-500/10 border border-amber-500/40 rounded-xl p-4",
                    p { class: "text-sm text-amber-200",
                        "Downloads are only available in native builds (desktop/mobile app). Web builds can stream but do not persist offline files."
                    }
                }
            }

            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6 space-y-5",
                h2 { class: "text-lg font-semibold text-white", "Overview" }
                div { class: "grid grid-cols-1 md:grid-cols-4 gap-4",
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500", "Downloaded Songs" }
                        p { class: "text-2xl font-semibold text-white mt-2", "{stats.song_count}" }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500", "Downloaded Albums" }
                        p { class: "text-2xl font-semibold text-white mt-2", "{downloaded_albums.len()}" }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500", "Downloaded Playlists" }
                        p { class: "text-2xl font-semibold text-white mt-2", "{downloaded_playlists.len()}" }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500", "Storage Used" }
                        p { class: "text-2xl font-semibold text-white mt-2", "{format_size(stats.total_size_bytes)}" }
                    }
                }

                div { class: "space-y-3",
                    div { class: "flex items-center justify-between text-xs text-zinc-500",
                        span { "Size usage" }
                        span { "{used_size_mb:.1} / {size_limit_mb:.0} MB ({size_usage_percent:.0}%)" }
                    }
                    div { class: "h-2 w-full rounded-full bg-zinc-700/70 overflow-hidden",
                        div {
                            class: "h-full bg-emerald-500/80 transition-all",
                            style: "width: {size_usage_bar_width}",
                        }
                    }
                    div { class: "flex items-center justify-between text-xs text-zinc-500",
                        span { "Count usage" }
                        span { "{stats.song_count} / {count_limit} songs ({count_usage_percent:.0}%)" }
                    }
                    div { class: "h-2 w-full rounded-full bg-zinc-700/70 overflow-hidden",
                        div {
                            class: "h-full bg-cyan-500/80 transition-all",
                            style: "width: {count_usage_bar_width}",
                        }
                    }
                }

                if !active_downloads.is_empty() {
                    div { class: "space-y-2",
                        div { class: "flex items-center justify-between",
                            p { class: "text-xs uppercase tracking-wider text-zinc-500", "Active Downloads" }
                            p { class: "text-xs text-emerald-300", "{active_downloads.len()} in progress" }
                        }
                        div { class: "max-h-40 overflow-y-auto rounded-xl border border-zinc-700/50 bg-zinc-900/40 p-2 space-y-1",
                            for entry in active_downloads.iter().take(30) {
                                div {
                                    key: "active:{entry.server_id}:{entry.song_id}",
                                    class: "flex items-center justify-between gap-3 px-2 py-1.5 rounded-lg bg-zinc-900/50",
                                    div { class: "min-w-0",
                                        p { class: "text-sm text-zinc-200 truncate", "{entry.title}" }
                                        p { class: "text-xs text-zinc-500 truncate",
                                            "{entry.artist.clone().unwrap_or_else(|| \"Unknown artist\".to_string())}"
                                        }
                                    }
                                    Icon { name: "loader".to_string(), class: "w-4 h-4 text-emerald-400 flex-shrink-0".to_string() }
                                }
                            }
                        }
                    }
                }

                div { class: "grid grid-cols-2 gap-2 pt-2 sm:flex sm:flex-wrap sm:items-center sm:gap-3",
                    button {
                        class: if settings.downloads_enabled {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 text-center"
                        } else {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 text-center"
                        },
                        onclick: on_toggle_downloads,
                        if settings.downloads_enabled {
                            "Downloads Enabled"
                        } else {
                            "Downloads Disabled"
                        }
                    }
                    button {
                        class: if settings.auto_downloads_enabled {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 text-center"
                        } else {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 text-center"
                        },
                        onclick: on_toggle_auto_downloads,
                        if settings.auto_downloads_enabled {
                            "Auto Downloads On"
                        } else {
                            "Auto Downloads Off"
                        }
                    }
                    button {
                        class: if action_busy() {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-center"
                        } else {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors text-center"
                        },
                        disabled: action_busy(),
                        onclick: on_run_auto,
                        if action_busy() {
                            "Running..."
                        } else {
                            "Run Auto-Download"
                        }
                    }
                    button {
                        class: if action_busy() {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-center"
                        } else {
                            "w-full sm:w-auto px-3 py-2 rounded-lg border border-cyan-500/50 text-cyan-300 hover:text-white hover:border-cyan-400 transition-colors text-center"
                        },
                        disabled: action_busy(),
                        onclick: on_refresh_cached_assets,
                        if action_busy() {
                            "Working..."
                        } else {
                            "Refresh Cache"
                        }
                    }
                    button {
                        class: "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors text-center",
                        onclick: on_refresh,
                        "Refresh"
                    }
                    button {
                        class: "w-full sm:w-auto px-3 py-2 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-center",
                        onclick: on_clear_downloads,
                        "Clear Downloads"
                    }
                }
                if let Some(status) = action_status() {
                    p { class: "text-xs text-zinc-400", "{status}" }
                }
            }

            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6 space-y-4",
                div { class: "flex items-center justify-between gap-3",
                    h2 { class: "text-lg font-semibold text-white", "Downloaded Library" }
                    span { class: "text-sm text-zinc-500",
                        if active_tab() == DownloadsTab::Songs {
                            "{entries.len()} songs"
                        } else if active_tab() == DownloadsTab::Albums {
                            "{filtered_albums.len()} albums"
                        } else {
                            "{filtered_playlists.len()} playlists"
                        }
                    }
                }

                div { class: "flex flex-wrap items-center gap-2",
                    button {
                        class: if active_tab() == DownloadsTab::Songs {
                            "px-3 py-2 rounded-full bg-emerald-500/20 text-emerald-300 text-sm"
                        } else {
                            "px-3 py-2 rounded-full bg-zinc-900/60 border border-zinc-800 text-zinc-400 hover:text-white text-sm"
                        },
                        onclick: move |_| active_tab.set(DownloadsTab::Songs),
                        "Songs"
                    }
                    button {
                        class: if active_tab() == DownloadsTab::Albums {
                            "px-3 py-2 rounded-full bg-emerald-500/20 text-emerald-300 text-sm"
                        } else {
                            "px-3 py-2 rounded-full bg-zinc-900/60 border border-zinc-800 text-zinc-400 hover:text-white text-sm"
                        },
                        onclick: move |_| active_tab.set(DownloadsTab::Albums),
                        "Albums"
                    }
                    button {
                        class: if active_tab() == DownloadsTab::Playlists {
                            "px-3 py-2 rounded-full bg-emerald-500/20 text-emerald-300 text-sm"
                        } else {
                            "px-3 py-2 rounded-full bg-zinc-900/60 border border-zinc-800 text-zinc-400 hover:text-white text-sm"
                        },
                        onclick: move |_| active_tab.set(DownloadsTab::Playlists),
                        "Playlists"
                    }
                }

                input {
                    class: "w-full max-w-md px-3 py-2 rounded-lg bg-zinc-900/60 border border-zinc-800 text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50",
                    placeholder: if active_tab() == DownloadsTab::Songs {
                        "Search downloaded songs"
                    } else if active_tab() == DownloadsTab::Albums {
                        "Search downloaded albums"
                    } else {
                        "Search downloaded playlists"
                    },
                    value: search_query,
                    oninput: move |evt| search_query.set(evt.value()),
                }

                if active_tab() == DownloadsTab::Songs {
                    div { class: "flex flex-wrap items-center gap-2",
                        select {
                            class: "px-3 py-2 rounded-lg bg-zinc-900/60 border border-zinc-800 text-sm text-zinc-200 focus:outline-none focus:border-emerald-500/50",
                            value: download_song_sort_key(song_sort()),
                            oninput: move |evt| {
                                song_sort.set(parse_download_song_sort(&evt.value()));
                            },
                            option { value: "newest", "Sort: Recently downloaded" }
                            option { value: "title", "Sort: Title" }
                            option { value: "artist", "Sort: Artist" }
                            option { value: "album", "Sort: Album" }
                            option { value: "size", "Sort: Size" }
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors text-xs",
                            onclick: on_play_top_result,
                            disabled: entries.is_empty(),
                            "Play Top Result"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                            onclick: on_queue_visible_results,
                            disabled: visible_song_entries.is_empty(),
                            "Queue Visible"
                        }
                    }

                    if !entries.is_empty() {
                        div { class: "flex flex-wrap items-center gap-2",
                            button {
                                class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                onclick: on_toggle_select_all_visible,
                                if selected_visible_song_count == visible_song_entries.len() {
                                    "Unselect Visible"
                                } else {
                                    "Select Visible"
                                }
                            }
                            if selected_visible_song_count > 0 {
                                button {
                                    class: "px-3 py-2 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-xs",
                                    onclick: on_delete_selected,
                                    "Delete Selected"
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors text-xs",
                                    onclick: on_clear_selection,
                                    "Clear Selection"
                                }
                                span { class: "text-xs text-zinc-500", "{selected_visible_song_count} selected" }
                            }
                        }
                    }
                    if entries.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded songs yet." }
                    } else {
                        p { class: "text-xs text-zinc-500",
                            "Showing {visible_song_entries.len()} of {entries.len()} matching songs."
                        }
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for entry in visible_song_entries.iter().cloned() {
                                {
                                    let selection_key =
                                        download_song_key(&entry.server_id, &entry.song_id);
                                    let is_selected = selected_song_keys().contains(&selection_key);
                                    let cover_id = entry
                                        .cover_art_id
                                        .as_ref()
                                        .filter(|value| !value.trim().is_empty())
                                        .cloned()
                                        .or_else(|| {
                                            entry.album_id
                                                .as_ref()
                                                .filter(|value| !value.trim().is_empty())
                                                .cloned()
                                        });
                                    let cover_url = cover_id.as_ref().and_then(|cover| {
                                        servers_snapshot
                                            .iter()
                                            .find(|server| server.id == entry.server_id)
                                            .map(|server| {
                                                NavidromeClient::new(server.clone())
                                                    .get_cover_art_url(cover, 100)
                                            })
                                    });
                                    rsx! {
                                        div {
                                            key: "{entry.server_id}:{entry.song_id}",
                                            class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 flex items-start justify-between gap-3",
                                            div { class: "flex items-start gap-3 min-w-0",
                                                input {
                                                    r#type: "checkbox",
                                                    class: "mt-1 accent-emerald-500 flex-shrink-0",
                                                    checked: is_selected,
                                                    onchange: {
                                                        let entry = entry.clone();
                                                        let mut selected_song_keys = selected_song_keys.clone();
                                                        move |evt| {
                                                            let mut selected = selected_song_keys();
                                                            let key = download_song_key(&entry.server_id, &entry.song_id);
                                                            if evt.checked() {
                                                                selected.insert(key);
                                                            } else {
                                                                selected.remove(&key);
                                                            }
                                                            selected_song_keys.set(selected);
                                                        }
                                                    },
                                                }
                                                if let Some(url) = cover_url {
                                                    if let Some(album_id) = entry.album_id.clone() {
                                                        button {
                                                            class: "w-12 h-12 rounded-lg overflow-hidden border border-zinc-800/80 flex-shrink-0",
                                                            onclick: {
                                                                let album_id = album_id.clone();
                                                                let server_id = entry.server_id.clone();
                                                                move |evt: MouseEvent| {
                                                                    evt.stop_propagation();
                                                                    navigation.navigate_to(AppView::AlbumDetailView {
                                                                        album_id: album_id.clone(),
                                                                        server_id: server_id.clone(),
                                                                    });
                                                                }
                                                            },
                                                            img {
                                                                src: "{url}",
                                                                alt: "{entry.title}",
                                                                class: "w-full h-full object-cover",
                                                                loading: "lazy",
                                                            }
                                                        }
                                                    } else {
                                                        img {
                                                            src: "{url}",
                                                            alt: "{entry.title}",
                                                            class: "w-12 h-12 rounded-lg object-cover border border-zinc-800/80 flex-shrink-0",
                                                            loading: "lazy",
                                                        }
                                                    }
                                                } else {
                                                    div { class: "w-12 h-12 rounded-lg bg-zinc-800 border border-zinc-800/80 flex-shrink-0" }
                                                }
                                                div { class: "min-w-0",
                                                    p { class: "text-sm font-medium text-white truncate", "{entry.title}" }
                                                    p { class: "text-xs text-zinc-400 truncate",
                                                        "{entry.artist.clone().unwrap_or_else(|| \"Unknown artist\".to_string())}"
                                                    }
                                                    if let Some(album) = entry.album.clone() {
                                                        p { class: "text-xs text-zinc-500 truncate", "{album}" }
                                                    }
                                                }
                                            }
                                            div { class: "flex-shrink-0 grid grid-cols-2 gap-2 w-32",
                                                button {
                                                    class: "w-full px-2 py-1 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors text-xs",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        let servers_snapshot = servers_snapshot.clone();
                                                        move |_| {
                                                            let song = to_download_song(&entry, &servers_snapshot);
                                                            queue.set(vec![song.clone()]);
                                                            queue_index.set(0);
                                                            now_playing.set(Some(song));
                                                            is_playing.set(true);
                                                        }
                                                    },
                                                    "Play"
                                                }
                                                button {
                                                    class: "w-full px-2 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        let servers_snapshot = servers_snapshot.clone();
                                                        let mut queue = queue.clone();
                                                        let mut queue_index = queue_index.clone();
                                                        let mut now_playing = now_playing.clone();
                                                        move |_| {
                                                            let song = to_download_song(&entry, &servers_snapshot);
                                                            let mut queue_entries = queue();
                                                            let was_empty = queue_entries.is_empty();
                                                            queue_entries.push(song.clone());
                                                            queue.set(queue_entries);
                                                            if was_empty {
                                                                queue_index.set(0);
                                                                now_playing.set(Some(song));
                                                            }
                                                        }
                                                    },
                                                    "Queue"
                                                }
                                                button {
                                                    class: "col-span-2 w-full px-2 py-1 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-xs",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        let mut action_status = action_status.clone();
                                                        let mut refresh_nonce = refresh_nonce.clone();
                                                        let mut selected_song_keys = selected_song_keys.clone();
                                                        move |_| {
                                                            let removed = remove_downloaded_song(&entry.server_id, &entry.song_id);
                                                            let mut selected = selected_song_keys();
                                                            selected.remove(&download_song_key(&entry.server_id, &entry.song_id));
                                                            selected_song_keys.set(selected);
                                                            action_status.set(Some(format!(
                                                                "Removed {removed} download(s) for \"{}\".",
                                                                entry.title
                                                            )));
                                                            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                                                        }
                                                    },
                                                    "Delete"
                                                }
                                                p { class: "col-span-2 text-right text-xs text-zinc-300", "{format_size(entry.size_bytes)}" }
                                                p { class: "col-span-2 text-right text-[11px] text-zinc-500", "{format_updated(entry.updated_at_ms)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if has_more_song_entries {
                            button {
                                class: "mt-3 px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                onclick: move |_| {
                                    song_visible_limit.with_mut(|count| {
                                        *count = count.saturating_add(DOWNLOADS_SONG_PAGE_SIZE);
                                    });
                                },
                                "Load {DOWNLOADS_SONG_PAGE_SIZE} More Songs"
                            }
                        } else {
                            p { class: "mt-3 text-xs text-zinc-500",
                                "Showing all {entries.len()} matching songs."
                            }
                        }
                    }
                } else if active_tab() == DownloadsTab::Albums {
                    if filtered_albums.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded albums yet." }
                    } else {
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for album in visible_albums {
                                {
                                    let cover_id = album_cover_ids
                                        .get(&(album.server_id.clone(), album.collection_id.clone()))
                                        .cloned()
                                        .or_else(|| {
                                            if album.collection_id.starts_with("name:") {
                                                None
                                            } else {
                                                Some(album.collection_id.clone())
                                            }
                                        });
                                    let cover_url = cover_id.as_ref().and_then(|cover_id| {
                                        servers_snapshot
                                            .iter()
                                            .find(|server| server.id == album.server_id)
                                            .map(|server| {
                                                NavidromeClient::new(server.clone())
                                                    .get_cover_art_url(cover_id, 120)
                                            })
                                    });
                                    rsx! {
                                        div {
                                            key: "album:{album.server_id}:{album.collection_id}",
                                            class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 flex items-start justify-between gap-3",
                                            button {
                                                class: "min-w-0 flex-1 text-left hover:text-emerald-300 transition-colors",
                                                disabled: album.collection_id.starts_with("name:"),
                                                onclick: {
                                                    let album_id = album.collection_id.clone();
                                                    let server_id = album.server_id.clone();
                                                    move |_| {
                                                        if album_id.starts_with("name:") {
                                                            return;
                                                        }
                                                        navigation.navigate_to(AppView::AlbumDetailView {
                                                            album_id: album_id.clone(),
                                                            server_id: server_id.clone(),
                                                        });
                                                    }
                                                },
                                                div { class: "flex items-center gap-3 min-w-0",
                                                    if let Some(url) = cover_url {
                                                        img {
                                                            src: "{url}",
                                                            alt: "{album.name}",
                                                            class: "w-12 h-12 rounded-lg object-cover border border-zinc-800/80 flex-shrink-0",
                                                            loading: "lazy",
                                                        }
                                                    } else {
                                                        div { class: "w-12 h-12 rounded-lg bg-zinc-800 border border-zinc-800/80 flex-shrink-0" }
                                                    }
                                                    div { class: "min-w-0",
                                                        p { class: "text-sm font-medium text-white truncate", "{album.name}" }
                                                        p { class: "text-xs text-zinc-500", "{album.song_count} songs" }
                                                    }
                                                }
                                            }
                                            div { class: "text-right flex-shrink-0 space-y-2 min-w-[6rem]",
                                                p { class: "text-[11px] text-zinc-500", "{format_updated(album.updated_at_ms)}" }
                                                button {
                                                    class: "px-2 py-1 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-xs",
                                                    onclick: {
                                                        let album = album.clone();
                                                        let mut action_status = action_status.clone();
                                                        let mut refresh_nonce = refresh_nonce.clone();
                                                        move |_| {
                                                            let removed = remove_downloaded_album(
                                                                &album.server_id,
                                                                &album.collection_id,
                                                                &album.name,
                                                            );
                                                            action_status.set(Some(format!(
                                                                "Removed {removed} song(s) from album \"{}\".",
                                                                album.name
                                                            )));
                                                            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                                                        }
                                                    },
                                                    "Delete"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if has_more_albums {
                            button {
                                class: "mt-3 px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                onclick: move |_| {
                                    album_visible_limit.with_mut(|count| {
                                        *count = count.saturating_add(DOWNLOADS_COLLECTION_PAGE_SIZE);
                                    });
                                },
                                "Load {DOWNLOADS_COLLECTION_PAGE_SIZE} More Albums"
                            }
                        } else {
                            p { class: "mt-3 text-xs text-zinc-500",
                                "Showing all {filtered_albums.len()} matching albums."
                            }
                        }
                    }
                } else {
                    if filtered_playlists.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded playlists yet." }
                    } else {
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for playlist in visible_playlists {
                                div {
                                    key: "playlist:{playlist.server_id}:{playlist.collection_id}",
                                    class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 flex items-start justify-between gap-3",
                                    button {
                                        class: "min-w-0 flex-1 text-left hover:text-emerald-300 transition-colors",
                                        onclick: {
                                            let playlist_id = playlist.collection_id.clone();
                                            let server_id = playlist.server_id.clone();
                                            move |_| {
                                                navigation.navigate_to(AppView::PlaylistDetailView {
                                                    playlist_id: playlist_id.clone(),
                                                    server_id: server_id.clone(),
                                                });
                                            }
                                        },
                                        div { class: "min-w-0",
                                            p { class: "text-sm font-medium text-white truncate", "{playlist.name}" }
                                            p { class: "text-xs text-zinc-500", "{playlist.song_count} songs" }
                                        }
                                    }
                                    div { class: "text-right flex-shrink-0 space-y-2 min-w-[6rem]",
                                        p { class: "text-[11px] text-zinc-500", "{format_updated(playlist.updated_at_ms)}" }
                                        button {
                                            class: "px-2 py-1 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-xs",
                                            onclick: {
                                                let playlist = playlist.clone();
                                                let mut action_status = action_status.clone();
                                                let mut refresh_nonce = refresh_nonce.clone();
                                                move |_| {
                                                    let removed = remove_downloaded_collection(
                                                        "playlist",
                                                        &playlist.server_id,
                                                        &playlist.collection_id,
                                                    );
                                                    action_status.set(Some(format!(
                                                        "Removed {removed} playlist download marker(s) for \"{}\".",
                                                        playlist.name
                                                    )));
                                                    refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                                                }
                                            },
                                            "Delete"
                                        }
                                    }
                                }
                            }
                        }
                        if has_more_playlists {
                            button {
                                class: "mt-3 px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs",
                                onclick: move |_| {
                                    playlist_visible_limit.with_mut(|count| {
                                        *count = count.saturating_add(DOWNLOADS_COLLECTION_PAGE_SIZE);
                                    });
                                },
                                "Load {DOWNLOADS_COLLECTION_PAGE_SIZE} More Playlists"
                            }
                        } else {
                            p { class: "mt-3 text-xs text-zinc-500",
                                "Showing all {filtered_playlists.len()} matching playlists."
                            }
                        }
                    }
                }
            }
        }
    }
}
