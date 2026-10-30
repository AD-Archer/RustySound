use crate::api::Song;
use crate::cache_service::{
    get_json as cache_get_json, is_offline_mode, put_json as cache_put_json,
};
use once_cell::sync::Lazy;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::header::{HeaderMap, HeaderValue, REFERER, USER_AGENT};
use serde::Deserialize;
use serde::Serialize;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

static LYRICS_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
static LYRICS_SUCCESS_CACHE: Lazy<Mutex<HashMap<String, LyricsResult>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub const DEFAULT_LYRICS_PROVIDER_KEYS: [&str; 3] = ["lrclib", "genius", "netease"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LyricsProvider {
    Netease,
    Lrclib,
    Genius,
}

impl LyricsProvider {
    pub fn key(self) -> &'static str {
        match self {
            Self::Netease => "netease",
            Self::Lrclib => "lrclib",
            Self::Genius => "genius",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Netease => "Netease",
            Self::Lrclib => "LRCLIB",
            Self::Genius => "Genius",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "netease" => Some(Self::Netease),
            "lrclib" | "lrc-lib" | "lrlib" => Some(Self::Lrclib),
            "genius" => Some(Self::Genius),
            _ => None,
        }
    }
}

pub fn default_lyrics_provider_order() -> Vec<String> {
    DEFAULT_LYRICS_PROVIDER_KEYS
        .iter()
        .map(|provider| provider.to_string())
        .collect()
}

fn normalize_provider_keys(order: &[String], append_defaults: bool) -> Vec<String> {
    let mut normalized = Vec::new();

    for key in order {
        let Some(provider) = LyricsProvider::from_key(key) else {
            continue;
        };
        let canonical = provider.key().to_string();
        if !normalized.iter().any(|existing| existing == &canonical) {
            normalized.push(canonical);
        }
    }

    if append_defaults {
        for provider in DEFAULT_LYRICS_PROVIDER_KEYS {
            if !normalized.iter().any(|existing| existing == provider) {
                normalized.push(provider.to_string());
            }
        }
    }

    normalized
}

pub fn normalize_lyrics_provider_order(order: &[String]) -> Vec<String> {
    normalize_provider_keys(order, true)
}

