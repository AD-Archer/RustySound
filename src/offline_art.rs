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
/// Covers give up rather than hang on a server that went away, so the saved
/// copy (see [`cached_variant`]) is shown instead.
#[cfg(not(target_arch = "wasm32"))]
static ART_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
});

// ---- Any saved copy of a cover ---------------------------------------------
//
// The cache keeps one file per (server, cover, size) and per route: the music
// server's `…__<size>.img` and the sync server's `…__<size>w.img`. Views ask
// for different sizes, and whether the sync server is up decides the route,
// so an exact-name lookup missed covers that were on disk all along: with the
// sync server gone (or offline) the art it had served never showed. Lookups
// now fall back to any saved copy of the same cover through this index.

/// Saved files per cover (`<server>__<cover>`): `(size, file name)`.
#[cfg(not(target_arch = "wasm32"))]
type CoverIndex = std::collections::HashMap<String, Vec<(u32, String)>>;

#[cfg(not(target_arch = "wasm32"))]
static COVER_INDEX: Lazy<Mutex<Option<CoverIndex>>> = Lazy::new(|| Mutex::new(None));

/// `<server>__<cover>` and the size of a cache file name; `None` for other files.
#[cfg(not(target_arch = "wasm32"))]
fn split_cover_file_name(name: &str) -> Option<(&str, u32)> {
    let (key, size) = name.strip_suffix(".img")?.rsplit_once("__")?;
    let size = size.strip_suffix('w').unwrap_or(size).parse().ok()?;
    (!key.is_empty()).then_some((key, size))
}

#[cfg(not(target_arch = "wasm32"))]
fn with_cover_index<T>(f: impl FnOnce(&mut CoverIndex) -> T) -> T {
    let mut guard = COVER_INDEX.lock().unwrap_or_else(|e| e.into_inner());
    let index = guard.get_or_insert_with(|| {
        let mut index = CoverIndex::new();
        let entries = cover_art_cache_dir().and_then(|dir| fs::read_dir(dir).ok());
        for entry in entries.into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some((key, size)) = split_cover_file_name(&name) {
                index.entry(key.to_string()).or_default().push((size, name.clone()));
            }
        }
        index
    });
    f(index)
}

/// Records a file just written to the cover cache.
#[cfg(not(target_arch = "wasm32"))]
fn note_cached(name: &str) {
    let Some((key, size)) = split_cover_file_name(name) else {
        return;
    };
    with_cover_index(|index| {
        let files = index.entry(key.to_string()).or_default();
        if !files.iter().any(|(_, file)| file == name) {
            files.push((size, name.to_string()));
        }
    });
}

/// Of the saved sizes, the one to show for `wanted`: the smallest at least
/// that big, else the biggest there is when it is at least `smallest`
/// (`None`: no smaller ones).
#[cfg(not(target_arch = "wasm32"))]
fn pick_variant(files: &[(u32, String)], wanted: u32, smallest: Option<u32>) -> Option<&str> {
    let bigger = files
        .iter()
        .filter(|(size, _)| *size >= wanted)
        .min_by_key(|(size, _)| *size);
    let pick = bigger.or_else(|| {
        let min = smallest?;
        files.iter().max_by_key(|(size, _)| *size).filter(|(size, _)| *size >= min)
    });
    pick.map(|(_, name)| name.as_str())
}

