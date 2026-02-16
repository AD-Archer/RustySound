use crate::api::Song;
use once_cell::sync::Lazy;
use reqwest::header::HeaderMap;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::header::{HeaderValue, REFERER, USER_AGENT};
use serde::Deserialize;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

static LYRICS_HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
static LYRICS_SUCCESS_CACHE: Lazy<Mutex<HashMap<String, LyricsResult>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub const DEFAULT_LYRICS_PROVIDER_KEYS: [&str; 3] = ["lrclib", "netease", "genius"];

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

pub fn normalize_lyrics_provider_order(order: &[String]) -> Vec<String> {
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

    for provider in DEFAULT_LYRICS_PROVIDER_KEYS {
        if !normalized.iter().any(|existing| existing == provider) {
            normalized.push(provider.to_string());
        }
    }

    normalized
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
    let normalized_provider_order = normalize_lyrics_provider_order(provider_order);
    let cache_key = lyrics_cache_key(query, &normalized_provider_order, timeout_seconds);

    if let Ok(cache) = LYRICS_SUCCESS_CACHE.lock() {
        if let Some(cached) = cache.get(&cache_key).cloned() {
            return Ok(cached);
        }
    }

    let providers = normalized_provider_order
        .iter()
        .filter_map(|key| LyricsProvider::from_key(key))
        .collect::<Vec<_>>();

    let mut errors = Vec::new();

    for provider in providers {
        match fetch_from_provider(provider, query, timeout_seconds).await {
            Ok(Some(result)) => {
                if let Ok(mut cache) = LYRICS_SUCCESS_CACHE.lock() {
                    cache.insert(cache_key.clone(), result.clone());
                }
                return Ok(result);
            }
            Ok(None) => errors.push(format!("{} returned no lyrics", provider.label())),
            Err(error) => errors.push(format!("{} failed: {}", provider.label(), error)),
        }
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
    let normalized_provider_order = normalize_lyrics_provider_order(provider_order);
    let providers = normalized_provider_order
        .iter()
        .filter_map(|key| LyricsProvider::from_key(key))
        .collect::<Vec<_>>();

    if providers.is_empty() {
        return Err("No lyrics providers configured.".to_string());
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
    flattened.sort_by(|(_, left_score, left_order), (_, right_score, right_order)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_order.cmp(right_order))
    });

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
    Err("provider unavailable in browser due CORS policy".to_string())
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
            let title = compact_whitespace(
                song.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
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
    Err("provider unavailable in browser due CORS policy".to_string())
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
            let duration_seconds = entry.duration.map(|seconds| seconds.round().max(0.0) as u32);
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

async fn fetch_from_lrclib(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let request_timeout = Duration::from_secs(timeout_seconds as u64);

    let mut params = vec![("track_name".to_string(), query.title.clone())];
    if !query.artist.trim().is_empty() {
        params.push(("artist_name".to_string(), query.artist.clone()));
    }
    if !query.album.trim().is_empty() {
        params.push(("album_name".to_string(), query.album.clone()));
    }
    if let Some(duration) = query.duration_seconds {
        params.push(("duration".to_string(), duration.to_string()));
    }

    let direct = LYRICS_HTTP_CLIENT
        .get("https://lrclib.net/api/get")
        .query(&params)
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

#[derive(Debug, Clone, Deserialize)]
struct GeniusSearchResponse {
    response: GeniusSearchPayload,
}

#[derive(Debug, Clone, Deserialize)]
struct GeniusSearchPayload {
    #[serde(default)]
    sections: Vec<GeniusSection>,
}

#[derive(Debug, Clone, Deserialize)]
struct GeniusSection {
    #[serde(default)]
    hits: Vec<GeniusHit>,
}

#[derive(Debug, Clone, Deserialize)]
struct GeniusHit {
    #[serde(default, rename = "type")]
    hit_type: String,
    result: GeniusSongResult,
}

#[derive(Debug, Clone, Deserialize)]
struct GeniusSongResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    primary_artist: Option<GeniusArtist>,
}

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
    Err("provider unavailable in browser due CORS policy".to_string())
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
    Err("provider unavailable in browser due CORS policy".to_string())
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
            let score = score_match(&hit.result.title, &artist, None, query);
            if hit.result.url.trim().is_empty() {
                None
            } else {
                Some((hit.result.url, score))
            }
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

fn netease_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (RustySound Lyrics Client; +https://github.com)"),
        );
        headers.insert(REFERER, HeaderValue::from_static("https://music.163.com/"));
        headers.insert("Origin", HeaderValue::from_static("https://music.163.com"));
    }
    headers
}

fn optional_browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    #[cfg(not(target_arch = "wasm32"))]
    {
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (RustySound Lyrics Client; +https://github.com)"),
        );
    }
    headers
}

fn lyrics_cache_key(query: &LyricsQuery, provider_order: &[String], timeout_seconds: u32) -> String {
    let title = normalize_for_match(&query.title);
    let artist = normalize_for_match(&query.artist);
    let album = normalize_for_match(&query.album);
    let duration = query.duration_seconds.unwrap_or(0);
    let providers = provider_order.join(",");
    format!("{title}|{artist}|{album}|{duration}|{timeout_seconds}|{providers}")
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

fn extract_genius_lyrics(html: &str) -> Option<String> {
    let mut captured = Vec::<String>::new();
    let mut remaining = html;

    while let Some(index) = remaining.find("data-lyrics-container=\"true\"") {
        let segment = &remaining[index..];
        let Some(content_start) = segment.find('>') else {
            break;
        };
        let after_start = &segment[content_start + 1..];
        let Some(content_end) = after_start.find("</div>") else {
            break;
        };

        let raw_block = &after_start[..content_end];
        let normalized = raw_block
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<br>", "\n");
        let text = decode_html_entities(&strip_html_tags(&normalized));
        let text = text.trim();
        if !text.is_empty() {
            captured.push(text.to_string());
        }

        remaining = &after_start[content_end + 6..];
    }

    if captured.is_empty() {
        None
    } else {
        Some(captured.join("\n"))
    }
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;

    for character in input.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }

    output
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
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
