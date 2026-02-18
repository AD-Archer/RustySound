// Authentication, native song feed mapping, and stream/cover-art utilities.
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

    fn auth_params_for_binary(&self) -> String {
        self.auth_params().replace("&f=json", "")
    }

    fn build_url(&self, endpoint: &str, extra_params: &[(&str, &str)]) -> String {
        if is_offline_mode() {
            return "offline://network-blocked".to_string();
        }
        let auth = self.auth_params();
        let mut url = format!("{}/rest/{}?{}", self.server.url, endpoint, auth);

        for (key, value) in extra_params {
            url.push_str(&format!("&{}={}", key, urlencoding_simple(value)));
        }

        url
    }

    fn build_url_owned(&self, endpoint: &str, extra_params: Vec<(String, String)>) -> String {
        if is_offline_mode() {
            return "offline://network-blocked".to_string();
        }
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

    fn invalidate_favorites_cache(&self) {
        let _ = cache_remove_prefix(&format!("api:getStarred2:v1:{}", self.server.id));
        let _ = cache_remove_prefix("view:favorites:v1:");
    }

    fn invalidate_playlist_cache(&self) {
        let _ = cache_remove_prefix(&format!("api:getPlaylists:v1:{}", self.server.id));
        let _ = cache_remove_prefix(&format!("api:getPlaylist:v1:{}:", self.server.id));
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
            .or_else(|| {
                payload
                    .get("data")
                    .and_then(|value| value.as_array())
                    .cloned()
            })
            .or_else(|| {
                payload
                    .get("items")
                    .and_then(|value| value.as_array())
                    .cloned()
            })
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
            let played =
                json_pick_string(&value, &["lastPlayed", "played", "playDate", "play_date"]);
            let year = json_pick_u32(&value, &["year"]);
            let genre = json_pick_string(&value, &["genre"]);

            let mut song = Song {
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
            };
            normalize_song_cover_art(&mut song);
            songs.push(song);
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
                .header("x-nd-authorization", format!("Bearer {}", session.token))
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
        #[cfg(target_arch = "wasm32")]
        let requested_size = size.min(160);
        #[cfg(not(target_arch = "wasm32"))]
        let requested_size = size;

        let normalized_cover_art_id = normalize_cover_art_id(cover_art_id);

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(data_url) =
            cached_cover_art_data_url(&self.server.id, &normalized_cover_art_id, requested_size)
        {
            return data_url;
        }

        #[cfg(not(target_arch = "wasm32"))]
        if is_offline_mode() {
            return String::new();
        }

        let remote_url = self.build_cover_art_network_url(&normalized_cover_art_id, requested_size);

        #[cfg(not(target_arch = "wasm32"))]
        maybe_prefetch_cover_art(
            self.server.id.clone(),
            normalized_cover_art_id.clone(),
            requested_size,
            remote_url.clone(),
        );

        remote_url
    }

    fn build_cover_art_network_url(&self, cover_art_id: &str, requested_size: u32) -> String {
        let auth = self.auth_params_for_binary();
        format!(
            "{}/rest/getCoverArt?{}&id={}&size={}",
            self.server.url,
            auth,
            urlencoding_simple(cover_art_id),
            requested_size
        )
    }

    #[allow(dead_code)]
    pub fn get_stream_url(&self, song_id: &str) -> String {
        let auth = self.auth_params_for_binary();
        format!(
            "{}/rest/stream?{}&id={}",
            self.server.url,
            auth,
            urlencoding_simple(song_id)
        )
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
}
