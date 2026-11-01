//! The SQLite index of local files, in the app database beside the settings.
//!
//! One row per file in `local_tracks`. Albums and artists are grouped from it
//! at query time (indexed columns, milliseconds for tens of thousands of
//! rows). Favourites, ratings and plays live in `local_marks`, keyed by the
//! stable ids, so they survive rescans and a folder being removed and added
//! again. Every function takes the connection, so tests run on `:memory:`.

use super::model::{
    self, Marks, TrackRecord, LOCAL_SOURCE_ID, UNKNOWN_ALBUM, UNKNOWN_ARTIST,
};
use crate::api::{Album, Artist, ReplayGain, SearchResult, Song};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use std::collections::HashMap;
use std::path::PathBuf;

/// Tables are created if missing, like the rest of the app database. A later
/// column goes in with `ALTER TABLE` behind a `pragma_table_info` check.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS local_folders (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    -- Security-scoped bookmark, for a sandboxed build later. Unused today.
    bookmark BLOB,
    added_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS local_tracks (
    id TEXT PRIMARY KEY,
    folder_id TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    size INTEGER NOT NULL,
    title TEXT NOT NULL,
    artist TEXT,
    album_artist TEXT,
    album TEXT,
    album_id TEXT NOT NULL,
    album_artist_id TEXT NOT NULL,
    artist_id TEXT NOT NULL,
    track INTEGER,
    disc INTEGER,
    year INTEGER,
    genre TEXT,
    duration INTEGER NOT NULL DEFAULT 0,
    bitrate INTEGER,
    suffix TEXT NOT NULL,
    content_type TEXT NOT NULL,
    art_id TEXT,
    lyrics TEXT,
    mbid TEXT,
    rg_track_gain REAL,
    rg_album_gain REAL,
    rg_track_peak REAL,
    rg_album_peak REAL,
    added_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS local_tracks_path ON local_tracks (folder_id, rel_path);
CREATE INDEX IF NOT EXISTS local_tracks_album ON local_tracks (album_id);
CREATE INDEX IF NOT EXISTS local_tracks_album_artist ON local_tracks (album_artist_id);
CREATE INDEX IF NOT EXISTS local_tracks_artist ON local_tracks (artist_id);
CREATE INDEX IF NOT EXISTS local_tracks_art ON local_tracks (art_id);
CREATE TABLE IF NOT EXISTS local_marks (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    starred TEXT,
    rating INTEGER,
    play_count INTEGER NOT NULL DEFAULT 0,
    played TEXT,
    PRIMARY KEY (kind, id)
);
";

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA)
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

fn iso(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0).map(|t| t.to_rfc3339())
}

// ---- Folders --------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub id: String,
    pub path: String,
}

pub fn add_folder(conn: &Connection, path: &str) -> rusqlite::Result<Folder> {
    let id = model::folder_id(path);
    conn.execute(
        "INSERT OR IGNORE INTO local_folders (id, path, added_at) VALUES (?1, ?2, ?3)",
        params![id, path, now_secs()],
    )?;
    Ok(Folder { id, path: path.to_string() })
}

/// Registers a folder under a fixed id, updating its path. For the iPhone's
/// Documents folder, whose path changes when the app is reinstalled.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub fn set_folder(conn: &Connection, id: &str, path: &str) -> rusqlite::Result<Folder> {
    conn.execute(
        "INSERT INTO local_folders (id, path, added_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (id) DO UPDATE SET path = excluded.path",
        params![id, path, now_secs()],
    )?;
    Ok(Folder { id: id.to_string(), path: path.to_string() })
}

/// Forgets a folder and its files. Marks stay, so adding it back restores them.
pub fn remove_folder(conn: &mut Connection, id: &str) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM local_tracks WHERE folder_id = ?1", [id])?;
    tx.execute("DELETE FROM local_folders WHERE id = ?1", [id])?;
    tx.commit()
}

