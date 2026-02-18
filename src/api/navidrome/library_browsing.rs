// Read-oriented browsing APIs for artists, albums, songs, scan status, and favorites.
impl NavidromeClient {
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
        let cache_key = format!("api:getArtists:v1:{}", self.server.id);
        if let Some(cached) = cache_get_json::<Vec<Artist>>(&cache_key) {
            return Ok(cached);
        }

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

        let _ = cache_put_json(cache_key, &artists, Some(24));
        Ok(artists)
    }

    pub async fn get_albums(
        &self,
        album_type: &str,
        size: u32,
        offset: u32,
    ) -> Result<Vec<Album>, String> {
        let cache_key = format!(
            "api:getAlbumList2:v1:{}:{}:{}:{}",
            self.server.id, album_type, size, offset
        );
        if let Some(cached) = cache_get_json::<Vec<Album>>(&cache_key) {
            return Ok(cached);
        }

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
            normalize_album_cover_art(album);
        }

        let _ = cache_put_json(cache_key, &albums, Some(6));
        Ok(albums)
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        let cache_key = format!("api:getAlbum:v1:{}:{}", self.server.id, album_id);
        if let Some(cached) = cache_get_json::<(Album, Vec<Song>)>(&cache_key) {
            return Ok(cached);
        }

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
        normalize_album_cover_art(&mut album_with_songs.album);

        let mut songs = album_with_songs.song.take().unwrap_or_default();
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
            normalize_song_cover_art(song);
        }

        let album = album_with_songs.album;
        let payload = (album, songs);
        let _ = cache_put_json(cache_key, &payload, Some(12));
        Ok(payload)
    }

    pub async fn get_song(&self, song_id: &str) -> Result<Song, String> {
        let song_id = song_id.trim();
        if song_id.is_empty() {
            return Err("Song not found".to_string());
        }

        let cache_key = format!("api:getSong:v1:{}:{}", self.server.id, song_id);
        if let Some(cached) = cache_get_json::<Song>(&cache_key) {
            return Ok(cached);
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
        normalize_song_cover_art(&mut song);
        let _ = cache_put_json(cache_key, &song, Some(24));
        Ok(song)
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        let cache_key = format!("api:getArtist:v1:{}:{}", self.server.id, artist_id);
        if let Some(cached) = cache_get_json::<(Artist, Vec<Album>)>(&cache_key) {
            return Ok(cached);
        }

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
            normalize_album_cover_art(album);
        }

        let mut artist = Artist {
            id: artist_with_albums.id,
            name: artist_with_albums.name,
            album_count: artist_with_albums.album_count.unwrap_or(0),
            cover_art: artist_with_albums.cover_art,
            starred: artist_with_albums.starred,
            server_id: self.server.id.clone(),
        };
        normalize_artist_cover_art(&mut artist);
        let payload = (artist, albums);
        let _ = cache_put_json(cache_key, &payload, Some(24));
        Ok(payload)
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
            normalize_song_cover_art(song);
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
        let cache_key = format!("api:getStarred2:v1:{}", self.server.id);
        if let Some(cached) = cache_get_json::<(Vec<Artist>, Vec<Album>, Vec<Song>)>(&cache_key) {
            return Ok(cached);
        }

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
            normalize_artist_cover_art(artist);
        }
        for album in &mut albums {
            album.server_id = self.server.id.clone();
            normalize_album_cover_art(album);
        }
        for song in &mut songs {
            song.server_id = self.server.id.clone();
            song.server_name = self.server.name.clone();
            normalize_song_cover_art(song);
        }

        let payload = (artists, albums, songs);
        let _ = cache_put_json(cache_key, &payload, Some(12));
        Ok(payload)
    }
}
