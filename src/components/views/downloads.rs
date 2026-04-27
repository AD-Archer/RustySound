use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::{save_settings, AppSettings};
use crate::offline_audio::{
    clear_downloads, download_stats, list_active_downloads, list_downloaded_collection_memberships,
    list_downloaded_collections, list_downloaded_entries, refresh_downloaded_cache,
    remove_downloaded_album, remove_downloaded_collection, remove_downloaded_song,
    run_auto_download_pass, sync_downloaded_collection_members,
    sync_downloaded_collection_metadata, ActiveDownloadEntry, DownloadCollectionEntry,
    DownloadCollectionMembershipEntry, DownloadIndexEntry,
};
use dioxus::prelude::*;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb < 1024.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.2} GB", mb / 1024.0)
    }
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
                collection.total_song_count = collection.total_song_count.saturating_add(1);
                collection.downloaded_song_count =
                    collection.downloaded_song_count.saturating_add(1);
                if updated_at_ms > collection.updated_at_ms {
                    collection.updated_at_ms = updated_at_ms;
                }
            })
            .or_insert(DownloadCollectionEntry {
                kind: "album".to_string(),
                server_id: entry.server_id.clone(),
                collection_id: album_key,
                name: album_name,
                auto_download_tracked: false,
                song_count: 1,
                total_song_count: 1,
                downloaded_song_count: 1,
                updated_at_ms,
            });
    }

    let mut values: Vec<DownloadCollectionEntry> = map.into_values().collect();
    values.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    values
}

fn collection_download_count_key(entry: &DownloadCollectionEntry) -> (String, String, String) {
    (
        entry.kind.clone(),
        entry.server_id.clone(),
        entry.collection_id.clone(),
    )
}

fn collection_progress(
    entry: &DownloadCollectionEntry,
    collection_download_counts: &HashMap<(String, String, String), usize>,
) -> (usize, usize, usize) {
    let downloaded_song_count = collection_download_counts
        .get(&collection_download_count_key(entry))
        .copied()
        .unwrap_or_else(|| entry.effective_downloaded_song_count());
    let total_song_count = entry
        .effective_total_song_count()
        .max(downloaded_song_count);
    let downloaded_song_count = downloaded_song_count.min(total_song_count);
    (
        downloaded_song_count,
        total_song_count,
        total_song_count.saturating_sub(downloaded_song_count),
    )
}

fn collection_detail_view(collection: &DownloadCollectionEntry) -> Option<AppView> {
    match collection.kind.as_str() {
        "album" if !collection.collection_id.starts_with("name:") => {
            Some(AppView::AlbumDetailView {
                album_id: collection.collection_id.clone(),
                server_id: collection.server_id.clone(),
            })
        }
        "playlist" => Some(AppView::PlaylistDetailView {
            playlist_id: collection.collection_id.clone(),
            server_id: collection.server_id.clone(),
        }),
        _ => None,
    }
}

#[derive(Clone, PartialEq, Eq)]
enum PendingDownloadsDelete {
    Song {
        server_id: String,
        song_id: String,
        title: String,
    },
    Collection {
        kind: String,
        server_id: String,
        collection_id: String,
        name: String,
    },
    ClearAll,
}

fn pending_delete_title(action: &PendingDownloadsDelete) -> &'static str {
    match action {
        PendingDownloadsDelete::Song { .. } => "Delete Song",
        PendingDownloadsDelete::Collection { kind, .. } if kind == "album" => "Delete Album",
        PendingDownloadsDelete::Collection { .. } => "Delete Playlist",
        PendingDownloadsDelete::ClearAll => "Clear Downloads",
    }
}

