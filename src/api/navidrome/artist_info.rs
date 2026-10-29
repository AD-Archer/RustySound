// Artist info with similar artists (getArtistInfo2), used by Discover.

/// One similar artist returned by `getArtistInfo2`.
///
/// Navidrome fills similar artists from its Last.fm/Spotify agents. With
/// `includeNotPresent=true` it also returns artists that are not in the library;
/// those carry the id `"-1"` (or an empty id), which this type exposes as `id: None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SimilarArtistInfo {
    /// Library artist id, or `None` when the artist is not on this server.
    pub id: Option<String>,
    /// Artist name as the server reports it.
    pub name: String,
    /// Cover art id for `getCoverArt` (only set when `id` is `Some`).
    pub cover_art: Option<String>,
    /// Absolute artist image URL from the server (only set when `id` is `Some`).
    pub image_url: Option<String>,
    /// Number of albums in the library (0 for off-server artists).
    pub album_count: u32,
    /// Server the response came from.
    pub server_id: String,
}

impl SimilarArtistInfo {
    /// True when the artist exists in this server's library.
    pub fn in_library(&self) -> bool {
        self.id.is_some()
    }
}

/// Parsed `getArtistInfo2` response: biography, links and similar artists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ArtistInfo {
    pub biography: Option<String>,
    pub music_brainz_id: Option<String>,
    pub last_fm_url: Option<String>,
    pub small_image_url: Option<String>,
    pub medium_image_url: Option<String>,
    pub large_image_url: Option<String>,
    /// Similar artists in the order the server ranked them.
    pub similar: Vec<SimilarArtistInfo>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArtistInfo2Payload {
    #[serde(default)]
    pub biography: Option<String>,
    #[serde(default, alias = "musicBrainzId")]
    pub music_brainz_id: Option<String>,
    #[serde(default, alias = "lastFmUrl")]
    pub last_fm_url: Option<String>,
    #[serde(default, alias = "smallImageUrl")]
    pub small_image_url: Option<String>,
    #[serde(default, alias = "mediumImageUrl")]
    pub medium_image_url: Option<String>,
    #[serde(default, alias = "largeImageUrl")]
    pub large_image_url: Option<String>,
    #[serde(default, alias = "similarArtist")]
    pub similar_artist: Option<Vec<SimilarArtistPayload>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SimilarArtistPayload {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default, alias = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default, alias = "artistImageUrl")]
    pub artist_image_url: Option<String>,
    #[serde(default, alias = "albumCount")]
    pub album_count: Option<u32>,
}

fn artist_info_non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Convert a decoded Subsonic envelope into [`ArtistInfo`]. Pure; used by tests.
pub fn artist_info_from_response(
    server_id: &str,
    json: SubsonicResponse,
) -> Result<ArtistInfo, String> {
    if json.subsonic_response.status != "ok" {
        return Err(json
            .subsonic_response
            .error
            .map(|e| e.message)
            .unwrap_or_else(|| "Unknown error".to_string()));
    }
    let payload = json.subsonic_response.artist_info2.unwrap_or_default();
    let similar = payload
        .similar_artist
        .unwrap_or_default()
        .into_iter()
        .filter(|artist| !artist.name.trim().is_empty())
        .map(|artist| {
            // Navidrome marks artists that are not in the library with id "-1".
            let id = artist_info_non_empty(artist.id).filter(|id| id != "-1");
            let in_library = id.is_some();
            let cover_art = if in_library {
                artist_info_non_empty(artist.cover_art).map(|v| normalize_cover_art_id(&v))
            } else {
                None
            };
            SimilarArtistInfo {
                id,
                name: artist.name.trim().to_string(),
                cover_art,
                // Navidrome builds image URLs from the artist id; for off-server
                // artists that URL does not resolve, so drop it.
                image_url: if in_library {
                    artist_info_non_empty(artist.artist_image_url)
                } else {
                    None
                },
                album_count: artist.album_count.unwrap_or(0),
                server_id: server_id.to_string(),
            }
        })
        .collect();

    Ok(ArtistInfo {
        biography: artist_info_non_empty(payload.biography),
        music_brainz_id: artist_info_non_empty(payload.music_brainz_id),
        last_fm_url: artist_info_non_empty(payload.last_fm_url),
        small_image_url: artist_info_non_empty(payload.small_image_url),
        medium_image_url: artist_info_non_empty(payload.medium_image_url),
        large_image_url: artist_info_non_empty(payload.large_image_url),
        similar,
    })
}

impl NavidromeClient {
    /// `getArtistInfo2`: biography and similar artists for a library artist.
    ///
    /// `include_not_present = true` asks the server to also list similar artists
    /// that are not in the library (`SimilarArtistInfo::id == None`). Cached 24 h.
    pub async fn get_artist_info2(
        &self,
        artist_id: &str,
        count: u32,
        include_not_present: bool,
    ) -> Result<ArtistInfo, String> {
        let cache_key = format!(
            "api:getArtistInfo2:v1:{}:{}:{}:{}",
            self.server.id, artist_id, count, include_not_present
        );
        if let Some(cached) = cache_get_json::<ArtistInfo>(&cache_key) {
            return Ok(cached);
        }

        let count_text = count.to_string();
        let url = self.build_url(
            "getArtistInfo2",
            &[
                ("id", artist_id),
                ("count", count_text.as_str()),
                (
                    "includeNotPresent",
                    if include_not_present { "true" } else { "false" },
                ),
            ],
        );
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;
        let info = artist_info_from_response(&self.server.id, json)?;
        let _ = cache_put_json(cache_key, &info, Some(24));
        Ok(info)
    }
}
