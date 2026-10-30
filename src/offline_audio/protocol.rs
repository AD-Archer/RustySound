//! Serves downloaded audio files to the desktop webview's `<audio>` element.
//!
//! macOS and Linux play offline songs through the webview, which cannot read
//! `file://` URLs from the `dioxus://` page. Instead of base64-encoding the whole
//! file into a `data:` URL, the audio controller registers a Dioxus asset handler
//! under [`ROUTE`]. It answers HTTP Range requests by reading only the requested
//! slice from disk on a worker thread, so playback starts after the first slice
//! and seeking jumps straight to the right byte offset.
//!
//! The handler only serves plain file names from the audio cache directory; it
//! never accepts a path.

#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
use std::path::Path;

/// Path segment the handler is registered under (`dioxus://index.html/<ROUTE>/<file>`).
#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
const ROUTE: &str = "rs-offline-audio";

/// The page origin Dioxus desktop serves the app from on macOS and Linux. The
/// URL must be absolute: the bridge compares it with `audio.src`, which the
/// webview always reports in absolute form.
#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
const ORIGIN: &str = "dioxus://index.html";

/// Upper bound for one response body. WebKit asks for open-ended ranges
/// (`bytes=N-`); answering with a bounded slice keeps memory flat and makes the
/// media loader come back for the next slice.
const MAX_CHUNK_BYTES: u64 = 2 * 1024 * 1024;

/// Builds the URL the webview loads for a downloaded file.
#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
pub(crate) fn url_for_cached_file(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    is_safe_file_name(name).then(|| format!("{ORIGIN}/{ROUTE}/{name}"))
}

/// Registers the offline-audio handler for the lifetime of the calling component.
#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
pub(crate) fn use_offline_audio_handler() {
    dioxus::desktop::use_asset_handler(ROUTE, |request, responder| {
        std::thread::spawn(move || responder.respond(serve(&request)));
    });
}

#[cfg(all(feature = "desktop", any(target_os = "macos", target_os = "linux")))]
fn serve(
    request: &dioxus::desktop::wry::http::Request<Vec<u8>>,
) -> dioxus::desktop::wry::http::Response<Vec<u8>> {
    use dioxus::desktop::wry::http::{header, Method, Response, StatusCode};
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let error = |status: StatusCode| {
        Response::builder()
            .status(status)
            .body(Vec::new())
            .expect("static response parts are valid")
    };

    let name = request
        .uri()
        .path()
        .trim_start_matches('/')
        .strip_prefix(ROUTE)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or_default();
    if !is_safe_file_name(name) {
        return error(StatusCode::NOT_FOUND);
    }
    let Some(path) = super::audio_cache_dir().map(|dir| dir.join(name)) else {
        return error(StatusCode::NOT_FOUND);
    };
    let Ok(mut file) = File::open(&path) else {
        return error(StatusCode::NOT_FOUND);
    };
    let Ok(total) = file.metadata().map(|meta| meta.len()) else {
        return error(StatusCode::INTERNAL_SERVER_ERROR);
    };

    let range_header = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    let Some(slice) = resolve_range(range_header, total) else {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Vec::new())
            .expect("static response parts are valid");
    };

    let mut head = [0u8; 12];
    let head_len = file.read(&mut head).unwrap_or(0);
    let mime = mime_for(name, &head[..head_len]);

    let body = if request.method() == Method::HEAD {
        Vec::new()
    } else {
        let mut body = vec![0u8; slice.len() as usize];
        if file.seek(SeekFrom::Start(slice.start)).is_err() || file.read_exact(&mut body).is_err() {
            return error(StatusCode::INTERNAL_SERVER_ERROR);
        }
        body
    };

    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, slice.len().to_string())
        .header(header::CACHE_CONTROL, "no-store");
    response = if slice.partial {
        response.status(StatusCode::PARTIAL_CONTENT).header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{total}", slice.start, slice.end_inclusive()),
        )
    } else {
        response.status(StatusCode::OK)
    };
    response
        .body(body)
        .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR))
}

/// The byte span a response carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slice {
    start: u64,
    /// Exclusive end.
    end: u64,
    /// `true` for a 206 response, `false` for a plain 200 with the whole file.
    partial: bool,
}

impl Slice {
    fn len(&self) -> u64 {
        self.end - self.start
    }

    fn end_inclusive(&self) -> u64 {
        self.end.saturating_sub(1)
    }
}

