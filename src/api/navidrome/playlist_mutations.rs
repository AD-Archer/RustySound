// Playlist write/update operations.
//
// Creating a playlist and adding, removing and reordering its songs go
// through the offline queue (`api::offline_queue`): while the server can't
// be reached they wait there, the cached playlist is changed to match, and
// the call succeeds. The `_now` versions are the server calls themselves,
// which replay uses.
use crate::api::offline_queue::{self, Change};

impl NavidromeClient {
    pub async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Playlist name cannot be empty.".to_string());
        }

        let url = self.build_url_owned(
            "updatePlaylist",
            vec![
                ("playlistId".to_string(), playlist_id.to_string()),
                ("name".to_string(), trimmed.to_string()),
            ],
        );
        self.subsonic_get(&url).await?;

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
        Ok(())
    }

    /// The server's id for a playlist that was created offline and has
    /// synced since; a view may still hold the offline id.
    fn playlist_id(&self, playlist_id: &str) -> String {
        offline_queue::resolve_playlist_id(&self.server.id, playlist_id)
    }

    fn playlist_cache_key(&self, playlist_id: &str) -> String {
        format!("api:getPlaylist:v1:{}:{}", self.server.id, playlist_id)
    }

    /// The cached playlist, if any (offline edits change it in place).
    fn cached_playlist(&self, playlist_id: &str) -> Option<(Playlist, Vec<Song>)> {
        cache_get_json(&self.playlist_cache_key(playlist_id))
    }

    /// Writes an offline edit into the cached playlist and the cached list,
    /// so reopening it shows the change before it syncs.
    fn cache_offline_edit(&self, playlist: Playlist, songs: Vec<Song>) {
        // Jellyfin playlists can't be edited here, so nothing is queued.
        if self.server.is_jellyfin() {
            return;
        }
        let key = format!("api:getPlaylists:v1:{}", self.server.id);
        if let Some(mut list) = cache_get_json::<Vec<Playlist>>(&key) {
            match list.iter_mut().find(|p| p.id == playlist.id) {
                Some(entry) => entry.song_count = playlist.song_count,
                None => list.insert(0, playlist.clone()),
            }
            let _ = cache_put_json(key, &list, Some(12));
        }
        let _ = cache_put_json(self.playlist_cache_key(&playlist.id), &(playlist, songs), Some(12));
    }

    pub async fn create_playlist(
        &self,
        name: &str,
        comment: Option<&str>,
        song_ids: &[String],
    ) -> Result<Option<String>, String> {
        let send = self.create_playlist_now(name, comment, song_ids);
        offline_queue::submit(&self.server, send, || {
            let temp_id = offline_queue::temp_playlist_id();
            let playlist = Playlist {
                id: temp_id.clone(),
                name: name.trim().to_string(),
                comment: comment.map(str::to_string),
                song_count: song_ids.len() as u32,
                owner: Some(self.server.username.clone()),
                server_id: self.server.id.clone(),
                ..Playlist::default()
            };
            self.cache_offline_edit(playlist, Vec::new());
            let change = Change::CreatePlaylist {
                temp_id: temp_id.clone(),
                name: name.trim().to_string(),
                comment: comment.map(str::to_string),
                song_ids: song_ids.to_vec(),
            };
            Some((change, Some(temp_id)))
        })
        .await
    }

    pub(crate) async fn create_playlist_now(
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
        let json = self.subsonic_get(&url).await?;

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
        let playlist_id = self.playlist_id(playlist_id);
        let send = self.add_songs_to_playlist_now(&playlist_id, song_ids);
        offline_queue::submit(&self.server, send, || {
            // The songs themselves aren't at hand here; the count is.
            if let Some((mut playlist, songs)) = self.cached_playlist(&playlist_id) {
                playlist.song_count += song_ids.len() as u32;
                self.cache_offline_edit(playlist, songs);
            }
            let change = Change::AddToPlaylist { playlist_id: playlist_id.clone(), song_ids: song_ids.to_vec() };
            Some((change, ()))
        })
        .await
    }

    pub(crate) async fn add_songs_to_playlist_now(
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
        self.subsonic_get(&url).await?;

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
        self.subsonic_get(&url).await?;

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
        Ok(())
    }

    /// Removes the songs at `song_indices`. Offline, the change is queued by
    /// song id (from the cached playlist), so replay can find them again in
    /// whatever the playlist has become.
    pub async fn remove_songs_from_playlist(
        &self,
        playlist_id: &str,
        song_indices: &[usize],
    ) -> Result<(), String> {
        if song_indices.is_empty() {
            return Ok(());
        }
        let playlist_id = self.playlist_id(playlist_id);
        let send = self.remove_songs_from_playlist_now(&playlist_id, song_indices);
        offline_queue::submit(&self.server, send, || {
            let (mut playlist, songs) = self.cached_playlist(&playlist_id)?;
            let song_ids: Vec<String> =
                song_indices.iter().filter_map(|&i| songs.get(i)).map(|s| s.id.clone()).collect();
            let kept: Vec<Song> = songs
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !song_indices.contains(i))
                .map(|(_, song)| song)
                .collect();
            playlist.song_count = kept.len() as u32;
            self.cache_offline_edit(playlist, kept);
            Some((Change::RemoveFromPlaylist { playlist_id: playlist_id.clone(), song_ids }, ()))
        })
        .await
    }

    pub(crate) async fn remove_songs_from_playlist_now(
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
        self.subsonic_get(&url).await?;

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
        Ok(())
    }

    /// Saves the playlist in a new order (`ordered_song_ids` is all of it).
    pub async fn reorder_playlist(
        &self,
        playlist_id: &str,
        ordered_song_ids: &[String],
        existing_song_count: usize,
    ) -> Result<(), String> {
        if ordered_song_ids.is_empty() && existing_song_count == 0 {
            return Ok(());
        }
        let playlist_id = self.playlist_id(playlist_id);
        let send = self.reorder_playlist_now(&playlist_id, ordered_song_ids, existing_song_count);
        offline_queue::submit(&self.server, send, || {
            if let Some((mut playlist, mut songs)) = self.cached_playlist(&playlist_id) {
                let mut ordered = Vec::with_capacity(ordered_song_ids.len());
                for id in ordered_song_ids {
                    if let Some(i) = songs.iter().position(|s| &s.id == id) {
                        ordered.push(songs.remove(i));
                    }
                }
                playlist.song_count = ordered.len() as u32;
                self.cache_offline_edit(playlist, ordered);
            }
            let change = Change::ReorderPlaylist { playlist_id: playlist_id.clone(), song_ids: ordered_song_ids.to_vec() };
            Some((change, ()))
        })
        .await
    }

    pub(crate) async fn reorder_playlist_now(
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
        self.subsonic_get(&url).await?;

        let _ = cache_remove_prefix(&format!(
            "api:getPlaylist:v1:{}:{}",
            self.server.id, playlist_id
        ));
        self.invalidate_playlist_cache();
        Ok(())
    }
}
