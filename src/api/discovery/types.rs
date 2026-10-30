//! Public data types returned by the Discover loaders.
//!
//! Every type is `Clone + PartialEq` so it can be used as a Dioxus prop, and
//! `Serialize + Deserialize` so whole sections can be cached.

use crate::api::models::{Album, Song};
use crate::db::AppSettings;
use serde::{Deserialize, Serialize};

/// Why an album or song is on the Discover screen. The human text is in the
/// item's `reason` field; the kind is for icons, grouping or analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonKind {
    /// A heavily played album or song that hasn't been played for a while
    /// (the user's Rewind window, 6 months by default).
    Rewind,
    /// Recently added to the library.
    NewInLibrary,
    /// By an in-library artist similar to one the user plays a lot.
    BecauseYouPlay,
}

/// An album from the user's server, with a one-line reason it is shown.
///
/// `album` is a normal [`Album`]: `album.server_id` + `album.cover_art` give the
/// cover via `NavidromeClient::get_cover_art_url`, and `album.id` opens
/// `/album/:album_id/:server_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverAlbum {
    pub album: Album,
    /// e.g. "Last played 14 months ago · 212 plays", "Added 2 days ago",
    /// "Because you play Mira Okafor".
    pub reason: String,
    pub kind: ReasonKind,
}

/// A song from the user's server, with a one-line reason it is shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverSong {
    pub song: Song,
    /// e.g. "Last played 2 years ago · 58 plays".
    pub reason: String,
    pub kind: ReasonKind,
}

/// Which generator produced a [`Mix`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MixKind {
    /// Songs similar to recently played artists, minus songs already played a lot.
    Discovery,
    /// Low- or no-play tracks from the albums the user plays most.
    DeepCuts,
    /// Most-played songs not played in 6+ months.
    Rewind,
    /// Songs by one top artist and artists similar to them.
    ArtistRadio,
}

/// A cover art reference on a specific server. Build the URL with
/// `NavidromeClient::new(server).get_cover_art_url(&cover.cover_art, size)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CoverRef {
    pub server_id: String,
    pub cover_art: String,
}

/// A generated playlist ("mix"), built only from real library data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mix {
    /// Stable id for the day, e.g. "discovery", "deep-cuts", "rewind",
    /// "radio:<server_id>:<artist_id>". Use as a list key.
    pub id: String,
    pub kind: MixKind,
    /// Display name, e.g. "Discovery Mix", "Mira Okafor Radio".
    pub name: String,
    /// One-line description, e.g. "25 songs like Mira Okafor and Junie Marlow".
    pub blurb: String,
    /// Playable songs, in play order. May span several servers (`song.server_id`).
    pub songs: Vec<Song>,
    /// Up to 4 distinct covers for a 2×2 mosaic (fewer if the mix has fewer albums).
    pub covers: Vec<CoverRef>,
    pub song_count: usize,
    pub total_seconds: u64,
    /// e.g. "1 hr 38 min" or "52 min".
    pub length_label: String,
    /// e.g. "Updated today".
    pub freshness: String,
    /// Local date (YYYY-MM-DD) the mix was generated for.
    pub generated_on: String,
}

/// What an off-server recommendation points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OffServerKind {
    Artist,
    Album,
    Track,
}

/// Where a recommendation (or its art/preview) came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoverySource {
    /// The user's own server (`getArtistInfo2`, backed by its Last.fm/Spotify agents).
    Navidrome,
    LastFm,
    ListenBrainz,
    /// iTunes Search API (art, previews and Apple Music links only).
    ITunes,
}

impl DiscoverySource {
    /// Short label for UI, e.g. "Last.fm".
    pub fn label(&self) -> &'static str {
        match self {
            // Suggested by the server, not in its library: "Your server" read as
            // "on your server" under the "Outside your server" heading.
            Self::Navidrome => "Server pick",
            Self::LastFm => "Last.fm",
            Self::ListenBrainz => "ListenBrainz",
            Self::ITunes => "Apple Music",
        }
    }
}

