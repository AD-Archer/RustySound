#[cfg(not(target_arch = "wasm32"))]
use crate::api::{
    fetch_lyrics_with_fallback, normalize_lyrics_provider_order, LyricsQuery, NavidromeClient,
};
use crate::api::{ServerConfig, Song};
use crate::db::AppSettings;
#[cfg(not(target_arch = "wasm32"))]
use crate::db::ArtworkDownloadPreference;
use serde::{Deserialize, Serialize};

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "linux")
))]
use base64::{engine::general_purpose, Engine as _};
#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "macos", target_os = "linux"))
))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
static AUDIO_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[cfg(not(target_arch = "wasm32"))]
const AUDIO_CACHE_SUBDIR: &str = "audio_cache";
#[cfg(not(target_arch = "wasm32"))]
const DOWNLOAD_INDEX_FILE: &str = "download_index.json";
#[cfg(not(target_arch = "wasm32"))]
const COLLECTION_INDEX_FILE: &str = "download_collections.json";
#[cfg(not(target_arch = "wasm32"))]
const DOWNLOAD_ARTWORK_SIZES: [u32; 7] = [80, 100, 120, 160, 300, 500, 512];
#[cfg(not(target_arch = "wasm32"))]
const CACHE_AUDIO_EXTENSIONS: [&str; 8] =
    ["audio", "mp3", "flac", "ogg", "m4a", "aac", "wav", "mp4"];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DownloadStats {
    pub song_count: usize,
    pub total_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AutoDownloadReport {
    pub attempted: usize,
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub purged: usize,
    pub indexed: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DownloadBatchReport {
    pub attempted: usize,
    pub downloaded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub purged: usize,
    pub indexed: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DownloadCacheRefreshReport {
    pub scanned: usize,
    pub missing_servers: usize,
    pub lyrics_attempted: usize,
    pub lyrics_warmed: usize,
    pub artwork_refreshed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DownloadIndexEntry {
    pub server_id: String,
    #[serde(default)]
    pub server_name: Option<String>,
    pub song_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(default)]
    pub album_id: Option<String>,
    #[serde(default)]
    pub cover_art_id: Option<String>,
    pub size_bytes: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DownloadCollectionEntry {
    pub kind: String,
    pub server_id: String,
    pub collection_id: String,
    pub name: String,
    pub song_count: usize,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ActiveDownloadEntry {
    pub server_id: String,
    pub song_id: String,
    pub title: String,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default)]
    pub album: Option<String>,
    pub started_at_ms: u64,
}

#[cfg(not(target_arch = "wasm32"))]
static ACTIVE_DOWNLOADS: Lazy<std::sync::Mutex<Vec<ActiveDownloadEntry>>> =
    Lazy::new(|| std::sync::Mutex::new(Vec::new()));

#[cfg(not(target_arch = "wasm32"))]
fn now_timestamp_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(not(target_arch = "wasm32"))]
fn sanitize_file_component(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_cache_dir() -> Option<PathBuf> {
    let base = dirs::cache_dir()?
        .join("rustysound")
        .join(AUDIO_CACHE_SUBDIR);
    let _ = fs::create_dir_all(&base);
    Some(base)
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_cache_file_path_by_ids(server_id: &str, song_id: &str) -> Option<PathBuf> {
    let dir = audio_cache_dir()?;
    let sid = sanitize_file_component(server_id);
    let sanitized_song_id = sanitize_file_component(song_id);
    let stem = format!("{sid}__{sanitized_song_id}");

    for ext in CACHE_AUDIO_EXTENSIONS {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    Some(dir.join(format!("{stem}.audio")))
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_cache_file_path(song: &Song) -> Option<PathBuf> {
    let dir = audio_cache_dir()?;
    let sid = sanitize_file_component(&song.server_id);
    let sanitized_song_id = sanitize_file_component(&song.id);
    let stem = format!("{sid}__{sanitized_song_id}");

    for ext in CACHE_AUDIO_EXTENSIONS {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let preferred_ext = preferred_audio_file_extension(song);
    Some(dir.join(format!("{stem}.{preferred_ext}")))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "macos", target_os = "linux"))
))]
fn path_to_file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.starts_with('/') {
        format!("file://{normalized}")
    } else {
        format!("file:///{normalized}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn preferred_audio_file_extension(song: &Song) -> &'static str {
    let suffix = song
        .suffix
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match suffix.as_str() {
        "mp3" => "mp3",
        "flac" => "flac",
        "ogg" | "oga" => "ogg",
        "m4a" | "mp4" => "m4a",
        "aac" => "aac",
        "wav" => "wav",
        _ => {
            let content_type = song
                .content_type
                .as_deref()
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            match content_type.as_str() {
                "audio/flac" => "flac",
                "audio/ogg" => "ogg",
                "audio/mp4" => "m4a",
                "audio/aac" => "aac",
                "audio/wav" | "audio/x-wav" => "wav",
                "audio/mpeg" => "mp3",
                _ => "audio",
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn is_cached_audio_extension(ext: &str) -> bool {
    CACHE_AUDIO_EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(ext))
}

#[cfg(not(target_arch = "wasm32"))]
fn remove_audio_cache_files_by_ids(server_id: &str, song_id: &str) -> usize {
    let Some(dir) = audio_cache_dir() else {
        return 0;
    };

    let sid = sanitize_file_component(server_id);
    let sanitized_song_id = sanitize_file_component(song_id);
    let stem = format!("{sid}__{sanitized_song_id}");
    let mut removed = 0usize;

    for ext in CACHE_AUDIO_EXTENSIONS {
        let candidate = dir.join(format!("{stem}.{ext}"));
        if candidate.exists() && fs::remove_file(&candidate).is_ok() {
            removed = removed.saturating_add(1);
        }
    }

    removed
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "linux")
))]
fn audio_mime_type(song: &Song) -> &'static str {
    if let Some(content_type) = song.content_type.as_deref() {
        let normalized = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if normalized.starts_with("audio/") {
            return match normalized.as_str() {
                "audio/mpeg" => "audio/mpeg",
                "audio/flac" => "audio/flac",
                "audio/ogg" => "audio/ogg",
                "audio/mp4" => "audio/mp4",
                "audio/aac" => "audio/aac",
                "audio/wav" => "audio/wav",
                "audio/x-wav" => "audio/wav",
                _ => "audio/mpeg",
            };
        }
    }

    match song
        .suffix
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        _ => "audio/mpeg",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn index_file_path() -> Option<PathBuf> {
    Some(audio_cache_dir()?.join(DOWNLOAD_INDEX_FILE))
}

#[cfg(not(target_arch = "wasm32"))]
fn collection_index_file_path() -> Option<PathBuf> {
    Some(audio_cache_dir()?.join(COLLECTION_INDEX_FILE))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_download_index() -> Vec<DownloadIndexEntry> {
    let Some(path) = index_file_path() else {
        return Vec::new();
    };
    let Ok(json) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<DownloadIndexEntry>>(&json).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn save_download_index(index: &[DownloadIndexEntry]) {
    let Some(path) = index_file_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(index) {
        let _ = fs::write(path, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_collection_index() -> Vec<DownloadCollectionEntry> {
    let Some(path) = collection_index_file_path() else {
        return Vec::new();
    };
    let Ok(json) = fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<DownloadCollectionEntry>>(&json).unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn save_collection_index(index: &[DownloadCollectionEntry]) {
    let Some(path) = collection_index_file_path() else {
        return;
    };
    if let Ok(json) = serde_json::to_string(index) {
        let _ = fs::write(path, json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn upsert_download_index(song: &Song, size_bytes: u64) {
    let mut index = load_download_index();
    if let Some(entry) = index
        .iter_mut()
        .find(|entry| entry.server_id == song.server_id && entry.song_id == song.id)
    {
        entry.server_name = if song.server_name.trim().is_empty() {
            None
        } else {
            Some(song.server_name.clone())
        };
        entry.title = song.title.clone();
        entry.artist = song.artist.clone();
        entry.album = song.album.clone();
        entry.album_id = song.album_id.clone();
        entry.cover_art_id = song.cover_art.clone();
        entry.size_bytes = size_bytes;
        entry.updated_at_ms = now_timestamp_millis();
    } else {
        index.push(DownloadIndexEntry {
            server_id: song.server_id.clone(),
            server_name: if song.server_name.trim().is_empty() {
                None
            } else {
                Some(song.server_name.clone())
            },
            song_id: song.id.clone(),
            title: song.title.clone(),
            artist: song.artist.clone(),
            album: song.album.clone(),
            album_id: song.album_id.clone(),
            cover_art_id: song.cover_art.clone(),
            size_bytes,
            updated_at_ms: now_timestamp_millis(),
        });
    }
    save_download_index(&index);
}

#[cfg(not(target_arch = "wasm32"))]
fn purge_index_missing_files() -> Vec<DownloadIndexEntry> {
    let mut index = load_download_index();
    let original_len = index.len();
    index.retain(|entry| {
        audio_cache_file_path_by_ids(&entry.server_id, &entry.song_id)
            .is_some_and(|path| path.exists())
    });
    if index.len() != original_len {
        save_download_index(&index);
    }
    index
}

#[cfg(not(target_arch = "wasm32"))]
fn prune_audio_cache(max_size_mb: u32) {
    let Some(dir) = audio_cache_dir() else {
        return;
    };

    let max_bytes = (max_size_mb.clamp(25, 131_072) as u64) * 1024 * 1024;
    let mut entries = Vec::<(PathBuf, u64, std::time::SystemTime)>::new();
    let mut total_bytes = 0u64;

    let Ok(read_dir) = fs::read_dir(&dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == DOWNLOAD_INDEX_FILE || name == COLLECTION_INDEX_FILE)
        {
            continue;
        }
        let size = meta.len();
        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        total_bytes = total_bytes.saturating_add(size);
        entries.push((path, size, modified));
    }

    if total_bytes <= max_bytes {
        return;
    }

    entries.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in entries {
        if total_bytes <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total_bytes = total_bytes.saturating_sub(size);
        }
    }
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "linux")
))]
pub fn cached_audio_url(song: &Song) -> Option<String> {
    let path = audio_cache_file_path(song)?;
    if !path.exists() {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let encoded = general_purpose::STANDARD.encode(bytes);
    let mime = audio_mime_type(song);
    Some(format!("data:{mime};base64,{encoded}"))
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(any(target_os = "macos", target_os = "linux"))
))]
pub fn cached_audio_url(song: &Song) -> Option<String> {
    let path = audio_cache_file_path(song)?;
    if path.exists() {
        Some(path_to_file_url(&path))
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
pub fn cached_audio_url(_song: &Song) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn is_song_downloaded(song: &Song) -> bool {
    audio_cache_file_path(song).is_some_and(|path| path.exists())
}

#[cfg(target_arch = "wasm32")]
pub fn is_song_downloaded(_song: &Song) -> bool {
    false
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_stats() -> DownloadStats {
    let entries = purge_index_missing_files();
    let song_count = entries.len();
    let total_size_bytes = entries.iter().map(|entry| entry.size_bytes).sum();
    DownloadStats {
        song_count,
        total_size_bytes,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn download_stats() -> DownloadStats {
    DownloadStats::default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_downloaded_entries() -> Vec<DownloadIndexEntry> {
    let mut entries = purge_index_missing_files();
    entries.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    entries
}

#[cfg(target_arch = "wasm32")]
pub fn list_downloaded_entries() -> Vec<DownloadIndexEntry> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn mark_collection_downloaded(
    kind: &str,
    server_id: &str,
    collection_id: &str,
    name: &str,
    song_count: usize,
) {
    if kind.trim().is_empty() || server_id.trim().is_empty() || collection_id.trim().is_empty() {
        return;
    }

    let mut index = load_collection_index();
    if let Some(entry) = index.iter_mut().find(|entry| {
        entry.kind == kind && entry.server_id == server_id && entry.collection_id == collection_id
    }) {
        entry.name = name.to_string();
        entry.song_count = song_count;
        entry.updated_at_ms = now_timestamp_millis();
    } else {
        index.push(DownloadCollectionEntry {
            kind: kind.to_string(),
            server_id: server_id.to_string(),
            collection_id: collection_id.to_string(),
            name: name.to_string(),
            song_count,
            updated_at_ms: now_timestamp_millis(),
        });
    }
    save_collection_index(&index);
}

#[cfg(target_arch = "wasm32")]
pub fn mark_collection_downloaded(
    _kind: &str,
    _server_id: &str,
    _collection_id: &str,
    _name: &str,
    _song_count: usize,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_downloaded_collections() -> Vec<DownloadCollectionEntry> {
    let mut entries = load_collection_index();
    entries.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
    entries
}

#[cfg(target_arch = "wasm32")]
pub fn list_downloaded_collections() -> Vec<DownloadCollectionEntry> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
fn push_active_download(song: &Song) {
    let key_server_id = song.server_id.trim();
    let key_song_id = song.id.trim();
    if key_server_id.is_empty() || key_song_id.is_empty() {
        return;
    }

    let mut downloads = ACTIVE_DOWNLOADS
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if downloads
        .iter()
        .any(|entry| entry.server_id == key_server_id && entry.song_id == key_song_id)
    {
        return;
    }

    downloads.push(ActiveDownloadEntry {
        server_id: key_server_id.to_string(),
        song_id: key_song_id.to_string(),
        title: song.title.clone(),
        artist: song.artist.clone(),
        album: song.album.clone(),
        started_at_ms: now_timestamp_millis(),
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn pop_active_download(server_id: &str, song_id: &str) {
    let mut downloads = ACTIVE_DOWNLOADS
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    downloads.retain(|entry| !(entry.server_id == server_id && entry.song_id == song_id));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn list_active_downloads() -> Vec<ActiveDownloadEntry> {
    let downloads = ACTIVE_DOWNLOADS
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let mut snapshot = downloads.clone();
    snapshot.sort_by(|left, right| left.started_at_ms.cmp(&right.started_at_ms));
    snapshot
}

#[cfg(target_arch = "wasm32")]
pub fn list_active_downloads() -> Vec<ActiveDownloadEntry> {
    Vec::new()
}

#[cfg(not(target_arch = "wasm32"))]
struct ActiveDownloadGuard {
    server_id: String,
    song_id: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl ActiveDownloadGuard {
    fn new(song: &Song) -> Self {
        push_active_download(song);
        Self {
            server_id: song.server_id.clone(),
            song_id: song.id.clone(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ActiveDownloadGuard {
    fn drop(&mut self) {
        pop_active_download(&self.server_id, &self.song_id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_album_collections_with_index(entries: &[DownloadIndexEntry]) {
    let current = load_collection_index();
    if current.is_empty() {
        return;
    }

    let mut next = Vec::<DownloadCollectionEntry>::with_capacity(current.len());
    for mut collection in current {
        if collection.kind != "album" {
            next.push(collection);
            continue;
        }

        let album_name_key = collection.collection_id.strip_prefix("name:");
        let mut song_count = 0usize;
        let mut newest_updated_at = 0u64;
        for entry in entries {
            if entry.server_id != collection.server_id {
                continue;
            }

            let matches = if let Some(name_key) = album_name_key {
                entry
                    .album
                    .as_ref()
                    .is_some_and(|value| value.trim() == name_key)
            } else {
                entry
                    .album_id
                    .as_ref()
                    .is_some_and(|value| value.trim() == collection.collection_id)
            };

            if matches {
                song_count = song_count.saturating_add(1);
                newest_updated_at = newest_updated_at.max(entry.updated_at_ms);
            }
        }

        if song_count == 0 {
            continue;
        }

        collection.song_count = song_count;
        if newest_updated_at > 0 {
            collection.updated_at_ms = newest_updated_at;
        }
        next.push(collection);
    }

    save_collection_index(&next);
}

#[cfg(not(target_arch = "wasm32"))]
fn remove_download_index_keys(keys: &HashSet<(String, String)>) -> usize {
    if keys.is_empty() {
        return 0;
    }

    let mut index = load_download_index();
    let mut removed_entries = Vec::<DownloadIndexEntry>::new();
    index.retain(|entry| {
        let key = (entry.server_id.clone(), entry.song_id.clone());
        if keys.contains(&key) {
            removed_entries.push(entry.clone());
            false
        } else {
            true
        }
    });

    if removed_entries.is_empty() {
        return 0;
    }

    for entry in &removed_entries {
        let _ = remove_audio_cache_files_by_ids(&entry.server_id, &entry.song_id);
    }

    save_download_index(&index);
    sync_album_collections_with_index(&index);
    removed_entries.len()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove_downloaded_song(server_id: &str, song_id: &str) -> usize {
    if server_id.trim().is_empty() || song_id.trim().is_empty() {
        return 0;
    }

    let mut keys = HashSet::<(String, String)>::new();
    keys.insert((server_id.trim().to_string(), song_id.trim().to_string()));
    remove_download_index_keys(&keys)
}

#[cfg(target_arch = "wasm32")]
pub fn remove_downloaded_song(_server_id: &str, _song_id: &str) -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove_downloaded_songs(keys: &[(String, String)]) -> usize {
    let key_set: HashSet<(String, String)> = keys
        .iter()
        .filter_map(|(server_id, song_id)| {
            let server_id = server_id.trim();
            let song_id = song_id.trim();
            if server_id.is_empty() || song_id.is_empty() {
                None
            } else {
                Some((server_id.to_string(), song_id.to_string()))
            }
        })
        .collect();
    remove_download_index_keys(&key_set)
}

#[cfg(target_arch = "wasm32")]
pub fn remove_downloaded_songs(_keys: &[(String, String)]) -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove_downloaded_collection(kind: &str, server_id: &str, collection_id: &str) -> usize {
    if kind.trim().is_empty() || server_id.trim().is_empty() || collection_id.trim().is_empty() {
        return 0;
    }

    let mut index = load_collection_index();
    let before = index.len();
    index.retain(|entry| {
        !(entry.kind == kind
            && entry.server_id == server_id.trim()
            && entry.collection_id == collection_id.trim())
    });
    if index.len() != before {
        save_collection_index(&index);
    }
    before.saturating_sub(index.len())
}

#[cfg(target_arch = "wasm32")]
pub fn remove_downloaded_collection(_kind: &str, _server_id: &str, _collection_id: &str) -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn remove_downloaded_album(server_id: &str, collection_id: &str, fallback_name: &str) -> usize {
    if server_id.trim().is_empty() || collection_id.trim().is_empty() {
        return 0;
    }

    let mut keys = HashSet::<(String, String)>::new();
    let by_name = collection_id
        .strip_prefix("name:")
        .map(str::to_string)
        .or_else(|| {
            let trimmed = fallback_name.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

    for entry in load_download_index() {
        if entry.server_id != server_id.trim() {
            continue;
        }

        let matches = if let Some(name_key) = by_name.as_ref() {
            collection_id.starts_with("name:")
                && entry
                    .album
                    .as_ref()
                    .is_some_and(|value| value.trim() == name_key)
        } else {
            entry
                .album_id
                .as_ref()
                .is_some_and(|value| value.trim() == collection_id.trim())
        };

        if matches {
            keys.insert((entry.server_id.clone(), entry.song_id.clone()));
        }
    }

    let removed = remove_download_index_keys(&keys);
    let _ = remove_downloaded_collection("album", server_id, collection_id);
    removed
}

#[cfg(target_arch = "wasm32")]
pub fn remove_downloaded_album(
    _server_id: &str,
    _collection_id: &str,
    _fallback_name: &str,
) -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn clear_downloads() -> usize {
    let Some(dir) = audio_cache_dir() else {
        return 0;
    };

    let mut removed = 0usize;
    let Ok(read_dir) = fs::read_dir(&dir) else {
        return 0;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(is_cached_audio_extension)
            && fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }

    save_download_index(&[]);
    save_collection_index(&[]);
    removed
}

#[cfg(target_arch = "wasm32")]
pub fn clear_downloads() -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
pub fn prune_download_cache(max_count: u32, max_size_mb: u32) -> usize {
    let Some(dir) = audio_cache_dir() else {
        return 0;
    };

    let mut files = Vec::<(PathBuf, u64, std::time::SystemTime, String, String)>::new();
    let mut total_bytes = 0u64;

    let Ok(read_dir) = fs::read_dir(&dir) else {
        return 0;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| !is_cached_audio_extension(ext))
        {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        let mut parts = stem.splitn(2, "__");
        let sid = parts.next().unwrap_or_default().to_string();
        let song_id = parts.next().unwrap_or_default().to_string();

        let size = meta.len();
        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        total_bytes = total_bytes.saturating_add(size);
        files.push((path, size, modified, sid, song_id));
    }

    let max_count = max_count.clamp(25, 20_000) as usize;
    let max_bytes = (max_size_mb.clamp(256, 131_072) as u64) * 1024 * 1024;

    if files.len() <= max_count && total_bytes <= max_bytes {
        return 0;
    }

    files.sort_by_key(|(_, _, modified, _, _)| *modified);
    let mut removed_keys = HashSet::<(String, String)>::new();
    let mut removed = 0usize;

    for (path, size, _, sid, song_id) in files {
        if removed > 0 && total_bytes <= max_bytes {
            let remaining = list_downloaded_entries().len().saturating_sub(removed);
            if remaining <= max_count {
                break;
            }
        }

        if fs::remove_file(&path).is_ok() {
            total_bytes = total_bytes.saturating_sub(size);
            removed += 1;
            removed_keys.insert((sid, song_id));
        }

        if total_bytes <= max_bytes {
            let current_count = list_downloaded_entries().len().saturating_sub(removed);
            if current_count <= max_count {
                break;
            }
        }
    }

    if removed > 0 {
        let mut index = load_download_index();
        let previous = index.len();
        index.retain(|entry| {
            let key = (
                sanitize_file_component(&entry.server_id),
                sanitize_file_component(&entry.song_id),
            );
            !removed_keys.contains(&key)
                && audio_cache_file_path_by_ids(&entry.server_id, &entry.song_id)
                    .is_some_and(|path| path.exists())
        });
        if index.len() != previous {
            save_download_index(&index);
        }
    }

    removed
}

#[cfg(target_arch = "wasm32")]
pub fn prune_download_cache(_max_count: u32, _max_size_mb: u32) -> usize {
    0
}

#[cfg(not(target_arch = "wasm32"))]
fn auto_download_favorite_limit(tier: u8) -> usize {
    match tier {
        3 => 150,
        2 => 100,
        _ => 50,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_unique_song(target: &mut Vec<Song>, seen: &mut HashSet<String>, song: Song) {
    if song.id.trim().is_empty() || song.server_id.trim().is_empty() {
        return;
    }
    let key = format!("{}::{}", song.server_id, song.id);
    if seen.insert(key) {
        target.push(song);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn is_id3_cover_art_id(cover_art_id: &str) -> bool {
    cover_art_id.trim().to_ascii_lowercase().starts_with("mf-")
}

#[cfg(not(target_arch = "wasm32"))]
fn include_cover_for_preference(cover_art_id: &str, preference: ArtworkDownloadPreference) -> bool {
    match preference {
        ArtworkDownloadPreference::ServerOnly => !is_id3_cover_art_id(cover_art_id),
        ArtworkDownloadPreference::Id3Only => is_id3_cover_art_id(cover_art_id),
        ArtworkDownloadPreference::PreferServer | ArtworkDownloadPreference::PreferId3 => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn song_cover_art_candidates(song: &Song, preference: ArtworkDownloadPreference) -> Vec<String> {
    let mut output = Vec::<String>::new();

    if let Some(primary) = song
        .cover_art
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if include_cover_for_preference(primary, preference) {
            output.push(primary.to_string());
        }
    }

    if let Some(album_id) = song
        .album_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        if include_cover_for_preference(album_id, preference)
            && !output.iter().any(|entry| entry == album_id)
        {
            output.push(album_id.to_string());
        }
    }

    output
}

#[cfg(not(target_arch = "wasm32"))]
fn warm_song_cover_art(
    client: &NavidromeClient,
    song: &Song,
    preference: ArtworkDownloadPreference,
    seen_requests: &mut HashSet<String>,
) -> usize {
    let mut warmed = 0usize;
    for cover_art_id in song_cover_art_candidates(song, preference) {
        for size in DOWNLOAD_ARTWORK_SIZES {
            let request_key = format!("{}:{cover_art_id}:{size}", song.server_id);
            if !seen_requests.insert(request_key) {
                continue;
            }
            let url = client.get_cover_art_url(&cover_art_id, size);
            if !url.trim().is_empty() {
                warmed += 1;
            }
        }
    }
    warmed
}

#[cfg(not(target_arch = "wasm32"))]
async fn warm_song_lyrics(song: &Song, settings: &AppSettings) -> Option<bool> {
    let query = LyricsQuery::from_song(song);
    if query.title.trim().is_empty() {
        return None;
    }

    let timeout_seconds = settings.lyrics_request_timeout_secs.clamp(1, 20);
    let lrclib_order = vec!["lrclib".to_string()];
    let lrclib_warmed = fetch_lyrics_with_fallback(&query, &lrclib_order, timeout_seconds)
        .await
        .is_ok();

    let provider_order = normalize_lyrics_provider_order(&settings.lyrics_provider_order);
    let provider_warmed = if provider_order == lrclib_order {
        false
    } else {
        fetch_lyrics_with_fallback(&query, &provider_order, timeout_seconds)
            .await
            .is_ok()
    };

    Some(lrclib_warmed || provider_warmed)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn run_auto_download_pass(
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> Result<AutoDownloadReport, String> {
    let mut report = AutoDownloadReport::default();
    if !settings.downloads_enabled || !settings.auto_downloads_enabled {
        report.indexed = list_downloaded_entries().len();
        return Ok(report);
    }

    let active_servers: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    if active_servers.is_empty() {
        report.indexed = list_downloaded_entries().len();
        return Ok(report);
    }

    let mut candidates = Vec::<Song>::new();
    let mut seen = HashSet::<String>::new();
    let favorite_limit = auto_download_favorite_limit(settings.auto_download_tier.clamp(1, 3));

    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server.clone());

        if let Ok((starred_artists, starred_albums, mut starred_songs)) = client.get_starred().await
        {
            let _ = starred_artists;
            starred_songs.sort_by(|left, right| right.played.cmp(&left.played));
            for song in starred_songs.into_iter().take(favorite_limit) {
                push_unique_song(&mut candidates, &mut seen, song);
            }

            for album in starred_albums
                .into_iter()
                .take(settings.auto_download_album_count.clamp(0, 25) as usize)
            {
                if let Ok((_, songs)) = client.get_album(&album.id).await {
                    for song in songs {
                        push_unique_song(&mut candidates, &mut seen, song);
                    }
                }
            }
        }

        if let Ok(mut playlists) = client.get_playlists().await {
            playlists.sort_by(|left, right| {
                right
                    .changed
                    .cmp(&left.changed)
                    .then_with(|| right.created.cmp(&left.created))
            });

            for playlist in playlists
                .into_iter()
                .take(settings.auto_download_playlist_count.clamp(0, 25) as usize)
            {
                if let Ok((_, songs)) = client.get_playlist(&playlist.id).await {
                    for song in songs {
                        push_unique_song(&mut candidates, &mut seen, song);
                    }
                }
            }
        }
    }

    let max_count = settings.download_limit_count.clamp(25, 20_000) as usize;
    if candidates.len() > max_count {
        candidates.truncate(max_count);
    }

    report.attempted = candidates.len();
    for song in candidates {
        if is_song_downloaded(&song) {
            report.skipped += 1;
            continue;
        }

        match prefetch_song_audio(&song, servers, settings).await {
            Ok(()) => report.downloaded += 1,
            Err(_) => report.failed += 1,
        }

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    }

    report.purged = prune_download_cache(settings.download_limit_count, settings.download_limit_mb);
    report.indexed = list_downloaded_entries().len();

    Ok(report)
}

#[cfg(target_arch = "wasm32")]
pub async fn run_auto_download_pass(
    _servers: &[ServerConfig],
    _settings: &AppSettings,
) -> Result<AutoDownloadReport, String> {
    Ok(AutoDownloadReport::default())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn download_songs_batch(
    songs: &[Song],
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> DownloadBatchReport {
    let mut report = DownloadBatchReport::default();
    if songs.is_empty() {
        report.indexed = list_downloaded_entries().len();
        return report;
    }

    let mut seen = HashSet::<String>::new();
    let mut ordered = Vec::<Song>::new();
    for song in songs {
        if song.id.trim().is_empty() || song.server_id.trim().is_empty() {
            continue;
        }
        let key = format!("{}::{}", song.server_id, song.id);
        if seen.insert(key) {
            ordered.push(song.clone());
        }
    }

    report.attempted = ordered.len();
    if report.attempted == 0 {
        report.indexed = list_downloaded_entries().len();
        return report;
    }

    let mut effective_settings = settings.clone();
    effective_settings.downloads_enabled = true;

    for song in ordered {
        if is_song_downloaded(&song) {
            report.skipped += 1;
            continue;
        }

        match prefetch_song_audio(&song, servers, &effective_settings).await {
            Ok(()) => report.downloaded += 1,
            Err(_) => report.failed += 1,
        }

        tokio::time::sleep(std::time::Duration::from_millis(70)).await;
    }

    report.purged = prune_download_cache(
        effective_settings.download_limit_count,
        effective_settings.download_limit_mb,
    );
    report.indexed = list_downloaded_entries().len();
    report
}

#[cfg(target_arch = "wasm32")]
pub async fn download_songs_batch(
    _songs: &[Song],
    _servers: &[ServerConfig],
    _settings: &AppSettings,
) -> DownloadBatchReport {
    DownloadBatchReport::default()
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn refresh_downloaded_cache(
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> Result<DownloadCacheRefreshReport, String> {
    let entries = list_downloaded_entries();
    let mut report = DownloadCacheRefreshReport {
        scanned: entries.len(),
        ..DownloadCacheRefreshReport::default()
    };
    if entries.is_empty() {
        return Ok(report);
    }

    let server_map: HashMap<String, ServerConfig> = servers
        .iter()
        .cloned()
        .map(|server| (server.id.clone(), server))
        .collect();
    let mut seen_cover_requests = HashSet::<String>::new();

    for entry in entries {
        let Some(server) = server_map.get(&entry.server_id).cloned() else {
            report.missing_servers += 1;
            continue;
        };

        let song = Song {
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
            server_name: entry
                .server_name
                .clone()
                .unwrap_or_else(|| server.name.clone()),
            ..Song::default()
        };

        let client = NavidromeClient::new(server);
        report.artwork_refreshed += warm_song_cover_art(
            &client,
            &song,
            settings.artwork_download_preference,
            &mut seen_cover_requests,
        );

        if let Some(warmed) = warm_song_lyrics(&song, settings).await {
            report.lyrics_attempted += 1;
            if warmed {
                report.lyrics_warmed += 1;
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(35)).await;
    }

    Ok(report)
}

#[cfg(target_arch = "wasm32")]
pub async fn refresh_downloaded_cache(
    _servers: &[ServerConfig],
    _settings: &AppSettings,
) -> Result<DownloadCacheRefreshReport, String> {
    Ok(DownloadCacheRefreshReport::default())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn prefetch_song_audio(
    song: &Song,
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> Result<(), String> {
    if !settings.cache_enabled && !settings.downloads_enabled {
        return Ok(());
    }
    if song.server_name == "Radio" || song.id.trim().is_empty() {
        return Ok(());
    }

    let Some(path) = audio_cache_file_path(song) else {
        return Err("Audio cache path is unavailable.".to_string());
    };
    if path.exists() {
        if let Ok(meta) = fs::metadata(&path) {
            upsert_download_index(song, meta.len());
        }
        return Ok(());
    }

    let Some(server) = servers
        .iter()
        .find(|server| server.id == song.server_id)
        .cloned()
    else {
        return Ok(());
    };

    let client = NavidromeClient::new(server);
    let _active_download_guard = ActiveDownloadGuard::new(song);
    let stream_url = client.get_stream_url(&song.id);
    let response = AUDIO_HTTP_CLIENT
        .get(stream_url)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    if !response.status().is_success() {
        return Err(format!(
            "Audio prefetch failed with status {}",
            response.status()
        ));
    }

    let max_per_song_bytes = 80u64 * 1024 * 1024;
    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
    if bytes.is_empty() {
        return Err("Audio prefetch wrote no bytes.".to_string());
    }
    let mut payload = bytes.to_vec();
    if payload.len() as u64 > max_per_song_bytes {
        payload.truncate(max_per_song_bytes as usize);
    }

    tokio::fs::write(&path, &payload)
        .await
        .map_err(|err| err.to_string())?;

    upsert_download_index(song, payload.len() as u64);

    // Warm cover art alongside downloads so album/song artwork is available offline.
    let mut seen_cover_requests = HashSet::<String>::new();
    let _ = warm_song_cover_art(
        &client,
        song,
        settings.artwork_download_preference,
        &mut seen_cover_requests,
    );

    // Warm lyrics alongside audio download so offline playback has text available.
    let _ = warm_song_lyrics(song, settings).await;

    let size_budget_mb = if settings.downloads_enabled {
        settings.download_limit_mb
    } else {
        settings.cache_size_mb
    };
    prune_audio_cache(size_budget_mb);
    let _ = prune_download_cache(settings.download_limit_count, settings.download_limit_mb);

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn prefetch_song_audio(
    _song: &Song,
    _servers: &[ServerConfig],
    _settings: &AppSettings,
) -> Result<(), String> {
    Ok(())
}
