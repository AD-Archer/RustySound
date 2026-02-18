// Subsonic response model types used by Navidrome API parsing.
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
