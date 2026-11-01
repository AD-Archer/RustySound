//! One client for every music source, so views list, search and open items
//! without caring where they live: Subsonic servers, and the music stored on
//! the device ("On This Device", `server_id = "local"`).
//!
//! This is the `MusicClient` facade from `.redesign/research/jellyfin.md`:
//! the method names are `NavidromeClient`'s, so a call site changes only
//! `NavidromeClient::new(server)` to `MusicClient::new(server)` (or
//! [`MusicClient::sources`] / [`MusicClient::for_source`]). A Jellyfin
//! variant slots in beside these two. Only the library calls are here so
//! far; server-only features (radio, similar songs, playlists, scrobbling)
//! still use `NavidromeClient` with a server from the list, which local
//! items never match, so they skip those gracefully.

use super::{Album, Artist, NavidromeClient, SearchResult, ServerConfig, Song};
use crate::local_music::{self, LocalLibrary, LOCAL_SOURCE_ID};

pub enum MusicClient {
    Subsonic(NavidromeClient),
    Local(LocalLibrary),
}

impl MusicClient {
    pub fn new(server: ServerConfig) -> Self {
        if local_music::is_local(&server.id) {
            Self::Local(LocalLibrary)
        } else {
            Self::Subsonic(NavidromeClient::new(server))
        }
    }

    /// Every active server, plus On This Device when it has music.
    pub fn sources(servers: &[ServerConfig]) -> Vec<Self> {
        let mut clients: Vec<Self> = servers
            .iter()
            .filter(|s| s.active && !local_music::is_local(&s.id))
            .cloned()
            .map(|s| Self::Subsonic(NavidromeClient::new(s)))
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
        servers
            .iter()
            .find(|s| s.id == server_id)
            .cloned()
            .map(|s| Self::Subsonic(NavidromeClient::new(s)))
    }

    pub fn source_id(&self) -> &str {
        match self {
            Self::Subsonic(client) => &client.server.id,
            Self::Local(_) => LOCAL_SOURCE_ID,
        }
    }

    pub async fn get_album_page(&self, list_type: &str, genre: Option<&str>, size: u32, offset: u32) -> Result<Vec<Album>, String> {
        match self {
            Self::Subsonic(c) => c.get_album_page(list_type, genre, size, offset).await,
            Self::Local(c) => c.get_album_page(list_type, genre, size, offset).await,
        }
    }

    pub async fn search_page(&self, query: &str, kind: &str, size: u32, offset: u32) -> Result<SearchResult, String> {
        match self {
            Self::Subsonic(c) => c.search_page(query, kind, size, offset).await,
            Self::Local(c) => c.search_page(query, kind, size, offset).await,
        }
    }

    pub async fn search(&self, query: &str, artist_count: u32, album_count: u32, song_count: u32) -> Result<SearchResult, String> {
        match self {
            Self::Subsonic(c) => c.search(query, artist_count, album_count, song_count).await,
            Self::Local(c) => c.search(query, artist_count, album_count, song_count).await,
        }
    }

    /// Navidrome's native song list (sortable). Errors on other Subsonic
    /// servers; callers fall back to `search_page`.
    pub async fn get_native_song_page(&self, sort: &str, ascending: bool, offset: usize, size: usize) -> Result<Vec<Song>, String> {
        match self {
            Self::Subsonic(c) => c.get_native_song_page(sort, ascending, offset, size).await,
            Self::Local(c) => c.get_native_song_page(sort, ascending, offset, size).await,
        }
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        match self {
            Self::Subsonic(c) => c.get_artists().await,
            Self::Local(c) => c.get_artists().await,
        }
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        match self {
            Self::Subsonic(c) => c.get_artist(artist_id).await,
            Self::Local(c) => c.get_artist(artist_id).await,
        }
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        match self {
            Self::Subsonic(c) => c.get_album(album_id).await,
            Self::Local(c) => c.get_album(album_id).await,
        }
    }

    pub async fn get_top_songs(&self, artist: &str, count: u32) -> Result<Vec<Song>, String> {
        match self {
            Self::Subsonic(c) => c.get_top_songs(artist, count).await,
            Self::Local(c) => c.get_top_songs(artist, count).await,
        }
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        match self {
            Self::Subsonic(c) => c.get_starred().await,
            Self::Local(c) => c.get_starred().await,
        }
    }

    /// Favourites: on the server, or in the local index for local items.
    pub async fn star(&self, id: &str, item_type: &str) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.star(id, item_type).await,
            Self::Local(c) => c.star(id, item_type).await,
        }
    }

    pub async fn unstar(&self, id: &str, item_type: &str) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.unstar(id, item_type).await,
            Self::Local(c) => c.unstar(id, item_type).await,
        }
    }

    pub async fn set_rating(&self, id: &str, rating: u32) -> Result<(), String> {
        match self {
            Self::Subsonic(c) => c.set_rating(id, rating).await,
            Self::Local(c) => c.set_rating(id, rating).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool) -> ServerConfig {
        ServerConfig { id: id.into(), name: id.into(), url: "http://x".into(), username: "u".into(), password: "p".into(), active }
    }

    #[test]
    fn sources_are_the_active_servers() {
        let clients = MusicClient::sources(&[server("a", true), server("b", false)]);
        assert_eq!(clients.iter().map(|c| c.source_id()).collect::<Vec<_>>(), vec!["a"]);
        assert!(MusicClient::for_source(&[server("a", true)], "missing").is_none());
        assert_eq!(MusicClient::Local(LocalLibrary).source_id(), LOCAL_SOURCE_ID);
        // "local" never becomes a Subsonic client, even from a server entry.
        assert!(matches!(MusicClient::new(crate::local_music::source_config()), MusicClient::Local(_)));
    }
}