/// The bytes of another saved copy of the cover named `name`
/// (`<server>__<cover>__<size>[w].img`), from either route, at least as big,
/// or smaller down to `smallest` px (see [`pick_variant`]).
#[cfg(not(target_arch = "wasm32"))]
fn cached_variant(name: &str, smallest: Option<u32>) -> Option<Vec<u8>> {
    let (key, wanted) = split_cover_file_name(name)?;
    let dir = cover_art_cache_dir()?;
    loop {
        let file = with_cover_index(|index| {
            let files = index.get(key)?;
            pick_variant(files, wanted, smallest).map(str::to_string)
        })?;
        match fs::read(dir.join(&file)) {
            Ok(bytes) if !bytes.is_empty() => return Some(bytes),
            // Gone from disk: forget it and try the next one.
            _ => with_cover_index(|index| {
                if let Some(files) = index.get_mut(key) {
                    files.retain(|(_, f)| *f != file);
                }
            }),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        image_mime(bytes),
        general_purpose::STANDARD.encode(bytes)
    )
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
fn cover_art_cache_dir() -> Option<PathBuf> {
    let base = app_cache_dir()?.join(COVER_ART_CACHE_SUBDIR);
    let _ = fs::create_dir_all(&base);
    Some(base)
}

/// The cover cache folder, where the local-music scanner writes embedded art.
#[cfg(any(target_os = "macos", target_os = "ios"))]
#[cfg_attr(not(any(feature = "desktop", feature = "mobile")), allow(dead_code))]
pub(crate) fn cover_art_dir() -> Option<PathBuf> {
    cover_art_cache_dir()
}

#[cfg(not(target_arch = "wasm32"))]
fn cover_art_file_path(server_id: &str, cover_art_id: &str, size: u32) -> Option<PathBuf> {
    let dir = cover_art_cache_dir()?;
    let sid = sanitize_file_component(server_id);
    let aid = sanitize_file_component(cover_art_id);
    Some(dir.join(format!("{sid}__{aid}__{size}.img")))
}

/// The saved cover as a data URL: this size, else a bigger saved copy
/// (from the music server or the sync server).
#[cfg(not(target_arch = "wasm32"))]
pub fn cached_cover_art_data_url(server_id: &str, cover_art_id: &str, size: u32) -> Option<String> {
    let path = cover_art_file_path(server_id, cover_art_id, size)?;
    if let Some(bytes) = fs::read(&path).ok().filter(|bytes| !bytes.is_empty()) {
        return Some(data_url(&bytes));
    }
    let name = path.file_name()?.to_str()?;
    cached_variant(name, None).map(|bytes| data_url(&bytes))
}

/// A saved copy of the cover at least `smallest` px as a data URL, for when
/// the server can't be asked (offline) or while the right size is fetched.
#[cfg(not(target_arch = "wasm32"))]
pub fn smaller_cached_cover_art_data_url(
    server_id: &str,
    cover_art_id: &str,
    size: u32,
    smallest: u32,
) -> Option<String> {
    let path = cover_art_file_path(server_id, cover_art_id, size)?;
    let name = path.file_name()?.to_str()?;
    cached_variant(name, Some(smallest)).map(|bytes| data_url(&bytes))
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
                    if !bytes.is_empty() && tokio::fs::write(&path, bytes).await.is_ok() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            note_cached(name);
                        }
                    }
                }
            }
        }

        let mut inflight = IN_FLIGHT_ART.lock().unwrap_or_else(|e| e.into_inner());
        inflight.remove(&inflight_key);
    });
}

/// The bytes of a cover URL as `song_metadata` builds it: a data URL (a
/// cached or local cover) or the server's cover URL (one download; nothing
/// in offline mode). For the Mac Now Playing widget.
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub(crate) async fn cover_bytes(url: &str) -> Option<Vec<u8>> {
    if let Some(data) = url.strip_prefix("data:") {
        let (_, encoded) = data.split_once(";base64,")?;
        return general_purpose::STANDARD.decode(encoded).ok().filter(|bytes| !bytes.is_empty());
    }
    if crate::cache_service::is_offline_mode() || !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let response = ART_HTTP_CLIENT.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec()).filter(|bytes| !bytes.is_empty())
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

