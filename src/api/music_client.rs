//! One client for every music source, so views list, search, open and play
//! items without caring where they live: Subsonic servers, Jellyfin servers,
//! and the music stored on the device ("On This Device", `server_id =
//! "local"`).
//!
//! This is the `MusicClient` facade from `.redesign/research/jellyfin.md`:
//! the method names are `NavidromeClient`'s, so a call site changes only
//! `NavidromeClient::new(server)` to `MusicClient::new(server)` (or
//! [`MusicClient::sources`] / [`MusicClient::for_source`]). Features only
//! some sources have are described by [`Capabilities`]; views hide them from
//! those flags, never from the kind of server. Server-only features not
//! listed here (Discover mixes, radio stations, bookmarks, scans) still use
//! `NavidromeClient`, which refuses Jellyfin servers without a request.

use super::offline_queue::{self, Change};
use super::{
    Album, Artist, JellyfinClient, NavidromeClient,
    PlaybackReportState, Playlist, SearchResult, ServerConfig, ServerKind, Song,
};
use crate::local_music::{self, LocalLibrary, LOCAL_SOURCE_ID};

pub enum MusicClient {
    Subsonic(NavidromeClient),
    Jellyfin(JellyfinClient),
    Local(LocalLibrary),
}

/// What a source can do beyond browsing, searching, favourites and playback,
/// which every source has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// 1–5 star ratings (Jellyfin only has favourites and thumbs).
    pub star_ratings: bool,
    /// Creating and editing playlists. Browsing them needs only `playlists`.
    pub playlist_edits: bool,
    pub playlists: bool,
    /// Saving the play queue on the server: "Continue from iPhone", and
    /// "Also playing on" in Now Playing.
    pub play_queue_sync: bool,
    /// Share links.
    pub sharing: bool,
    /// Internet radio stations kept on the server.
    pub internet_radio: bool,
    /// Resume positions saved on the server.
    pub bookmarks: bool,
    /// Start Radio from a song (similar songs, or Jellyfin's instant mix).
    pub song_radio: bool,
    /// Quick Listen: a preview streamed from the server.
    pub quick_listen: bool,
}

impl Capabilities {
    pub const SUBSONIC: Self = Self {
        star_ratings: true,
        playlist_edits: true,
        playlists: true,
        play_queue_sync: true,
        sharing: true,
        internet_radio: true,
        bookmarks: true,
        song_radio: true,
        quick_listen: true,
    };
    pub const JELLYFIN: Self = Self {
        star_ratings: false,
        playlist_edits: false,
        playlists: true,
        play_queue_sync: false,
        sharing: false,
        internet_radio: false,
        bookmarks: false,
        song_radio: true,
        quick_listen: true,
    };
    /// Music on the device keeps its own favourites and ratings.
    pub const LOCAL: Self = Self {
        star_ratings: true,
        ..Self::NONE
    };
    pub const NONE: Self = Self {
        star_ratings: false,
        playlist_edits: false,
        playlists: false,
        play_queue_sync: false,
        sharing: false,
        internet_radio: false,
        bookmarks: false,
        song_radio: false,
        quick_listen: false,
    };

    pub fn of(server: &ServerConfig) -> Self {
        if local_music::is_local(&server.id) {
            return Self::LOCAL;
        }
        match server.kind {
            ServerKind::Subsonic => Self::SUBSONIC,
            ServerKind::Jellyfin => Self::JELLYFIN,
        }
    }

    /// The capabilities of the source an item's `server_id` names. A source
    /// that isn't configured (any more) can do nothing.
    pub fn for_source(servers: &[ServerConfig], server_id: &str) -> Self {
        if local_music::is_local(server_id) {
            return Self::LOCAL;
        }
        servers
            .iter()
            .find(|s| s.id == server_id)
            .map(Self::of)
            .unwrap_or(Self::NONE)
    }
}

/// Runs the same call on whichever client this is.
macro_rules! each {
    ($self:ident, $c:ident => $call:expr) => {
        match $self {
            MusicClient::Subsonic($c) => $call,
            MusicClient::Jellyfin($c) => $call,
            MusicClient::Local($c) => $call,
        }
    };
}

