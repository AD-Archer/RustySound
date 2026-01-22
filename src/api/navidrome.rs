use crate::api::models::*;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);
static AUTH_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

const CLIENT_NAME: &str = "RustySound";
const API_VERSION: &str = "1.16.1";

pub struct NavidromeClient {
    pub server: ServerConfig,
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

    pub async fn create_bookmark(
        &self,
        song_id: &str,
        position_ms: u64,
        comment: Option<&str>,
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

        Ok(())
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

    pub async fn add_album_to_playlist(
        &self,
        playlist_id: &str,
        album_id: &str,
    ) -> Result<(), String> {
        let (_, songs) = self.get_album(album_id).await?;
        let song_ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
        self.add_songs_to_playlist(playlist_id, &song_ids).await
    }

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
            return Err(
                json.subsonic_response
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string()),
            );
        }

        Ok(())
    }

    pub async fn remove_songs_from_playlist(
        &self,
        playlist_id: &str,
        song_ids: &[String],
    ) -> Result<(), String> {
        if song_ids.is_empty() {
            return Ok(());
        }

        let mut params = vec![("playlistId".to_string(), playlist_id.to_string())];
        for song_id in song_ids {
            params.push(("songIdToRemove".to_string(), song_id.clone()));
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

    pub async fn create_similar_playlist(
        &self,
        seed_song_id: &str,
        name: Option<&str>,
        count: u32,
    ) -> Result<Option<String>, String> {
        let songs = self.get_similar_songs(seed_song_id, count).await?;
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