pub fn folders(conn: &Connection) -> rusqlite::Result<Vec<Folder>> {
    let mut stmt = conn.prepare("SELECT id, path FROM local_folders ORDER BY added_at, path")?;
    let rows = stmt.query_map([], |row| Ok(Folder { id: row.get(0)?, path: row.get(1)? }))?;
    rows.collect()
}

// ---- Scanner writes ---------------------------------------------------------------

/// `rel_path -> (mtime, size)` of what's indexed for a folder.
pub fn known_files(conn: &Connection, folder_id: &str) -> rusqlite::Result<HashMap<String, (i64, u64)>> {
    let mut stmt = conn.prepare("SELECT rel_path, mtime, size FROM local_tracks WHERE folder_id = ?1")?;
    let rows = stmt.query_map([folder_id], |row| {
        Ok((row.get::<_, String>(0)?, (row.get::<_, i64>(1)?, row.get::<_, i64>(2)? as u64)))
    })?;
    rows.collect()
}

/// Inserts or updates files in one short transaction. `added_at` keeps the
/// first time a file was seen ("Newest" sorts by it).
pub fn upsert_tracks(conn: &mut Connection, records: &[TrackRecord]) -> rusqlite::Result<()> {
    if records.is_empty() {
        return Ok(());
    }
    let now = now_secs();
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO local_tracks (id, folder_id, rel_path, mtime, size, title, artist,
                album_artist, album, album_id, album_artist_id, artist_id, track, disc, year,
                genre, duration, bitrate, suffix, content_type, art_id, lyrics, mbid,
                rg_track_gain, rg_album_gain, rg_track_peak, rg_album_peak, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)
             ON CONFLICT (id) DO UPDATE SET
                mtime = excluded.mtime, size = excluded.size, title = excluded.title,
                artist = excluded.artist, album_artist = excluded.album_artist,
                album = excluded.album, album_id = excluded.album_id,
                album_artist_id = excluded.album_artist_id, artist_id = excluded.artist_id,
                track = excluded.track, disc = excluded.disc, year = excluded.year,
                genre = excluded.genre, duration = excluded.duration,
                bitrate = excluded.bitrate, suffix = excluded.suffix,
                content_type = excluded.content_type, art_id = excluded.art_id,
                lyrics = excluded.lyrics, mbid = excluded.mbid,
                rg_track_gain = excluded.rg_track_gain, rg_album_gain = excluded.rg_album_gain,
                rg_track_peak = excluded.rg_track_peak, rg_album_peak = excluded.rg_album_peak",
        )?;
        for r in records {
            let gain = r.replay_gain.clone().unwrap_or_default();
            stmt.execute(params![
                r.id,
                r.folder_id,
                r.rel_path,
                r.mtime,
                r.size as i64,
                r.title,
                r.artist,
                r.album_artist,
                r.album,
                r.album_id(),
                r.album_artist_id(),
                r.artist_id(),
                r.track,
                r.disc,
                r.year,
                r.genre,
                r.duration,
                r.bitrate,
                r.suffix,
                r.content_type,
                r.art_id,
                r.lyrics,
                r.mbid,
                gain.track_gain,
                gain.album_gain,
                gain.track_peak,
                gain.album_peak,
                now,
            ])?;
        }
    }
    tx.commit()
}

pub fn delete_files(conn: &mut Connection, folder_id: &str, rel_paths: &[String]) -> rusqlite::Result<()> {
    if rel_paths.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("DELETE FROM local_tracks WHERE folder_id = ?1 AND rel_path = ?2")?;
        for rel in rel_paths {
            stmt.execute(params![folder_id, rel])?;
        }
    }
    tx.commit()
}

pub fn track_count(conn: &Connection) -> rusqlite::Result<usize> {
    conn.query_row("SELECT COUNT(*) FROM local_tracks", [], |row| row.get::<_, i64>(0))
        .map(|n| n as usize)
}

