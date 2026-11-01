//! Jellyfin's wire types, with only the fields RustySound reads (names as in
//! the 12.4 OpenAPI spec), and their mapping to the app's models. Pure; the
//! tests cover the mapping.

use crate::api::models::{Album, Artist, Playlist, ReplayGain, Song};
use serde::Deserialize;
use std::collections::HashMap;

/// .NET ticks: 10,000 per millisecond.
pub const TICKS_PER_MS: i64 = 10_000;
pub const TICKS_PER_SECOND: i64 = 1_000 * TICKS_PER_MS;

/// The `starred` value of a Jellyfin favourite. Jellyfin keeps no date for
/// it, and the app only asks whether `starred` is set.
pub const FAVORITE_MARK: &str = "favorite";

pub fn ticks_to_seconds(ticks: Option<i64>) -> u32 {
    ticks
        .filter(|ticks| *ticks > 0)
        .map(|ticks| u32::try_from(ticks / TICKS_PER_SECOND).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

pub fn ms_to_ticks(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX / TICKS_PER_MS) * TICKS_PER_MS
}

fn non_negative(value: Option<i32>) -> Option<u32> {
    value.and_then(|n| u32::try_from(n).ok())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `GET /System/Info/Public`, the unauthenticated probe.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PublicSystemInfo {
    pub server_name: Option<String>,
    pub version: Option<String>,
    pub product_name: Option<String>,
    pub id: Option<String>,
}

impl PublicSystemInfo {
    /// Jellyfin (and servers that speak its API, like Navidrome's
    /// `/jellyfin`) answer with an id and a version.
    pub fn is_jellyfin(&self) -> bool {
        let has = |v: &Option<String>| v.as_deref().is_some_and(|v| !v.trim().is_empty());
        has(&self.id) && has(&self.version)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserDto {
    pub id: String,
    pub name: Option<String>,
}

/// `POST /Users/AuthenticateByName`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct AuthenticationResult {
    pub user: Option<UserDto>,
    pub access_token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct NameGuidPair {
    pub name: Option<String>,
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserItemDataDto {
    pub is_favorite: bool,
    pub play_count: Option<i32>,
    pub last_played_date: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BaseItemDto {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "Type")]
    pub item_type: Option<String>,
    pub run_time_ticks: Option<i64>,
    pub index_number: Option<i32>,
    pub production_year: Option<i32>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub album_artist: Option<String>,
    pub album_artists: Option<Vec<NameGuidPair>>,
    pub artist_items: Option<Vec<NameGuidPair>>,
    pub artists: Option<Vec<String>>,
    pub image_tags: Option<HashMap<String, Option<String>>>,
    pub album_primary_image_tag: Option<String>,
    pub user_data: Option<UserItemDataDto>,
    pub normalization_gain: Option<f64>,
    pub album_normalization_gain: Option<f64>,
    pub genres: Option<Vec<String>>,
    pub date_created: Option<String>,
    pub child_count: Option<i32>,
    pub song_count: Option<i32>,
    pub album_count: Option<i32>,
    pub container: Option<String>,
    pub overview: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct BaseItemDtoQueryResult {
    pub items: Option<Vec<BaseItemDto>>,
}

impl BaseItemDtoQueryResult {
    pub fn into_items(self) -> Vec<BaseItemDto> {
        self.items.unwrap_or_default()
    }
}

/// `GET /Audio/{id}/Lyrics`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LyricDto {
    pub lyrics: Option<Vec<LyricLineDto>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LyricLineDto {
    pub text: Option<String>,
    /// Ticks from the start of the song; `None` on unsynced lyrics.
    pub start: Option<i64>,
    pub cues: Option<Vec<LyricLineCue>>,
}

/// One timed word: its character range in the line, and when it's sung.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LyricLineCue {
    pub position: i32,
    pub end_position: i32,
    pub start: i64,
    pub end: Option<i64>,
}

impl BaseItemDto {
    fn has_primary_image(&self) -> bool {
        self.image_tags
            .as_ref()
            .and_then(|tags| tags.get("Primary"))
            .is_some_and(|tag| tag.as_deref().is_some_and(|t| !t.is_empty()))
    }

    /// The item's own cover when it has one, else `fallback`'s.
    fn cover(&self, fallback: Option<&str>) -> Option<String> {
        if self.has_primary_image() {
            Some(self.id.clone())
        } else {
            fallback.filter(|id| !id.is_empty()).map(str::to_string)
        }
    }

    fn starred(&self) -> Option<String> {
        self.user_data
            .as_ref()
            .filter(|data| data.is_favorite)
            .map(|_| FAVORITE_MARK.to_string())
    }

    fn play_count(&self) -> Option<u32> {
        non_negative(self.user_data.as_ref().and_then(|data| data.play_count))
    }

    fn played(&self) -> Option<String> {
        non_empty(self.user_data.as_ref().and_then(|data| data.last_played_date.clone()))
    }

    fn genre(&self) -> Option<String> {
        self.genres.as_ref().and_then(|g| g.first().cloned()).and_then(|g| non_empty(Some(g)))
    }

    fn first_album_artist(&self) -> Option<&NameGuidPair> {
        self.album_artists.as_ref().and_then(|a| a.first())
    }

    fn album_artist_name(&self) -> Option<String> {
        non_empty(self.album_artist.clone())
            .or_else(|| self.first_album_artist().and_then(|a| non_empty(a.name.clone())))
    }

    /// Every performer, as one display string ("A, B"), which
    /// `parse_artist_names` splits back up.
    fn artist_names(&self) -> Option<String> {
        let names: Vec<String> = match (&self.artist_items, &self.artists) {
            (Some(items), _) if !items.is_empty() => {
                items.iter().filter_map(|a| non_empty(a.name.clone())).collect()
            }
            (_, Some(names)) => names.iter().filter_map(|n| non_empty(Some(n.clone()))).collect(),
            _ => Vec::new(),
        };
        if names.is_empty() {
            self.album_artist_name()
        } else {
            Some(names.join(", "))
        }
    }
}

/// A container Jellyfin names with one extension ("flac", not
/// "mov,mp4,m4a,…"), and its MIME type.
fn suffix_and_type(container: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(suffix) = container.map(str::trim).filter(|c| !c.is_empty() && !c.contains(',')) else {
        return (None, None);
    };
    let mime = match suffix.to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "m4b" | "mp4" | "aac" | "alac" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "webm" | "webma" => "audio/webm",
        _ => "",
    };
    (Some(suffix.to_string()), Some(mime.to_string()).filter(|m| !m.is_empty()))
}

/// `NormalizationGain` is the dB gain Jellyfin computed from the track's
/// loudness (to its -18 LUFS reference, as ReplayGain 2.0), so it is the
/// track gain; the album's goes alongside. Peaks aren't sent.
fn replay_gain(item: &BaseItemDto) -> Option<ReplayGain> {
    let finite = |v: Option<f64>| v.filter(|v| v.is_finite());
    let track_gain = finite(item.normalization_gain);
    let album_gain = finite(item.album_normalization_gain);
    (track_gain.is_some() || album_gain.is_some()).then(|| ReplayGain {
        track_gain,
        album_gain,
        ..Default::default()
    })
}

pub fn to_song(item: BaseItemDto, server_id: &str, server_name: &str) -> Song {
    let album_cover = item
        .album_primary_image_tag
        .as_deref()
        .filter(|tag| !tag.is_empty())
        .and(item.album_id.as_deref());
    let (suffix, content_type) = suffix_and_type(item.container.as_deref());
    Song {
        title: non_empty(item.name.clone()).unwrap_or_else(|| "Unknown Song".to_string()),
        album: non_empty(item.album.clone()),
        album_id: non_empty(item.album_id.clone()),
        artist: item.artist_names(),
        artist_id: item
            .artist_items
            .as_ref()
            .and_then(|a| a.first())
            .or_else(|| item.first_album_artist())
            .map(|a| a.id.clone())
            .filter(|id| !id.is_empty()),
        duration: ticks_to_seconds(item.run_time_ticks),
        track: non_negative(item.index_number),
        cover_art: item.cover(album_cover),
        content_type,
        suffix,
        starred: item.starred(),
        play_count: item.play_count(),
        played: item.played(),
        year: non_negative(item.production_year),
        genre: item.genre(),
        replay_gain: replay_gain(&item),
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        id: item.id,
        ..Default::default()
    }
}

pub fn to_album(item: BaseItemDto, server_id: &str) -> Album {
    Album {
        name: non_empty(item.name.clone()).unwrap_or_else(|| "Unknown Album".to_string()),
        artist: item.album_artist_name().or_else(|| item.artist_names()).unwrap_or_default(),
        artist_id: item.first_album_artist().map(|a| a.id.clone()).filter(|id| !id.is_empty()),
        cover_art: item.cover(None),
        song_count: non_negative(item.child_count.or(item.song_count)).unwrap_or(0),
        duration: ticks_to_seconds(item.run_time_ticks),
        year: non_negative(item.production_year),
        genre: item.genre(),
        starred: item.starred(),
        play_count: item.play_count(),
        played: item.played(),
        created: non_empty(item.date_created.clone()),
        server_id: server_id.to_string(),
        id: item.id,
        ..Default::default()
    }
}

pub fn to_artist(item: BaseItemDto, server_id: &str) -> Artist {
    Artist {
        name: non_empty(item.name.clone()).unwrap_or_else(|| "Unknown Artist".to_string()),
        album_count: non_negative(item.album_count.or(item.child_count)).unwrap_or(0),
        cover_art: item.cover(None),
        starred: item.starred(),
        server_id: server_id.to_string(),
        id: item.id,
    }
}

pub fn to_playlist(item: BaseItemDto, server_id: &str) -> Playlist {
    Playlist {
        name: non_empty(item.name.clone()).unwrap_or_else(|| "Playlist".to_string()),
        comment: non_empty(item.overview.clone()),
        song_count: non_negative(item.child_count.or(item.song_count)).unwrap_or(0),
        duration: ticks_to_seconds(item.run_time_ticks),
        cover_art: item.cover(None),
        starred: item.starred(),
        created: non_empty(item.date_created.clone()),
        server_id: server_id.to_string(),
        id: item.id,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song_json() -> &'static str {
        r#"{
            "Name": "Lanterns",
            "Id": "8e3b0c4a9f1d4e2b8a7c6d5e4f3a2b1c",
            "ServerId": "srv",
            "Type": "Audio",
            "RunTimeTicks": 2468000000,
            "IndexNumber": 3,
            "ParentIndexNumber": 1,
            "ProductionYear": 2019,
            "Album": "Lanterns Over Harbor",
            "AlbumId": "album-1",
            "AlbumArtist": "The Paper Comets",
            "AlbumArtists": [{ "Name": "The Paper Comets", "Id": "artist-1" }],
            "ArtistItems": [{ "Name": "The Paper Comets", "Id": "artist-1" }, { "Name": "Mira Solenne", "Id": "artist-2" }],
            "Artists": ["The Paper Comets", "Mira Solenne"],
            "ImageTags": {},
            "AlbumPrimaryImageTag": "tag-a",
            "UserData": { "IsFavorite": true, "PlayCount": 7, "LastPlayedDate": "2026-10-01T10:00:00Z", "Played": true },
            "NormalizationGain": -6.5,
            "Genres": ["Indie Rock"],
            "Container": "flac",
            "MediaType": "Audio"
        }"#
    }

    #[test]
    fn a_song_maps_ticks_artists_images_and_favourites() {
        let item: BaseItemDto = serde_json::from_str(song_json()).unwrap();
        let song = to_song(item, "jf", "Jelly");
        // 2,468,000,000 ticks = 246.8 s.
        assert_eq!(song.duration, 246);
        assert_eq!(song.track, Some(3));
        assert_eq!(song.artist.as_deref(), Some("The Paper Comets, Mira Solenne"));
        assert_eq!(song.artist_id.as_deref(), Some("artist-1"));
        // No image of its own: the album's.
        assert_eq!(song.cover_art.as_deref(), Some("album-1"));
        assert_eq!(song.starred.as_deref(), Some(FAVORITE_MARK));
        assert_eq!(song.play_count, Some(7));
        assert_eq!(song.played.as_deref(), Some("2026-10-01T10:00:00Z"));
        assert_eq!(song.suffix.as_deref(), Some("flac"));
        assert_eq!(song.content_type.as_deref(), Some("audio/flac"));
        assert_eq!(song.genre.as_deref(), Some("Indie Rock"));
        assert_eq!(song.replay_gain.as_ref().and_then(|g| g.track_gain), Some(-6.5));
        assert_eq!((song.server_id.as_str(), song.server_name.as_str()), ("jf", "Jelly"));
    }

    #[test]
    fn missing_fields_degrade_quietly() {
        let item: BaseItemDto = serde_json::from_str(
            r#"{ "Id": "x", "Name": null, "RunTimeTicks": null, "ImageTags": { "Primary": "t" },
                 "UserData": { "IsFavorite": false }, "Container": "mov,mp4,m4a,3gp,3g2,mj2", "IndexNumber": -1 }"#,
        )
        .unwrap();
        let song = to_song(item, "jf", "Jelly");
        assert_eq!(song.title, "Unknown Song");
        assert_eq!(song.duration, 0);
        assert_eq!(song.track, None);
        assert_eq!(song.cover_art.as_deref(), Some("x"));
        assert!(song.starred.is_none());
        assert!(song.suffix.is_none() && song.replay_gain.is_none());
    }

    #[test]
    fn albums_artists_and_playlists_map() {
        let album: BaseItemDto = serde_json::from_str(
            r#"{ "Id": "al", "Name": "Glass Weather", "Type": "MusicAlbum", "AlbumArtist": "Mira Solenne",
                 "AlbumArtists": [{ "Name": "Mira Solenne", "Id": "ar" }], "ChildCount": 9,
                 "RunTimeTicks": 18000000000, "ProductionYear": 2021, "ImageTags": { "Primary": "abc" },
                 "DateCreated": "2026-01-02T00:00:00Z" }"#,
        )
        .unwrap();
        let album = to_album(album, "jf");
        assert_eq!((album.artist.as_str(), album.artist_id.as_deref()), ("Mira Solenne", Some("ar")));
        assert_eq!((album.song_count, album.duration, album.year), (9, 1800, Some(2021)));
        assert_eq!(album.cover_art.as_deref(), Some("al"));
        assert_eq!(album.created.as_deref(), Some("2026-01-02T00:00:00Z"));

        let artist: BaseItemDto = serde_json::from_str(
            r#"{ "Id": "ar", "Name": "Mira Solenne", "Type": "MusicArtist", "AlbumCount": 2, "ImageTags": {},
                 "UserData": { "IsFavorite": true } }"#,
        )
        .unwrap();
        let artist = to_artist(artist, "jf");
        assert_eq!((artist.album_count, artist.cover_art.clone()), (2, None));
        assert!(artist.starred.is_some());

        let playlist: BaseItemDto =
            serde_json::from_str(r#"{ "Id": "pl", "Name": "Late Night Drive", "ChildCount": 12, "RunTimeTicks": 36000000000 }"#)
                .unwrap();
        let playlist = to_playlist(playlist, "jf");
        assert_eq!((playlist.song_count, playlist.duration), (12, 3600));
    }

    #[test]
    fn ticks_convert_both_ways() {
        assert_eq!(ticks_to_seconds(Some(10_000_000)), 1);
        assert_eq!(ticks_to_seconds(Some(-5)), 0);
        assert_eq!(ms_to_ticks(1_500), 15_000_000);
    }

    #[test]
    fn the_probe_recognises_jellyfin() {
        let jellyfin: PublicSystemInfo = serde_json::from_str(
            r#"{ "LocalAddress": "http://10.0.0.2:8096", "ServerName": "den", "Version": "12.4.0",
                 "ProductName": "Jellyfin Server", "OperatingSystem": "", "Id": "f00d", "StartupWizardCompleted": true }"#,
        )
        .unwrap();
        assert!(jellyfin.is_jellyfin());
        let other: PublicSystemInfo = serde_json::from_str(r#"{ "error": "not found" }"#).unwrap();
        assert!(!other.is_jellyfin());
    }
}
