// Offset-paged list endpoints for the Library views (agent E, phase 2).
//
// These never refetch from 0 to show more: each call returns one page at
// `offset`, and the views append it. They are not cached on purpose; the
// views keep loaded pages in memory for as long as they are mounted.

impl NavidromeClient {
    async fn subsonic_get(&self, url: &str) -> Result<SubsonicResponse, String> {
        let response = HTTP_CLIENT
            .get(url)
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
        Ok(json)
    }

    /// One page of `getAlbumList2`. `genre` switches to `type=byGenre`.
    pub async fn get_album_page(
        &self,
        list_type: &str,
        genre: Option<&str>,
        size: u32,
        offset: u32,
    ) -> Result<Vec<Album>, String> {
        let size_text = size.to_string();
        let offset_text = offset.to_string();
        let mut params: Vec<(&str, &str)> = vec![("size", &size_text), ("offset", &offset_text)];
        match genre.filter(|g| !g.trim().is_empty()) {
            Some(genre) => {
                params.push(("type", "byGenre"));
                params.push(("genre", genre));
            }
            None => params.push(("type", list_type)),
        }
        let url = self.build_url("getAlbumList2", &params);
        let json = self.subsonic_get(&url).await?;
        let mut albums = json
            .subsonic_response
            .album_list2
            .and_then(|list| list.album)
            .unwrap_or_default();
        for album in &mut albums {
            album.server_id = self.server.id.clone();
            normalize_album_cover_art(album);
        }
        Ok(albums)
    }

    /// One page of `search3` for a single kind. `kind` is "artist", "album" or "song".
    pub async fn search_page(
        &self,
        query: &str,
        kind: &str,
        size: u32,
        offset: u32,
    ) -> Result<SearchResult, String> {
        let size_text = size.to_string();
        let offset_text = offset.to_string();
        let mut params: Vec<(&str, &str)> = vec![
            ("query", query),
            ("artistCount", "0"),
            ("albumCount", "0"),
            ("songCount", "0"),
        ];
        let (count_key, offset_key) = match kind {
            "artist" => ("artistCount", "artistOffset"),
            "album" => ("albumCount", "albumOffset"),
            _ => ("songCount", "songOffset"),
        };
        params.retain(|(key, _)| *key != count_key);
        params.push((count_key, &size_text));
        params.push((offset_key, &offset_text));
        let url = self.build_url("search3", &params);
        let json = self.subsonic_get(&url).await?;
        let search = json.subsonic_response.search_result3.unwrap_or_default();
        let mut result = SearchResult {
            artists: search.artist.unwrap_or_default(),
            albums: search.album.unwrap_or_default(),
            songs: search.song.unwrap_or_default(),
        };
        for artist in &mut result.artists {
            artist.server_id = self.server.id.clone();
            normalize_artist_cover_art(artist);
        }
        for album in &mut result.albums {
            album.server_id = self.server.id.clone();
            normalize_album_cover_art(album);
        }
        for song in &mut result.songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
            normalize_song_cover_art(song);
        }
        Ok(result)
    }

    /// One page of Navidrome's native song list, sorted by any native field
    /// (`play_date`, `play_count`, `title`, `rating`, `duration`, …).
    /// Errors on servers without the native API; callers fall back to `search_page`.
    pub async fn get_native_song_page(
        &self,
        sort: &str,
        ascending: bool,
        offset: usize,
        size: usize,
    ) -> Result<Vec<Song>, String> {
        let url = self.native_base_url(&format!(
            "api/song?_start={}&_end={}&_sort={}&_order={}",
            offset,
            offset + size,
            urlencoding::encode(sort),
            if ascending { "ASC" } else { "DESC" }
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
}