/// Where a song's file is, for the audio route and AVPlayer. The only way
/// from an id to a path.
pub fn file_path(conn: &Connection, song_id: &str) -> rusqlite::Result<Option<PathBuf>> {
    conn.query_row(
        "SELECT f.path, t.rel_path FROM local_tracks t JOIN local_folders f ON f.id = t.folder_id
         WHERE t.id = ?1",
        [song_id],
        |row| Ok(PathBuf::from(row.get::<_, String>(0)?).join(row.get::<_, String>(1)?)),
    )
    .optional()
}

/// A file that carries the picture `art_id`, to re-extract it after the
/// cover cache was cleared.
pub fn art_source(conn: &Connection, art_id: &str) -> rusqlite::Result<Option<PathBuf>> {
    conn.query_row(
        "SELECT f.path, t.rel_path FROM local_tracks t JOIN local_folders f ON f.id = t.folder_id
         WHERE t.art_id = ?1 LIMIT 1",
        [art_id],
        |row| Ok(PathBuf::from(row.get::<_, String>(0)?).join(row.get::<_, String>(1)?)),
    )
    .optional()
}

pub fn embedded_lyrics(conn: &Connection, song_id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT lyrics FROM local_tracks WHERE id = ?1", [song_id], |row| {
        row.get::<_, Option<String>>(0)
    })
    .optional()
    .map(Option::flatten)
}

// ---- Marks ----------------------------------------------------------------------------

/// Which mark row an id belongs to, from its prefix.
fn kind_of(id: &str) -> &'static str {
    if id.starts_with("la-") {
        "album"
    } else if id.starts_with("lr-") {
        "artist"
    } else {
        "song"
    }
}

pub fn set_starred(conn: &Connection, id: &str, on: bool) -> rusqlite::Result<()> {
    let value = on.then(|| iso(now_secs())).flatten();
    conn.execute(
        "INSERT INTO local_marks (kind, id, starred) VALUES (?1, ?2, ?3)
         ON CONFLICT (kind, id) DO UPDATE SET starred = excluded.starred",
        params![kind_of(id), id, value],
    )?;
    Ok(())
}

pub fn set_rating(conn: &Connection, id: &str, rating: u32) -> rusqlite::Result<()> {
    let value = (rating > 0).then_some(rating.min(5));
    conn.execute(
        "INSERT INTO local_marks (kind, id, rating) VALUES (?1, ?2, ?3)
         ON CONFLICT (kind, id) DO UPDATE SET rating = excluded.rating",
        params![kind_of(id), id, value],
    )?;
    Ok(())
}

pub fn record_play(conn: &Connection, song_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO local_marks (kind, id, play_count, played) VALUES ('song', ?1, 1, ?2)
         ON CONFLICT (kind, id) DO UPDATE SET play_count = play_count + 1, played = excluded.played",
        params![song_id, iso(now_secs())],
    )?;
    Ok(())
}

// ---- Songs ----------------------------------------------------------------------------

const SONG_SELECT: &str = "SELECT t.id, t.folder_id, t.rel_path, t.mtime, t.size, t.title, t.artist,
    t.album_artist, t.album, t.track, t.disc, t.year, t.genre, t.duration, t.bitrate, t.suffix,
    t.content_type, t.art_id, m.starred, m.rating, COALESCE(m.play_count, 0), m.played,
    t.rg_track_gain, t.rg_album_gain, t.rg_track_peak, t.rg_album_peak
    FROM local_tracks t LEFT JOIN local_marks m ON m.kind = 'song' AND m.id = t.id";