/// Runs a server call; the device's music answers `$local` instead.
macro_rules! servers_only {
    ($self:ident, $c:ident => $call:expr, $local:expr) => {
        match $self {
            MusicClient::Subsonic($c) => $call,
            MusicClient::Jellyfin($c) => $call,
            MusicClient::Local(_) => $local,
        }
    };
}

impl MusicClient {
    pub fn new(server: ServerConfig) -> Self {
        if local_music::is_local(&server.id) {
            return Self::Local(LocalLibrary);
        }
        match server.kind {
            ServerKind::Subsonic => Self::Subsonic(NavidromeClient::new(server)),
            ServerKind::Jellyfin => Self::Jellyfin(JellyfinClient::new(server)),
        }
    }

    /// Every active server, plus On This Device when it has music.
    pub fn sources(servers: &[ServerConfig]) -> Vec<Self> {
        let mut clients: Vec<Self> = servers
            .iter()
            .filter(|s| s.active && !local_music::is_local(&s.id))
            .cloned()
            .map(Self::new)
            .collect();
        if local_music::has_music_now() {
            clients.push(Self::Local(LocalLibrary));
        }
        clients
    }

    /// The source an item's `server_id` names, if it's configured.
    pub fn for_source(servers: &[ServerConfig], server_id: &str) -> Option<Self> {
        if local_music::is_local(server_id) {
            return local_music::SUPPORTED.then_some(Self::Local(LocalLibrary));
        }
        servers.iter().find(|s| s.id == server_id).cloned().map(Self::new)
    }

    pub fn source_id(&self) -> &str {
        match self {
            Self::Subsonic(client) => &client.server.id,
            Self::Jellyfin(client) => &client.server.id,
            Self::Local(_) => LOCAL_SOURCE_ID,
        }
    }

    /// Whether the server answers and accepts the saved sign-in.
    pub async fn ping(&self) -> Result<bool, String> {
        servers_only!(self, c => c.ping().await, Ok(true))
    }

    pub async fn get_album_page(&self, list_type: &str, genre: Option<&str>, size: u32, offset: u32) -> Result<Vec<Album>, String> {
        each!(self, c => c.get_album_page(list_type, genre, size, offset).await)
    }

    /// A cached album list, for Home and Discover.
    pub async fn get_albums(&self, list_type: &str, size: u32, offset: u32) -> Result<Vec<Album>, String> {
        match self {
            Self::Subsonic(c) => c.get_albums(list_type, size, offset).await,
            Self::Jellyfin(c) => c.get_albums(list_type, size, offset).await,
            Self::Local(c) => c.get_album_page(list_type, None, size, offset).await,
        }
    }

    pub async fn search_page(&self, query: &str, kind: &str, size: u32, offset: u32) -> Result<SearchResult, String> {
        each!(self, c => c.search_page(query, kind, size, offset).await)
    }

    pub async fn search(&self, query: &str, artist_count: u32, album_count: u32, song_count: u32) -> Result<SearchResult, String> {
        each!(self, c => c.search(query, artist_count, album_count, song_count).await)
    }

    /// A sortable page of every song: Navidrome's native list, or Jellyfin's
    /// own. Errors on other Subsonic servers; callers fall back to
    /// `search_page`.
    pub async fn get_native_song_page(&self, sort: &str, ascending: bool, offset: usize, size: usize) -> Result<Vec<Song>, String> {
        each!(self, c => c.get_native_song_page(sort, ascending, offset, size).await)
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        each!(self, c => c.get_artists().await)
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        each!(self, c => c.get_artist(artist_id).await)
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        each!(self, c => c.get_album(album_id).await)
    }

    pub async fn get_top_songs(&self, artist: &str, count: u32) -> Result<Vec<Song>, String> {
        each!(self, c => c.get_top_songs(artist, count).await)
    }

    /// The library's most played songs (Navidrome's native list on Subsonic).
    pub async fn get_most_played_songs(&self, count: usize) -> Result<Vec<Song>, String> {
        self.get_native_song_page("play_count", false, 0, count).await
    }

