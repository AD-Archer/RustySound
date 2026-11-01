//! The index as the app reads it: a connection to the app database, queries
//! run off the main thread, and the id → file lookups the audio route, the
//! cover route and AVPlayer use. Mac and iPhone.

use super::index::{self, Folder};
use super::model::{self, is_local_id};
use crate::api::{Album, Artist, SearchResult, Song};
use once_cell::sync::{Lazy, OnceCell};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Opens the app database with the local tables in place. Each caller gets
/// its own connection (SQLite connections aren't shared across threads).
pub(super) fn open() -> Result<Connection, String> {
    static MIGRATED: OnceCell<()> = OnceCell::new();
    let conn = crate::db::get_db_connection().map_err(|e| e.to_string())?;
    MIGRATED
        .get_or_try_init(|| index::migrate(&conn))
        .map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Runs `job` on the blocking pool, so list queries never stall rendering.
pub(super) async fn run<T, F>(job: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut conn = open()?;
        job(&mut conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Lookups for the audio and cover routes ------------------------------------

/// `song id -> file`. The audio route asks once per 2 MB slice; this keeps
/// those asks off the database. Cleared after every scan.
static PATHS: Lazy<Mutex<HashMap<String, PathBuf>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub(super) fn forget_paths() {
    PATHS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// The file of an indexed song. Ids that aren't the scanner's are refused,
/// so nothing from a URL ever reaches the filesystem as a path.
pub fn song_file(song_id: &str) -> Option<PathBuf> {
    if !is_local_id(song_id) {
        return None;
    }
    if let Some(path) = PATHS.lock().unwrap_or_else(|e| e.into_inner()).get(song_id) {
        return Some(path.clone());
    }
    let path = index::file_path(&open().ok()?, song_id).ok()??;
    PATHS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(song_id.to_string(), path.clone());
    Some(path)
}

/// Rebuilds a local cover from its file after the cover cache was cleared.
/// `art_id` comes from a cache file name; the bytes are written back there.
pub fn rebuild_art(art_id: &str, cache_file: &std::path::Path) -> Option<Vec<u8>> {
    if !is_local_id(art_id) {
        return None;
    }
    let source = index::art_source(&open().ok()?, art_id).ok()??;
    let bytes = super::scan::read_picture(&source)?;
    if model::art_id(&bytes) != art_id {
        return None;
    }
    let _ = std::fs::write(cache_file, &bytes);
    Some(bytes)
}

pub fn embedded_lyrics(song_id: &str) -> Option<String> {
    index::embedded_lyrics(&open().ok()?, song_id).ok()?
}

pub(super) fn folders() -> Vec<Folder> {
    open().and_then(|c| index::folders(&c).map_err(|e| e.to_string())).unwrap_or_default()
}

pub(super) fn track_count() -> usize {
    open().and_then(|c| index::track_count(&c).map_err(|e| e.to_string())).unwrap_or(0)
}

// ---- The client --------------------------------------------------------------------

/// The device's music as a source, with `NavidromeClient`'s method names so
/// `MusicClient` can hand it the same calls. Server-only features (radio,
/// similar songs, scrobbling, play-queue sync) have no method here.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalLibrary;

impl LocalLibrary {
    pub async fn get_album_page(
        &self,
        list_type: &str,
        genre: Option<&str>,
        size: u32,
        offset: u32,
    ) -> Result<Vec<Album>, String> {
        let (list_type, genre) = (list_type.to_string(), genre.map(str::to_string));
        run(move |c| index::albums_page(c, &list_type, genre.as_deref(), size, offset)).await
    }

    pub async fn search_page(&self, query: &str, kind: &str, size: u32, offset: u32) -> Result<SearchResult, String> {
        let (query, kind) = (query.to_string(), kind.to_string());
        run(move |c| {
            let mut result = SearchResult::default();
            match kind.as_str() {
                "artist" => result.artists = index::search_artists(c, &query, size, offset)?,
                "album" => result.albums = index::search_albums(c, &query, size, offset)?,
                _ => result.songs = index::search_songs(c, &query, size, offset)?,
            }
            Ok(result)
        })
        .await
    }

    pub async fn search(&self, query: &str, artist_count: u32, album_count: u32, song_count: u32) -> Result<SearchResult, String> {
        let query = query.to_string();
        run(move |c| index::search(c, &query, artist_count, album_count, song_count)).await
    }

    pub async fn get_native_song_page(&self, sort: &str, ascending: bool, offset: usize, size: usize) -> Result<Vec<Song>, String> {
        let sort = sort.to_string();
        run(move |c| index::songs_page(c, &sort, ascending, offset, size)).await
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        run(|c| index::artists(c)).await
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        let id = artist_id.to_string();
        run(move |c| index::artist(c, &id)).await?.ok_or_else(|| "Artist not found".to_string())
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        let id = album_id.to_string();
        run(move |c| index::album(c, &id)).await?.ok_or_else(|| "Album not found".to_string())
    }

    pub async fn get_top_songs(&self, artist: &str, count: u32) -> Result<Vec<Song>, String> {
        let artist = artist.to_string();
        run(move |c| index::top_songs(c, &artist, count)).await
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        run(|c| index::starred(c)).await
    }

    /// Autoplay for music on the device: same artist, then same genre.
    pub async fn get_similar_songs(&self, seed: &Song, count: u32) -> Result<Vec<Song>, String> {
        let artist = seed.artist.clone().unwrap_or_default();
        let genre = seed.genre.clone().unwrap_or_default();
        let id = seed.id.clone();
        run(move |c| index::similar_songs(c, &artist, &genre, &id, count)).await
    }

    pub async fn star(&self, id: &str, _item_type: &str) -> Result<(), String> {
        let id = id.to_string();
        run(move |c| index::set_starred(c, &id, true)).await
    }

    pub async fn unstar(&self, id: &str, _item_type: &str) -> Result<(), String> {
        let id = id.to_string();
        run(move |c| index::set_starred(c, &id, false)).await
    }

    pub async fn set_rating(&self, id: &str, rating: u32) -> Result<(), String> {
        let id = id.to_string();
        run(move |c| index::set_rating(c, &id, rating)).await
    }

    /// Counts a finished play (the local stand-in for scrobbling).
    pub async fn record_play(&self, song_id: &str) -> Result<(), String> {
        let id = song_id.to_string();
        run(move |c| index::record_play(c, &id)).await
    }
}