fn song_from_row(row: &Row) -> rusqlite::Result<Song> {
    let gain = ReplayGain {
        track_gain: row.get(22)?,
        album_gain: row.get(23)?,
        track_peak: row.get(24)?,
        album_peak: row.get(25)?,
        ..Default::default()
    };
    let record = TrackRecord {
        id: row.get(0)?,
        folder_id: row.get(1)?,
        rel_path: row.get(2)?,
        mtime: row.get(3)?,
        size: row.get::<_, i64>(4)? as u64,
        title: row.get(5)?,
        artist: row.get(6)?,
        album_artist: row.get(7)?,
        album: row.get(8)?,
        track: row.get(9)?,
        disc: row.get(10)?,
        year: row.get(11)?,
        genre: row.get(12)?,
        duration: row.get(13)?,
        bitrate: row.get(14)?,
        suffix: row.get(15)?,
        content_type: row.get(16)?,
        art_id: row.get(17)?,
        lyrics: None,
        mbid: None,
        replay_gain: (gain.track_gain.is_some() || gain.album_gain.is_some()).then_some(gain),
    };
    let marks = Marks {
        starred: row.get(18)?,
        rating: row.get(19)?,
        play_count: row.get(20)?,
        played: row.get(21)?,
    };
    Ok(record.to_song(&marks))
}

fn query_songs(conn: &Connection, sql: &str, args: Vec<rusqlite::types::Value>) -> rusqlite::Result<Vec<Song>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(args), song_from_row)?;
    rows.collect()
}

#[cfg(test)]
pub fn song(conn: &Connection, id: &str) -> rusqlite::Result<Option<Song>> {
    Ok(query_songs(conn, &format!("{SONG_SELECT} WHERE t.id = ?1"), vec![id.to_string().into()])?
        .into_iter()
        .next())
}

/// One page of every song, sorted like Navidrome's native song list
/// (`play_date`, `play_count`, `title`, `rating`, `duration`).
pub fn songs_page(
    conn: &Connection,
    sort: &str,
    ascending: bool,
    offset: usize,
    size: usize,
) -> rusqlite::Result<Vec<Song>> {
    let column = match sort {
        "play_date" => "m.played",
        "play_count" => "COALESCE(m.play_count, 0)",
        "rating" => "COALESCE(m.rating, 0)",
        "duration" => "t.duration",
        _ => "t.title COLLATE NOCASE",
    };
    let direction = if ascending { "ASC" } else { "DESC" };
    let sql = format!(
        "{SONG_SELECT} ORDER BY {column} {direction}, t.title COLLATE NOCASE, t.id LIMIT ?1 OFFSET ?2"
    );
    query_songs(conn, &sql, vec![(size as i64).into(), (offset as i64).into()])
}

/// `LIKE` patterns for each word of `query`; every word must match.
fn like_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|word| {
            let escaped = word.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            format!("%{escaped}%")
        })
        .collect()
}

/// `(sql, args)` requiring every term to match one of `columns`.
fn match_clause(columns: &[&str], terms: &[String]) -> (String, Vec<rusqlite::types::Value>) {
    if terms.is_empty() {
        return ("1".to_string(), Vec::new());
    }
    let mut parts = Vec::new();
    let mut args = Vec::new();
    for term in terms {
        let any: Vec<String> = columns.iter().map(|c| format!("{c} LIKE ? ESCAPE '\\'")).collect();
        parts.push(format!("({})", any.join(" OR ")));
        for _ in columns {
            args.push(term.clone().into());
        }
    }
    (parts.join(" AND "), args)
}

pub fn search_songs(conn: &Connection, query: &str, size: u32, offset: u32) -> rusqlite::Result<Vec<Song>> {
    let (clause, mut args) = match_clause(&["t.title", "t.artist", "t.album"], &like_terms(query));
    args.push((size as i64).into());
    args.push((offset as i64).into());
    let sql = format!("{SONG_SELECT} WHERE {clause} ORDER BY t.title COLLATE NOCASE, t.id LIMIT ? OFFSET ?");
    query_songs(conn, &sql, args)
}