fn normalize_lyrics_provider_selection(order: &[String]) -> Vec<String> {
    let normalized = normalize_provider_keys(order, false);
    if normalized.is_empty() {
        default_lyrics_provider_order()
    } else {
        normalized
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_seconds: Option<u32>,
}

impl LyricsQuery {
    pub fn from_song(song: &Song) -> Self {
        Self {
            title: compact_whitespace(&song.title),
            artist: compact_whitespace(song.artist.as_deref().unwrap_or_default()),
            album: compact_whitespace(song.album.as_deref().unwrap_or_default()),
            duration_seconds: Some(song.duration).filter(|duration| *duration > 0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    pub timestamp_seconds: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LyricsResult {
    pub provider: LyricsProvider,
    pub plain_lyrics: String,
    pub synced_lines: Vec<LyricLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentLyricLine {
    timestamp_seconds: f64,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentLyricsResult {
    provider_key: String,
    plain_lyrics: String,
    synced_lines: Vec<PersistentLyricLine>,
}

impl PersistentLyricsResult {
    fn to_runtime(&self) -> Option<LyricsResult> {
        let provider = LyricsProvider::from_key(&self.provider_key)?;
        Some(LyricsResult {
            provider,
            plain_lyrics: self.plain_lyrics.clone(),
            synced_lines: self
                .synced_lines
                .iter()
                .map(|line| LyricLine {
                    timestamp_seconds: line.timestamp_seconds,
                    text: line.text.clone(),
                })
                .collect(),
        })
    }
}

impl From<&LyricsResult> for PersistentLyricsResult {
    fn from(value: &LyricsResult) -> Self {
        Self {
            provider_key: value.provider.key().to_string(),
            plain_lyrics: value.plain_lyrics.clone(),
            synced_lines: value
                .synced_lines
                .iter()
                .map(|line| PersistentLyricLine {
                    timestamp_seconds: line.timestamp_seconds,
                    text: line.text.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricsSearchCandidate {
    pub provider: LyricsProvider,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_seconds: Option<u32>,
    pub query: LyricsQuery,
}

pub async fn fetch_lyrics_with_fallback(
    query: &LyricsQuery,
    provider_order: &[String],
    timeout_seconds: u32,
) -> Result<LyricsResult, String> {
    if query.title.trim().is_empty() {
        return Err("Missing song title for lyrics lookup.".to_string());
    }

    let timeout_seconds = timeout_seconds.clamp(1, 20);
    let normalized_provider_order = normalize_lyrics_provider_selection(provider_order);
    let scoped_cache_key = lyrics_cache_key(query, &normalized_provider_order, timeout_seconds);
    let query_cache_key = lyrics_query_cache_key(query);
    // v2: results cached before the stricter Genius matching and header
    // stripping (2026-10) could be another song's lyrics or carry page junk.
    let scoped_persistent_cache_key = format!("lyrics:v2:result:{scoped_cache_key}");
    let query_persistent_cache_key = format!("lyrics:v2:result:{query_cache_key}");
    let miss_cache_key = format!("lyrics:v2:miss:{scoped_cache_key}");

    if let Ok(cache) = LYRICS_SUCCESS_CACHE.lock() {
        if let Some(cached) = cache.get(&scoped_cache_key).cloned() {
            return Ok(cached);
        }
        if let Some(cached) = cache.get(&query_cache_key).cloned() {
            return Ok(cached);
        }
    }

    if let Some(cached) = cache_get_json::<PersistentLyricsResult>(&scoped_persistent_cache_key) {
        if let Some(runtime) = cached.to_runtime() {
            if let Ok(mut cache) = LYRICS_SUCCESS_CACHE.lock() {
                cache.insert(scoped_cache_key.clone(), runtime.clone());
                cache.insert(query_cache_key.clone(), runtime.clone());
            }
            return Ok(runtime);
        }
    }

    if let Some(cached) = cache_get_json::<PersistentLyricsResult>(&query_persistent_cache_key) {
        if let Some(runtime) = cached.to_runtime() {
            if let Ok(mut cache) = LYRICS_SUCCESS_CACHE.lock() {
                cache.insert(scoped_cache_key.clone(), runtime.clone());
                cache.insert(query_cache_key.clone(), runtime.clone());
            }
            return Ok(runtime);
        }
    }

    if cache_get_json::<bool>(&miss_cache_key).unwrap_or(false) {
        return Err("No lyrics found for this song.".to_string());
    }

    if is_offline_mode() {
        return Err(
            "Offline mode is enabled. Disable offline mode to fetch new lyrics.".to_string(),
        );
    }

    let providers = normalized_provider_order
        .iter()
        .filter_map(|key| LyricsProvider::from_key(key))
        .collect::<Vec<_>>();

    let mut errors = Vec::new();
    let mut saw_hard_error = false;

    for provider in providers {
        match fetch_from_provider(provider, query, timeout_seconds).await {
            Ok(Some(result)) => {
                if let Ok(mut cache) = LYRICS_SUCCESS_CACHE.lock() {
                    cache.insert(scoped_cache_key.clone(), result.clone());
                    cache.insert(query_cache_key.clone(), result.clone());
                }
                let persistent = PersistentLyricsResult::from(&result);
                let _ = cache_put_json(
                    scoped_persistent_cache_key.clone(),
                    &persistent,
                    Some(24 * 14),
                );
                let _ = cache_put_json(
                    query_persistent_cache_key.clone(),
                    &persistent,
                    Some(24 * 14),
                );
                return Ok(result);
            }
            Ok(None) => errors.push(format!("{} returned no lyrics", provider.label())),
            Err(error) => {
                saw_hard_error = true;
                errors.push(format!("{} failed: {}", provider.label(), error));
            }
        }
    }

    if !saw_hard_error {
        let _ = cache_put_json(miss_cache_key, &true, Some(24));
    }

    if errors.is_empty() {
        Err("No lyrics providers configured.".to_string())
    } else {
        Err(errors.join(" | "))
    }
}

pub async fn search_lyrics_candidates(
    query: &LyricsQuery,
    provider_order: &[String],
    timeout_seconds: u32,
) -> Result<Vec<LyricsSearchCandidate>, String> {
    if query.title.trim().is_empty() {
        return Err("Missing song title for lyrics search.".to_string());
    }

    let timeout_seconds = timeout_seconds.clamp(1, 20);
    let normalized_provider_order = normalize_lyrics_provider_selection(provider_order);
    let providers = normalized_provider_order
        .iter()
        .filter_map(|key| LyricsProvider::from_key(key))
        .collect::<Vec<_>>();

    if providers.is_empty() {
        return Err("No lyrics providers configured.".to_string());
    }

    if is_offline_mode() {
        return Err(
            "Offline mode is enabled. Disable offline mode to search new lyrics.".to_string(),
        );
    }

    let mut ranked = Vec::<(LyricsSearchCandidate, i32, usize)>::new();
    let mut errors = Vec::<String>::new();

    for (order_index, provider) in providers.iter().enumerate() {
        match search_provider_candidates(*provider, query, timeout_seconds).await {
            Ok(entries) => {
                ranked.extend(
                    entries
                        .into_iter()
                        .map(|(candidate, score)| (candidate, score, order_index)),
                );
            }
            Err(error) => {
                errors.push(format!("{} failed: {}", provider.label(), error));
            }
        }
    }

    let mut deduped = HashMap::<String, (LyricsSearchCandidate, i32, usize)>::new();
    for (candidate, score, provider_order) in ranked {
        let key = format!(
            "{}|{}|{}|{}",
            candidate.provider.key(),
            normalize_for_match(&candidate.title),
            normalize_for_match(&candidate.artist),
            candidate.duration_seconds.unwrap_or_default()
        );
        let replace = deduped
            .get(&key)
            .map(|(_, existing_score, existing_order)| {
                score > *existing_score
                    || (score == *existing_score && provider_order < *existing_order)
            })
            .unwrap_or(true);
        if replace {
            deduped.insert(key, (candidate, score, provider_order));
        }
    }

    let mut flattened = deduped.into_values().collect::<Vec<_>>();
    flattened.sort_by(
        |(_, left_score, left_order), (_, right_score, right_order)| {
            right_score
                .cmp(left_score)
                .then_with(|| left_order.cmp(right_order))
        },
    );

    let candidates = flattened
        .into_iter()
        .map(|(candidate, _, _)| candidate)
        .take(30)
        .collect::<Vec<_>>();

    if candidates.is_empty() && !errors.is_empty() {
        Err(errors.join(" | "))
    } else {
        Ok(candidates)
    }
}

async fn search_provider_candidates(
    provider: LyricsProvider,
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    match provider {
        LyricsProvider::Netease => search_netease_candidates(query, timeout_seconds).await,
        LyricsProvider::Lrclib => search_lrclib_candidates(query, timeout_seconds).await,
        LyricsProvider::Genius => search_genius_candidates(query, timeout_seconds).await,
    }
}

async fn fetch_from_provider(
    provider: LyricsProvider,
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    match provider {
        LyricsProvider::Netease => fetch_from_netease(query, timeout_seconds).await,
        LyricsProvider::Lrclib => fetch_from_lrclib(query, timeout_seconds).await,
        LyricsProvider::Genius => fetch_from_genius(query, timeout_seconds).await,
    }
}

#[cfg(target_arch = "wasm32")]
async fn search_netease_candidates(
    _query: &LyricsQuery,
    _timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    Ok(Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
async fn search_netease_candidates(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    let search_phrase = compact_whitespace(&format!("{} {}", query.title, query.artist));
    if search_phrase.is_empty() {
        return Ok(Vec::new());
    }

    let headers = netease_headers();
    let search_response = LYRICS_HTTP_CLIENT
        .post("https://music.163.com/api/search/get/web")
        .headers(headers)
        .form(&[
            ("s", search_phrase.as_str()),
            ("type", "1"),
            ("offset", "0"),
            ("total", "true"),
            ("limit", "12"),
        ])
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search_response.status().is_success() {
        return Err(format!(
            "search status {}",
            search_response.status().as_u16()
        ));
    }

    let search_json: Value = search_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let Some(songs) = search_json
        .get("result")
        .and_then(|value| value.get("songs"))
        .and_then(Value::as_array)
    else {
        return Ok(Vec::new());
    };

    let mut candidates = songs
        .iter()
        .filter_map(|song| {
            let title =
                compact_whitespace(song.get("name").and_then(Value::as_str).unwrap_or_default());
            if title.is_empty() {
                return None;
            }
            let artist = compact_whitespace(
                song.get("artists")
                    .and_then(Value::as_array)
                    .and_then(|artists| artists.first())
                    .and_then(|artist| artist.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let album = compact_whitespace(
                song.get("album")
                    .and_then(|album| album.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let duration_seconds = song
                .get("duration")
                .or_else(|| song.get("dt"))
                .and_then(Value::as_u64)
                .map(|duration_ms| (duration_ms / 1000) as u32);
            let candidate_query = LyricsQuery {
                title: title.clone(),
                artist: artist.clone(),
                album: album.clone(),
                duration_seconds,
            };
            let score = score_match(&title, &artist, duration_seconds, query);
            Some((
                LyricsSearchCandidate {
                    provider: LyricsProvider::Netease,
                    title,
                    artist,
                    album,
                    duration_seconds,
                    query: candidate_query,
                },
                score,
            ))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|(_, left_score), (_, right_score)| right_score.cmp(left_score));
    candidates.truncate(20);
    Ok(candidates)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_from_netease(
    _query: &LyricsQuery,
    _timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_from_netease(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let search_phrase = compact_whitespace(&format!("{} {}", query.title, query.artist));
    if search_phrase.is_empty() {
        return Ok(None);
    }

    let headers = netease_headers();
    let search_response = LYRICS_HTTP_CLIENT
        .post("https://music.163.com/api/search/get/web")
        .headers(headers.clone())
        .form(&[
            ("s", search_phrase.as_str()),
            ("type", "1"),
            ("offset", "0"),
            ("total", "true"),
            ("limit", "8"),
        ])
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search_response.status().is_success() {
        return Err(format!(
            "search status {}",
            search_response.status().as_u16()
        ));
    }

    let search_json: Value = search_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let Some(songs) = search_json
        .get("result")
        .and_then(|value| value.get("songs"))
        .and_then(Value::as_array)
    else {
        return Ok(None);
    };

    let best_song_id = songs
        .iter()
        .filter_map(|song| {
            let song_id = song.get("id").and_then(Value::as_u64)?;
            let title = song
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let artist = song
                .get("artists")
                .and_then(Value::as_array)
                .and_then(|artists| artists.first())
                .and_then(|artist| artist.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let duration_seconds = song
                .get("duration")
                .or_else(|| song.get("dt"))
                .and_then(Value::as_u64)
                .map(|duration_ms| (duration_ms / 1000) as u32);
            let score = score_match(&title, &artist, duration_seconds, query);
            Some((song_id, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(song_id, _)| song_id);

    let Some(song_id) = best_song_id else {
        return Ok(None);
    };

    let lyrics_response = LYRICS_HTTP_CLIENT
        .get("https://music.163.com/api/song/lyric")
        .headers(headers)
        .query(&[
            ("id", song_id.to_string()),
            ("lv", "1".to_string()),
            ("kv", "1".to_string()),
            ("tv", "-1".to_string()),
        ])
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !lyrics_response.status().is_success() {
        return Err(format!(
            "lyrics status {}",
            lyrics_response.status().as_u16()
        ));
    }

    let lyrics_json: Value = lyrics_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let synced_lyrics = lyrics_json
        .get("lrc")
        .and_then(|value| value.get("lyric"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if synced_lyrics.trim().is_empty() {
        return Ok(None);
    }

    let synced_lines = parse_lrc_lines(synced_lyrics);
    let plain_lyrics = if synced_lines.is_empty() {
        strip_lrc_metadata(synced_lyrics)
    } else {
        synced_lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };

    if plain_lyrics.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(LyricsResult {
        provider: LyricsProvider::Netease,
        plain_lyrics,
        synced_lines,
    }))
}

#[derive(Debug, Clone, Deserialize)]
struct LrclibResponse {
    #[serde(default, rename = "trackName")]
    track_name: String,
    #[serde(default, rename = "artistName")]
    artist_name: String,
    #[serde(default, rename = "albumName")]
    album_name: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default, rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(default, rename = "plainLyrics")]
    plain_lyrics: Option<String>,
}

async fn search_lrclib_candidates(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    let request_timeout = Duration::from_secs(timeout_seconds as u64);
    let mut params = vec![("track_name".to_string(), query.title.clone())];
    if !query.artist.trim().is_empty() {
        params.push(("artist_name".to_string(), query.artist.clone()));
    }
    if !query.album.trim().is_empty() {
        params.push(("album_name".to_string(), query.album.clone()));
    }

    let search = LYRICS_HTTP_CLIENT
        .get("https://lrclib.net/api/search")
        .query(&params)
        .timeout(request_timeout)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search.status().is_success() {
        return Err(format!("search status {}", search.status().as_u16()));
    }

    let mut candidates = search
        .json::<Vec<LrclibResponse>>()
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|entry| {
            if build_lrclib_result(entry.clone()).is_none() {
                return None;
            }

            let title = compact_whitespace(&entry.track_name);
            if title.is_empty() {
                return None;
            }
            let artist = compact_whitespace(&entry.artist_name);
            let album = compact_whitespace(&entry.album_name);
            let duration_seconds = entry
                .duration
                .map(|seconds| seconds.round().max(0.0) as u32);
            let candidate_query = LyricsQuery {
                title: title.clone(),
                artist: artist.clone(),
                album: album.clone(),
                duration_seconds,
            };
            let score = score_match(&title, &artist, duration_seconds, query);
            Some((
                LyricsSearchCandidate {
                    provider: LyricsProvider::Lrclib,
                    title,
                    artist,
                    album,
                    duration_seconds,
                    query: candidate_query,
                },
                score,
            ))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|(_, left_score), (_, right_score)| right_score.cmp(left_score));
    candidates.truncate(30);
    Ok(candidates)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_from_lrclib(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let request_timeout = Duration::from_secs(timeout_seconds as u64);
    let mut search_params = vec![("track_name".to_string(), query.title.clone())];
    if !query.artist.trim().is_empty() {
        search_params.push(("artist_name".to_string(), query.artist.clone()));
    }
    if !query.album.trim().is_empty() {
        search_params.push(("album_name".to_string(), query.album.clone()));
    }

    // Web: avoid `/api/get` which frequently returns 404 and spams console/network logs.
    let search = LYRICS_HTTP_CLIENT
        .get("https://lrclib.net/api/search")
        .query(&search_params)
        .timeout(request_timeout)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search.status().is_success() {
        return Err(format!("search status {}", search.status().as_u16()));
    }

    let candidates: Vec<LrclibResponse> = search.json().await.map_err(|error| error.to_string())?;
    let best = candidates
        .into_iter()
        .map(|entry| {
            let score = score_match(
                &entry.track_name,
                &entry.artist_name,
                entry
                    .duration
                    .map(|seconds| seconds.round().max(0.0) as u32),
                query,
            );
            (entry, score)
        })
        .max_by(|(_, left_score), (_, right_score)| left_score.cmp(right_score))
        .map(|(entry, _)| entry);

    Ok(best.and_then(build_lrclib_result))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_from_lrclib(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let request_timeout = Duration::from_secs(timeout_seconds as u64);

    let mut direct_params = vec![("track_name".to_string(), query.title.clone())];
    if !query.artist.trim().is_empty() {
        direct_params.push(("artist_name".to_string(), query.artist.clone()));
    }
    if !query.album.trim().is_empty() {
        direct_params.push(("album_name".to_string(), query.album.clone()));
    }
    if let Some(duration) = query.duration_seconds {
        direct_params.push(("duration".to_string(), duration.to_string()));
    }

    let direct = LYRICS_HTTP_CLIENT
        .get("https://lrclib.net/api/get")
        .query(&direct_params)
        .timeout(request_timeout)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if direct.status().is_success() {
        let candidate: LrclibResponse = direct.json().await.map_err(|error| error.to_string())?;
        if let Some(result) = build_lrclib_result(candidate) {
            return Ok(Some(result));
        }
    }

    let mut search_params = vec![("track_name".to_string(), query.title.clone())];
    if !query.artist.trim().is_empty() {
        search_params.push(("artist_name".to_string(), query.artist.clone()));
    }
    if !query.album.trim().is_empty() {
        search_params.push(("album_name".to_string(), query.album.clone()));
    }

    let search = LYRICS_HTTP_CLIENT
        .get("https://lrclib.net/api/search")
        .query(&search_params)
        .timeout(request_timeout)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search.status().is_success() {
        return Err(format!("search status {}", search.status().as_u16()));
    }

    let candidates: Vec<LrclibResponse> = search.json().await.map_err(|error| error.to_string())?;
    let best = candidates
        .into_iter()
        .map(|entry| {
            let score = score_match(
                &entry.track_name,
                &entry.artist_name,
                entry
                    .duration
                    .map(|seconds| seconds.round().max(0.0) as u32),
                query,
            );
            (entry, score)
        })
        .max_by(|(_, left_score), (_, right_score)| left_score.cmp(right_score))
        .map(|(entry, _)| entry);

    Ok(best.and_then(build_lrclib_result))
}

fn build_lrclib_result(response: LrclibResponse) -> Option<LyricsResult> {
    let synced_source = response.synced_lyrics.unwrap_or_default();
    let synced_lines = parse_lrc_lines(&synced_source);

    let plain = response.plain_lyrics.unwrap_or_default().trim().to_string();
    let plain_lyrics = if !plain.is_empty() {
        plain
    } else if !synced_lines.is_empty() {
        synced_lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        strip_lrc_metadata(&synced_source)
    };

    if plain_lyrics.trim().is_empty() {
        return None;
    }

    Some(LyricsResult {
        provider: LyricsProvider::Lrclib,
        plain_lyrics,
        synced_lines,
    })
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusSearchResponse {
    response: GeniusSearchPayload,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusSearchPayload {
    #[serde(default)]
    sections: Vec<GeniusSection>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusSection {
    #[serde(default)]
    hits: Vec<GeniusHit>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusHit {
    #[serde(default, rename = "type")]
    hit_type: String,
    result: GeniusSongResult,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusSongResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    primary_artist: Option<GeniusArtist>,
    /// Every credited artist ("Howard Ashman & Alan Menken").
    #[serde(default)]
    artist_names: String,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Deserialize)]
struct GeniusArtist {
    #[serde(default)]
    name: String,
}

#[cfg(target_arch = "wasm32")]
async fn search_genius_candidates(
    _query: &LyricsQuery,
    _timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    Ok(Vec::new())
}

#[cfg(not(target_arch = "wasm32"))]
async fn search_genius_candidates(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Vec<(LyricsSearchCandidate, i32)>, String> {
    let search_phrase = compact_whitespace(&format!("{} {}", query.title, query.artist));
    if search_phrase.is_empty() {
        return Ok(Vec::new());
    }

    let search_response = LYRICS_HTTP_CLIENT
        .get("https://genius.com/api/search/multi")
        .query(&[("q", search_phrase.as_str()), ("per_page", "12")])
        .headers(optional_browser_headers())
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search_response.status().is_success() {
        return Err(format!(
            "search status {}",
            search_response.status().as_u16()
        ));
    }

    let payload: GeniusSearchResponse = search_response
        .json()
        .await
        .map_err(|error| error.to_string())?;

    let mut candidates = payload
        .response
        .sections
        .into_iter()
        .flat_map(|section| section.hits)
        .filter(|hit| hit.hit_type.eq_ignore_ascii_case("song"))
        .filter_map(|hit| {
            let title = compact_whitespace(&hit.result.title);
            if title.is_empty() {
                return None;
            }
            let artist = compact_whitespace(
                hit.result
                    .primary_artist
                    .as_ref()
                    .map(|artist| artist.name.as_str())
                    .unwrap_or_default(),
            );
            let candidate_query = LyricsQuery {
                title: title.clone(),
                artist: artist.clone(),
                album: String::new(),
                duration_seconds: None,
            };
            let score = score_match(&title, &artist, None, query);
            Some((
                LyricsSearchCandidate {
                    provider: LyricsProvider::Genius,
                    title,
                    artist,
                    album: String::new(),
                    duration_seconds: None,
                    query: candidate_query,
                },
                score,
            ))
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|(_, left_score), (_, right_score)| right_score.cmp(left_score));
    candidates.truncate(20);
    Ok(candidates)
}

#[cfg(target_arch = "wasm32")]
async fn fetch_from_genius(
    _query: &LyricsQuery,
    _timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_from_genius(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let search_phrase = compact_whitespace(&format!("{} {}", query.title, query.artist));
    if search_phrase.is_empty() {
        return Ok(None);
    }

    let search_response = LYRICS_HTTP_CLIENT
        .get("https://genius.com/api/search/multi")
        .query(&[("q", search_phrase.as_str()), ("per_page", "5")])
        .headers(optional_browser_headers())
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !search_response.status().is_success() {
        return Err(format!(
            "search status {}",
            search_response.status().as_u16()
        ));
    }

    let payload: GeniusSearchResponse = search_response
        .json()
        .await
        .map_err(|error| error.to_string())?;

    // Only hits whose title and artist really match: a wrong song's lyrics
    // are worse than "No lyrics".
    let best_url = payload
        .response
        .sections
        .into_iter()
        .flat_map(|section| section.hits)
        .filter(|hit| hit.hit_type.eq_ignore_ascii_case("song"))
        .filter_map(|hit| {
            let artist = hit
                .result
                .primary_artist
                .as_ref()
                .map(|artist| artist.name.clone())
                .unwrap_or_default();
            let credited = if hit.result.artist_names.trim().is_empty() {
                artist.clone()
            } else {
                hit.result.artist_names.clone()
            };
            if hit.result.url.trim().is_empty()
                || !genius_hit_matches(&hit.result.title, &credited, query)
            {
                return None;
            }
            let score = score_match(&hit.result.title, &artist, None, query);
            Some((hit.result.url, score))
        })
        .max_by(|(_, left_score), (_, right_score)| left_score.cmp(right_score))
        .map(|(url, _)| url);

    let Some(url) = best_url else {
        return Ok(None);
    };

    let html_response = LYRICS_HTTP_CLIENT
        .get(url)
        .headers(optional_browser_headers())
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await
        .map_err(|error| error.to_string())?;

    if !html_response.status().is_success() {
        return Err(format!("page status {}", html_response.status().as_u16()));
    }

    let html = html_response
        .text()
        .await
        .map_err(|error| error.to_string())?;
    let plain_lyrics = extract_genius_lyrics(&html).unwrap_or_default();
    if plain_lyrics.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(LyricsResult {
        provider: LyricsProvider::Genius,
        plain_lyrics,
        synced_lines: Vec::new(),
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn netease_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (RustySound Lyrics Client; +https://github.com)"),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://music.163.com/"));
    headers.insert("Origin", HeaderValue::from_static("https://music.163.com"));
    headers
}

#[cfg(not(target_arch = "wasm32"))]
fn optional_browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Mozilla/5.0 (RustySound Lyrics Client; +https://github.com)"),
    );
    headers
}

fn lyrics_cache_key(
    query: &LyricsQuery,
    provider_order: &[String],
    timeout_seconds: u32,
) -> String {
    let base = lyrics_query_cache_key(query);
    let providers = provider_order.join(",");
    format!("{base}|{timeout_seconds}|{providers}")
}

fn lyrics_query_cache_key(query: &LyricsQuery) -> String {
    let title = normalize_for_match(&query.title);
    let artist = normalize_for_match(&query.artist);
    let album = normalize_for_match(&query.album);
    let duration = query.duration_seconds.unwrap_or(0);
    format!("{title}|{artist}|{album}|{duration}")
}

fn score_match(title: &str, artist: &str, duration: Option<u32>, query: &LyricsQuery) -> i32 {
    let title = normalize_for_match(title);
    let artist = normalize_for_match(artist);
    let query_title = normalize_for_match(&query.title);
    let query_artist = normalize_for_match(&query.artist);

    let mut score = 0;

    if title == query_title {
        score += 10;
    } else if title.contains(&query_title) || query_title.contains(&title) {
        score += 6;
    }

    if !query_artist.is_empty() {
        if artist == query_artist {
            score += 8;
        } else if artist.contains(&query_artist) || query_artist.contains(&artist) {
            score += 4;
        }
    }

    if let (Some(found_duration), Some(query_duration)) = (duration, query.duration_seconds) {
        let diff = found_duration.abs_diff(query_duration);
        if diff <= 2 {
            score += 4;
        } else if diff <= 6 {
            score += 2;
        } else if diff <= 12 {
            score += 1;
        }
    }

    score
}

fn parse_lrc_lines(raw_lrc: &str) -> Vec<LyricLine> {
    let mut lines = Vec::<LyricLine>::new();

    for raw_line in raw_lrc.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut timestamps = Vec::<f64>::new();
        let mut rest = trimmed;

        loop {
            if !rest.starts_with('[') {
                break;
            }
            let Some(end_index) = rest.find(']') else {
                break;
            };

            let token = &rest[1..end_index];
            if let Some(value) = parse_lrc_timestamp(token) {
                timestamps.push(value);
            }
            rest = &rest[end_index + 1..];
        }

        if timestamps.is_empty() {
            continue;
        }

        let text = rest.trim();
        if text.is_empty() {
            continue;
        }

        for timestamp in timestamps {
            lines.push(LyricLine {
                timestamp_seconds: timestamp,
                text: text.to_string(),
            });
        }
    }

    lines.sort_by(|left, right| {
        left.timestamp_seconds
            .partial_cmp(&right.timestamp_seconds)
            .unwrap_or(Ordering::Equal)
    });

    lines
}

fn parse_lrc_timestamp(token: &str) -> Option<f64> {
    if token.contains(':') {
        let mut segments = token.split(':').collect::<Vec<_>>();
        if segments.len() < 2 || segments.len() > 3 {
            return None;
        }

        let seconds_segment = segments.pop()?.replace(',', ".");
        let seconds = seconds_segment.parse::<f64>().ok()?;

        let minutes = segments.pop()?.parse::<f64>().ok()?;
        let hours = if let Some(hours_segment) = segments.pop() {
            hours_segment.parse::<f64>().ok()?
        } else {
            0.0
        };

        return Some(hours * 3600.0 + minutes * 60.0 + seconds);
    }

    None
}

fn strip_lrc_metadata(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            if line.starts_with("[ti:")
                || line.starts_with("[ar:")
                || line.starts_with("[al:")
                || line.starts_with("[by:")
                || line.starts_with("[offset:")
            {
                return None;
            }

            let mut content = line;
            while content.starts_with('[') {
                let Some(end_index) = content.find(']') else {
                    break;
                };
                content = &content[end_index + 1..];
            }

            let content = content.trim();
            if content.is_empty() {
                None
            } else {
                Some(content.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Plain lyrics from a Genius song page.
///
/// The lyrics sit in one or more `data-lyrics-container="true"` elements
/// (split around ads). Each container also holds page furniture that is not
/// lyrics: the header ("2 Contributors", "Translations", "<Title> Lyrics",
/// the "Read More" description) and in-read ads, all marked
/// `data-exclude-from-selection="true"`. Containers nest further elements
/// (annotation links, italics), so the walk tracks depth instead of stopping
/// at the first `</div>`.
#[cfg(not(target_arch = "wasm32"))]
fn extract_genius_lyrics(html: &str) -> Option<String> {
    const MARKER: &str = "data-lyrics-container=\"true\"";
    let mut blocks = Vec::<String>::new();
    let mut cursor = 0usize;

    while let Some(found) = html[cursor..].find(MARKER) {
        let marker_at = cursor + found;
        let Some(tag_end) = html[marker_at..].find('>') else {
            break;
        };
        let body_start = marker_at + tag_end + 1;
        let (raw_text, consumed) = genius_container_text(&html[body_start..]);
        let block = clean_genius_text(&decode_html_entities(&raw_text));
        if !block.is_empty() {
            blocks.push(block);
        }
        cursor = body_start + consumed.max(1);
    }

    // Containers split wherever the ads go; a blank line only where the next
    // one opens a new section ("[Chorus]").
    let mut joined = String::new();
    for block in &blocks {
        if !joined.is_empty() {
            joined.push_str(if block.starts_with('[') { "\n\n" } else { "\n" });
        }
        joined.push_str(block);
    }
    let joined = clean_genius_text(&joined);
    (!joined.is_empty()).then_some(joined)
}

/// Text of one lyrics container whose opening tag has been consumed: `<br>`
/// becomes a newline and excluded subtrees are dropped. Returns the text and
/// the number of bytes up to and including the container's closing tag.
#[cfg(not(target_arch = "wasm32"))]
fn genius_container_text(body: &str) -> (String, usize) {
    let mut out = String::new();
    // The container itself is open.
    let mut depth = 1usize;
    // Depth of the excluded element being skipped, if any.
    let mut skip_from: Option<usize> = None;
    let mut index = 0usize;

    while index < body.len() {
        let rest = &body[index..];
        if !rest.starts_with('<') {
            let next = rest.find('<').unwrap_or(rest.len());
            if skip_from.is_none() {
                out.push_str(&rest[..next]);
            }
            index += next;
            continue;
        }

        if rest.starts_with("<!--") {
            index += rest.find("-->").map(|end| end + 3).unwrap_or(rest.len());
            continue;
        }

        let Some(tag_len) = rest.find('>') else {
            break;
        };
        let tag = &rest[1..tag_len];
        index += tag_len + 1;

        if let Some(closing) = tag.strip_prefix('/') {
            if is_void_html_element(&html_tag_name(closing)) {
                continue;
            }
            depth = depth.saturating_sub(1);
            if skip_from.is_some_and(|open_depth| depth < open_depth) {
                skip_from = None;
            }
            if depth == 0 {
                return (out, index);
            }
            continue;
        }

        let name = html_tag_name(tag);
        if name == "br" {
            if skip_from.is_none() {
                out.push('\n');
            }
            continue;
        }
        if tag.ends_with('/') || is_void_html_element(&name) || name.starts_with('!') {
            continue;
        }
        if name == "script" || name == "style" {
            // Raw text: jump past the matching close tag.
            let close = format!("</{name}");
            let after = &body[index..];
            index += after
                .find(&close)
                .and_then(|at| after[at..].find('>').map(|end| at + end + 1))
                .unwrap_or(after.len());
            continue;
        }

        depth += 1;
        if skip_from.is_none() && is_genius_excluded_tag(tag) {
            skip_from = Some(depth);
        }
    }

    (out, body.len())
}

#[cfg(not(target_arch = "wasm32"))]
fn html_tag_name(tag: &str) -> String {
    tag.trim_start()
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(not(target_arch = "wasm32"))]
fn is_void_html_element(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
            | "source" | "track" | "wbr"
    )
}

/// Page furniture inside a lyrics container.
#[cfg(not(target_arch = "wasm32"))]
fn is_genius_excluded_tag(tag: &str) -> bool {
    tag.contains("data-exclude-from-selection=\"true\"")
        || tag.contains("LyricsHeader__")
        || tag.contains("ContributorsCreditSong__")
        || tag.contains("SongBioPreview__")
        || tag.contains("InreadContainer__")
        || tag.contains("RightSidebar__")
        || tag.contains("RecommendedSongs__")
}

/// Line-level cleanup for text that slipped past the markup filter (older
/// page layouts): a leading "N Contributors…<Title> Lyrics" header, the
/// "You might also like" block, and the trailing "123Embed".
#[cfg(not(target_arch = "wasm32"))]
fn clean_genius_text(text: &str) -> String {
    let mut lines = text
        .lines()
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>();

    // Header junk at the very top.
    while let Some(first) = lines.iter().position(|line| !line.is_empty()) {
        let line = &lines[first];
        if is_genius_header_line(line) {
            lines.remove(first);
        } else {
            break;
        }
    }

    lines.retain(|line| !line.eq_ignore_ascii_case("You might also like"));

    if let Some(last) = lines.iter().rposition(|line| !line.is_empty()) {
        let stripped = strip_genius_embed_suffix(&lines[last]);
        lines[last] = stripped;
    }

    // Collapse runs of blank lines to one and trim the ends.
    let mut out = Vec::<String>::new();
    for line in lines {
        if line.is_empty() && out.last().is_none_or(|prev: &String| prev.is_empty()) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|line| line.is_empty()) {
        out.pop();
    }
    out.join("\n")
}

#[cfg(not(target_arch = "wasm32"))]
fn is_genius_header_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let digits = lower.chars().take_while(|c| c.is_ascii_digit()).count();
    let after_count = lower[digits..].trim_start();
    (digits > 0 && after_count.starts_with("contributor"))
        || lower == "translations"
        || lower == "read more"
}

#[cfg(not(target_arch = "wasm32"))]
fn strip_genius_embed_suffix(line: &str) -> String {
    let Some(head) = line.strip_suffix("Embed") else {
        return line.to_string();
    };
    let head = head.trim_end_matches(|c: char| c.is_ascii_digit());
    // "You might also like12Embed" and "Lyrics…Embed" variants.
    let head = head
        .strip_suffix("You might also like")
        .unwrap_or(head)
        .trim_end();
    head.to_string()
}

/// Named entities Genius emits plus every numeric (`&#39;`, `&#x27;`) one.
#[cfg(not(target_arch = "wasm32"))]
fn decode_html_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        output.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        let Some(semi) = after.find(';').filter(|&semi| semi > 0 && semi <= 10) else {
            output.push('&');
            rest = after;
            continue;
        };
        let entity = &after[..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse::<u32>().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(character) => {
                output.push(character);
                rest = &after[semi + 1..];
            }
            None => {
                output.push('&');
                rest = after;
            }
        }
    }
    output.push_str(rest);
    output
}

/// Whether a Genius search hit is the song being played. Genius search is
/// full-text, so its top hit for an obscure song is often a different song
/// entirely; showing those lyrics is worse than showing none. The main
/// titles (bracketed versions and "feat." credits removed) must agree, and
/// when the song has an artist, the hit's artist must share its name words.
#[cfg(not(target_arch = "wasm32"))]
fn genius_hit_matches(hit_title: &str, hit_artist: &str, query: &LyricsQuery) -> bool {
    let wanted_title = main_title_words(&query.title);
    let found_title = main_title_words(hit_title);
    if wanted_title.is_empty() || found_title.is_empty() {
        return false;
    }
    if word_similarity(&wanted_title, &found_title) < GENIUS_MIN_TITLE_SIMILARITY {
        return false;
    }

    let wanted_artist = artist_words(&query.artist);
    if wanted_artist.is_empty() {
        return true;
    }
    let found_artist = artist_words(hit_artist);
    !found_artist.is_empty()
        && artist_overlap(&wanted_artist, &found_artist) >= GENIUS_MIN_ARTIST_OVERLAP
}

/// Dice similarity of the main title words.
#[cfg(not(target_arch = "wasm32"))]
const GENIUS_MIN_TITLE_SIMILARITY: f64 = 0.8;
/// Share of the shorter artist name's words found in the other.
#[cfg(not(target_arch = "wasm32"))]
const GENIUS_MIN_ARTIST_OVERLAP: f64 = 0.6;

/// Lowercase words with accents folded and punctuation dropped.
#[cfg(not(target_arch = "wasm32"))]
fn match_words(value: &str) -> Vec<String> {
    let folded = value
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
            'ç' | 'Ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
            'ñ' | 'Ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
            'ý' | 'ÿ' | 'Ý' => 'y',
            '’' | '\'' => '\0',
            other if other.is_alphanumeric() => other,
            _ => ' ',
        })
        .filter(|c| *c != '\0')
        .collect::<String>()
        .to_lowercase();
    folded.split_whitespace().map(str::to_string).collect()
}

/// Title words before any "(…)", "[…]", " - …" version suffix or "feat."
/// credit, so "Song (Remastered 2011)" and "Song - Live" compare as "Song".
#[cfg(not(target_arch = "wasm32"))]
fn main_title_words(title: &str) -> Vec<String> {
    let mut main = title;
    for cut in ['(', '['] {
        if let Some(at) = main.find(cut).filter(|&at| at > 0) {
            main = &main[..at];
        }
    }
    if let Some(at) = main.find(" - ").filter(|&at| at > 0) {
        main = &main[..at];
    }
    let lower = main.to_lowercase();
    for credit in [" feat.", " feat ", " ft.", " featuring "] {
        if let Some(at) = lower.find(credit).filter(|&at| at > 0) {
            if main.is_char_boundary(at) {
                main = &main[..at];
            }
            break;
        }
    }
    match_words(main)
}

/// Artist name words without joiners and articles ("The", "&", "and", "x",
/// "feat."), so "Jay-Z & Kanye West" and "JAY-Z" share "jay z".
#[cfg(not(target_arch = "wasm32"))]
fn artist_words(artist: &str) -> Vec<String> {
    match_words(artist)
        .into_iter()
        .filter(|word| {
            !matches!(
                word.as_str(),
                "the" | "and" | "x" | "feat" | "ft" | "featuring" | "with" | "vs"
            )
        })
        .collect()
}

/// Sørensen–Dice coefficient over word multisets.
#[cfg(not(target_arch = "wasm32"))]
fn word_similarity(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let mut unmatched = right.to_vec();
    let mut shared = 0usize;
    for word in left {
        if let Some(at) = unmatched.iter().position(|other| other == word) {
            unmatched.swap_remove(at);
            shared += 1;
        }
    }
    (2 * shared) as f64 / (left.len() + right.len()) as f64
}

/// Share of the shorter name's words that appear in the longer one.
#[cfg(not(target_arch = "wasm32"))]
fn artist_overlap(left: &[String], right: &[String]) -> f64 {
    let (short, long) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    if short.is_empty() {
        return 0.0;
    }
    let shared = short.iter().filter(|word| long.contains(word)).count();
    shared as f64 / short.len() as f64
}

fn compact_whitespace(value: &str) -> String {
    value
        .split_whitespace()
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_for_match(value: &str) -> String {
    compact_whitespace(value)
        .to_ascii_lowercase()
        .replace("feat.", "")
        .replace("feat", "")
        .replace("ft.", "")
        .replace('(', " ")
        .replace(')', " ")
        .replace('[', " ")
        .replace(']', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod genius_tests {
    use super::*;

    const CURRENT_PAGE: &str = include_str!("fixtures/genius_page_current.html");
    const LEGACY_PAGE: &str = include_str!("fixtures/genius_page_legacy.html");

    fn query(title: &str, artist: &str) -> LyricsQuery {
        LyricsQuery {
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration_seconds: None,
        }
    }

    #[test]
    fn current_layout_drops_header_ads_and_sidebar() {
        let lyrics = extract_genius_lyrics(CURRENT_PAGE).expect("lyrics");
        assert_eq!(
            lyrics,
            "[Verse 1]\n\
             Lanterns over the water\n\
             Paper crowns for a sleeping daughter\n\
             We don't wake her, we don't say\n\
             Kingdoms hollow as the day\n\
             \n\
             [Chorus]\n\
             Oh, the hollow, oh, the hollow & the hum\n\
             Every echo says \"come home\"\n\
             \n\
             [Verse 2]\n\
             Salt on the window, it\u{2019}s late\n\
             We carry the kingdom by weight\n\
             \n\
             [Outro]\n\
             Hollow, hollow"
        );
        for junk in [
            "Contributors",
            "Hollow Kingdom Lyrics",
            "Read More",
            "Advertisement",
            "You might also like",
            "Other Song",
            "How to Format",
            "not this",
        ] {
            assert!(!lyrics.contains(junk), "leaked {junk:?}");
        }
    }

    #[test]
    fn legacy_layout_strips_text_header_and_embed() {
        let lyrics = extract_genius_lyrics(LEGACY_PAGE).expect("lyrics");
        assert_eq!(
            lyrics,
            "[Verse]\n\
             Sand in the hourglass, sand in my shoe\n\
             Moon on the dunes and the dunes on the moon\n\
             \n\
             [Chorus]\n\
             Ride, ride, ride till the morning\n\
             \n\
             [Outro]\n\
             And the wind keeps the time"
        );
    }

    #[test]
    fn page_without_lyrics_gives_none() {
        assert_eq!(extract_genius_lyrics("<html><body><p>Nope</p></body></html>"), None);
        let empty = r#"<div data-lyrics-container="true"><div data-exclude-from-selection="true">3 Contributors</div></div>"#;
        assert_eq!(extract_genius_lyrics(empty), None);
    }

    #[test]
    fn entities_decode_named_and_numeric() {
        assert_eq!(
            decode_html_entities("Rock &amp; Roll &#x27;n&#39; &#8220;soul&#8221; &bogus; 5 &lt; 6 & co"),
            "Rock & Roll 'n' \u{201c}soul\u{201d} &bogus; 5 < 6 & co"
        );
    }

    #[test]
    fn unrelated_genius_hit_is_rejected() {
        // The smoke test's mock song matched a Broadway demo on Genius.
        let wanted = query("Hollow Kingdom", "The Hollow Voyagers");
        assert!(!genius_hit_matches("Arabian Nights (Demo)", "Howard Ashman & Alan Menken", &wanted));
        // Same title, different artist.
        assert!(!genius_hit_matches("Hollow Kingdom", "Someone Else", &wanted));
        // Same artist, a different song that shares one word.
        assert!(!genius_hit_matches("Hollow", "The Hollow Voyagers", &wanted));
        assert!(!genius_hit_matches("Kingdom Come", "The Hollow Voyagers", &wanted));
    }

    #[test]
    fn real_variants_of_the_song_are_accepted() {
        let wanted = query("Hollow Kingdom", "The Hollow Voyagers");
        assert!(genius_hit_matches("Hollow Kingdom", "The Hollow Voyagers", &wanted));
        assert!(genius_hit_matches("Hollow Kingdom (Remastered 2011)", "Hollow Voyagers", &wanted));
        assert!(genius_hit_matches("Hollow Kingdom - Live", "The Hollow Voyagers & Friends", &wanted));
        assert!(genius_hit_matches("Hollow Kingdom (feat. Someone)", "The Hollow Voyagers", &wanted));

        let accented = query("Déjà Vu", "Beyoncé feat. Jay-Z");
        assert!(genius_hit_matches("Deja Vu", "Beyoncé", &accented));
        let duo = query("Arabian Nights", "Alan Menken");
        assert!(genius_hit_matches("Arabian Nights (Demo)", "Howard Ashman & Alan Menken", &duo));
    }

    #[test]
    fn missing_artist_matches_on_title_alone() {
        let wanted = query("Hollow Kingdom", "");
        assert!(genius_hit_matches("Hollow Kingdom", "Anyone", &wanted));
        assert!(!genius_hit_matches("Arabian Nights", "Anyone", &wanted));
        assert!(!genius_hit_matches("Hollow Kingdom", "Anyone", &query("", "Anyone")));
    }
}
