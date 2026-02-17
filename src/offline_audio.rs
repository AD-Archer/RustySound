use crate::api::{ServerConfig, Song};
#[cfg(not(target_arch = "wasm32"))]
use crate::api::NavidromeClient;
use crate::db::AppSettings;

#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
static AUDIO_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

#[cfg(not(target_arch = "wasm32"))]
const AUDIO_CACHE_SUBDIR: &str = "audio_cache";

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
    let base = dirs::cache_dir()?.join("rustysound").join(AUDIO_CACHE_SUBDIR);
    let _ = fs::create_dir_all(&base);
    Some(base)
}

#[cfg(not(target_arch = "wasm32"))]
fn audio_cache_file_path(song: &Song) -> Option<PathBuf> {
    let dir = audio_cache_dir()?;
    let sid = sanitize_file_component(&song.server_id);
    let song_id = sanitize_file_component(&song.id);
    Some(dir.join(format!("{sid}__{song_id}.audio")))
}

#[cfg(not(target_arch = "wasm32"))]
fn path_to_file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.starts_with('/') {
        format!("file://{normalized}")
    } else {
        format!("file:///{normalized}")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn prune_audio_cache(max_size_mb: u32) {
    let Some(dir) = audio_cache_dir() else {
        return;
    };

    let max_bytes = (max_size_mb.clamp(25, 2048) as u64) * 1024 * 1024;
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
        let size = meta.len();
        let modified = meta
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
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

#[cfg(not(target_arch = "wasm32"))]
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
pub async fn prefetch_song_audio(
    song: &Song,
    servers: &[ServerConfig],
    settings: &AppSettings,
) -> Result<(), String> {
    if !settings.cache_enabled {
        return Ok(());
    }
    if song.server_name == "Radio" || song.id.trim().is_empty() {
        return Ok(());
    }

    let Some(path) = audio_cache_file_path(song) else {
        return Err("Audio cache path is unavailable.".to_string());
    };
    if path.exists() {
        return Ok(());
    }

    let Some(server) = servers.iter().find(|server| server.id == song.server_id).cloned() else {
        return Ok(());
    };

    let client = NavidromeClient::new(server);
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

    prune_audio_cache(settings.cache_size_mb);
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