    pub async fn get_random_songs(&self, size: u32) -> Result<Vec<Song>, String> {
        servers_only!(self, c => c.get_random_songs(size).await, Ok(Vec::new()))
    }

    /// Songs like `id`, for Start Radio.
    pub async fn get_similar_songs(&self, id: &str, count: u32) -> Result<Vec<Song>, String> {
        servers_only!(self, c => c.get_similar_songs(id, count).await, Ok(Vec::new()))
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        each!(self, c => c.get_starred().await)
    }

    /// The server this client talks to; `None` for music on the device.
    fn server(&self) -> Option<&ServerConfig> {
        match self {
            Self::Subsonic(c) => Some(&c.server),
            Self::Jellyfin(c) => Some(&c.server),
            Self::Local(_) => None,
        }
    }

    /// Favourites: on the server, or in the local index for local items.
    /// While the server can't be reached the change waits in the offline
    /// queue and this still succeeds (`offline_queue`).
    pub async fn star(&self, id: &str, item_type: &str) -> Result<(), String> {
        self.favorite(id, item_type, true).await
    }

    pub async fn unstar(&self, id: &str, item_type: &str) -> Result<(), String> {
        self.favorite(id, item_type, false).await
    }

    async fn favorite(&self, id: &str, item_type: &str, starred: bool) -> Result<(), String> {
        let send = self.send_favorite(id, item_type, starred);
        let Some(server) = self.server() else {
            return send.await;
        };
        let change = || {
            let (id, item_type) = (id.to_string(), item_type.to_string());
            Some((Change::Star { id, item_type, starred }, ()))
        };
        offline_queue::submit(server, send, change).await
    }

    /// The favourite call itself, never queued (replay sends with this).
    pub(crate) async fn send_favorite(&self, id: &str, item_type: &str, starred: bool) -> Result<(), String> {
        if starred {
            each!(self, c => c.star(id, item_type).await)
        } else {
            each!(self, c => c.unstar(id, item_type).await)
        }
    }

    /// 1–5 stars, where [`Capabilities::star_ratings`] says so. Queued like
    /// favourites while the server can't be reached.
    pub async fn set_rating(&self, id: &str, rating: u32) -> Result<(), String> {
        let send = self.send_rating(id, rating);
        match self {
            Self::Subsonic(c) => {
                let change = || Some((Change::Rate { id: id.to_string(), rating }, ()));
                offline_queue::submit(&c.server, send, change).await
            }
            Self::Local(_) | Self::Jellyfin(_) => send.await,
        }
    }