/// A recommendation that is NOT in the user's library. Never playable from the
/// server: offer `preview_url` (30 s clip) and `external_url` instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OffServerItem {
    /// Stable key, e.g. "artist:ilse varga" or "album:delta pines:northbound static".
    pub id: String,
    pub kind: OffServerKind,
    /// Artist name for `Artist`, album title for `Album`, track title for `Track`.
    pub title: String,
    /// Artist name (same as `title` for `Artist` items).
    pub artist: String,
    /// e.g. "Similar to Mira Okafor", "New release · You listen to Delta Pines".
    pub reason: String,
    /// The library artist this was derived from, when there is one.
    pub because_artist: Option<String>,
    /// Who recommended it.
    pub source: DiscoverySource,
    /// Absolute image URL (iTunes/Last.fm/Cover Art Archive), when known.
    pub art_url: Option<String>,
    /// 30-second preview audio URL (iTunes m4a), when known and previews are enabled.
    pub preview_url: Option<String>,
    /// Track name the preview plays (useful for Artist/Album items).
    pub preview_title: Option<String>,
    /// Link out (Apple Music, Last.fm or MusicBrainz page), when known.
    pub external_url: Option<String>,
    /// Release date (YYYY-MM-DD) for new releases, when known.
    pub release_date: Option<String>,
}

/// Health of one discovery source during the last load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum SourceStatus {
    /// Settings don't enable it (e.g. no Last.fm API key).
    #[default]
    NotConfigured,
    /// Turned off in settings, or offline mode is on.
    Disabled,
    /// Ran and contributed this many items.
    Ok(usize),
    /// Ran but failed; the message is safe to show (never contains the API key).
    Failed(String),
}

/// Per-source status for the off-server section. Settings can use it to show
/// "Invalid Last.fm API key" etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OffServerStatus {
    pub navidrome: SourceStatus,
    pub lastfm: SourceStatus,
    pub listenbrainz: SourceStatus,
    pub itunes: SourceStatus,
    /// External HTTP requests this load actually made (cache hits excluded).
    pub external_requests: u32,
}

/// Cover Flow + album shelves. Cheapest section; load it first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct StageSection {
    /// Curated albums for Cover Flow (≤ 9), each with a reason line.
    pub stage: Vec<DiscoverAlbum>,
    /// Recently added albums, newest first (≤ 20). "New" segment.
    pub new_in_library: Vec<DiscoverAlbum>,
    /// Local date (YYYY-MM-DD) this was generated for.
    pub generated_on: String,
}

/// Generated mixes and rewind songs. "For You" segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MixesSection {
    /// 0–5 mixes in display order: Discovery, Deep Cuts, Rewind, then artist radios.
    pub mixes: Vec<Mix>,
    pub generated_on: String,
}

/// The "Rewind" segment for one window, from [`super::load_discover_rewind`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RewindSection {
    /// The window this was computed for, in months (3, 6 or 12).
    pub months: u32,
    /// Heavily played albums not played within the window (≤ 12), most played first.
    pub albums: Vec<DiscoverAlbum>,
    /// Heavily played songs not played within the window (≤ 20), most played first.
    pub songs: Vec<DiscoverSong>,
    pub generated_on: String,
}

/// Recommendations from outside the library. "New" segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OffServerSection {
    /// ≤ 12 items, interleaved across sources.
    pub items: Vec<OffServerItem>,
    pub status: OffServerStatus,
    pub generated_on: String,
}

/// Everything the Discover screen shows, from [`super::load_discover`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DiscoverFeed {
    pub stage: StageSection,
    pub mixes: MixesSection,
    pub off_server: OffServerSection,
}

/// Discovery-relevant settings, read from [`AppSettings`]. Empty strings become `None`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscoveryConfig {
    pub lastfm_api_key: Option<String>,
    pub lastfm_username: Option<String>,
    pub listenbrainz_username: Option<String>,
    pub offserver_enabled: bool,
    pub previews_enabled: bool,
}

fn trimmed(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

impl DiscoveryConfig {
    /// Read the discovery fields from the app settings.
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self {
            lastfm_api_key: trimmed(&settings.lastfm_api_key),
            lastfm_username: trimmed(&settings.lastfm_username),
            listenbrainz_username: trimmed(&settings.listenbrainz_username),
            offserver_enabled: settings.discovery_offserver_enabled,
            previews_enabled: settings.discovery_previews_enabled,
        }
    }

    /// Hash of the settings that change results; part of cache keys. Never
    /// contains the raw API key.
    pub(crate) fn signature(&self) -> String {
        let raw = format!(
            "{}|{}|{}|{}|{}",
            self.lastfm_api_key.as_deref().unwrap_or(""),
            self.lastfm_username.as_deref().unwrap_or(""),
            self.listenbrainz_username.as_deref().unwrap_or(""),
            self.offserver_enabled,
            self.previews_enabled
        );
        format!("{:016x}", super::util::fnv1a(&raw))
    }
}
