//! The library snapshot every Discover section reads: album lists and the
//! most-played songs, merged across all active servers.

use super::cache::{memo_get, memo_put};
use super::http::server_call;
use super::util::normalize_name;
use crate::api::models::{Album, ServerConfig, Song};
use crate::api::navidrome::{NativeSongSortField, NativeSortOrder, NavidromeClient};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Albums per list per server.
const NEWEST_SIZE: u32 = 24;
const FREQUENT_SIZE: u32 = 100;
const RECENT_SIZE: u32 = 30;
/// Most-played songs per server (Navidrome native API).
const TOP_SONGS: usize = 300;

/// Library data for one Discover generation. Server requests: 4 per server
/// (3 album lists + 1 native song list), all cached by the client for hours.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibrarySnapshot {
    /// `getAlbumList2?type=newest`, newest first.
    pub newest: Vec<Album>,
    /// `getAlbumList2?type=frequent`, most played first.
    pub frequent: Vec<Album>,
    /// `getAlbumList2?type=recent`, most recently played first.
    pub recent: Vec<Album>,
    /// Most-played songs (Navidrome native API). Empty on plain Subsonic servers.
    pub top_songs: Vec<Song>,
    /// Servers whose album lists loaded.
    pub servers_ok: usize,
}

/// An artist to seed recommendations from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistSeed {
    pub server_id: String,
    pub artist_id: String,
    pub name: String,
    /// Plays aggregated from the frequent albums and top songs.
    pub plays: u64,
}

fn interleave<T: Clone>(lists: &[Vec<T>]) -> Vec<T> {
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..longest {
        for list in lists {
            if let Some(item) = list.get(i) {
                out.push(item.clone());
            }
        }
    }
    out
}

/// Fetch (or reuse from the in-memory memo) the snapshot for these servers.
pub async fn load_snapshot(servers: &[ServerConfig], memo_key: &str) -> LibrarySnapshot {
    if let Some(snapshot) = memo_get::<LibrarySnapshot>(memo_key) {
        return snapshot;
    }

    let per_server = join_all(servers.iter().cloned().map(|server| async move {
        let client = NavidromeClient::new(server);
        let (newest, frequent, recent, top) = futures_util::join!(
            server_call(client.get_albums("newest", NEWEST_SIZE, 0)),
            server_call(client.get_albums("frequent", FREQUENT_SIZE, 0)),
            server_call(client.get_albums("recent", RECENT_SIZE, 0)),
            server_call(client.get_native_songs(
                NativeSongSortField::PlayCount,
                NativeSortOrder::Desc,
                0,
                TOP_SONGS,
            )),
        );
        let ok = newest.is_ok() || frequent.is_ok() || recent.is_ok();
        (
            newest.unwrap_or_default(),
            frequent.unwrap_or_default(),
            recent.unwrap_or_default(),
            top.unwrap_or_default(),
            ok,
        )
    }))
    .await;

    let mut newest = Vec::new();
    let mut frequent = Vec::new();
    let mut recent = Vec::new();
    let mut top_songs = Vec::new();
    let mut servers_ok = 0;
    for (n, f, r, t, ok) in per_server {
        newest.push(n);
        frequent.push(f);
        recent.push(r);
        top_songs.extend(t);
        if ok {
            servers_ok += 1;
        }
    }

    // Album lists are already ordered per server; interleave to merge fairly.
    let mut newest = interleave(&newest);
    newest.sort_by_key(|album| {
        std::cmp::Reverse(
            album
                .created
                .as_deref()
                .and_then(super::util::parse_timestamp)
                .unwrap_or(0),
        )
    });
    let mut frequent = interleave(&frequent);
    frequent.sort_by_key(|album| std::cmp::Reverse(album.play_count.unwrap_or(0)));
    let recent = interleave(&recent);
    top_songs.sort_by_key(|song| std::cmp::Reverse(song.play_count.unwrap_or(0)));

    let snapshot = LibrarySnapshot {
        newest,
        frequent,
        recent,
        top_songs,
        servers_ok,
    };
    if servers_ok > 0 {
        memo_put(memo_key, &snapshot);
    }
    snapshot
}

fn artist_key(server_id: &str, name: &str) -> String {
    format!("{server_id}::{}", normalize_name(name))
}