fn pending_delete_message(action: &PendingDownloadsDelete) -> String {
    match action {
        PendingDownloadsDelete::Song { title, .. } => {
            format!("Delete the downloaded song \"{title}\" from this device?")
        }
        PendingDownloadsDelete::Collection { kind, name, .. } if kind == "album" => {
            format!("Delete the downloaded album \"{name}\" from this device?")
        }
        PendingDownloadsDelete::Collection { name, .. } => {
            format!("Delete the downloaded playlist \"{name}\" from this device?")
        }
        PendingDownloadsDelete::ClearAll => {
            "Delete all downloaded songs, albums, and playlists from this device?".to_string()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DownloadsTab {
    Songs,
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

fn download_entry_cover_url(
    entry: &DownloadIndexEntry,
    servers: &[ServerConfig],
    size: u32,
) -> Option<String> {
    let cover_id = entry
        .cover_art_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .or_else(|| {
            entry
                .album_id
                .as_ref()
                .filter(|value| !value.trim().is_empty())
                .cloned()
        })?;

    servers
        .iter()
        .find(|server| server.id == entry.server_id)
        .map(|server| NavidromeClient::new(server.clone()).get_cover_art_url(&cover_id, size))
}

fn ordered_download_entries_for_song_ids(
    server_id: &str,
    song_ids: &[String],
    entry_lookup: &HashMap<(String, String), DownloadIndexEntry>,
) -> Vec<DownloadIndexEntry> {
    let mut ordered = Vec::<DownloadIndexEntry>::new();
    let mut seen = HashSet::<String>::new();
    for song_id in song_ids {
        let trimmed = song_id.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            continue;
        }
        if let Some(entry) = entry_lookup.get(&(server_id.to_string(), trimmed.to_string())) {
            ordered.push(entry.clone());
        }
    }
    ordered
}

fn replace_queue_with_download_entries(
    entries: &[DownloadIndexEntry],
    servers: &[ServerConfig],
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
) -> bool {
    let songs: Vec<Song> = entries
        .iter()
        .map(|entry| to_download_song(entry, servers))
        .collect();
    let Some(first_song) = songs.first().cloned() else {
        return false;
    };
    queue.set(songs);
    queue_index.set(0);
    now_playing.set(Some(first_song));
    is_playing.set(true);
    true
}

fn replace_queue_with_shuffled_download_entries(
    entries: &[DownloadIndexEntry],
    servers: &[ServerConfig],
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
) -> bool {
    let mut songs: Vec<Song> = entries
        .iter()
        .map(|entry| to_download_song(entry, servers))
        .collect();
    if songs.is_empty() {
        return false;
    }

    songs.shuffle(&mut rand::thread_rng());
    let first_song = songs[0].clone();
    queue.set(songs);
    queue_index.set(0);
    now_playing.set(Some(first_song));
    is_playing.set(true);
    true
}

#[component]
pub fn DownloadsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let refresh_nonce = use_signal(|| 0u64);
    let action_busy = use_signal(|| false);
    let action_status = use_signal(|| None::<String>);
    let mut search_query = use_signal(String::new);
    let active_tab = use_signal(|| DownloadsTab::Songs);
    let selected_song_keys = use_signal(HashSet::<String>::new);
    let mut song_sort = use_signal(|| DownloadSongSort::Title);
    let mut album_sort = use_signal(|| "title"); // "title", "recent", "oldest"
    let mut playlist_sort = use_signal(|| "title");
    let song_visible_limit = use_signal(|| DOWNLOADS_SONG_PAGE_SIZE);
    let album_visible_limit = use_signal(|| DOWNLOADS_COLLECTION_PAGE_SIZE);
    let playlist_visible_limit = use_signal(|| DOWNLOADS_COLLECTION_PAGE_SIZE);
    let selected_collection_modal = use_signal(|| None::<DownloadCollectionEntry>);
    let collection_metadata_sync_signature = use_signal(String::new);
    let pending_delete = use_signal(|| None::<PendingDownloadsDelete>);

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
    let collection_memberships: Vec<DownloadCollectionMembershipEntry> =
        list_downloaded_collection_memberships();
    let downloaded_entry_lookup: HashMap<(String, String), DownloadIndexEntry> = all_entries
        .iter()
        .map(|entry| {
            (
                (entry.server_id.clone(), entry.song_id.clone()),
                entry.clone(),
            )
        })
        .collect();
    let collection_song_ids = collection_memberships.iter().fold(
        HashMap::<(String, String, String), Vec<String>>::new(),
        |mut map, entry| {
            map.insert(
                (
                    entry.kind.clone(),
                    entry.server_id.clone(),
                    entry.collection_id.clone(),
                ),
                entry.song_ids.clone(),
            );
            map
        },
    );
    let collection_download_counts = collection_memberships.iter().fold(
        HashMap::<(String, String, String), usize>::new(),
        |mut map, entry| {
            let count = entry
                .song_ids
                .iter()
                .filter(|song_id| {
                    downloaded_entry_lookup
                        .contains_key(&(entry.server_id.clone(), song_id.trim().to_string()))
                })
                .count();
            map.insert(
                (
                    entry.kind.clone(),
                    entry.server_id.clone(),
                    entry.collection_id.clone(),
                ),
                count,
            );
            map
        },
    );
    {
        let selected_collection_modal = selected_collection_modal.clone();
        let servers = servers.clone();
        let collection_song_ids = collection_song_ids.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        use_effect(move || {
            let Some(collection) = selected_collection_modal() else {
                return;
            };
            if collection.kind != "playlist" {
                return;
            }
            if collection_song_ids.contains_key(&(
                collection.kind.clone(),
                collection.server_id.clone(),
                collection.collection_id.clone(),
            )) {
                return;
            }

            spawn(async move {
                let servers_snapshot = servers();
                let Some(server) = servers_snapshot
                    .iter()
                    .find(|server| server.id == collection.server_id)
                    .cloned()
                else {
                    return;
                };
                let client = NavidromeClient::new(server);
                if let Ok((_, songs)) = client.get_playlist(&collection.collection_id).await {
                    sync_downloaded_collection_members(
                        "playlist",
                        &collection.server_id,
                        &collection.collection_id,
                        &songs,
                    );
                    refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                }
            });
        });
    }
    {
        let servers = servers.clone();
        let mut collection_metadata_sync_signature = collection_metadata_sync_signature.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        let sync_signature = format!(
            "{}:{}:{}:{}",
            all_entries.len(),
            all_entries
                .iter()
                .map(|entry| entry.updated_at_ms)
                .max()
                .unwrap_or(0),
            collections.len(),
            collections
                .iter()
                .map(|entry| entry.updated_at_ms)
                .max()
                .unwrap_or(0)
        );
        use_effect(move || {
            if sync_signature.is_empty() || collection_metadata_sync_signature() == sync_signature {
                return;
            }

            collection_metadata_sync_signature.set(sync_signature.clone());
            let servers_snapshot = servers();
            spawn(async move {
                let changed = sync_downloaded_collection_metadata(&servers_snapshot).await;
                if changed > 0 {
                    refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                }
            });
        });
    }
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
            .iter()
            .filter(|entry| {
                let title = entry.title.to_ascii_lowercase();
                let artist = normalize_download_field(&entry.artist);
                let album = normalize_download_field(&entry.album);
                title.contains(&query) || artist.contains(&query) || album.contains(&query)
            })
            .cloned()
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
    let _has_more_song_entries = entries.len() > visible_song_count;

    let mut filtered_albums: Vec<DownloadCollectionEntry> = if query.is_empty() {
        downloaded_albums.clone()
    } else {
        downloaded_albums
            .iter()
            .filter(|album| album.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    };
    match album_sort() {
        "title" => filtered_albums.sort_by(|left, right| left.name.cmp(&right.name)),
        "oldest" => {
            filtered_albums.sort_by(|left, right| left.updated_at_ms.cmp(&right.updated_at_ms))
        }
        _ => filtered_albums.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms)), // recent (default)
    }
    let visible_album_count = album_visible_limit().min(filtered_albums.len());
    let visible_albums: Vec<DownloadCollectionEntry> = filtered_albums
        .iter()
        .take(visible_album_count)
        .cloned()
        .collect();
    let _has_more_albums = filtered_albums.len() > visible_album_count;

    let mut filtered_playlists: Vec<DownloadCollectionEntry> = if query.is_empty() {
        downloaded_playlists.clone()
    } else {
        downloaded_playlists
            .iter()
            .filter(|playlist| playlist.name.to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    };
    match playlist_sort() {
        "title" => filtered_playlists.sort_by(|left, right| left.name.cmp(&right.name)),
        "oldest" => {
            filtered_playlists.sort_by(|left, right| left.updated_at_ms.cmp(&right.updated_at_ms))
        }
        _ => filtered_playlists.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms)), // recent (default)
    }
    let visible_playlist_count = playlist_visible_limit().min(filtered_playlists.len());
    let visible_playlists: Vec<DownloadCollectionEntry> = filtered_playlists
        .iter()
        .take(visible_playlist_count)
        .cloned()
        .collect();
    let _has_more_playlists = filtered_playlists.len() > visible_playlist_count;

    let _selected_visible_song_count = {
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
        let servers = servers.clone();
        let mut action_busy = action_busy.clone();
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        move |_| {
            if action_busy() {
                return;
            }

            let servers_snapshot = servers();
            action_busy.set(true);
            action_status.set(Some("Refreshing downloaded collections...".to_string()));
            spawn(async move {
                let changed = sync_downloaded_collection_metadata(&servers_snapshot).await;
                action_status.set(Some(if changed > 0 {
                    format!("Refresh complete: {changed} collection(s) updated.")
                } else {
                    "Refresh complete: no collection changes found.".to_string()
                }));
                refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                action_busy.set(false);
            });
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
        let mut pending_delete = pending_delete.clone();
        move |_| {
            pending_delete.set(Some(PendingDownloadsDelete::ClearAll));
        }
    };

    let on_confirm_delete = {
        let mut pending_delete = pending_delete.clone();
        let mut action_status = action_status.clone();
        let mut refresh_nonce = refresh_nonce.clone();
        let mut selected_song_keys = selected_song_keys.clone();
        let mut selected_collection_modal = selected_collection_modal.clone();
        move |_| {
            let Some(action) = pending_delete() else {
                return;
            };
            pending_delete.set(None);

            match action {
                PendingDownloadsDelete::Song {
                    server_id,
                    song_id,
                    title,
                } => {
                    let _ = remove_downloaded_song(&server_id, &song_id);
                    selected_song_keys.with_mut(|keys| {
                        keys.remove(&download_song_key(&server_id, &song_id));
                    });
                    action_status.set(Some(format!("Removed \"{title}\".")));
                }
                PendingDownloadsDelete::Collection {
                    kind,
                    server_id,
                    collection_id,
                    name,
                } => {
                    if kind == "album" {
                        let removed = remove_downloaded_album(&server_id, &collection_id, &name);
                        action_status.set(Some(format!(
                            "Removed {removed} song(s) from album \"{name}\"."
                        )));
                    } else {
                        let _ = remove_downloaded_collection(&kind, &server_id, &collection_id);
                        action_status
                            .set(Some(format!("Removed playlist \"{name}\" from downloads.")));
                    }
                    selected_collection_modal.set(None);
                }
                PendingDownloadsDelete::ClearAll => {
                    let removed = clear_downloads();
                    selected_song_keys.set(HashSet::new());
                    selected_collection_modal.set(None);
                    action_status.set(Some(format!("Removed {removed} downloaded songs.")));
                }
            }

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
        div { class: "space-y-6",
            // Header
            header { class: "page-header gap-2",
                div {
                    h1 { class: "page-title", "Downloads" }
                    p { class: "page-subtitle", "Browse your offline library" }
                }
            }

            if !native_downloads_supported {
                section { class: "bg-amber-500/10 border border-amber-500/40 rounded-xl p-4",
                    p { class: "text-sm text-amber-200",
                        "Downloads are only available in native builds. Web builds can stream but do not persist offline files."
                    }
                }
            }

            // Search bar
            div { class: "flex gap-3",
                div { class: "relative flex-1 max-w-md",
                    Icon {
                        name: "search".to_string(),
                        class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2.5 rounded-lg bg-zinc-800/50 border border-zinc-700/50 text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50",
                        placeholder: "Search downloads...",
                        value: search_query,
                        oninput: move |evt| search_query.set(evt.value()),
                    }
                }
            }

            // Songs Section with horizontal scroll
            section { class: "space-y-3",
                div { class: "flex items-center justify-between gap-3",
                    h2 { class: "text-lg font-semibold text-white flex items-center gap-2",
                        Icon {
                            name: "music".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                        "Songs"
                    }
                    span { class: "text-sm text-zinc-500", "{entries.len()} songs" }
                }

                // Sort filter pills
                div { class: "flex flex-wrap gap-2",
                    for (sort_type , label) in [
                        (DownloadSongSort::Newest, "Newest"),
                        (DownloadSongSort::Title, "A-Z"),
                        (DownloadSongSort::Artist, "Artist"),
                        (DownloadSongSort::Album, "Album"),
                        (DownloadSongSort::Size, "Size"),
                    ]
                        .iter()
                    {
                        button {
                            class: if song_sort() == *sort_type { "px-3 py-1.5 rounded-full bg-emerald-500/20 text-emerald-300 text-xs font-medium" } else { "px-3 py-1.5 rounded-full bg-zinc-800/50 text-zinc-400 text-xs font-medium hover:text-zinc-200" },
                            onclick: move |_| song_sort.set(*sort_type),
                            "{label}"
                        }
                    }
                }

                // Horizontal scroll carousel for songs
                if entries.is_empty() {
                    div { class: "text-center text-sm text-zinc-500 py-8", "No downloaded songs yet." }
                } else {
                    div { class: "page-carousel",
                        div { class: "flex gap-3 min-w-min",
                            for entry in visible_song_entries.iter().take(20) {
                                {
                                    let entry = entry.clone();
                                    let selection_key = download_song_key(&entry.server_id, &entry.song_id);
                                    let _is_selected = selected_song_keys().contains(&selection_key);
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
                                    let cover_url = cover_id
                                        .as_ref()
                                        .and_then(|cover| {
                                            servers_snapshot
                                                .iter()
                                                .find(|server| server.id == entry.server_id)
                                                .map(|server| {
                                                    NavidromeClient::new(server.clone())
                                                        .get_cover_art_url(cover, 120)
                                                })
                                        });
                                    rsx! {
                                        div {
                                            key: "{entry.server_id}:{entry.song_id}",
                                            class: "w-28 flex-shrink-0 rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-2 flex flex-col gap-2 hover:bg-zinc-900/60 transition-colors",
                                            if let Some(url) = cover_url {
                                                button {
                                                    class: "w-full aspect-square rounded-lg overflow-hidden border border-zinc-800/80",
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
                                                    img {
                                                        src: "{url}",
                                                        alt: "{entry.title}",
                                                        class: "w-full h-full object-cover",
                                                        loading: "lazy",
                                                    }
                                                }
                                            } else {
                                                div { class: "w-full aspect-square rounded-lg bg-zinc-800 border border-zinc-800/80" }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-xs font-medium text-white truncate", "{entry.title}" }
                                                p { class: "text-[11px] text-zinc-400 truncate",
                                                    "{entry.artist.clone().unwrap_or_else(|| \"Unknown\".to_string())}"
                                                }
                                            }
                                            div { class: "flex gap-1",
                                                button {
                                                    class: "flex-1 px-2 py-1 rounded text-[10px] border border-emerald-500/50 text-emerald-300 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors",
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
                                                    Icon {
                                                        name: "play".to_string(),
                                                        class: "w-3 h-3 mx-auto".to_string(),
                                                    }
                                                }
                                                button {
                                                    class: "flex-1 px-2 py-1 rounded text-[10px] border border-rose-500/50 text-rose-300 hover:bg-rose-500 hover:border-rose-500 hover:text-white transition-colors",
                                                    onclick: {
                                                        let entry = entry.clone();
                                                        let mut pending_delete = pending_delete.clone();
                                                        move |_| {
                                                            pending_delete
                                                                .set(
                                                                    Some(PendingDownloadsDelete::Song {
                                                                        server_id: entry.server_id.clone(),
                                                                        song_id: entry.song_id.clone(),
                                                                        title: entry.title.clone(),
                                                                    }),
                                                                );
                                                        }
                                                    },
                                                    Icon {
                                                        name: "trash".to_string(),
                                                        class: "w-3 h-3 mx-auto".to_string(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Albums Section with horizontal scroll
            section { class: "space-y-3",
                div { class: "flex items-center justify-between gap-3",
                    h2 { class: "text-lg font-semibold text-white flex items-center gap-2",
                        Icon {
                            name: "album".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                        "Albums"
                    }
                    span { class: "text-sm text-zinc-500", "{filtered_albums.len()} albums" }
                }

                // Sort filter pills
                div { class: "flex flex-wrap gap-2",
                    for (sort_val , label) in [("title", "A-Z"), ("recent", "Recent"), ("oldest", "Oldest")].iter() {
                        button {
                            class: if album_sort() == *sort_val { "px-3 py-1.5 rounded-full bg-emerald-500/20 text-emerald-300 text-xs font-medium" } else { "px-3 py-1.5 rounded-full bg-zinc-800/50 text-zinc-400 text-xs font-medium hover:text-zinc-200" },
                            onclick: move |_| album_sort.set(*sort_val),
                            "{label}"
                        }
                    }
                }

                // Horizontal scroll carousel for albums
                if filtered_albums.is_empty() {
                    div { class: "text-center text-sm text-zinc-500 py-8", "No downloaded albums yet." }
                } else {
                    div { class: "page-carousel",
                        div { class: "flex gap-3 min-w-min",
                            for album in visible_albums.iter() {
                                {
                                    let album = album.clone();
                                    let (downloaded_song_count, total_song_count, missing_song_count) =
                                        collection_progress(&album, &collection_download_counts);
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
                                    let cover_url = cover_id
                                        .as_ref()
                                        .and_then(|cover_id| {
                                            servers_snapshot
                                                .iter()
                                                .find(|server| server.id == album.server_id)
                                                .map(|server| {
                                                    NavidromeClient::new(server.clone())
                                                        .get_cover_art_url(cover_id, 140)
                                                })
                                        });
                                    rsx! {
                                        button {
                                            key: "album:{album.server_id}:{album.collection_id}",
                                            class: "w-28 flex-shrink-0 rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-2 flex flex-col gap-2 hover:bg-zinc-900/60 transition-colors text-left",
                                            onclick: {
                                                let album = album.clone();
                                                let mut selected_collection_modal = selected_collection_modal.clone();
                                                move |_| selected_collection_modal.set(Some(album.clone()))
                                            },
                                            if let Some(url) = cover_url {
                                                img {
                                                    src: "{url}",
                                                    alt: "{album.name}",
                                                    class: "w-full aspect-square rounded-lg object-cover border border-zinc-800/80",
                                                    loading: "lazy",
                                                }
                                            } else {
                                                div { class: "w-full aspect-square rounded-lg bg-zinc-800 border border-zinc-800/80" }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-xs font-medium text-white truncate", "{album.name}" }
                                                p { class: "text-[11px] text-zinc-400 truncate",
                                                    "{downloaded_song_count}/{total_song_count} downloaded"
                                                }
                                                p { class: if missing_song_count == 0 { "text-[10px] text-emerald-300 truncate" } else { "text-[10px] text-amber-300 truncate" },
                                                    if missing_song_count == 0 {
                                                        "Complete"
                                                    } else {
                                                        "{missing_song_count} missing"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Playlists Section with horizontal scroll
            section { class: "space-y-3",
                div { class: "flex items-center justify-between gap-3",
                    h2 { class: "text-lg font-semibold text-white flex items-center gap-2",
                        Icon {
                            name: "playlist".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                        "Playlists"
                    }
                    span { class: "text-sm text-zinc-500", "{filtered_playlists.len()} playlists" }
                }

                // Sort filter pills
                div { class: "flex flex-wrap gap-2",
                    for (sort_val , label) in [("title", "A-Z"), ("recent", "Recent"), ("oldest", "Oldest")].iter() {
                        button {
                            class: if playlist_sort() == *sort_val { "px-3 py-1.5 rounded-full bg-emerald-500/20 text-emerald-300 text-xs font-medium" } else { "px-3 py-1.5 rounded-full bg-zinc-800/50 text-zinc-400 text-xs font-medium hover:text-zinc-200" },
                            onclick: move |_| playlist_sort.set(*sort_val),
                            "{label}"
                        }
                    }
                }

                // Horizontal scroll carousel for playlists
                if filtered_playlists.is_empty() {
                    div { class: "text-center text-sm text-zinc-500 py-8",
                        "No downloaded playlists yet."
                    }
                } else {
                    div { class: "page-carousel",
                        div { class: "flex gap-3 min-w-min",
                            for playlist in visible_playlists.iter() {
                                {
                                    let playlist = playlist.clone();
                                    let (downloaded_song_count, total_song_count, missing_song_count) =
                                        collection_progress(&playlist, &collection_download_counts);
                                    let cover_url = collection_song_ids
                                        .get(
                                            &(
                                                "playlist".to_string(),
                                                playlist.server_id.clone(),
                                                playlist.collection_id.clone(),
                                            ),
                                        )
                                        .and_then(|song_ids| {
                                            song_ids
                                                .iter()
                                                .find_map(|song_id| {
                                                    downloaded_entry_lookup
                                                        .get(
                                                            &(playlist.server_id.clone(), song_id.trim().to_string()),
                                                        )
                                                        .and_then(|entry| {
                                                            download_entry_cover_url(entry, &servers_snapshot, 140)
                                                        })
                                                })
                                        });
                                    rsx! {
                                        button {
                                            key: "playlist:{playlist.server_id}:{playlist.collection_id}",
                                            class: "w-28 flex-shrink-0 rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-2 flex flex-col gap-2 hover:bg-zinc-900/60 transition-colors text-left",
                                            onclick: {
                                                let playlist = playlist.clone();
                                                let mut selected_collection_modal = selected_collection_modal.clone();
                                                move |_| selected_collection_modal.set(Some(playlist.clone()))
                                            },
                                            if let Some(url) = cover_url {
                                                img {
                                                    src: "{url}",
                                                    alt: "{playlist.name}",
                                                    class: "w-full aspect-square rounded-lg object-cover border border-zinc-800/80",
                                                    loading: "lazy",
                                                }
                                            } else {
                                                div { class: "w-full aspect-square rounded-lg bg-gradient-to-br from-violet-500/20 to-cyan-500/20 border border-zinc-800/80 flex items-center justify-center",
                                                    Icon {
                                                        name: "playlist".to_string(),
                                                        class: "w-6 h-6 text-zinc-400".to_string(),
                                                    }
                                                }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                p { class: "text-xs font-medium text-white truncate", "{playlist.name}" }
                                                p { class: "text-[11px] text-zinc-400 truncate",
                                                    "{downloaded_song_count}/{total_song_count} downloaded"
                                                }
                                                p { class: if missing_song_count == 0 { "text-[10px] text-emerald-300 truncate" } else { "text-[10px] text-amber-300 truncate" },
                                                    if missing_song_count == 0 {
                                                        "Complete"
                                                    } else {
                                                        "{missing_song_count} missing"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Modal overlay for album/playlist details
            if let Some(modal_collection) = selected_collection_modal() {
                {
                    let collection_key = (
                        modal_collection.kind.clone(),
                        modal_collection.server_id.clone(),
                        modal_collection.collection_id.clone(),
                    );
                    let modal_entries: Vec<DownloadIndexEntry> = if let Some(song_ids) =
                        collection_song_ids.get(&collection_key)
                    {
                        ordered_download_entries_for_song_ids(
                            &modal_collection.server_id,
                            song_ids,
                            &downloaded_entry_lookup,
                        )
                    } else if modal_collection.kind == "album" {
                        if modal_collection.collection_id.starts_with("name:") {
                            all_entries
                                .iter()
                                .filter(|entry| {
                                    entry.server_id == modal_collection.server_id
                                        && entry
                                            .album
                                            .as_ref()
                                            .is_some_and(|album| album.trim() == modal_collection.name)
                                })
                                .cloned()
                                .collect()
                        } else {
                            all_entries
                                .iter()
                                .filter(|entry| {
                                    entry.server_id == modal_collection.server_id
                                        && entry
                                            .album_id
                                            .as_ref()
                                            .is_some_and(|id| id == &modal_collection.collection_id)
                                })
                                .cloned()
                                .collect()
                        }
                    } else {
                        Vec::new()
                    };
                    let cover_id = if modal_collection.kind == "album" {
                        album_cover_ids
                            .get(
                                &(
                                    modal_collection.server_id.clone(),
                                    modal_collection.collection_id.clone(),
                                ),
                            )
                            .cloned()
                            .or_else(|| {
                                modal_entries
                                    .first()
                                    .and_then(|entry| {
                                        entry.cover_art_id.clone().or_else(|| entry.album_id.clone())
                                    })
                            })
                    } else {
                        modal_entries
                            .first()
                            .and_then(|entry| {
                                entry.cover_art_id.clone().or_else(|| entry.album_id.clone())
                            })
                    };
                    let cover_url = cover_id
                        .as_ref()
                        .and_then(|cover_id| {
                            servers_snapshot
                                .iter()
                                .find(|server| server.id == modal_collection.server_id)
                                .map(|server| {
                                    NavidromeClient::new(server.clone())
                                        .get_cover_art_url(cover_id, 200)
                                })
                        });
                    let modal_kind_label = if modal_collection.kind == "playlist" {
                        "playlist"
                    } else {
                        "album"
                    };
                    let collection_detail_target = collection_detail_view(&modal_collection);
                    let (_, modal_total_song_count, modal_missing_song_count) = collection_progress(
                        &modal_collection,
                        &collection_download_counts,
                    );
                    rsx! {
                        div {
                            class: "fixed inset-0 z-[210] bg-zinc-950/95 backdrop-blur-sm overflow-y-auto px-4 py-8 flex items-center justify-center",
                            onclick: {
                                let mut selected_collection_modal = selected_collection_modal.clone();
                                move |_| selected_collection_modal.set(None)
                            },
                            div {
                                class: "w-full max-w-2xl max-h-[calc(100dvh-2rem)] md:max-h-[80vh] bg-zinc-900/60 border border-zinc-700/50 rounded-2xl p-4 md:p-6 flex flex-col gap-4 overflow-hidden min-h-0",
                                onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                div { class: "flex items-start justify-between gap-4",
                                    div { class: "flex items-start gap-4 min-w-0",
                                        if let Some(url) = cover_url {
                                            img {
                                                src: "{url}",
                                                alt: "{modal_collection.name}",
                                                class: "w-24 h-24 rounded-lg object-cover border border-zinc-800/80 flex-shrink-0",
                                                loading: "lazy",
                                            }
                                        } else {
                                            div { class: "w-24 h-24 rounded-lg border border-zinc-800/80 bg-gradient-to-br from-zinc-800 to-zinc-900 flex items-center justify-center flex-shrink-0",
                                                Icon {
                                                    name: if modal_collection.kind == "playlist" { "playlist".to_string() } else { "album".to_string() },
                                                    class: "w-8 h-8 text-zinc-500".to_string(),
                                                }
                                            }
                                        }
                                        div { class: "min-w-0 space-y-1",
                                            p { class: "text-[11px] uppercase tracking-[0.2em] text-zinc-500",
                                                "{modal_kind_label}"
                                            }
                                            h2 { class: "text-xl font-semibold text-white truncate", "{modal_collection.name}" }
                                            p { class: if modal_missing_song_count == 0 { "text-sm text-emerald-300" } else { "text-sm text-zinc-400" },
                                                if modal_missing_song_count == 0 {
                                                    "{modal_entries.len()} of {modal_total_song_count} downloaded"
                                                } else {
                                                    "{modal_entries.len()} of {modal_total_song_count} downloaded • {modal_missing_song_count} missing"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "p-1 hover:bg-zinc-800/50 rounded-lg transition-colors flex-shrink-0",
                                        onclick: {
                                            let mut selected_collection_modal = selected_collection_modal.clone();
                                            move |_| selected_collection_modal.set(None)
                                        },
                                        Icon {
                                            name: "x".to_string(),
                                            class: "w-5 h-5 text-zinc-400".to_string(),
                                        }
                                    }
                                }

                                div { class: "flex flex-wrap gap-2",
                                    button {
                                        class: if modal_entries.is_empty() { "p-2.5 rounded-full border border-zinc-800 text-zinc-600 cursor-not-allowed" } else { "p-2.5 rounded-full border border-emerald-500/50 text-emerald-300 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors" },
                                        title: "Play {modal_kind_label}",
                                        aria_label: "Play {modal_kind_label}",
                                        disabled: modal_entries.is_empty(),
                                        onclick: {
                                            let modal_entries = modal_entries.clone();
                                            let servers_snapshot = servers_snapshot.clone();
                                            let queue = queue.clone();
                                            let queue_index = queue_index.clone();
                                            let now_playing = now_playing.clone();
                                            let is_playing = is_playing.clone();
                                            move |_| {
                                                let _ = replace_queue_with_download_entries(
                                                    &modal_entries,
                                                    &servers_snapshot,
                                                    queue,
                                                    queue_index,
                                                    now_playing,
                                                    is_playing,
                                                );
                                            }
                                        },
                                        Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                                    }
                                    button {
                                        class: if modal_entries.is_empty() { "p-2.5 rounded-full border border-zinc-800 text-zinc-600 cursor-not-allowed" } else { "p-2.5 rounded-full border border-emerald-500/50 text-emerald-300 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors" },
                                        title: "Open add menu",
                                        aria_label: "Open add menu",
                                        disabled: modal_entries.is_empty(),
                                        onclick: {
                                            let modal_entries = modal_entries.clone();
                                            let servers_snapshot = servers_snapshot.clone();
                                            let mut add_menu = add_menu.clone();
                                            let collection_name = modal_collection.name.clone();
                                            move |_| {
                                                let songs: Vec<Song> = modal_entries
                                                    .iter()
                                                    .map(|entry| to_download_song(entry, &servers_snapshot))
                                                    .collect();
                                                if songs.is_empty() {
                                                    return;
                                                }
                                                add_menu.open(AddIntent::from_songs(collection_name.clone(), songs));
                                            }
                                        },
                                        Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                                    }
                                    button {
                                        class: if modal_entries.is_empty() { "p-2.5 rounded-full border border-zinc-800 text-zinc-600 cursor-not-allowed" } else { "p-2.5 rounded-full border border-amber-500/50 text-amber-300 hover:bg-amber-500 hover:border-amber-500 hover:text-white transition-colors" },
                                        title: "Shuffle {modal_kind_label}",
                                        aria_label: "Shuffle {modal_kind_label}",
                                        disabled: modal_entries.is_empty(),
                                        onclick: {
                                            let modal_entries = modal_entries.clone();
                                            let servers_snapshot = servers_snapshot.clone();
                                            let queue = queue.clone();
                                            let queue_index = queue_index.clone();
                                            let now_playing = now_playing.clone();
                                            let is_playing = is_playing.clone();
                                            move |_| {
                                                let _ = replace_queue_with_shuffled_download_entries(
                                                    &modal_entries,
                                                    &servers_snapshot,
                                                    queue,
                                                    queue_index,
                                                    now_playing,
                                                    is_playing,
                                                );
                                            }
                                        },
                                        Icon { name: "shuffle".to_string(), class: "w-4 h-4".to_string() }
                                    }
                                    button {
                                        class: "p-2.5 rounded-full border border-rose-500/50 text-rose-300 hover:bg-rose-500 hover:border-rose-500 hover:text-white transition-colors",
                                        title: "Delete download",
                                        aria_label: "Delete download",
                                        onclick: {
                                            let modal_collection = modal_collection.clone();
                                            let mut pending_delete = pending_delete.clone();
                                            move |_| {
                                                pending_delete
                                                    .set(
                                                        Some(PendingDownloadsDelete::Collection {
                                                            kind: modal_collection.kind.clone(),
                                                            server_id: modal_collection.server_id.clone(),
                                                            collection_id: modal_collection.collection_id.clone(),
                                                            name: modal_collection.name.clone(),
                                                        }),
                                                    );
                                            }
                                        },
                                        Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                                    }
                                    button {
                                        class: if collection_detail_target.is_some() { "p-2.5 rounded-full border border-cyan-500/50 text-cyan-300 hover:bg-cyan-500 hover:border-cyan-500 hover:text-white transition-colors" } else { "p-2.5 rounded-full border border-zinc-800 text-zinc-600 cursor-not-allowed" },
                                        title: if modal_collection.kind == "playlist" { "Open playlist page" } else { "Open album page" },
                                        aria_label: if modal_collection.kind == "playlist" { "Open playlist page" } else { "Open album page" },
                                        disabled: collection_detail_target.is_none(),
                                        onclick: {
                                            let mut selected_collection_modal = selected_collection_modal.clone();
                                            let navigation = navigation;
                                            let collection_detail_target = collection_detail_target.clone();
                                            move |_| {
                                                let Some(target) = collection_detail_target.clone() else {
                                                    return;
                                                };
                                                selected_collection_modal.set(None);
                                                navigation.navigate_to(target);
                                            }
                                        },
                                        Icon { name: "eye".to_string(), class: "w-4 h-4".to_string() }
                                    }
                                }

                                if modal_entries.is_empty() {
                                    p { class: "text-sm text-zinc-400 text-center py-8",
                                        "No downloaded songs found for this {modal_kind_label}."
                                    }
                                } else {
                                    div { class: "touch-scroll-y flex-1 space-y-1 -mx-4 md:-mx-6 px-4 md:px-6 pb-1",
                                        for entry in modal_entries.iter() {
                                            {
                                                let entry = entry.clone();
                                                let cover_url = download_entry_cover_url(&entry, &servers_snapshot, 80);
                                                rsx! {
                                                    div {
                                                        key: "{entry.server_id}:{entry.song_id}",
                                                        class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors group",
                                                        div { class: "flex items-center gap-3 flex-1 min-w-0",
                                                            if let Some(url) = cover_url {
                                                                img {
                                                                    src: "{url}",
                                                                    alt: "{entry.title}",
                                                                    class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80 flex-shrink-0",
                                                                    loading: "lazy",
                                                                }
                                                            } else {
                                                                div { class: "w-10 h-10 rounded-md border border-zinc-800/80 bg-gradient-to-br from-violet-500/15 to-cyan-500/15 flex items-center justify-center flex-shrink-0",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-4 h-4 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            }
                                                            div { class: "min-w-0 flex-1",
                                                                p { class: "text-sm text-white truncate", "{entry.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate",
                                                                    "{entry.artist.clone().unwrap_or_else(|| \"Unknown\".to_string())}"
                                                                }
                                                            }
                                                        }
                                                        div { class: "flex gap-1",
                                                            button {
                                                                class: "p-1.5 rounded text-emerald-300 hover:text-emerald-200 hover:bg-emerald-500/20 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                                                                title: "Play song",
                                                                aria_label: "Play song",
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
                                                                Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                                                            }
                                                            button {
                                                                class: "p-1.5 rounded text-rose-300 hover:text-rose-200 hover:bg-rose-500/20 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                                                                title: "Delete song",
                                                                aria_label: "Delete song",
                                                                onclick: {
                                                                    let entry = entry.clone();
                                                                    let mut pending_delete = pending_delete.clone();
                                                                    move |_| {
                                                                        pending_delete
                                                                            .set(
                                                                                Some(PendingDownloadsDelete::Song {
                                                                                    server_id: entry.server_id.clone(),
                                                                                    song_id: entry.song_id.clone(),
                                                                                    title: entry.title.clone(),
                                                                                }),
                                                                            );
                                                                    }
                                                                },
                                                                Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Settings section moved to bottom
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6 space-y-5 mt-8",
                h2 { class: "text-lg font-semibold text-white flex items-center gap-2",
                    Icon {
                        name: "settings".to_string(),
                        class: "w-5 h-5".to_string(),
                    }
                    "Downloads Settings"
                }
                p { class: "text-sm text-zinc-400",
                    "Use the settings page to edit download preferences."
                }

                div { class: "grid grid-cols-1 md:grid-cols-4 gap-4",
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500",
                            "Downloaded Songs"
                        }
                        p { class: "text-2xl font-semibold text-white mt-2", "{stats.song_count}" }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500",
                            "Downloaded Albums"
                        }
                        p { class: "text-2xl font-semibold text-white mt-2",
                            "{downloaded_albums.len()}"
                        }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500",
                            "Downloaded Playlists"
                        }
                        p { class: "text-2xl font-semibold text-white mt-2",
                            "{downloaded_playlists.len()}"
                        }
                    }
                    div { class: "bg-zinc-900/50 rounded-xl p-4",
                        p { class: "text-xs uppercase tracking-wider text-zinc-500",
                            "Storage Used"
                        }
                        p { class: "text-2xl font-semibold text-white mt-2",
                            "{format_size(stats.total_size_bytes)}"
                        }
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
                            p { class: "text-xs uppercase tracking-wider text-zinc-500",
                                "Active Downloads"
                            }
                            p { class: "text-xs text-emerald-300",
                                "{active_downloads.len()} in progress"
                            }
                        }
                        div { class: "max-h-40 overflow-y-auto rounded-xl border border-zinc-700/50 bg-zinc-900/40 p-2 space-y-1",
                            for entry in active_downloads.iter().take(30) {
                                div {
                                    key: "active:{entry.server_id}:{entry.song_id}",
                                    class: "flex items-center justify-between gap-3 px-2 py-1.5 rounded-lg bg-zinc-900/50",
                                    div { class: "min-w-0",
                                        p { class: "text-sm text-zinc-200 truncate",
                                            "{entry.title}"
                                        }
                                        p { class: "text-xs text-zinc-500 truncate",
                                            "{entry.artist.clone().unwrap_or_else(|| \"Unknown artist\".to_string())}"
                                        }
                                    }
                                    Icon {
                                        name: "loader".to_string(),
                                        class: "w-4 h-4 text-emerald-400 flex-shrink-0 animate-spin".to_string(),
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "grid grid-cols-2 gap-2 pt-2 sm:flex sm:flex-wrap sm:items-center sm:gap-3",
                    button {
                        class: if settings.downloads_enabled { "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 text-center flex items-center justify-center gap-2 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors" } else { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 text-center flex items-center justify-center gap-2 hover:bg-zinc-700 hover:border-zinc-500 hover:text-white transition-colors" },
                        onclick: on_toggle_downloads,
                        Icon {
                            name: if settings.downloads_enabled { "check".to_string() } else { "x".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                        if settings.downloads_enabled {
                            "Downloads ON"
                        } else {
                            "Downloads OFF"
                        }
                    }
                    button {
                        class: if settings.auto_downloads_enabled { "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 text-center flex items-center justify-center gap-2 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors" } else { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 text-center flex items-center justify-center gap-2 hover:bg-zinc-700 hover:border-zinc-500 hover:text-white transition-colors" },
                        onclick: on_toggle_auto_downloads,
                        Icon {
                            name: if settings.auto_downloads_enabled { "check".to_string() } else { "x".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                        if settings.auto_downloads_enabled {
                            "Auto-Download ON"
                        } else {
                            "Auto-Download OFF"
                        }
                    }
                    button {
                        class: if action_busy() { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-center flex items-center justify-center gap-2" } else { "w-full sm:w-auto px-3 py-2 rounded-lg border border-emerald-500/50 text-emerald-300 hover:bg-emerald-500 hover:border-emerald-500 hover:text-white transition-colors text-center flex items-center justify-center gap-2" },
                        disabled: action_busy(),
                        onclick: on_run_auto,
                        Icon {
                            name: "download".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        if action_busy() {
                            "Running..."
                        } else {
                            "Run Now"
                        }
                    }
                    button {
                        class: if action_busy() { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-center flex items-center justify-center gap-2" } else { "w-full sm:w-auto px-3 py-2 rounded-lg border border-cyan-500/50 text-cyan-300 hover:bg-cyan-500 hover:border-cyan-500 hover:text-white transition-colors text-center flex items-center justify-center gap-2" },
                        disabled: action_busy(),
                        onclick: on_refresh_cached_assets,
                        Icon {
                            name: "refresh-cw".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        if action_busy() {
                            "Working..."
                        } else {
                            "Refresh"
                        }
                    }
                    button {
                        class: if action_busy() { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-center flex items-center justify-center gap-2" } else { "w-full sm:w-auto px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:bg-zinc-700 hover:border-zinc-500 hover:text-white transition-colors text-center flex items-center justify-center gap-2" },
                        disabled: action_busy(),
                        onclick: on_refresh,
                        Icon {
                            name: "refresh-cw".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        if action_busy() {
                            "Refreshing..."
                        } else {
                            "Refresh"
                        }
                    }
                    button {
                        class: "w-full sm:w-auto px-3 py-2 rounded-lg border border-rose-500/50 text-rose-300 hover:bg-rose-500 hover:border-rose-500 hover:text-white transition-colors text-center flex items-center justify-center gap-2",
                        onclick: on_clear_downloads,
                        Icon {
                            name: "trash".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Clear"
                    }
                }
                if let Some(status) = action_status() {
                    p { class: "text-xs text-zinc-400 mt-3", "{status}" }
                }
            }
            if let Some(delete_action) = pending_delete() {
                div {
                    class: "fixed inset-0 z-[220] bg-black/60 backdrop-blur-sm flex items-center justify-center px-4",
                    onclick: {
                        let mut pending_delete = pending_delete.clone();
                        move |_| pending_delete.set(None)
                    },
                    div {
                        class: "w-full max-w-md rounded-2xl border border-zinc-700 bg-zinc-900 p-6 shadow-2xl",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        h2 { class: "text-xl font-semibold text-white mb-3",
                            "{pending_delete_title(&delete_action)}"
                        }
                        p { class: "text-sm text-zinc-300 mb-6",
                            "{pending_delete_message(&delete_action)}"
                        }
                        div { class: "flex items-center justify-end gap-3",
                            button {
                                class: "px-4 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors",
                                onclick: {
                                    let mut pending_delete = pending_delete.clone();
                                    move |_| pending_delete.set(None)
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg bg-rose-600 text-white hover:bg-rose-500 transition-colors",
                                onclick: on_confirm_delete,
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}
