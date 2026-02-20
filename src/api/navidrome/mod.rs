use crate::api::models::*;
use crate::cache_service::{
    get_json as cache_get_json, is_offline_mode, put_json as cache_put_json,
    remove_by_prefix as cache_remove_prefix,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::offline_art::{cached_cover_art_data_url, maybe_prefetch_cover_art};
use chrono::{DateTime, NaiveDateTime, Utc};
#[cfg(target_arch = "wasm32")]
use dioxus::document;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
static AUTH_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static NATIVE_AUTH_CACHE: Lazy<Mutex<HashMap<String, NativeAuthSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const CLIENT_NAME: &str = "RustySound";
const API_VERSION: &str = "1.16.1";

pub struct NavidromeClient {
    pub server: ServerConfig,
}

#[derive(Clone)]
struct NativeAuthSession {
    token: String,
    client_unique_id: String,
}

#[derive(Debug, Clone, Copy)]
pub enum NativeSongSortField {
    PlayDate,
    PlayCount,
}

impl NativeSongSortField {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::PlayDate => "play_date",
            Self::PlayCount => "play_count",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum NativeSortOrder {
    Desc,
}

impl NativeSortOrder {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Desc => "DESC",
        }
    }
}

#[derive(Debug, Serialize)]
struct NativeLoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct NativeLoginResponse {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

fn json_pick_value<'a>(
    value: &'a serde_json::Value,
    keys: &[&str],
) -> Option<&'a serde_json::Value> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(found) = object.get(*key) {
            return Some(found);
        }
    }
    None
}

fn json_pick_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    let picked = json_pick_value(value, keys)?;
    match picked {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    }
}

fn json_pick_u32(value: &serde_json::Value, keys: &[&str]) -> Option<u32> {
    let picked = json_pick_value(value, keys)?;
    match picked {
        serde_json::Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                return u32::try_from(unsigned).ok();
            }
            if let Some(signed) = number.as_i64() {
                return u32::try_from(signed.max(0) as u64).ok();
            }
            if let Some(float) = number.as_f64() {
                if float.is_finite() && float >= 0.0 {
                    return u32::try_from(float.round() as u64).ok();
                }
            }
            None
        }
        serde_json::Value::String(text) => text.trim().parse::<u32>().ok(),
        _ => None,
    }
}

