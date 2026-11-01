use serde::{Deserialize, Serialize};

/// Which API a server speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    /// Subsonic / OpenSubsonic (Navidrome and friends). Servers saved before
    /// Jellyfin support load as this.
    #[default]
    Subsonic,
    Jellyfin,
}

// The native database stores the kind as text; the web build saves JSON.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl ServerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Subsonic => "subsonic",
            Self::Jellyfin => "jellyfin",
        }
    }

    /// The stored text back to a kind; anything unknown is Subsonic.
    pub fn from_stored(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("jellyfin") {
            Self::Jellyfin
        } else {
            Self::Subsonic
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    /// Subsonic only. Jellyfin servers keep an access token instead and
    /// leave this empty.
    pub password: String,
    pub active: bool,
    #[serde(default)]
    pub kind: ServerKind,
    /// Jellyfin: the signed-in user's id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    /// Jellyfin: the access token from `AuthenticateByName`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
}

impl ServerConfig {
    pub fn new(name: String, url: String, username: String, password: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            url: url.trim_end_matches('/').to_string(),
            username,
            password,
            active: true,
            ..Default::default()
        }
    }

    /// A Jellyfin server, signed in as `user_id` with `access_token`.
    #[cfg(test)]
    pub fn jellyfin(name: String, url: String, username: String, user_id: String, access_token: String) -> Self {
        Self {
            kind: ServerKind::Jellyfin,
            user_id,
            access_token,
            ..Self::new(name, url, username, String::new())
        }
    }

    pub fn is_jellyfin(&self) -> bool {
        self.kind == ServerKind::Jellyfin
    }

    /// The configured server with this id.
    pub fn find<'a>(servers: &'a [ServerConfig], id: &str) -> Option<&'a ServerConfig> {
        servers.iter().find(|server| server.id == id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    #[serde(default, alias = "albumCount")]
    pub album_count: u32,
    #[serde(default, alias = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub starred: Option<String>,
    #[serde(default)]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default, alias = "artistId")]
    pub artist_id: Option<String>,
    #[serde(default, alias = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default, alias = "songCount")]
    pub song_count: u32,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub starred: Option<String>,
    #[serde(default, alias = "userRating", alias = "rating")]
    pub user_rating: Option<u32>,
    /// Total plays of the album (Subsonic `playCount`, API 1.14+). `None` when the server omits it.
    #[serde(default, alias = "playCount")]
    pub play_count: Option<u32>,
    /// When the album was last played (OpenSubsonic `played`, ISO 8601). Navidrome sends it.
    #[serde(default)]
    pub played: Option<String>,
    /// When the album was added to the library (Subsonic `created`, ISO 8601).
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Song {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default, alias = "albumId")]
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist: Option<String>,
    #[serde(default, alias = "artistId")]
    pub artist_id: Option<String>,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub track: Option<u32>,
    #[serde(default, alias = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default, alias = "contentType")]
    pub content_type: Option<String>,
    #[serde(default, alias = "streamUrl")]
    pub stream_url: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
    #[serde(default)]
    pub bitrate: Option<u32>,
    #[serde(default)]
    pub starred: Option<String>,
    #[serde(default, alias = "userRating", alias = "rating")]
    pub user_rating: Option<u32>,
    #[serde(default, alias = "playCount")]
    pub play_count: Option<u32>,
    #[serde(default, alias = "lastPlayed")]
    pub played: Option<String>,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
    /// OpenSubsonic `replayGain`, for volume levelling. `None` when the
    /// server sends none (plain Subsonic, untagged files).
    #[serde(default, alias = "replayGain", skip_serializing_if = "Option::is_none")]
    pub replay_gain: Option<ReplayGain>,
    #[serde(skip)]
    pub queue_meta: Option<QueueSongMeta>,
}