/// Artists ranked by plays (frequent albums + top songs). Pure.
pub fn top_artists(snapshot: &LibrarySnapshot) -> Vec<ArtistSeed> {
    let mut by_key: HashMap<String, ArtistSeed> = HashMap::new();
    let mut add = |server_id: &str, artist_id: Option<&String>, name: &str, plays: u64| {
        let (Some(artist_id), false) = (artist_id, name.trim().is_empty()) else {
            return;
        };
        if artist_id.trim().is_empty() {
            return;
        }
        let entry = by_key
            .entry(artist_key(server_id, name))
            .or_insert_with(|| ArtistSeed {
                server_id: server_id.to_string(),
                artist_id: artist_id.clone(),
                name: name.trim().to_string(),
                plays: 0,
            });
        entry.plays += plays;
    };
    for album in &snapshot.frequent {
        add(
            &album.server_id,
            album.artist_id.as_ref(),
            &album.artist,
            u64::from(album.play_count.unwrap_or(0)),
        );
    }
    for song in &snapshot.top_songs {
        if let Some(name) = song.artist.as_deref() {
            add(
                &song.server_id,
                song.artist_id.as_ref(),
                name,
                u64::from(song.play_count.unwrap_or(0)),
            );
        }
    }
    let mut seeds: Vec<ArtistSeed> = by_key.into_values().filter(|s| s.plays > 0).collect();
    seeds.sort_by(|a, b| b.plays.cmp(&a.plays).then_with(|| a.name.cmp(&b.name)));
    seeds
}

/// Artists of recently played albums, most recent first, unique. Pure.
pub fn recent_artists(snapshot: &LibrarySnapshot) -> Vec<ArtistSeed> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for album in &snapshot.recent {
        let Some(artist_id) = album.artist_id.as_ref().filter(|id| !id.trim().is_empty()) else {
            continue;
        };
        if album.artist.trim().is_empty() || is_various_artists(&album.artist) {
            continue;
        }
        if seen.insert(artist_key(&album.server_id, &album.artist)) {
            out.push(ArtistSeed {
                server_id: album.server_id.clone(),
                artist_id: artist_id.clone(),
                name: album.artist.trim().to_string(),
                plays: u64::from(album.play_count.unwrap_or(0)),
            });
        }
    }
    out
}

/// The two top artists whose similar artists feed the stage ("Because you
/// play …") and the off-server section. Drawn from the top 6 by the day's
/// seed, so both sections agree and share cached `getArtistInfo2` calls. Pure.
pub fn pick_similar_seeds(snapshot: &LibrarySnapshot, seed: u64) -> Vec<ArtistSeed> {
    let top: Vec<ArtistSeed> = top_artists(snapshot)
        .into_iter()
        .filter(|a| !is_various_artists(&a.name))
        .collect();
    super::util::SeededRng::new(seed ^ 0x5eed).pick_ordered(&top, 6, 2)
}

/// "Various Artists" and friends make poor seeds.
pub fn is_various_artists(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "variousartists" | "various" | "va" | "unknownartist" | "unknown"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(server: &str, artist: &str, artist_id: &str, plays: u32) -> Album {
        Album {
            id: format!("al-{artist}-{plays}"),
            name: format!("{artist} album"),
            artist: artist.to_string(),
            artist_id: Some(artist_id.to_string()),
            play_count: Some(plays),
            server_id: server.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn ranks_top_artists_by_combined_plays() {
        let snapshot = LibrarySnapshot {
            frequent: vec![
                album("s1", "Mira Okafor", "ar1", 40),
                album("s1", "Junie Marlow", "ar2", 90),
                album("s1", "Mira Okafor", "ar1", 70),
            ],
            top_songs: vec![Song {
                artist: Some("Mira Okafor".into()),
                artist_id: Some("ar1".into()),
                play_count: Some(5),
                server_id: "s1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let seeds = top_artists(&snapshot);
        assert_eq!(seeds[0].name, "Mira Okafor");
        assert_eq!(seeds[0].plays, 115);
        assert_eq!(seeds[1].name, "Junie Marlow");
    }

    #[test]
    fn recent_artists_skip_various_and_dupes() {
        let snapshot = LibrarySnapshot {
            recent: vec![
                album("s1", "Various Artists", "ar0", 1),
                album("s1", "Pell Avenue", "ar3", 1),
                album("s1", "Pell Avenue", "ar3", 2),
                album("s2", "Pell Avenue", "x9", 2),
            ],
            ..Default::default()
        };
        let seeds = recent_artists(&snapshot);
        assert_eq!(seeds.len(), 2, "same name on two servers is two seeds");
        assert!(seeds.iter().all(|s| s.name == "Pell Avenue"));
    }
}