    /// The rating call itself, never queued (replay sends with this).
    pub(crate) async fn send_rating(&self, id: &str, rating: u32) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.set_rating(id, rating).await,
            Self::Local(c) => c.set_rating(id, rating).await,
            Self::Jellyfin(_) => Err("Jellyfin has no star ratings.".to_string()),
        }
    }

    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, String> {
        servers_only!(self, c => c.get_playlists().await, Ok(Vec::new()))
    }

    pub async fn get_playlist(&self, playlist_id: &str) -> Result<(Playlist, Vec<Song>), String> {
        servers_only!(self, c => c.get_playlist(playlist_id).await, Err("Not on a server.".to_string()))
    }

    pub fn refresh_playlist_cache(&self) {
        servers_only!(self, c => c.refresh_playlist_cache(), ())
    }

    /// The URL a song streams from. Empty for local songs, which play from
    /// their files (`local_music::stream_url`).
    pub fn get_stream_url(&self, song_id: &str) -> String {
        servers_only!(self, c => c.get_stream_url(song_id), String::new())
    }

    pub fn get_cover_art_url(&self, cover_art_id: &str, size: u32) -> String {
        servers_only!(
            self,
            c => c.get_cover_art_url(cover_art_id, size),
            local_music::cover_art_data_url(cover_art_id).unwrap_or_default()
        )
    }

    /// Cover URL for an `<img>`, cheap enough to call while rendering.
    pub fn get_cover_art_display_url(&self, cover_art_id: &str, size: u32) -> String {
        servers_only!(
            self,
            c => c.get_cover_art_display_url(cover_art_id, size),
            local_music::cover_art_src(cover_art_id).unwrap_or_default()
        )
    }

    /// Whether this source hears about playback through
    /// [`Self::report_playback`]. Those that don't get `scrobble` instead.
    pub async fn reports_playback(&self) -> bool {
        match self {
            Self::Subsonic(c) => c.sync_support().await.playback_report,
            Self::Jellyfin(_) => true,
            Self::Local(_) => false,
        }
    }

    /// Starting, progress, pause and stop reports. Subsonic servers need
    /// the `playbackReport` extension (see [`Self::reports_playback`]).
    pub async fn report_playback(&self, song_id: &str, state: PlaybackReportState, position_ms: u64, rate: f64) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.report_playback(song_id, state, position_ms, rate).await,
            Self::Jellyfin(c) => c.report_playback(song_id, state, position_ms).await,
            Self::Local(_) => Ok(()),
        }
    }

    /// The old way to report a play: "now playing" (`finished` false) or a
    /// counted play. Jellyfin counts plays from its reports.
    pub async fn scrobble(&self, song_id: &str, finished: bool) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.scrobble(song_id, finished).await,
            Self::Jellyfin(_) | Self::Local(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool) -> ServerConfig {
        ServerConfig { id: id.into(), name: id.into(), url: "http://x".into(), username: "u".into(), password: "p".into(), active, ..Default::default() }
    }

    fn jellyfin(id: &str) -> ServerConfig {
        ServerConfig { id: id.into(), ..ServerConfig::jellyfin(id.into(), "http://jf".into(), "u".into(), "uid".into(), "tok".into()) }
    }

    #[test]
    fn sources_are_the_active_servers() {
        let clients = MusicClient::sources(&[server("a", true), server("b", false), jellyfin("j")]);
        assert_eq!(clients.iter().map(|c| c.source_id()).collect::<Vec<_>>(), vec!["a", "j"]);
        assert!(matches!(clients[1], MusicClient::Jellyfin(_)));
        assert!(MusicClient::for_source(&[server("a", true)], "missing").is_none());
        assert_eq!(MusicClient::Local(LocalLibrary).source_id(), LOCAL_SOURCE_ID);
        // "local" never becomes a Subsonic client, even from a server entry.
        assert!(matches!(MusicClient::new(crate::local_music::source_config()), MusicClient::Local(_)));
    }

    #[test]
    fn the_capability_matrix() {
        let servers = [server("a", true), jellyfin("j")];
        let subsonic = Capabilities::for_source(&servers, "a");
        let jelly = Capabilities::for_source(&servers, "j");
        assert_eq!(subsonic, Capabilities::SUBSONIC);
        assert_eq!(jelly, Capabilities::JELLYFIN);
        // Jellyfin has no stars, server play queue, share links or radio
        // stations, and its playlists are read-only for now.
        assert!(!jelly.star_ratings && !jelly.play_queue_sync && !jelly.sharing && !jelly.internet_radio);
        assert!(!jelly.bookmarks && !jelly.playlist_edits);
        assert!(jelly.playlists && jelly.song_radio && jelly.quick_listen);
        // Everything Subsonic.
        assert!(subsonic.star_ratings && subsonic.play_queue_sync && subsonic.sharing && subsonic.playlist_edits);
        // The device's music keeps ratings locally and nothing else.
        let local = Capabilities::for_source(&servers, LOCAL_SOURCE_ID);
        assert_eq!(local, Capabilities::LOCAL);
        assert!(local.star_ratings && !local.playlists && !local.song_radio);
        assert_eq!(Capabilities::for_source(&servers, "gone"), Capabilities::NONE);
    }

    #[test]
    fn jellyfin_streams_come_from_jellyfin() {
        let client = MusicClient::new(jellyfin("j"));
        assert!(client.get_stream_url("song-1").starts_with("http://jf/Audio/song-1/universal?"));
        assert!(MusicClient::new(server("a", true)).get_stream_url("song-1").starts_with("http://x/rest/stream?"));
    }
}
