#[cfg(not(target_arch = "wasm32"))]
use crate::cache_service::is_enabled as cache_enabled;
#[cfg(not(target_arch = "wasm32"))]
use crate::storage::app_cache_dir;
#[cfg(not(target_arch = "wasm32"))]
use base64::{engine::general_purpose, Engine as _};
#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
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
    let base = app_cache_dir()?.join(COVER_ART_CACHE_SUBDIR);
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
pub fn cached_cover_art_data_url(server_id: &str, cover_art_id: &str, size: u32) -> Option<String> {
    let path = cover_art_file_path(server_id, cover_art_id, size)?;
    let bytes = fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let encoded = general_purpose::STANDARD.encode(bytes);
    Some(format!("data:image/jpeg;base64,{encoded}"))
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

// ---- Cover art for the webview ---------------------------------------------
//
// Lists render covers through `Artwork`, which used to call
// `cached_cover_art_data_url` while rendering: a synchronous file read plus a
// base64 encode on the main thread for every row that scrolled in, and a data
// URL of tens of kilobytes in every DOM edit. Covers that weren't cached yet
// were downloaded twice (once by the webview, once by the cache prefetch).
//
// Native builds now point `<img>` at a local route instead. The handler runs
// off the main thread: it serves the cached file, or downloads the cover once,
// stores it in the cache and serves it. Rendering a row does no file work.

/// First path segment of the route (`/<ROUTE>/<file>?src=<remote url>`).
#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
const COVER_ROUTE: &str = "rs-cover-art";

/// URL for `<img src>`: the local cover route on native builds with a webview,
/// so cached covers never pass through render. `remote_url` is what the
/// handler downloads on a cache miss.
#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
pub fn cover_art_display_url(
    server_id: &str,
    cover_art_id: &str,
    size: u32,
    remote_url: &str,
) -> String {
    let sid = sanitize_file_component(server_id);
    let aid = sanitize_file_component(cover_art_id);
    format!(
        "/{COVER_ROUTE}/{sid}__{aid}__{size}.img?src={}",
        urlencoding::encode(remote_url)
    )
}

#[cfg(all(not(target_arch = "wasm32"), feature = "desktop"))]
use dioxus::desktop as webview_host;
#[cfg(all(not(target_arch = "wasm32"), feature = "mobile", not(feature = "desktop")))]
use dioxus::mobile as webview_host;

/// Registers the cover route for the lifetime of the calling component (the app shell).
#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
pub fn use_cover_art_handler() {
    // The handler is called on the main thread; all file and network work
    // moves to the async runtime.
    let runtime = dioxus::prelude::use_hook(|| tokio::runtime::Handle::try_current().ok());
    webview_host::use_asset_handler(COVER_ROUTE, move |request, responder| {
        let uri = request.uri().clone();
        let job = async move {
            responder.respond(serve_cover(&uri).await);
        };
        match runtime.clone() {
            Some(handle) => {
                handle.spawn(job);
            }
            None => {
                std::thread::spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(job);
                    }
                });
            }
        }
    });
}

#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
async fn serve_cover(
    uri: &webview_host::wry::http::Uri,
) -> webview_host::wry::http::Response<Vec<u8>> {
    use webview_host::wry::http::{header, Response, StatusCode};
    let status = |code: StatusCode| {
        Response::builder()
            .status(code)
            .body(Vec::new())
            .expect("static response parts are valid")
    };
    let ok = |bytes: Vec<u8>| {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, image_mime(&bytes))
            .header(header::CACHE_CONTROL, "max-age=604800")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
    };

    let name = uri
        .path()
        .trim_start_matches('/')
        .strip_prefix(COVER_ROUTE)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or_default();
    if !is_cover_file_name(name) {
        return status(StatusCode::NOT_FOUND);
    }
    let path = cover_art_cache_dir().map(|dir| dir.join(name));
    if let Some(path) = path.as_ref() {
        if let Ok(bytes) = tokio::fs::read(path).await {
            if !bytes.is_empty() {
                return ok(bytes);
            }
        }
    }
    if crate::cache_service::is_offline_mode() {
        return status(StatusCode::NOT_FOUND);
    }
    let remote = uri
        .query()
        .unwrap_or_default()
        .split('&')
        .find_map(|pair| pair.strip_prefix("src="))
        .and_then(|value| urlencoding::decode(value).ok())
        .map(|value| value.into_owned())
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"));
    let Some(remote) = remote else {
        return status(StatusCode::NOT_FOUND);
    };
    let Ok(response) = ART_HTTP_CLIENT.get(&remote).send().await else {
        return status(StatusCode::BAD_GATEWAY);
    };
    if !response.status().is_success() {
        return status(StatusCode::BAD_GATEWAY);
    }
    let bytes = match response.bytes().await {
        Ok(bytes) if !bytes.is_empty() => bytes.to_vec(),
        _ => return status(StatusCode::BAD_GATEWAY),
    };
    if cache_enabled(true) {
        if let Some(path) = path {
            let _ = tokio::fs::write(path, &bytes).await;
        }
    }
    ok(bytes)
}

/// Accepts only the names `cover_art_file_path` writes: `<server>__<cover>__<size>.img`.
#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "desktop", feature = "mobile")))]
fn is_cover_file_name(name: &str) -> bool {
    name.strip_suffix(".img").is_some_and(|stem| {
        !stem.is_empty()
            && stem
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    })
}

/// Content type sniffed from the first bytes (the cache stores bare bytes).
#[cfg(all(not(target_arch = "wasm32"), any(test, feature = "desktop", feature = "mobile")))]
fn image_mime(bytes: &[u8]) -> &'static str {
    let head = &bytes[..bytes.len().min(512)];
    if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if head.starts_with(b"GIF8") {
        "image/gif"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        "image/webp"
    } else if head.get(4..12) == Some(b"ftypavif") {
        "image/avif"
    } else if head.windows(4).any(|w| w == b"<svg") {
        "image/svg+xml"
    } else {
        "image/jpeg"
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn cover_file_names() {
        assert!(is_cover_file_name("srv-1__al-abc_123__400.img"));
        assert!(!is_cover_file_name("../rustysound.db"));
        assert!(!is_cover_file_name("a/b__c__4.img"));
        assert!(!is_cover_file_name(".img"));
        assert!(!is_cover_file_name("a__b__4.jpg"));
    }

    #[test]
    fn sniffs_image_types() {
        assert_eq!(image_mime(&[0x89, b'P', b'N', b'G', 0, 0]), "image/png");
        assert_eq!(image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(image_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), "image/svg+xml");
        assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
    }
}
