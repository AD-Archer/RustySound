//! Loading play history for Wrapped: every source's songs, most recently
//! played first, paged only as far back as a year needs.
//!
//! - Navidrome and the device's own index sort songs by `play_date`;
//!   Jellyfin by `DatePlayed`. Each page is 500 songs.
//! - Subsonic servers without Navidrome's native API fall back to albums by
//!   last play (`getAlbumList2?type=recent`), so their years have no top songs.
//! - The pages already read are kept in memory for ten minutes, so moving
//!   between years only reads further back. Worked-out years are cached in
//!   memory and in the app cache (past years for a week).

use super::wrapped::{compute_year, year_bounds, years_with_data, Listen, WrappedYear};
use crate::api::discovery::{parse_timestamp, server_call, servers_signature};
use crate::api::{MusicClient, ServerConfig};
use crate::cache_service::{get_json, put_json};
use chrono::{Datelike, Local};
use futures_util::future::join_all;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

const PAGE: usize = 500;
/// Songs read per source at most (40 pages).
const MAX_PER_SOURCE: usize = 20_000;
/// Newest albums read per source when looking for new artists.
const MAX_NEW_ALBUMS: usize = 1_500;
/// Read pages and this year's result go stale after this long.
const FRESH_SECS: i64 = 600;
const CACHE_PREFIX: &str = "view:wrapped:v1:";

/// One source's history, read so far.
#[derive(Clone)]
struct SourceHistory {
    id: String,
    listens: Vec<Listen>,
    next_offset: usize,
    /// Reading albums (no song history on this server).
    albums: bool,
    /// Nothing older to read.
    done: bool,
    /// The oldest last play read.
    oldest: Option<i64>,
}

struct HistoryState {
    read_at: i64,
    sources: Vec<SourceHistory>,
}

static HISTORY: Lazy<Mutex<HashMap<String, HistoryState>>> = Lazy::new(|| Mutex::new(HashMap::new()));
/// Worked-out years: (when, result).
static YEARS: Lazy<Mutex<HashMap<String, (i64, WrappedYear)>>> = Lazy::new(|| Mutex::new(HashMap::new()));
/// Albums added to the library per year, as `server::album` keys.
static ADDED: Lazy<Mutex<HashMap<String, HashSet<String>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// The active servers, plus On This Device when it has music.
fn signature(servers: &[ServerConfig]) -> String {
    let active: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    let mut sig = servers_signature(&active);
    if crate::local_music::has_music_now() {
        sig.push_str("|local");
    }
    sig
}

/// The current year in local time.
pub fn current_year() -> i32 {
    Local::now().year()
}

async fn read_page(client: &MusicClient, source: &mut SourceHistory) -> Vec<Listen> {
    if !source.albums {
        match server_call(client.get_native_song_page("play_date", false, source.next_offset, PAGE)).await {
            Ok(songs) => {
                source.done = songs.len() < PAGE;
                source.next_offset += songs.len();
                return listens_until_unplayed(source, songs.iter().map(|s| (Listen::from_song(s), s.played.is_some())));
            }
            // Plain Subsonic: no song list sorted by play date.
            Err(_) if source.next_offset == 0 && matches!(client, MusicClient::Subsonic(_)) => {
                source.albums = true;
            }
            Err(_) => {
                source.done = true;
                return Vec::new();
            }
        }
    }
    match server_call(client.get_album_page("recent", None, PAGE as u32, source.next_offset as u32)).await {
        Ok(albums) => {
            source.done = albums.len() < PAGE;
            source.next_offset += albums.len();
            listens_until_unplayed(source, albums.iter().map(|a| (Listen::from_album(a), a.played.is_some())))
        }
        Err(_) => {
            source.done = true;
            Vec::new()
        }
    }
}

/// Keeps the played items of a page. The list is newest first, so the first
/// item without a play date ends the history.
fn listens_until_unplayed(
    source: &mut SourceHistory,
    items: impl Iterator<Item = (Option<Listen>, bool)>,
) -> Vec<Listen> {
    let mut out = Vec::new();
    for (listen, has_date) in items {
        match listen {
            Some(listen) => {
                source.oldest = Some(source.oldest.map_or(listen.last_played, |o| o.min(listen.last_played)));
                out.push(listen);
            }
            None if !has_date => {
                source.done = true;
                break;
            }
            None => {}
        }
    }
    out
}