/// Like [`cover_art_display_url`] for a cover from the sync server's media
/// cache (`art_url`, WebP): cached under its own name (`…__<size>w.img`,
/// so the JPEG-labelled data URLs never pick up a WebP), with the music
/// server's URL as `alt`, fetched when the sync server fails.
#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
pub fn cover_art_display_url_via_sync(
    server_id: &str,
    cover_art_id: &str,
    size: u32,
    art_url: &str,
    fallback_url: &str,
) -> String {
    let sid = sanitize_file_component(server_id);
    let aid = sanitize_file_component(cover_art_id);
    format!(
        "/{COVER_ROUTE}/{sid}__{aid}__{size}w.img?src={}&alt={}",
        urlencoding::encode(art_url),
        urlencoding::encode(fallback_url)
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
    // A local cover whose cache file was cleared: read it from the song again.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if let Some(path) = path.clone().filter(|_| name.starts_with("local__")) {
        let name = name.to_string();
        let rebuilt = tokio::task::spawn_blocking(move || crate::local_music::rebuild_cover(&name, &path))
            .await
            .ok()
            .flatten();
        return match rebuilt {
            Some(bytes) => ok(bytes),
            None => status(StatusCode::NOT_FOUND),
        };
    }
    // Another saved copy of this cover: the other route's (sync server or
    // music server), or a bigger size. No network, so it shows whether or
    // not either server is up.
    let variant = |smallest: Option<u32>| {
        let name = name.to_string();
        async move {
            tokio::task::spawn_blocking(move || cached_variant(&name, smallest))
                .await
                .ok()
                .flatten()
        }
    };
    if let Some(bytes) = variant(None).await {
        return ok(bytes);
    }
    if crate::cache_service::is_offline_mode() {
        return match variant(Some(0)).await {
            Some(bytes) => ok(bytes),
            None => status(StatusCode::NOT_FOUND),
        };
    }
    let param = |key: &str| {
        uri.query()
            .unwrap_or_default()
            .split('&')
            .find_map(|pair| pair.strip_prefix(key))
            .and_then(|value| urlencoding::decode(value).ok())
            .map(|value| value.into_owned())
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
    };
    let downloaded = match param("src=") {
        Some(remote) => match download(&remote).await {
            Some(bytes) => Some(bytes),
            // The sync server failed: the music server's own cover instead.
            None => match param("alt=") {
                Some(fallback) => {
                    if let Some((server_id, _)) = name.split_once("__") {
                        crate::components::remote_sync::media::note_art_failure(server_id);
                    }
                    download(&fallback).await
                }
                None => None,
            },
        },
        None => None,
    };
    let Some(bytes) = downloaded else {
        // Neither server answered: a smaller saved copy beats no cover.
        return match variant(Some(0)).await {
            Some(bytes) => ok(bytes),
            None => status(StatusCode::BAD_GATEWAY),
        };
    };
    if cache_enabled(true) {
        if let Some(path) = path {
            if tokio::fs::write(&path, &bytes).await.is_ok() {
                note_cached(name);
            }
        }
    }
    ok(bytes)
}

/// One cover download; `None` on any failure or an empty body.
#[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
async fn download(url: &str) -> Option<Vec<u8>> {
    let response = ART_HTTP_CLIENT.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec()).filter(|bytes| !bytes.is_empty())
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
#[cfg(not(target_arch = "wasm32"))]
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
        assert!(is_cover_file_name("srv-1__al-abc_123__400w.img"), "a sync-server cover");
        assert!(!is_cover_file_name("../rustysound.db"));
        assert!(!is_cover_file_name("a/b__c__4.img"));
        assert!(!is_cover_file_name(".img"));
        assert!(!is_cover_file_name("a__b__4.jpg"));
    }

    #[test]
    fn splits_cache_file_names() {
        assert_eq!(split_cover_file_name("srv-1__al-1__400.img"), Some(("srv-1__al-1", 400)));
        assert_eq!(split_cover_file_name("srv-1__al-1__400w.img"), Some(("srv-1__al-1", 400)), "sync copy, same key");
        assert_eq!(split_cover_file_name("srv__a__b__80.img"), Some(("srv__a__b", 80)), "ids with double underscores");
        assert_eq!(split_cover_file_name("cover-abc.img"), None);
        assert_eq!(split_cover_file_name("srv__al__big.img"), None);
    }

    #[test]
    fn picks_a_saved_copy() {
        let files = vec![
            (80, "s__a__80.img".to_string()),
            (400, "s__a__400w.img".to_string()),
            (512, "s__a__512.img".to_string()),
        ];
        assert_eq!(pick_variant(&files, 400, None), Some("s__a__400w.img"), "the sync server's copy");
        assert_eq!(pick_variant(&files, 300, None), Some("s__a__400w.img"), "smallest big enough");
        assert_eq!(pick_variant(&files, 1000, None), None, "no smaller ones unless asked");
        assert_eq!(pick_variant(&files, 1000, Some(500)), Some("s__a__512.img"));
        assert_eq!(pick_variant(&files, 2000, Some(1000)), None, "too small to stand in");
        assert_eq!(pick_variant(&files, 2000, Some(0)), Some("s__a__512.img"), "offline: anything");
        assert_eq!(pick_variant(&[], 80, Some(0)), None);
    }

    #[test]
    fn sniffs_image_types() {
        assert_eq!(image_mime(&[0x89, b'P', b'N', b'G', 0, 0]), "image/png");
        assert_eq!(image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(image_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), "image/svg+xml");
        assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
    }
}
