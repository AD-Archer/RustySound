#[cfg(not(target_arch = "wasm32"))]
use crate::cache_service::is_enabled as cache_enabled;
#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(not(target_arch = "wasm32"))]
const COVER_ART_CACHE_SUBDIR: &str = "cover_art_cache";

#[cfg(not(target_arch = "wasm32"))]
static IN_FLIGHT_ART: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));
#[cfg(not(target_arch = "wasm32"))]
static ART_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

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
fn cover_art_cache_dir() -> Option<PathBuf> {
    let base = dirs::cache_dir()?.join("rustysound").join(COVER_ART_CACHE_SUBDIR);
    let _ = fs::create_dir_all(&base);
    Some(base)
}

#[cfg(not(target_arch = "wasm32"))]
fn cover_art_file_path(server_id: &str, cover_art_id: &str, size: u32) -> Option<PathBuf> {
    let dir = cover_art_cache_dir()?;
    let sid = sanitize_file_component(server_id);
    let aid = sanitize_file_component(cover_art_id);
    Some(dir.join(format!("{sid}__{aid}__{size}.img")))
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
pub fn cached_cover_art_url(server_id: &str, cover_art_id: &str, size: u32) -> Option<String> {
    let path = cover_art_file_path(server_id, cover_art_id, size)?;
    if path.exists() {
        Some(path_to_file_url(&path))
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
pub fn cached_cover_art_url(_server_id: &str, _cover_art_id: &str, _size: u32) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn maybe_prefetch_cover_art(
    server_id: String,
    cover_art_id: String,
    size: u32,
    remote_url: String,
) {
    if !cache_enabled(true) {
        return;
    }

    let Some(path) = cover_art_file_path(&server_id, &cover_art_id, size) else {
        return;
    };
    if path.exists() {
        return;
    }

    let inflight_key = format!("{server_id}:{cover_art_id}:{size}");
    {
        let mut inflight = IN_FLIGHT_ART.lock().unwrap_or_else(|e| e.into_inner());
        if !inflight.insert(inflight_key.clone()) {
            return;
        }
    }

    tokio::spawn(async move {
        if let Ok(response) = ART_HTTP_CLIENT.get(remote_url).send().await {
            if response.status().is_success() {
                if let Ok(bytes) = response.bytes().await {
                    if !bytes.is_empty() {
                        let _ = tokio::fs::write(&path, bytes).await;
                    }
                }
            }
        }

        let mut inflight = IN_FLIGHT_ART.lock().unwrap_or_else(|e| e.into_inner());
        inflight.remove(&inflight_key);
    });
}

#[cfg(target_arch = "wasm32")]
pub fn maybe_prefetch_cover_art(
    _server_id: String,
    _cover_art_id: String,
    _size: u32,
    _remote_url: String,
) {
}
