use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::{AppView, Navigation};
use crate::db::{save_settings, AppSettings};
use crate::offline_audio::{
    clear_downloads, download_stats, list_downloaded_collections, list_downloaded_entries,
    refresh_downloaded_cache, run_auto_download_pass, DownloadCollectionEntry, DownloadIndexEntry,
};
use chrono::{DateTime, Local, Utc};
use dioxus::prelude::*;
use std::collections::HashMap;

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

    #[cfg(target_arch = "wasm32")]
    let native_downloads_supported = false;
    #[cfg(not(target_arch = "wasm32"))]
    let native_downloads_supported = true;

    let _refresh = refresh_nonce();
    let settings = app_settings();
    let stats = download_stats();
    let all_entries = list_downloaded_entries();
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
    let entries: Vec<DownloadIndexEntry> = if query.is_empty() {
        all_entries.clone()
    } else {
        all_entries
            .into_iter()
            .filter(|entry| {
                let title = entry.title.to_ascii_lowercase();
                let artist = entry
                    .artist
                    .as_ref()
                    .map(|value| value.to_ascii_lowercase())
                    .unwrap_or_default();
                let album = entry
                    .album
                    .as_ref()
                    .map(|value| value.to_ascii_lowercase())
                    .unwrap_or_default();
                title.contains(&query) || artist.contains(&query) || album.contains(&query)
            })
            .collect()
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
        move |_| {
            let removed = clear_downloads();
            action_status.set(Some(format!("Removed {removed} downloaded songs.")));
            refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
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

                div { class: "flex flex-wrap items-center gap-3 pt-2",
                    button {
                        class: if settings.downloads_enabled {
                            "px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300"
                        } else {
                            "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300"
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
                            "px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300"
                        } else {
                            "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300"
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
                            "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed"
                        } else {
                            "px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors"
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
                            "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed"
                        } else {
                            "px-3 py-2 rounded-lg border border-cyan-500/50 text-cyan-300 hover:text-white hover:border-cyan-400 transition-colors"
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
                        class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors",
                        onclick: on_refresh,
                        "Refresh"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors",
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
                            "{downloaded_albums.len()} albums"
                        } else {
                            "{downloaded_playlists.len()} playlists"
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

                if active_tab() == DownloadsTab::Songs {
                    input {
                        class: "w-full max-w-md px-3 py-2 rounded-lg bg-zinc-900/60 border border-zinc-800 text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50",
                        placeholder: "Search downloaded songs",
                        value: search_query,
                        oninput: move |evt| search_query.set(evt.value()),
                    }
                    if entries.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded songs yet." }
                    } else {
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for entry in entries {
                                {
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
                                        servers()
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
                                            div { class: "text-right flex-shrink-0 space-y-2",
                                                button {
                                                    class: "px-2 py-1 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors text-xs",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        let servers = servers();
                                                        move |_| {
                                                            let server_name = servers
                                                                .iter()
                                                                .find(|server| server.id == entry.server_id)
                                                                .map(|server| server.name.clone())
                                                                .unwrap_or_else(|| "Offline".to_string());
                                                            let song = Song {
                                                                id: entry.song_id.clone(),
                                                                title: entry.title.clone(),
                                                                album: entry.album.clone(),
                                                                album_id: entry.album_id.clone(),
                                                                artist: entry.artist.clone(),
                                                                cover_art: entry.cover_art_id.clone().or_else(|| entry.album_id.clone()),
                                                                duration: 0,
                                                                server_id: entry.server_id.clone(),
                                                                server_name,
                                                                ..Song::default()
                                                            };
                                                            queue.set(vec![song.clone()]);
                                                            queue_index.set(0);
                                                            now_playing.set(Some(song));
                                                            is_playing.set(true);
                                                        }
                                                    },
                                                    "Play"
                                                }
                                                p { class: "text-xs text-zinc-300", "{format_size(entry.size_bytes)}" }
                                                p { class: "text-[11px] text-zinc-500", "{format_updated(entry.updated_at_ms)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if active_tab() == DownloadsTab::Albums {
                    if downloaded_albums.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded albums yet." }
                    } else {
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for album in downloaded_albums {
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
                                        servers()
                                            .iter()
                                            .find(|server| server.id == album.server_id)
                                            .map(|server| {
                                                NavidromeClient::new(server.clone())
                                                    .get_cover_art_url(cover_id, 120)
                                            })
                                    });
                                    rsx! {
                                        button {
                                            key: "album:{album.server_id}:{album.collection_id}",
                                            class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 flex items-start justify-between gap-3 text-left hover:border-emerald-500/40 transition-colors",
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
                                            p { class: "text-[11px] text-zinc-500 flex-shrink-0", "{format_updated(album.updated_at_ms)}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if downloaded_playlists.is_empty() {
                        div { class: "text-sm text-zinc-500 py-10 text-center", "No downloaded playlists yet." }
                    } else {
                        div { class: "space-y-2 max-h-[60vh] overflow-y-auto pr-1",
                            for playlist in downloaded_playlists {
                                button {
                                    key: "playlist:{playlist.server_id}:{playlist.collection_id}",
                                    class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 flex items-start justify-between gap-3 text-left hover:border-emerald-500/40 transition-colors",
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
                                    p { class: "text-[11px] text-zinc-500 flex-shrink-0", "{format_updated(playlist.updated_at_ms)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