/// An artist's most played songs (artist pages' "Top Songs").
pub fn top_songs(conn: &Connection, artist: &str, count: u32) -> rusqlite::Result<Vec<Song>> {
    let sql = format!(
        "{SONG_SELECT} WHERE t.artist = ?1 COLLATE NOCASE OR t.album_artist = ?1 COLLATE NOCASE
         ORDER BY COALESCE(m.play_count, 0) DESC, t.title COLLATE NOCASE LIMIT ?2"
    );
    query_songs(conn, &sql, vec![artist.to_string().into(), (count as i64).into()])
}

// ---- Albums ----------------------------------------------------------------------------

/// Albums grouped from their tracks. Plays come from the songs' marks;
/// favourite and rating from the album's own.
fn album_select(filter: &str) -> String {
    format!(
        "SELECT t.album_id,
            COALESCE(MIN(t.album), '{UNKNOWN_ALBUM}'),
            COALESCE(MIN(COALESCE(t.album_artist, t.artist)), '{UNKNOWN_ARTIST}'),
            MIN(t.album_artist_id),
            MIN(t.art_id),
            COUNT(*),
            COALESCE(SUM(t.duration), 0),
            MAX(t.year),
            MIN(t.genre),
            MAX(am.starred),
            MAX(am.rating),
            COALESCE(SUM(sm.play_count), 0) AS plays,
            MAX(sm.played) AS last_played,
            MIN(t.added_at) AS created
         FROM local_tracks t
         LEFT JOIN local_marks sm ON sm.kind = 'song' AND sm.id = t.id
         LEFT JOIN local_marks am ON am.kind = 'album' AND am.id = t.album_id
         WHERE {filter}
         GROUP BY t.album_id"
    )
}

fn album_from_row(row: &Row) -> rusqlite::Result<Album> {
    let plays: i64 = row.get(11)?;
    Ok(Album {
        id: row.get(0)?,
        name: row.get(1)?,
        artist: row.get(2)?,
        artist_id: row.get(3)?,
        cover_art: row.get(4)?,
        song_count: row.get::<_, i64>(5)? as u32,
        duration: row.get::<_, i64>(6)? as u32,
        year: row.get(7)?,
        genre: row.get(8)?,
        starred: row.get(9)?,
        user_rating: row.get(10)?,
        play_count: Some(plays as u32),
        played: row.get(12)?,
        created: iso(row.get(13)?),
        server_id: LOCAL_SOURCE_ID.to_string(),
    })
}

fn query_albums(conn: &Connection, sql: &str, args: Vec<rusqlite::types::Value>) -> rusqlite::Result<Vec<Album>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(args), album_from_row)?;
    rows.collect()
}

/// One page of albums, with the same list types as `getAlbumList2`
/// (`recent`, `frequent`, `newest`, `alphabeticalByName`, `random`), or the
/// albums of a genre.
pub fn albums_page(
    conn: &Connection,
    list_type: &str,
    genre: Option<&str>,
    size: u32,
    offset: u32,
) -> rusqlite::Result<Vec<Album>> {
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    let filter = match genre.filter(|g| !g.trim().is_empty()) {
        Some(genre) => {
            args.push(genre.to_string().into());
            "t.genre = ? COLLATE NOCASE"
        }
        None => "1",
    };
    let (having, order) = match (genre.is_some(), list_type) {
        (true, _) | (false, "alphabeticalByName") => ("", "2 COLLATE NOCASE, 3 COLLATE NOCASE"),
        (false, "recent") => ("HAVING last_played IS NOT NULL", "last_played DESC"),
        (false, "frequent") => ("HAVING plays > 0", "plays DESC"),
        (false, "newest") => ("", "created DESC, 2 COLLATE NOCASE"),
        (false, "random") => ("", "RANDOM()"),
        (false, "starred") => ("HAVING MAX(am.starred) IS NOT NULL", "2 COLLATE NOCASE"),
        _ => ("", "2 COLLATE NOCASE"),
    };
    args.push((size as i64).into());
    args.push((offset as i64).into());
    let sql = format!("{} {having} ORDER BY {order} LIMIT ? OFFSET ?", album_select(filter));
    query_albums(conn, &sql, args)
}

