// Playlist write/update operations.
impl NavidromeClient {
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
        self.invalidate_playlist_cache();
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

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
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

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
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

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
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

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
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
}
