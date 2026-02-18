// Bookmark/favorite/rating APIs plus playlist read endpoints.
impl NavidromeClient {
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

        if item_type == "playlist" {
            self.invalidate_playlist_cache();
        } else {
            self.invalidate_favorites_cache();
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

        if item_type == "playlist" {
            self.invalidate_playlist_cache();
        } else {
            self.invalidate_favorites_cache();
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
        let cache_key = format!("api:getPlaylists:v1:{}", self.server.id);
        if let Some(cached) = cache_get_json::<Vec<Playlist>>(&cache_key) {
            return Ok(cached);
        }

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
            normalize_playlist_cover_art(playlist);
        }

        let _ = cache_put_json(cache_key, &playlists, Some(12));
        Ok(playlists)
    }

    pub async fn get_playlist(&self, playlist_id: &str) -> Result<(Playlist, Vec<Song>), String> {
        let cache_key = format!("api:getPlaylist:v1:{}:{}", self.server.id, playlist_id);
        if let Some(cached) = cache_get_json::<(Playlist, Vec<Song>)>(&cache_key) {
            return Ok(cached);
        }

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
        normalize_playlist_cover_art(&mut playlist_with_entries.playlist);

        let mut songs = playlist_with_entries.entry.take().unwrap_or_default();
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
            normalize_song_cover_art(song);
        }

        let playlist = playlist_with_entries.playlist;
        let payload = (playlist, songs);
        let _ = cache_put_json(cache_key, &payload, Some(12));
        Ok(payload)
    }
}
