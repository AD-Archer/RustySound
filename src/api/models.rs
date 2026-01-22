use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub active: bool,
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
        }
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
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default)]
    pub genre: Option<String>,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
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
    pub server_id: String,
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

pub fn format_duration(seconds: u32) -> String {
    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{}:{:02}", mins, secs)
}