pub fn search_albums(conn: &Connection, query: &str, size: u32, offset: u32) -> rusqlite::Result<Vec<Album>> {
    let (clause, mut args) = match_clause(&["t.album", "t.album_artist", "t.artist"], &like_terms(query));
    args.push((size as i64).into());
    args.push((offset as i64).into());
    let sql = format!("{} ORDER BY 2 COLLATE NOCASE LIMIT ? OFFSET ?", album_select(&clause));
    query_albums(conn, &sql, args)
}

/// An album and its tracks in disc and track order.
pub fn album(conn: &Connection, id: &str) -> rusqlite::Result<Option<(Album, Vec<Song>)>> {
    let sql = album_select("t.album_id = ?");
    let Some(album) = query_albums(conn, &sql, vec![id.to_string().into()])?.into_iter().next() else {
        return Ok(None);
    };
    let songs = query_songs(
        conn,
        &format!(
            "{SONG_SELECT} WHERE t.album_id = ?1
             ORDER BY COALESCE(t.disc, 1), COALESCE(t.track, 0), t.title COLLATE NOCASE"
        ),
        vec![id.to_string().into()],
    )?;
    Ok(Some((album, songs)))
}

// ---- Artists ------------------------------------------------------------------------------

/// Album artists, as Navidrome's artist index lists them.
const ARTIST_SELECT: &str = "SELECT t.album_artist_id,
        COALESCE(MIN(COALESCE(t.album_artist, t.artist)), 'Unknown Artist'),
        COUNT(DISTINCT t.album_id),
        MIN(t.art_id),
        MAX(m.starred)
     FROM local_tracks t
     LEFT JOIN local_marks m ON m.kind = 'artist' AND m.id = t.album_artist_id";

fn artist_from_row(row: &Row) -> rusqlite::Result<Artist> {
    Ok(Artist {
        id: row.get(0)?,
        name: row.get(1)?,
        album_count: row.get::<_, i64>(2)? as u32,
        cover_art: row.get(3)?,
        starred: row.get(4)?,
        server_id: LOCAL_SOURCE_ID.to_string(),
    })
}

fn query_artists(conn: &Connection, sql: &str, args: Vec<rusqlite::types::Value>) -> rusqlite::Result<Vec<Artist>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params_from_iter(args), artist_from_row)?;
    rows.collect()
}

pub fn artists(conn: &Connection) -> rusqlite::Result<Vec<Artist>> {
    query_artists(conn, &format!("{ARTIST_SELECT} GROUP BY t.album_artist_id ORDER BY 2 COLLATE NOCASE"), Vec::new())
}

pub fn search_artists(conn: &Connection, query: &str, size: u32, offset: u32) -> rusqlite::Result<Vec<Artist>> {
    let (clause, mut args) = match_clause(&["COALESCE(t.album_artist, t.artist)"], &like_terms(query));
    args.push((size as i64).into());
    args.push((offset as i64).into());
    let sql = format!(
        "{ARTIST_SELECT} WHERE {clause} GROUP BY t.album_artist_id ORDER BY 2 COLLATE NOCASE LIMIT ? OFFSET ?"
    );
    query_artists(conn, &sql, args)
}