/// Maps a `Range` header onto the file, capping the body at [`MAX_CHUNK_BYTES`].
///
/// Returns `None` when the range can't be satisfied (416). Without a `Range`
/// header a small file is returned whole (200); a large one gets its first
/// slice as a 206 so a request never pulls the whole file into memory.
fn resolve_range(header: Option<&str>, total: u64) -> Option<Slice> {
    let capped = |start: u64, end: u64| Slice {
        start,
        end: end.min(start.saturating_add(MAX_CHUNK_BYTES)),
        partial: true,
    };

    let Some(spec) = header.map(str::trim).filter(|value| !value.is_empty()) else {
        return Some(if total <= MAX_CHUNK_BYTES {
            Slice {
                start: 0,
                end: total,
                partial: false,
            }
        } else {
            capped(0, total)
        });
    };

    // Only the first range of a multi-range request is honoured; media loaders
    // never send more than one.
    let first = spec.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start_raw, end_raw) = first.split_once('-')?;
    let (start_raw, end_raw) = (start_raw.trim(), end_raw.trim());

    if total == 0 {
        return None;
    }

    if start_raw.is_empty() {
        // Suffix range: the last N bytes.
        let suffix: u64 = end_raw.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        return Some(capped(total.saturating_sub(suffix), total));
    }

    let start: u64 = start_raw.parse().ok()?;
    if start >= total {
        return None;
    }
    let end = if end_raw.is_empty() {
        total
    } else {
        let last: u64 = end_raw.parse().ok()?;
        if last < start {
            return None;
        }
        last.saturating_add(1).min(total)
    };
    Some(capped(start, end))
}

/// Accepts only the file names the download cache writes: `<server>__<song>.<ext>`
/// built from `[A-Za-z0-9_-]` plus a known audio extension.
fn is_safe_file_name(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        && super::is_cached_audio_extension(ext)
}

/// Content type from the extension, or sniffed from the first bytes for the
/// generic `.audio` extension.
fn mime_for(name: &str, head: &[u8]) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        _ => sniff_mime(head),
    }
}

fn sniff_mime(head: &[u8]) -> &'static str {
    if head.starts_with(b"fLaC") {
        "audio/flac"
    } else if head.starts_with(b"OggS") {
        "audio/ogg"
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE") {
        "audio/wav"
    } else if head.get(4..8) == Some(b"ftyp") {
        "audio/mp4"
    } else if head.len() >= 2 && head[0] == 0xFF && head[1] & 0xF6 == 0xF0 {
        "audio/aac"
    } else {
        "audio/mpeg"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1024 * 1024;

    fn slice(start: u64, end: u64, partial: bool) -> Slice {
        Slice {
            start,
            end,
            partial,
        }
    }

    #[test]
    fn no_range_returns_small_files_whole() {
        assert_eq!(resolve_range(None, 1000), Some(slice(0, 1000, false)));
        assert_eq!(resolve_range(Some(""), 1000), Some(slice(0, 1000, false)));
    }

    #[test]
    fn no_range_on_large_file_returns_first_chunk() {
        assert_eq!(
            resolve_range(None, 300 * MB),
            Some(slice(0, MAX_CHUNK_BYTES, true))
        );
    }

    #[test]
    fn probe_range_is_exact() {
        let probe = resolve_range(Some("bytes=0-1"), 300 * MB);
        assert_eq!(probe, Some(slice(0, 2, true)));
        assert_eq!(probe.map(|s| (s.len(), s.end_inclusive())), Some((2, 1)));
    }

    #[test]
    fn open_ended_range_is_capped() {
        let start = 270 * MB;
        assert_eq!(
            resolve_range(Some("bytes=283115520-"), 300 * MB),
            Some(slice(start, start + MAX_CHUNK_BYTES, true))
        );
    }

    #[test]
    fn range_end_is_clamped_to_file() {
        assert_eq!(
            resolve_range(Some("bytes=900-5000"), 1000),
            Some(slice(900, 1000, true))
        );
    }

    #[test]
    fn suffix_range() {
        assert_eq!(
            resolve_range(Some("bytes=-100"), 1000),
            Some(slice(900, 1000, true))
        );
        assert_eq!(
            resolve_range(Some("bytes=-5000"), 1000),
            Some(slice(0, 1000, true))
        );
    }

    #[test]
    fn unsatisfiable_ranges() {
        assert_eq!(resolve_range(Some("bytes=1000-"), 1000), None);
        assert_eq!(resolve_range(Some("bytes=10-5"), 1000), None);
        assert_eq!(resolve_range(Some("bytes=-0"), 1000), None);
        assert_eq!(resolve_range(Some("items=0-1"), 1000), None);
        assert_eq!(resolve_range(Some("bytes=0-1"), 0), None);
    }

    #[test]
    fn only_cache_file_names_are_served() {
        assert!(is_safe_file_name("srv-1__song_42.flac"));
        assert!(is_safe_file_name("a__b.audio"));
        assert!(!is_safe_file_name("../download_index.json"));
        assert!(!is_safe_file_name("a/b.flac"));
        assert!(!is_safe_file_name(".flac"));
        assert!(!is_safe_file_name("song.exe"));
        assert!(!is_safe_file_name("download_index.json"));
    }

    #[test]
    fn mime_from_extension_or_magic() {
        assert_eq!(mime_for("a__b.flac", b""), "audio/flac");
        assert_eq!(mime_for("a__b.audio", b"fLaC\0\0\0\x22"), "audio/flac");
        assert_eq!(mime_for("a__b.audio", b"RIFF\0\0\0\0WAVEfmt "), "audio/wav");
        assert_eq!(mime_for("a__b.audio", b"\0\0\0\x20ftypM4A "), "audio/mp4");
        assert_eq!(mime_for("a__b.audio", b"ID3\x04"), "audio/mpeg");
    }
}
