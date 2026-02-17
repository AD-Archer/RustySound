use crate::api::models::*;
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

fn json_pick_value<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Value> {
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

impl NavidromeClient {
    pub fn new(server: ServerConfig) -> Self {
        Self { server }
    }

    fn auth_params(&self) -> String {
        let mut cache = AUTH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let cache_key = format!(
            "{}:{}:{}:{}",
            self.server.id, self.server.username, self.server.url, self.server.password
        );

        if let Some(value) = cache.get(&cache_key) {
            return value.clone();
        }

        let value = self.generate_auth_params();
        cache.insert(cache_key, value.clone());
        value
    }

    fn generate_auth_params(&self) -> String {
        // Generate random salt using getrandom (wasm-compatible)
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes).unwrap_or_default();

        let salt: String = bytes
            .iter()
            .map(|b| {
                let idx = (*b as usize) % 36;
                if idx < 10 {
                    (b'0' + idx as u8) as char
                } else {
                    (b'a' + (idx - 10) as u8) as char
                }
            })
            .collect();

        let token_input = format!("{}{}", self.server.password, salt);
        let token = format!("{:x}", md5::compute(token_input.as_bytes()));

        format!(
            "u={}&t={}&s={}&v={}&c={}&f=json",
            self.server.username, token, salt, API_VERSION, CLIENT_NAME
        )
    }

    fn build_url(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> String {
        let auth = self.auth_params();
        let mut url = format!("{}/rest/{}?{}", self.server.url, endpoint, auth);

        for (key, value) in extra_params {
            url.push_str(&format!("&{}={}", key, urlencoding_simple(value)));
        }

        url
    }

    fn build_url_owned(&self, endpoint: &str, extra_params: Vec<(String, String)>) -> String {
        let auth = self.auth_params();
        let mut url = format!("{}/rest/{}?{}", self.server.url, endpoint, auth);

        for (key, value) in extra_params {
            url.push_str(&format!("&{}={}", key, urlencoding_simple(&value)));
        }

        url
    }

    fn native_cache_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.server.id, self.server.username, self.server.url, self.server.password
        )
    }

    fn native_base_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.server.url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn clear_native_auth_session(&self) {
        let key = self.native_cache_key();
        let mut cache = NATIVE_AUTH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.remove(&key);
    }

    async fn ensure_native_auth_session(&self) -> Result<NativeAuthSession, String> {
        let key = self.native_cache_key();
        {
            let cache = NATIVE_AUTH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = cache.get(&key).cloned() {
                return Ok(session);
            }
        }

        let login_url = self.native_base_url("auth/login");
        let payload = NativeLoginRequest {
            username: self.server.username.clone(),
            password: self.server.password.clone(),
        };

        let response = HTTP_CLIENT
            .post(login_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!(
                "Native API login failed with status {}",
                response.status()
            ));
        }

        let login: NativeLoginResponse = response.json().await.map_err(|e| e.to_string())?;
        let token = login
            .token
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Native API login did not return a token.".to_string())?;
        let client_unique_id = login
            .id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let session = NativeAuthSession {
            token,
            client_unique_id,
        };
        let mut cache = NATIVE_AUTH_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.insert(key, session.clone());
        Ok(session)
    }

    fn normalize_native_song_list(&self, payload: serde_json::Value) -> Vec<Song> {
        let entries = payload
            .as_array()
            .cloned()
            .or_else(|| payload.get("data").and_then(|value| value.as_array()).cloned())
            .or_else(|| payload.get("items").and_then(|value| value.as_array()).cloned())
            .unwrap_or_default();

        let mut songs = Vec::new();
        for value in entries {
            let id = json_pick_string(&value, &["id", "mediaFileId"]);
            let Some(id) = id.filter(|value| !value.trim().is_empty()) else {
                continue;
            };

            let title = json_pick_string(&value, &["title", "name"])
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "Unknown Song".to_string());
            let album = json_pick_string(&value, &["album", "album_name", "albumName"]);
            let album_id = json_pick_string(&value, &["albumId", "album_id", "album_id_fk"]);
            let artist = json_pick_string(&value, &["artist", "artist_name", "artistName"]);
            let artist_id = json_pick_string(&value, &["artistId", "artist_id", "artist_id_fk"]);
            let duration = json_pick_u32(&value, &["duration", "duration_seconds"]).unwrap_or(0);
            let track = json_pick_u32(&value, &["track", "trackNumber", "track_number"]);
            let cover_art = json_pick_string(
                &value,
                &["coverArt", "coverArtId", "cover_art", "cover_art_id"],
            )
            .or_else(|| {
                if json_pick_bool(&value, &["hasCoverArt", "has_cover_art"]) == Some(true) {
                    Some(id.clone())
                } else {
                    None
                }
            });
            let content_type = json_pick_string(&value, &["contentType", "content_type"]);
            let suffix = json_pick_string(&value, &["suffix"]);
            let bitrate = json_pick_u32(&value, &["bitrate"]);
            let starred = match json_pick_bool(&value, &["starred", "isStarred"]) {
                Some(true) => Some("native".to_string()),
                _ => json_pick_string(&value, &["starredAt", "starred"]),
            };
            let user_rating =
                json_pick_u32(&value, &["userRating", "user_rating", "rating"]).map(|value| {
                    if value > 5 {
                        value.min(10).div_ceil(2)
                    } else {
                        value
                    }
                });
            let play_count = json_pick_u32(&value, &["playCount", "play_count"]);
            let played = json_pick_string(
                &value,
                &["lastPlayed", "played", "playDate", "play_date"],
            );
            let year = json_pick_u32(&value, &["year"]);
            let genre = json_pick_string(&value, &["genre"]);

            songs.push(Song {
                id,
                title,
                album,
                album_id,
                artist,
                artist_id,
                duration,
                track,
                cover_art,
                content_type,
                stream_url: None,
                suffix,
                bitrate,
                starred,
                user_rating,
                play_count,
                played,
                year,
                genre,
                server_id: self.server.id.clone(),
                server_name: self.server.name.clone(),
            });
        }

        songs
    }

    pub async fn get_native_songs(
        &self,
        sort: NativeSongSortField,
        order: NativeSortOrder,
        start: usize,
        end: usize,
    ) -> Result<Vec<Song>, String> {
        if end < start {
            return Ok(Vec::new());
        }

        let url = self.native_base_url(&format!(
            "api/song?_start={}&_end={}&_sort={}&_order={}",
            start,
            end,
            sort.as_query_value(),
            order.as_query_value()
        ));

        for attempt in 0..2 {
            let session = self.ensure_native_auth_session().await?;
            let response = HTTP_CLIENT
                .get(&url)
                .header(
                    "x-nd-authorization",
                    format!("Bearer {}", session.token),
                )
                .header("x-nd-client-unique-id", session.client_unique_id)
                .send()
                .await
                .map_err(|e| e.to_string())?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                self.clear_native_auth_session();
                continue;
            }

            if !response.status().is_success() {
                return Err(format!(
                    "Native songs request failed with status {}",
                    response.status()
                ));
            }

            let payload: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
            return Ok(self.normalize_native_song_list(payload));
        }

        Err("Native songs request could not be authorized.".to_string())
    }

    pub fn get_cover_art_url(&self, cover_art_id: &str, size: u32) -> String {
        self.build_url(
            "getCoverArt",
            &[("id", cover_art_id), ("size", &size.to_string())],
        )
    }

    #[allow(dead_code)]
    pub fn get_stream_url(&self, song_id: &str) -> String {
        self.build_url("stream", &[("id", song_id)])
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn read_icy_now_playing(stream_url: &str) -> Result<Option<IcyNowPlaying>, String> {
        for candidate_url in icy_metadata_candidate_urls(stream_url) {
            if let Ok(Some(now_playing)) = read_icy_now_playing_from_url(&candidate_url).await {
                return Ok(Some(now_playing));
            }
        }

        Ok(None)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn read_icy_now_playing(stream_url: &str) -> Result<Option<IcyNowPlaying>, String> {
        let seed_url = serde_json::to_string(stream_url).map_err(|e| e.to_string())?;
        let script = format!(
            r#"return (async () => {{
                const seedUrl = {seed_url};
                if (!seedUrl) return null;

                const urls = [seedUrl];
                try {{
                    const parsed = new URL(seedUrl);
                    const path = parsed.pathname || "";
                    if (!path.endsWith(";")) {{
                        parsed.pathname = path.endsWith("/") ? `${{path}};` : `${{path}}/;`;
                        const fallback = parsed.toString();
                        if (fallback !== seedUrl) {{
                            urls.push(fallback);
                        }}
                    }}
                }} catch (_err) {{}}

                const parseStreamTitle = (rawTitle) => {{
                    if (typeof rawTitle !== "string") return null;
                    const trimmed = rawTitle.trim();
                    if (!trimmed) return null;

                    let artist = null;
                    let title = trimmed;
                    const parts = trimmed.split(" - ");
                    if (parts.length >= 2) {{
                        const left = (parts.shift() || "").trim();
                        const right = parts.join(" - ").trim();
                        if (left && right) {{
                            artist = left;
                            title = right;
                        }}
                    }}

                    return {{
                        title,
                        artist,
                        raw_title: trimmed,
                    }};
                }};

                const metadataUrls = urls.filter((candidate) => {{
                    try {{
                        const parsed = new URL(candidate, window.location.href);
                        return parsed.origin === window.location.origin;
                    }} catch (_err) {{
                        return false;
                    }}
                }});

                if (metadataUrls.length === 0) {{
                    return null;
                }}

                const parseMetadataText = (text) => {{
                    if (!text) return null;
                    const match = text.match(/StreamTitle\s*=\s*['"]([^'"]+)['"]/i);
                    if (!match || !match[1]) return null;
                    return parseStreamTitle(match[1]);
                }};

                for (const url of metadataUrls) {{
                    try {{
                        const response = await fetch(url, {{
                            method: "GET",
                            headers: {{
                                "Icy-MetaData": "1",
                                "Accept": "*/*",
                            }},
                            cache: "no-store",
                            mode: "cors",
                            credentials: "omit",
                        }});

                        if (!response || !response.body) {{
                            continue;
                        }}

                        const reader = response.body.getReader();
                        const decoder = new TextDecoder("utf-8");
                        let carry = "";
                        let totalBytes = 0;
                        const maxBytes = 512 * 1024;

                        while (totalBytes < maxBytes) {{
                            const {{ value, done }} = await reader.read();
                            if (done) break;
                            if (!value) continue;

                            totalBytes += value.byteLength || value.length || 0;
                            const chunk = decoder.decode(value, {{ stream: true }});
                            const combined = carry + chunk;
                            const parsed = parseMetadataText(combined);
                            if (parsed) {{
                                try {{ await reader.cancel(); }} catch (_err) {{}}
                                return parsed;
                            }}
                            carry = combined.slice(-2048);
                        }}

                        try {{ await reader.cancel(); }} catch (_err) {{}}
                    }} catch (_err) {{
                        // Try next candidate URL.
                    }}
                }}

                return null;
            }})();"#
        );

        document::eval(&script)
            .join::<Option<IcyNowPlaying>>()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn ping(&self) -> Result<bool, String> {
        let url = self.build_url("ping", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        match json.subsonic_response.status.as_str() {
            "ok" => Ok(true),
            _ => Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string())),
        }
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        let url = self.build_url("getArtists", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut artists = Vec::new();
        if let Some(artists_data) = json.subsonic_response.artists {
            for index in artists_data.index.unwrap_or_default() {
                for mut artist in index.artist.unwrap_or_default() {
                    artist.server_id = self.server.id.clone();
                    artists.push(artist);
                }
            }
        }

        Ok(artists)
    }

    pub async fn get_albums(
        &self,
        album_type: &str,
        size: u32,
        offset: u32,
    ) -> Result<Vec<Album>, String> {
        let url = self.build_url(
            "getAlbumList2",
            &[
                ("type", album_type),
                ("size", &size.to_string()),
                ("offset", &offset.to_string()),
            ],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut albums = json
            .subsonic_response
            .album_list2
            .and_then(|al| al.album)
            .unwrap_or_default();

        for album in &mut albums {
            album.server_id = self.server.id.clone();
        }

        Ok(albums)
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        let url = self.build_url("getAlbum", &[("id", album_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut album_with_songs = json.subsonic_response.album.ok_or("Album not found")?;
        album_with_songs.server_id = self.server.id.clone();

        let mut songs = album_with_songs.song.take().unwrap_or_default();
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
        }

        let album = album_with_songs.album;
        Ok((album, songs))
    }

    pub async fn get_song(&self, song_id: &str) -> Result<Song, String> {
        let song_id = song_id.trim();
        if song_id.is_empty() {
            return Err("Song not found".to_string());
        }

        let url = self.build_url("getSong", &[("id", song_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut song = json.subsonic_response.song.ok_or("Song not found")?;
        song.server_id = self.server.id.clone();
        song.server_name = self.server.name.clone();
        Ok(song)
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        let url = self.build_url("getArtist", &[("id", artist_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut artist_with_albums = json
            .subsonic_response
            .artist_detail
            .ok_or("Artist not found")?;
        artist_with_albums.server_id = self.server.id.clone();

        let mut albums = artist_with_albums.album.take().unwrap_or_default();
        for album in &mut albums {
            album.server_id = self.server.id.clone();
        }

        let artist = Artist {
            id: artist_with_albums.id,
            name: artist_with_albums.name,
            album_count: artist_with_albums.album_count.unwrap_or(0),
            cover_art: artist_with_albums.cover_art,
            starred: artist_with_albums.starred,
            server_id: self.server.id.clone(),
        };
        Ok((artist, albums))
    }

    pub async fn get_random_songs(&self, size: u32) -> Result<Vec<Song>, String> {
        let url = self.build_url("getRandomSongs", &[("size", &size.to_string())]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(self.normalize_song_list(json.subsonic_response.random_songs))
    }

    fn normalize_song_list(&self, list: Option<SongList>) -> Vec<Song> {
        let mut songs = list.and_then(|l| l.song).unwrap_or_default();
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
        }
        songs
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub async fn get_similar_songs(&self, id: &str, count: u32) -> Result<Vec<Song>, String> {
        let url = self.build_url(
            "getSimilarSongs",
            &[("id", id), ("count", &count.to_string())],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(self.normalize_song_list(json.subsonic_response.similar_songs))
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub async fn get_similar_songs2(&self, id: &str, count: u32) -> Result<Vec<Song>, String> {
        let url = self.build_url(
            "getSimilarSongs2",
            &[("id", id), ("count", &count.to_string())],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(self.normalize_song_list(json.subsonic_response.similar_songs2))
    }

    pub async fn get_top_songs(&self, artist: &str, count: u32) -> Result<Vec<Song>, String> {
        let url = self.build_url(
            "getTopSongs",
            &[("artist", artist), ("count", &count.to_string())],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(self.normalize_song_list(json.subsonic_response.top_songs))
    }

    pub async fn get_scan_status(&self) -> Result<ScanStatus, String> {
        let url = self.build_url("getScanStatus", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;
        Self::extract_scan_status(json)
    }

    pub async fn start_scan(&self) -> Result<ScanStatus, String> {
        let url = self.build_url("startScan", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;
        Self::extract_scan_status(json)
    }

    fn extract_scan_status(json: SubsonicResponse) -> Result<ScanStatus, String> {
        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        if let Some(payload) = json.subsonic_response.scan_status {
            Ok(payload.into_status())
        } else {
            Err("No scan status returned".to_string())
        }
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        let url = self.build_url("getStarred2", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let starred = json.subsonic_response.starred2.unwrap_or_default();

        let mut artists = starred.artist.unwrap_or_default();
        let mut albums = starred.album.unwrap_or_default();
        let mut songs = starred.song.unwrap_or_default();

        for artist in &mut artists {
            artist.server_id = self.server.id.clone();
        }
        for album in &mut albums {
            album.server_id = self.server.id.clone();
        }
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
        }

        Ok((artists, albums, songs))
    }

    pub async fn get_bookmarks(&self) -> Result<Vec<Bookmark>, String> {
        let url = self.build_url("getBookmarks", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut bookmarks = json
            .subsonic_response
            .bookmarks
            .and_then(|b| b.bookmark)
            .unwrap_or_default();

        for bookmark in &mut bookmarks {
            if bookmark.id.is_empty() {
                bookmark.id = bookmark.entry.id.clone();
            }
            bookmark.server_id = self.server.id.clone();
            if bookmark.entry.server_id.is_empty() {
                bookmark.entry.server_id = self.server.id.clone();
            }
            if bookmark.entry.server_name.is_empty() {
                bookmark.entry.server_name = self.server.name.clone();
            }
        }

        Ok(bookmarks)
    }

    pub async fn star(&self, id: &str, item_type: &str) -> Result<(), String> {
        let param = match item_type {
            "artist" => "artistId",
            "album" => "albumId",
            _ => "id",
        };
        let url = self.build_url("star", &[(param, id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn create_bookmark(
        &self,
        song_id: &str,
        position_ms: u64,
        comment: Option<&str>,
    ) -> Result<(), String> {
        self.create_bookmark_with_limit(song_id, position_ms, comment, None)
            .await
    }

    pub async fn create_bookmark_with_limit(
        &self,
        song_id: &str,
        position_ms: u64,
        comment: Option<&str>,
        max_bookmarks: Option<usize>,
    ) -> Result<(), String> {
        let position_string = position_ms.to_string();
        let mut params: Vec<(&str, &str)> =
            vec![("id", song_id), ("position", position_string.as_str())];
        let comment_string;
        if let Some(text) = comment.filter(|c| !c.trim().is_empty()) {
            comment_string = text.to_string();
            params.push(("comment", comment_string.as_str()));
        }

        let url = self.build_url("createBookmark", &params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        if let Some(limit) = max_bookmarks.filter(|value| *value > 0) {
            self.prune_oldest_bookmarks(limit).await;
        }

        Ok(())
    }

    async fn prune_oldest_bookmarks(&self, max_bookmarks: usize) {
        let Ok(mut bookmarks) = self.get_bookmarks().await else {
            return;
        };

        if bookmarks.len() <= max_bookmarks {
            return;
        }

        bookmarks.sort_by(|left, right| {
            bookmark_sort_timestamp(left)
                .cmp(&bookmark_sort_timestamp(right))
                .then_with(|| left.id.cmp(&right.id))
        });

        let overflow = bookmarks.len().saturating_sub(max_bookmarks);
        for bookmark in bookmarks.into_iter().take(overflow) {
            let _ = self.delete_bookmark(&bookmark.entry.id).await;
        }
    }

    pub async fn delete_bookmark(&self, song_id: &str) -> Result<(), String> {
        let url = self.build_url("deleteBookmark", &[("id", song_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn unstar(&self, id: &str, item_type: &str) -> Result<(), String> {
        let param = match item_type {
            "artist" => "artistId",
            "album" => "albumId",
            _ => "id",
        };
        let url = self.build_url("unstar", &[(param, id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn set_rating(&self, id: &str, rating: u32) -> Result<(), String> {
        let url = self.build_url("setRating", &[("id", id), ("rating", &rating.to_string())]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, String> {
        let url = self.build_url("getPlaylists", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut playlists = json
            .subsonic_response
            .playlists
            .and_then(|p| p.playlist)
            .unwrap_or_default();

        for playlist in &mut playlists {
            playlist.server_id = self.server.id.clone();
        }

        Ok(playlists)
    }

    pub async fn get_playlist(&self, playlist_id: &str) -> Result<(Playlist, Vec<Song>), String> {
        let url = self.build_url("getPlaylist", &[("id", playlist_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut playlist_with_entries = json
            .subsonic_response
            .playlist
            .ok_or("Playlist not found")?;
        playlist_with_entries.server_id = self.server.id.clone();

        let mut songs = playlist_with_entries.entry.take().unwrap_or_default();
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
        }

        let playlist = playlist_with_entries.playlist;
        Ok((playlist, songs))
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        comment: Option<&str>,
        song_ids: &[String],
    ) -> Result<Option<String>, String> {
        let mut params = vec![("name".to_string(), name.to_string())];
        if let Some(comment) = comment {
            let trimmed = comment.trim();
            if !trimmed.is_empty() {
                params.push(("comment".to_string(), trimmed.to_string()));
            }
        }
        for song_id in song_ids {
            params.push(("songId".to_string(), song_id.clone()));
        }

        let url = self.build_url_owned("createPlaylist", params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let playlist_id = json.subsonic_response.playlist.map(|p| p.id.clone());
        Ok(playlist_id)
    }

    pub async fn add_songs_to_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[String],
    ) -> Result<(), String> {
        if song_ids.is_empty() {
            return Ok(());
        }

        let mut params = vec![("playlistId".to_string(), playlist_id.to_string())];
        for song_id in song_ids {
            params.push(("songIdToAdd".to_string(), song_id.clone()));
        }

        let url = self.build_url_owned("updatePlaylist", params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn add_album_to_playlist(
        &self,
        playlist_id: &str,
        album_id: &str,
    ) -> Result<(), String> {
        let (_, songs) = self.get_album(album_id).await?;
        let song_ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        self.add_songs_to_playlist(playlist_id, &song_ids).await
    }

    #[allow(dead_code)]
    pub async fn add_playlist_to_playlist(
        &self,
        source_playlist_id: &str,
        target_playlist_id: &str,
    ) -> Result<(), String> {
        let (_, songs) = self.get_playlist(source_playlist_id).await?;
        let song_ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        self.add_songs_to_playlist(target_playlist_id, &song_ids)
            .await
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<(), String> {
        let url = self.build_url("deletePlaylist", &[("id", playlist_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn remove_songs_from_playlist(
        &self,
        playlist_id: &str,
        song_indices: &[usize],
    ) -> Result<(), String> {
        if song_indices.is_empty() {
            return Ok(());
        }

        let mut params = vec![("playlistId".to_string(), playlist_id.to_string())];
        // Sort indices in descending order to remove from end to beginning
        // This prevents index shifting issues
        let mut sorted_indices = song_indices.to_vec();
        sorted_indices.sort_by(|a, b| b.cmp(a));

        for &index in &sorted_indices {
            params.push(("songIndexToRemove".to_string(), index.to_string()));
        }

        let url = self.build_url_owned("updatePlaylist", params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn reorder_playlist(
        &self,
        playlist_id: &str,
        ordered_song_ids: &[String],
        existing_song_count: usize,
    ) -> Result<(), String> {
        if ordered_song_ids.is_empty() && existing_song_count == 0 {
            return Ok(());
        }

        let mut params = vec![("playlistId".to_string(), playlist_id.to_string())];

        for index in (0..existing_song_count).rev() {
            params.push(("songIndexToRemove".to_string(), index.to_string()));
        }

        for song_id in ordered_song_ids {
            params.push(("songIdToAdd".to_string(), song_id.clone()));
        }

        let url = self.build_url_owned("updatePlaylist", params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub async fn create_similar_playlist(
        &self,
        seed_song_id: &str,
        name: Option<&str>,
        count: u32,
    ) -> Result<Option<String>, String> {
        let songs = self.get_similar_songs2(seed_song_id, count).await?;
        let mut song_ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        if song_ids.is_empty() {
            song_ids.push(seed_song_id.to_string());
        }
        let playlist_name = name
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| "Similar Mix".to_string());
        self.create_playlist(
            &playlist_name,
            Some("Auto-generated from similar songs"),
            &song_ids,
        )
        .await
    }

    pub async fn get_internet_radio_stations(&self) -> Result<Vec<RadioStation>, String> {
        let url = self.build_url("getInternetRadioStations", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let mut stations = json
            .subsonic_response
            .internet_radio_stations
            .and_then(|irs| irs.internet_radio_station)
            .unwrap_or_default();

        for station in &mut stations {
            station.server_id = self.server.id.clone();
        }

        Ok(stations)
    }

    pub async fn create_internet_radio_station(
        &self,
        name: &str,
        stream_url: &str,
        home_page_url: Option<&str>,
    ) -> Result<(), String> {
        let mut params = vec![("name", name), ("streamUrl", stream_url)];
        if let Some(url) = home_page_url.filter(|value| !value.trim().is_empty()) {
            params.push(("homePageUrl", url));
        }
        let url = self.build_url("createInternetRadioStation", &params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn update_internet_radio_station(
        &self,
        station_id: &str,
        name: &str,
        stream_url: &str,
        home_page_url: Option<&str>,
    ) -> Result<(), String> {
        let mut params = vec![
            ("id", station_id),
            ("name", name),
            ("streamUrl", stream_url),
        ];
        if let Some(url) = home_page_url.filter(|value| !value.trim().is_empty()) {
            params.push(("homePageUrl", url));
        }
        let url = self.build_url("updateInternetRadioStation", &params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn delete_internet_radio_station(&self, station_id: &str) -> Result<(), String> {
        let url = self.build_url("deleteInternetRadioStation", &[("id", station_id)]);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }

    pub async fn search(
        &self,
        query: &str,
        artist_count: u32,
        album_count: u32,
        song_count: u32,
    ) -> Result<SearchResult, String> {
        let url = self.build_url(
            "search3",
            &[
                ("query", query),
                ("artistCount", &artist_count.to_string()),
                ("albumCount", &album_count.to_string()),
                ("songCount", &song_count.to_string()),
            ],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        let search = json.subsonic_response.search_result3.unwrap_or_default();

        let mut artists = search.artist.unwrap_or_default();
        let mut albums = search.album.unwrap_or_default();
        let mut songs = search.song.unwrap_or_default();

        for artist in &mut artists {
            artist.server_id = self.server.id.clone();
        }
        for album in &mut albums {
            album.server_id = self.server.id.clone();
        }
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
        }

        Ok(SearchResult {
            artists,
            albums,
            songs,
        })
    }

    /// Report playback to Navidrome/Subsonic. If submission is false, it updates "Now Playing";
    /// when true, it scrobbles the play as finished.
    #[allow(dead_code)]
    pub async fn scrobble(&self, id: &str, submission: bool) -> Result<(), String> {
        let millis = Utc::now().timestamp_millis().to_string();
        let url = self.build_url(
            "scrobble",
            &[
                ("id", id),
                ("time", millis.as_str()),
                ("submission", if submission { "true" } else { "false" }),
            ],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;

        if json.subsonic_response.status != "ok" {
            return Err(json
                .subsonic_response
                .error
                .map(|e| e.message)
                .unwrap_or("Unknown error".to_string()));
        }

        Ok(())
    }
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

// Subsonic API Response structures
#[derive(Debug, Deserialize)]
pub struct SubsonicResponse {
    #[serde(alias = "subsonic-response")]
    pub subsonic_response: SubsonicResponseInner,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicResponseInner {
    pub status: String,
    pub error: Option<SubsonicError>,
    pub artists: Option<ArtistsContainer>,
    #[serde(alias = "albumList2")]
    pub album_list2: Option<AlbumList2>,
    pub album: Option<AlbumWithSongs>,
    pub song: Option<Song>,
    #[serde(alias = "artist")]
    pub artist_detail: Option<ArtistWithAlbums>,
    #[serde(alias = "randomSongs")]
    pub random_songs: Option<SongList>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    #[serde(alias = "similarSongs")]
    pub similar_songs: Option<SongList>,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    #[serde(alias = "similarSongs2")]
    pub similar_songs2: Option<SongList>,
    #[serde(alias = "topSongs")]
    pub top_songs: Option<SongList>,
    #[serde(alias = "starred2")]
    pub starred2: Option<Starred2>,
    pub playlists: Option<PlaylistsContainer>,
    pub playlist: Option<PlaylistWithEntries>,
    #[serde(alias = "internetRadioStations")]
    pub internet_radio_stations: Option<InternetRadioStations>,
    #[serde(alias = "searchResult3")]
    pub search_result3: Option<SearchResult3>,
    #[serde(alias = "scanStatus")]
    pub scan_status: Option<ScanStatusPayload>,
    pub bookmarks: Option<BookmarksContainer>,
}

#[derive(Debug, Deserialize)]
pub struct SubsonicError {
    #[allow(dead_code)]
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ArtistsContainer {
    pub index: Option<Vec<ArtistIndex>>,
}

#[derive(Debug, Deserialize)]
pub struct ArtistIndex {
    #[allow(dead_code)]
    pub name: String,
    pub artist: Option<Vec<Artist>>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumList2 {
    pub album: Option<Vec<Album>>,
}

#[derive(Debug, Deserialize)]
pub struct AlbumWithSongs {
    #[serde(flatten)]
    pub album: Album,
    pub song: Option<Vec<Song>>,
}

impl std::ops::Deref for AlbumWithSongs {
    type Target = Album;
    fn deref(&self) -> &Self::Target {
        &self.album
    }
}

impl std::ops::DerefMut for AlbumWithSongs {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.album
    }
}

#[derive(Debug, Deserialize)]
pub struct ArtistWithAlbums {
    pub id: String,
    pub name: String,
    #[serde(alias = "albumCount")]
    pub album_count: Option<u32>,
    #[serde(alias = "coverArt")]
    pub cover_art: Option<String>,
    pub starred: Option<String>,
    #[serde(default)]
    pub server_id: String,
    pub album: Option<Vec<Album>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SongList {
    pub song: Option<Vec<Song>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Starred2 {
    pub artist: Option<Vec<Artist>>,
    pub album: Option<Vec<Album>>,
    pub song: Option<Vec<Song>>,
}

#[derive(Debug, Deserialize)]
pub struct ScanStatusPayload {
    #[serde(rename = "status")]
    pub status: Option<String>,
    #[serde(rename = "currentTask")]
    pub current_task: Option<String>,
    #[serde(rename = "secondsRemaining")]
    pub seconds_remaining: Option<u64>,
    #[serde(rename = "secondsElapsed")]
    pub seconds_elapsed: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ScanStatus {
    pub status: String,
    pub current_task: Option<String>,
    pub seconds_remaining: Option<u64>,
    pub seconds_elapsed: Option<u64>,
}

impl ScanStatusPayload {
    fn into_status(self) -> ScanStatus {
        ScanStatus {
            status: self.status.unwrap_or_else(|| "unknown".to_string()),
            current_task: self.current_task,
            seconds_remaining: self.seconds_remaining,
            seconds_elapsed: self.seconds_elapsed,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PlaylistsContainer {
    pub playlist: Option<Vec<Playlist>>,
}

#[derive(Debug, Deserialize)]
pub struct PlaylistWithEntries {
    #[serde(flatten)]
    pub playlist: Playlist,
    pub entry: Option<Vec<Song>>,
}

impl std::ops::Deref for PlaylistWithEntries {
    type Target = Playlist;
    fn deref(&self) -> &Self::Target {
        &self.playlist
    }
}

impl std::ops::DerefMut for PlaylistWithEntries {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.playlist
    }
}

#[derive(Debug, Deserialize)]
pub struct InternetRadioStations {
    #[serde(alias = "internetRadioStation")]
    pub internet_radio_station: Option<Vec<RadioStation>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SearchResult3 {
    pub artist: Option<Vec<Artist>>,
    pub album: Option<Vec<Album>>,
    pub song: Option<Vec<Song>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct BookmarksContainer {
    pub bookmark: Option<Vec<Bookmark>>,
}