/// An artist and their albums: albums they're the album artist of, plus
/// albums with a track by them (a song's "Go to Artist" uses the track artist).
pub fn artist(conn: &Connection, id: &str) -> rusqlite::Result<Option<(Artist, Vec<Album>)>> {
    let albums = query_albums(
        conn,
        &format!(
            "{} ORDER BY MAX(t.year) DESC, 2 COLLATE NOCASE",
            album_select(
                "t.album_id IN (SELECT album_id FROM local_tracks WHERE album_artist_id = ?1 OR artist_id = ?1)"
            )
        ),
        vec![id.to_string().into()],
    )?;
    let named = conn
        .query_row(
            "SELECT COALESCE(album_artist, artist) FROM local_tracks WHERE album_artist_id = ?1
             UNION ALL SELECT artist FROM local_tracks WHERE artist_id = ?1 LIMIT 1",
            [id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?;
    let Some(name) = named else {
        return Ok(None);
    };
    let starred = conn
        .query_row("SELECT starred FROM local_marks WHERE kind = 'artist' AND id = ?1", [id], |row| {
            row.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten();
    let artist = Artist {
        id: id.to_string(),
        name: name.unwrap_or_else(|| UNKNOWN_ARTIST.to_string()),
        album_count: albums.len() as u32,
        cover_art: albums.iter().find_map(|a| a.cover_art.clone()),
        starred,
        server_id: LOCAL_SOURCE_ID.to_string(),
    };
    Ok(Some((artist, albums)))
}

// ---- Search and favourites ------------------------------------------------------------------

pub fn search(
    conn: &Connection,
    query: &str,
    artist_count: u32,
    album_count: u32,
    song_count: u32,
) -> rusqlite::Result<SearchResult> {
    Ok(SearchResult {
        artists: if artist_count > 0 { search_artists(conn, query, artist_count, 0)? } else { Vec::new() },
        albums: if album_count > 0 { search_albums(conn, query, album_count, 0)? } else { Vec::new() },
        songs: if song_count > 0 { search_songs(conn, query, song_count, 0)? } else { Vec::new() },
    })
}

pub fn starred(conn: &Connection) -> rusqlite::Result<(Vec<Artist>, Vec<Album>, Vec<Song>)> {
    let artists = query_artists(
        conn,
        &format!("{ARTIST_SELECT} GROUP BY t.album_artist_id HAVING MAX(m.starred) IS NOT NULL ORDER BY 2 COLLATE NOCASE"),
        Vec::new(),
    )?;
    let albums = albums_page(conn, "starred", None, 10_000, 0)?;
    let songs = query_songs(
        conn,
        &format!("{SONG_SELECT} WHERE m.starred IS NOT NULL ORDER BY m.starred DESC"),
        Vec::new(),
    )?;
    Ok((artists, albums, songs))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::local_music::model::{content_type_for, song_id};

    pub(crate) fn memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    pub(crate) fn rec(folder: &str, rel: &str, title: &str, artist: &str, album: &str, track: u32) -> TrackRecord {
        TrackRecord {
            id: song_id(folder, rel),
            folder_id: folder.into(),
            rel_path: rel.into(),
            mtime: 1,
            size: 100,
            title: title.into(),
            artist: Some(artist.into()),
            album: Some(album.into()),
            track: Some(track),
            duration: 200,
            suffix: "mp3".into(),
            content_type: content_type_for("mp3").into(),
            ..Default::default()
        }
    }

    fn seeded() -> Connection {
        let mut conn = memory();
        let folder = add_folder(&conn, "/Music").unwrap();
        let mut b2 = rec(&folder.id, "b/2.mp3", "Second", "Band", "Bee", 2);
        b2.art_id = Some("lc-1".into());
        b2.year = Some(2001);
        b2.replay_gain = Some(ReplayGain { track_gain: Some(-6.5), album_peak: Some(0.9), ..Default::default() });
        upsert_tracks(
            &mut conn,
            &[
                b2,
                rec(&folder.id, "b/1.mp3", "First", "Band", "Bee", 1),
                rec(&folder.id, "a/1.mp3", "Alpha", "Solo", "Aye", 1),
            ],
        )
        .unwrap();
        conn
    }

    #[test]
    fn albums_group_tracks_and_sort() {
        let conn = seeded();
        let albums = albums_page(&conn, "alphabeticalByName", None, 10, 0).unwrap();
        assert_eq!(albums.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["Aye", "Bee"]);
        let bee = &albums[1];
        assert_eq!(bee.song_count, 2);
        assert_eq!(bee.duration, 400);
        assert_eq!(bee.cover_art.as_deref(), Some("lc-1"));
        assert_eq!(bee.year, Some(2001));
        assert_eq!(bee.server_id, LOCAL_SOURCE_ID);
        // Nothing played yet: "recent" is empty, like a fresh server.
        assert!(albums_page(&conn, "recent", None, 10, 0).unwrap().is_empty());
        let (album, songs) = album(&conn, &bee.id).unwrap().unwrap();
        assert_eq!(album.name, "Bee");
        assert_eq!(songs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["First", "Second"]);
        assert!(songs[0].replay_gain.is_none());
        let gain = songs[1].replay_gain.clone().unwrap();
        assert_eq!((gain.track_gain, gain.album_peak), (Some(-6.5), Some(0.9)));
        assert!(super::album(&conn, "la-missing").unwrap().is_none());
    }

    #[test]
    fn plays_and_marks_show_up() {
        let conn = seeded();
        let songs = songs_page(&conn, "title", true, 0, 10).unwrap();
        let first = songs.iter().find(|s| s.title == "First").unwrap().clone();
        record_play(&conn, &first.id).unwrap();
        record_play(&conn, &first.id).unwrap();
        set_starred(&conn, &first.id, true).unwrap();
        set_rating(&conn, &first.id, 4).unwrap();
        let again = song(&conn, &first.id).unwrap().unwrap();
        assert_eq!(again.play_count, Some(2));
        assert!(again.starred.is_some());
        assert_eq!(again.user_rating, Some(4));
        let recent = albums_page(&conn, "recent", None, 10, 0).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].play_count, Some(2));
        let album_id = first.album_id.clone().unwrap();
        set_starred(&conn, &album_id, true).unwrap();
        let (_, albums, songs) = starred(&conn).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(songs.len(), 1);
        set_starred(&conn, &first.id, false).unwrap();
        set_rating(&conn, &first.id, 0).unwrap();
        let cleared = song(&conn, &first.id).unwrap().unwrap();
        assert!(cleared.starred.is_none());
        assert_eq!(cleared.user_rating, None);
        // Most played first.
        let by_plays = songs_page(&conn, "play_count", false, 0, 1).unwrap();
        assert_eq!(by_plays[0].id, first.id);
    }

    #[test]
    fn artists_and_search() {
        let conn = seeded();
        let list = artists(&conn).unwrap();
        assert_eq!(list.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), vec!["Band", "Solo"]);
        let (artist, albums) = artist(&conn, &list[0].id).unwrap().unwrap();
        assert_eq!(artist.name, "Band");
        assert_eq!(albums.len(), 1);
        let found = search(&conn, "band sec", 5, 5, 5).unwrap();
        assert_eq!(found.songs.len(), 1);
        assert_eq!(found.songs[0].title, "Second");
        assert!(found.artists.is_empty());
        let found = search(&conn, "aye", 5, 5, 5).unwrap();
        assert_eq!(found.albums.len(), 1);
        // LIKE wildcards in the query are literal.
        assert!(search(&conn, "%", 5, 5, 5).unwrap().songs.is_empty());
        assert_eq!(search_songs(&conn, "", 2, 0).unwrap().len(), 2);
    }

    #[test]
    fn removing_a_folder_forgets_its_files_but_keeps_marks() {
        let mut conn = seeded();
        let folder = folders(&conn).unwrap().remove(0);
        let id = songs_page(&conn, "title", true, 0, 1).unwrap()[0].id.clone();
        set_starred(&conn, &id, true).unwrap();
        assert!(file_path(&conn, &id).unwrap().unwrap().starts_with("/Music"));
        remove_folder(&mut conn, &folder.id).unwrap();
        assert_eq!(track_count(&conn).unwrap(), 0);
        assert!(file_path(&conn, &id).unwrap().is_none());
        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM local_marks WHERE starred IS NOT NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1);
    }
}