/// Reads every source back to `until` (epoch seconds), then returns all
/// listens read so far and whether that is everything.
async fn history_until(servers: &[ServerConfig], until: i64) -> (Vec<Listen>, bool) {
    let sig = signature(servers);
    let mut sources = {
        let mut cache = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(&sig) {
            Some(state) if now() - state.read_at < FRESH_SECS => state.sources.clone(),
            _ => {
                cache.remove(&sig);
                Vec::new()
            }
        }
    };
    let clients = MusicClient::sources(servers);
    if sources.is_empty() {
        sources = clients
            .iter()
            .map(|c| SourceHistory {
                id: c.source_id().to_string(),
                listens: Vec::new(),
                next_offset: 0,
                albums: false,
                done: false,
                oldest: None,
            })
            .collect();
    }
    let read = join_all(clients.into_iter().map(|client| {
        let mut source = sources.iter().find(|s| s.id == client.source_id()).cloned();
        async move {
            let source = source.as_mut()?;
            while !source.done
                && source.next_offset < MAX_PER_SOURCE
                && source.oldest.is_none_or(|oldest| oldest >= until)
            {
                let page = read_page(&client, source).await;
                source.listens.extend(page);
            }
            Some(source.clone())
        }
    }))
    .await;
    let sources: Vec<SourceHistory> = read.into_iter().flatten().collect();
    let complete = sources.iter().all(|s| s.done || s.next_offset >= MAX_PER_SOURCE);
    let listens = sources.iter().flat_map(|s| s.listens.iter().cloned()).collect();
    let mut cache = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let read_at = cache.get(&sig).map_or_else(now, |state| state.read_at);
    cache.insert(sig, HistoryState { read_at, sources });
    (listens, complete)
}

/// `server::album` keys of the albums added to the library since `start`.
async fn albums_added_since(servers: &[ServerConfig], start: i64, key: &str) -> HashSet<String> {
    if let Some(found) = ADDED.lock().unwrap_or_else(|e| e.into_inner()).get(key) {
        return found.clone();
    }
    let per_source = join_all(MusicClient::sources(servers).into_iter().map(|client| async move {
        let mut keys = Vec::new();
        let mut offset = 0;
        while offset < MAX_NEW_ALBUMS {
            let Ok(page) = server_call(client.get_album_page("newest", None, PAGE as u32, offset as u32)).await else {
                break;
            };
            let full = page.len() == PAGE;
            offset += page.len();
            let mut older = false;
            for album in page {
                match album.created.as_deref().and_then(parse_timestamp) {
                    Some(created) if created >= start => keys.push(format!("{}::{}", album.server_id, album.id)),
                    _ => older = true,
                }
            }
            if older || !full {
                break;
            }
        }
        keys
    }))
    .await;
    let found: HashSet<String> = per_source.into_iter().flatten().collect();
    ADDED.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string(), found.clone());
    found
}

fn cached_year(key: &str, current: bool) -> Option<WrappedYear> {
    let memo = YEARS.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned();
    match memo {
        Some((at, year)) if !current || now() - at < FRESH_SECS => Some(year),
        Some(_) => None,
        // The app cache keeps this year for ten minutes too (see `put_year`).
        None => get_json::<WrappedYear>(&format!("{CACHE_PREFIX}{key}")),
    }
}

fn put_year(key: &str, year: &WrappedYear) {
    YEARS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.to_string(), (now(), year.clone()));
    // put_json's expiry is in hours: skip the disk for the current year.
    if !year.so_far {
        let _ = put_json(format!("{CACHE_PREFIX}{key}"), year, Some(24 * 7));
    }
}

/// Wrapped for `year` across the active servers and the device. Reads back
/// to the start of the year before (for the comparison). Never fails: with
/// no history the year is empty.
pub async fn load_wrapped(servers: &[ServerConfig], year: i32) -> WrappedYear {
    let sig = signature(servers);
    let key = format!("{sig}:{year}");
    let current = year == current_year();
    if let Some(found) = cached_year(&key, current) {
        return found;
    }
    let tz = Local;
    let (start, _) = year_bounds(year, &tz);
    let (previous_start, _) = year_bounds(year - 1, &tz);
    let (listens, _) = history_until(servers, previous_start).await;
    let added = albums_added_since(servers, start, &format!("{sig}:{year}")).await;
    let now = now();
    let mut wrapped = compute_year(&listens, year, &tz, now, &added);
    let previous = compute_year(&listens, year - 1, &tz, now, &HashSet::new());
    wrapped.previous_minutes = (!previous.is_empty()).then_some(previous.minutes);
    put_year(&key, &wrapped);
    wrapped
}

/// Years with listening, newest first, always starting with this year.
/// Reads the whole history (up to the per-source cap) the first time.
pub async fn wrapped_years(servers: &[ServerConfig]) -> Vec<i32> {
    let (listens, _) = history_until(servers, i64::MIN).await;
    let mut years = years_with_data(&listens, &Local);
    let this_year = current_year();
    years.retain(|y| *y < this_year);
    years.insert(0, this_year);
    years
}
