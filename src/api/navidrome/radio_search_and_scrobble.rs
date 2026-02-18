// Internet radio management, search, and scrobble reporting.
impl NavidromeClient {
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
        let cache_key = format!(
            "api:search3:v1:{}:{}:{}:{}:{}",
            self.server.id,
            query.trim().to_lowercase(),
            artist_count,
            album_count,
            song_count
        );
        if let Some(cached) = cache_get_json::<SearchResult>(&cache_key) {
            return Ok(cached);
        }

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

        let payload = SearchResult {
            artists,
            albums,
            songs,
        };
        let _ = cache_put_json(cache_key, &payload, Some(4));
        Ok(payload)
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