fn json_pick_bool(value: &serde_json::Value, keys: &[&str]) -> Option<bool> {
    let picked = json_pick_value(value, keys)?;
    match picked {
        serde_json::Value::Bool(boolean) => Some(*boolean),
        serde_json::Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        },
        serde_json::Value::Number(number) => {
            if let Some(unsigned) = number.as_u64() {
                Some(unsigned > 0)
            } else if let Some(signed) = number.as_i64() {
                Some(signed > 0)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IcyNowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub raw_title: String,
}

include!("auth_native_and_stream.rs");
include!("library_browsing.rs");
include!("bookmarks_favorites_and_playlists.rs");
include!("playlist_mutations.rs");
include!("radio_search_and_scrobble.rs");

fn normalize_cover_art_id(cover_art_id: &str) -> String {
    let trimmed = cover_art_id.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Navidrome can expose cache-busted ids like `mf-abc123_69733a55` in some payloads.
    // Subsonic `getCoverArt` expects the stable id portion (`mf-abc123`).
    if let Some((base, suffix)) = trimmed.rsplit_once('_') {
        if !base.is_empty()
            && suffix.len() == 8
            && suffix.chars().all(|ch| ch.is_ascii_hexdigit())
            && (base.starts_with("mf-")
                || base.starts_with("ar-")
                || base.starts_with("al-")
                || base.starts_with("pl-"))
        {
            return base.to_string();
        }
    }

    trimmed.to_string()
}

fn normalized_cover_art_candidate(value: &str) -> Option<String> {
    let normalized = normalize_cover_art_id(value);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_cover_art_with_fallback(cover_art: &mut Option<String>, fallback_candidates: &[&str]) {
    if let Some(existing) = cover_art
        .as_ref()
        .and_then(|value| normalized_cover_art_candidate(value))
    {
        *cover_art = Some(existing);
        return;
    }

    for fallback in fallback_candidates {
        if let Some(value) = normalized_cover_art_candidate(fallback) {
            *cover_art = Some(value);
            return;
        }
    }

    *cover_art = None;
}

fn normalize_album_cover_art(album: &mut Album) {
    normalize_cover_art_with_fallback(&mut album.cover_art, &[album.id.as_str()]);
}

fn normalize_artist_cover_art(artist: &mut Artist) {
    normalize_cover_art_with_fallback(&mut artist.cover_art, &[artist.id.as_str()]);
}

fn normalize_playlist_cover_art(playlist: &mut Playlist) {
    normalize_cover_art_with_fallback(&mut playlist.cover_art, &[playlist.id.as_str()]);
}

fn normalize_song_cover_art(song: &mut Song) {
    let album_id = song.album_id.as_deref().unwrap_or("");
    normalize_cover_art_with_fallback(&mut song.cover_art, &[album_id, song.id.as_str()]);
}

#[cfg(not(target_arch = "wasm32"))]
fn icy_metadata_candidate_urls(stream_url: &str) -> Vec<String> {
    let mut urls = vec![stream_url.to_string()];

    if let Ok(mut parsed) = reqwest::Url::parse(stream_url) {
        let path = parsed.path().to_string();
        if !path.ends_with(';') {
            let fallback_path = if path.ends_with('/') {
                format!("{path};")
            } else {
                format!("{path}/;")
            };
            parsed.set_path(&fallback_path);
            let fallback = parsed.to_string();
            if fallback != stream_url {
                urls.push(fallback);
            }
        }
    }

    urls
}

fn parse_bookmark_timestamp(value: &str) -> Option<i64> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }

    None
}

fn bookmark_sort_timestamp(bookmark: &Bookmark) -> i64 {
    bookmark
        .changed
        .as_deref()
        .and_then(parse_bookmark_timestamp)
        .or_else(|| {
            bookmark
                .created
                .as_deref()
                .and_then(parse_bookmark_timestamp)
        })
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
async fn read_icy_now_playing_from_url(stream_url: &str) -> Result<Option<IcyNowPlaying>, String> {
    let mut response = HTTP_CLIENT
        .get(stream_url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", CLIENT_NAME)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let metaint = response
        .headers()
        .get("icy-metaint")
        .or_else(|| response.headers().get("Icy-MetaInt"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    if metaint == 0 {
        return Ok(None);
    }

    let mut buffer = Vec::<u8>::new();
    let mut audio_remaining = metaint;
    let mut metadata_len: Option<usize> = None;
    let mut blocks_checked = 0usize;
    let mut total_bytes = 0usize;
    const MAX_BLOCKS: usize = 8;
    const MAX_BYTES: usize = 1024 * 1024;

    while blocks_checked < MAX_BLOCKS && total_bytes < MAX_BYTES {
        let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? else {
            break;
        };
        total_bytes += chunk.len();
        buffer.extend_from_slice(&chunk);

        loop {
            if audio_remaining > 0 {
                if buffer.len() < audio_remaining {
                    audio_remaining -= buffer.len();
                    buffer.clear();
                    break;
                }
                buffer.drain(..audio_remaining);
                audio_remaining = 0;
            }

            if metadata_len.is_none() {
                if buffer.is_empty() {
                    break;
                }
                let len = buffer[0] as usize * 16;
                buffer.drain(..1);
                if len == 0 {
                    blocks_checked += 1;
                    audio_remaining = metaint;
                    continue;
                }
                metadata_len = Some(len);
            }

            let Some(len) = metadata_len else {
                break;
            };
            if buffer.len() < len {
                break;
            }

            let metadata_block: Vec<u8> = buffer.drain(..len).collect();
            blocks_checked += 1;
            metadata_len = None;
            audio_remaining = metaint;

            if let Some(now_playing) = parse_icy_metadata_block(&metadata_block) {
                return Ok(Some(now_playing));
            }
        }
    }

    Ok(None)
}

fn parse_icy_metadata_block(block: &[u8]) -> Option<IcyNowPlaying> {
    let raw = String::from_utf8_lossy(block).replace('\0', "");
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let stream_title = extract_icy_field(raw, "StreamTitle")?;
    let stream_title = stream_title.trim();
    if stream_title.is_empty() {
        return None;
    }

    let (artist, title) = if let Some((artist, title)) = stream_title.split_once(" - ") {
        let artist = artist.trim().to_string();
        let title = title.trim().to_string();
        if !artist.is_empty() && !title.is_empty() {
            (Some(artist), title)
        } else {
            (None, stream_title.to_string())
        }
    } else {
        (None, stream_title.to_string())
    };

    Some(IcyNowPlaying {
        title,
        artist,
        raw_title: stream_title.to_string(),
    })
}

fn extract_icy_field(raw: &str, field: &str) -> Option<String> {
    let single = format!("{field}='");
    if let Some(start) = raw.find(&single) {
        let tail = &raw[start + single.len()..];
        let end = tail.find("';").or_else(|| tail.find('\''))?;
        return Some(tail[..end].to_string());
    }

    let double = format!("{field}=\"");
    if let Some(start) = raw.find(&double) {
        let tail = &raw[start + double.len()..];
        let end = tail.find("\";").or_else(|| tail.find('"'))?;
        return Some(tail[..end].to_string());
    }

    None
}

// Simple URL encoding for parameters
fn urlencoding_simple(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

include!("response_models.rs");