/// OpenSubsonic `replayGain`: gains in dB, peaks as linear sample peaks
/// (1.0 = full scale). Every field is optional on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReplayGain {
    #[serde(default, alias = "trackGain")]
    pub track_gain: Option<f64>,
    #[serde(default, alias = "albumGain")]
    pub album_gain: Option<f64>,
    #[serde(default, alias = "trackPeak")]
    pub track_peak: Option<f64>,
    #[serde(default, alias = "albumPeak")]
    pub album_peak: Option<f64>,
    /// Gain the decoder already applies (Opus output gain). Not added again.
    #[serde(default, alias = "baseGain")]
    pub base_gain: Option<f64>,
    /// The server's gain for files without track or album gain.
    #[serde(default, alias = "fallbackGain")]
    pub fallback_gain: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueSourceKind {
    Album,
    Playlist,
    Favorites,
    RandomMix,
    Artist,
    /// Added by Autoplay when the queue was about to run out.
    Autoplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSongMeta {
    pub group_id: String,
    pub source_kind: QueueSourceKind,
    pub source_id: String,
    pub source_position: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Bookmark {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub position: u64,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub changed: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub entry: Song,
    #[serde(default)]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default, alias = "songCount")]
    pub song_count: u32,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default, alias = "coverArt")]
    pub cover_art: Option<String>,
    #[serde(default)]
    pub starred: Option<String>,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub changed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RadioStation {
    pub id: String,
    pub name: String,
    #[serde(alias = "streamUrl")]
    pub stream_url: String,
    #[serde(default, alias = "homePageUrl")]
    pub home_page_url: Option<String>,
    #[serde(default)]
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(default)]
    pub artists: Vec<Artist>,
    #[serde(default)]
    pub albums: Vec<Album>,
    #[serde(default)]
    pub songs: Vec<Song>,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            artists: Vec::new(),
            albums: Vec::new(),
            songs: Vec::new(),
        }
    }
}

/// `server_id::id`, the identity of an item across sources. Saved data
/// (offline downloads, the smart cache, playlist sync) stores this form, so
/// it must not change. Local items use `server_id = "local"` and Jellyfin
/// items their server's id, like any other source.
pub fn item_key(server_id: &str, id: &str) -> String {
    format!("{server_id}::{id}")
}

macro_rules! impl_item_identity {
    ($($kind:ty),+) => {$(
        // Not every kind has a caller yet: the views switch to these in
        // refactor chunks B-D.
        #[allow(dead_code)]
        impl $kind {
            /// [`item_key`]: `server_id::id`, for keys and dedup.
            pub fn key(&self) -> String {
                item_key(&self.server_id, &self.id)
            }

            /// Same item on the same source (cheaper than comparing keys).
            pub fn same_as(&self, other: &Self) -> bool {
                self.id == other.id && self.server_id == other.server_id
            }
        }
    )+};
}

impl_item_identity!(Song, Album, Artist, Playlist);

// Only downloads (native) call this so far; the views switch to it in
// refactor chunks B-D.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl Song {
    /// An internet-radio entry: no seeking, no queue actions, no scrobbles.
    /// Stations are queued as songs with the "Radio" source name and their
    /// stream URL; library songs never carry a stream URL.
    pub fn is_radio(&self) -> bool {
        self.server_name == "Radio"
            || self
                .stream_url
                .as_deref()
                .is_some_and(|url| !url.trim().is_empty())
    }
}

#[deprecated(note = "use crate::format::clock")]
pub fn format_duration(seconds: u32) -> String {
    crate::format::clock(seconds as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(server_id: &str, id: &str) -> Song {
        Song {
            id: id.to_string(),
            server_id: server_id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn keys_keep_the_saved_server_id_and_id_form() {
        assert_eq!(song("srv-1", "mf-9").key(), "srv-1::mf-9");
        assert_eq!(song("local", "a1b2").key(), "local::a1b2");
        // Jellyfin ids are GUIDs and keep their dashes.
        assert_eq!(
            song("jf", "0b2c1d4e-5f60-7182-93a4-b5c6d7e8f901").key(),
            "jf::0b2c1d4e-5f60-7182-93a4-b5c6d7e8f901"
        );
        let album = Album { id: "al-1".into(), server_id: "s".into(), ..Default::default() };
        let artist = Artist { id: "ar-1".into(), server_id: "s".into(), ..Default::default() };
        let playlist = Playlist { id: "pl-1".into(), server_id: "s".into(), ..Default::default() };
        assert_eq!(album.key(), "s::al-1");
        assert_eq!(artist.key(), "s::ar-1");
        assert_eq!(playlist.key(), "s::pl-1");
        assert_eq!(item_key("s", "x"), song("s", "x").key());
    }

    #[test]
    fn same_as_needs_both_the_id_and_the_source() {
        assert!(song("a", "1").same_as(&song("a", "1")));
        assert!(!song("a", "1").same_as(&song("b", "1")));
        assert!(!song("a", "1").same_as(&song("a", "2")));
        let mut retitled = song("a", "1");
        retitled.title = "Other".into();
        assert!(song("a", "1").same_as(&retitled));
    }

    #[test]
    fn radio_is_the_radio_source_or_a_stream_url() {
        assert!(!song("s", "1").is_radio());
        let by_name = Song { server_name: "Radio".into(), ..song("s", "1") };
        assert!(by_name.is_radio());
        let by_url = Song { stream_url: Some("https://radio.example/live".into()), ..song("s", "1") };
        assert!(by_url.is_radio());
        let blank_url = Song { stream_url: Some("  ".into()), ..song("s", "1") };
        assert!(!blank_url.is_radio());
    }
}
