//! Home's library-first shelves: Recently Played (the Cover Flow stage),
//! Playlists plus mixes, Albums you might like and New in your library.
//! "Songs you might like" rides on [`super::load_discover_mixes`].
//!
//! Every loader is a cheap read of data the other sections already fetch
//! (the shared library snapshot, the mixes' similar-songs lists, the client's
//! cached artist calls), plus the few requests listed on each function.

use super::http::server_call;
use super::library::{pick_similar_seeds, top_artists, LibrarySnapshot};
use super::mixes::HEAVY_PLAYS;
use super::reasons::{because_reason, relative_ago};
use super::types::{DiscoverAlbum, DiscoverSong, HomePlaylist, Mix, ReasonKind};
use super::util::{normalize_name, parse_timestamp};
use super::{cache, load_discover_mixes, offserver, stage, util, LoadContext};
use crate::api::models::{Album, Playlist, ServerConfig};
use crate::api::navidrome::NavidromeClient;
use crate::db::AppSettings;
use futures_util::future::join_all;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Recently played albums fetched per server (and shown at most).
pub const RECENT_LIMIT: usize = 24;
/// Recently played is live data: reuse a fetch for this long, so hopping
/// between screens doesn't refetch, but a new play shows up soon after.
const RECENT_TTL_SECS: i64 = 60;
/// "Albums you might like": cap and the `getArtist` calls it may make.
pub const ALBUMS_LIKE_SIZE: usize = 12;
pub const ALBUMS_LIKE_ARTISTS: usize = 6;
/// In-library similar artists tried per seed artist.
const ALBUMS_LIKE_PER_SEED: usize = 3;
/// "New in your library" shelf size.
pub const NEW_LIMIT: usize = 20;

static RECENT_MEMO: Lazy<Mutex<HashMap<String, (i64, Vec<DiscoverAlbum>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Forget the recently-played fetch (explicit refresh).
pub(super) fn clear_recent_memo() {
    RECENT_MEMO.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn album_key(album: &Album) -> String {
    format!("{}::{}", album.server_id, album.id)
}

/// "Played today", "Played 3 days ago"; "Recently played" when the server
/// doesn't say when.
pub fn played_reason(now: i64, played: Option<i64>) -> String {
    match played {
        Some(then) => format!("Played {}", relative_ago(now, then)),
        None => "Recently played".to_string(),
    }
}

/// Merge each server's `getAlbumList2?type=recent` into one list, most recent
/// first (albums without a `played` date keep their server order, after the
/// dated ones), without duplicates. Pure.
pub fn recently_played(per_server: Vec<Vec<Album>>, now: i64, limit: usize) -> Vec<DiscoverAlbum> {
    let longest = per_server.iter().map(Vec::len).max().unwrap_or(0);
    let mut merged: Vec<(usize, Album, Option<i64>)> = Vec::new();
    let mut seen = HashSet::new();
    for i in 0..longest {
        for list in &per_server {
            if let Some(album) = list.get(i) {
                if seen.insert(album_key(album)) {
                    let played = album
                        .played
                        .as_deref()
                        .and_then(parse_timestamp)
                        .filter(|t| *t > 946_684_800);
                    merged.push((merged.len(), album.clone(), played));
                }
            }
        }
    }
    // Stable: dated first, newest first; undated after, in merged order.
    merged.sort_by(|a, b| match (a.2, b.2) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.0.cmp(&b.0),
    });
    merged
        .into_iter()
        .take(limit)
        .map(|(_, album, played)| DiscoverAlbum {
            reason: played_reason(now, played),
            album,
            kind: ReasonKind::RecentlyPlayed,
        })
        .collect()
}

/// Recently played albums for the Cover Flow stage.
///
/// Server requests: 1 `getAlbumList2?type=recent` per server, fetched fresh
/// (the client's 6-hour list cache is skipped) and reused for a minute.
/// Empty when nothing has been played or no server answers.
pub async fn load_recently_played(servers: &[ServerConfig], settings: &AppSettings) -> Vec<DiscoverAlbum> {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return Vec::new();
    };
    let now = util::now_epoch_secs();
    {
        let memo = RECENT_MEMO.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, albums)) = memo.get(&ctx.servers_sig) {
            if now - at < RECENT_TTL_SECS {
                return albums.clone();
            }
        }
    }
    let offline = crate::cache_service::is_offline_mode();
    let lists = join_all(ctx.servers.iter().cloned().map(|server| async move {
        if !offline {
            let _ = crate::cache_service::remove_by_prefix(&format!(
                "api:getAlbumList2:v1:{}:recent:",
                server.id
            ));
        }
        server_call(NavidromeClient::new(server).get_albums("recent", RECENT_LIMIT as u32, 0)).await
    }))
    .await;
    let any_ok = lists.iter().any(Result::is_ok);
    let albums = recently_played(
        lists.into_iter().map(Result::unwrap_or_default).collect(),
        now,
        RECENT_LIMIT,
    );
    if any_ok {
        RECENT_MEMO
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ctx.servers_sig.clone(), (now, albums.clone()));
    }
    albums
}

/// Mixes first (fresh each day), then the user's playlists, most recently
/// changed first. Pure.
pub fn combine_playlists(mixes: &[Mix], playlists: Vec<Playlist>) -> Vec<HomePlaylist> {
    let mut playlists = playlists;
    let changed = |p: &Playlist| {
        p.changed
            .as_deref()
            .or(p.created.as_deref())
            .and_then(parse_timestamp)
            .unwrap_or(0)
    };
    playlists.sort_by(|a, b| {
        changed(b)
            .cmp(&changed(a))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    mixes
        .iter()
        .cloned()
        .map(HomePlaylist::Mix)
        .chain(playlists.into_iter().map(HomePlaylist::Playlist))
        .collect()
}

/// The Playlists shelf: generated mixes and the user's server playlists.
///
/// Server requests: 1 `getPlaylists` per server (cached by the client and
/// dropped when a playlist is saved), plus whatever the mixes need (shared
/// and cached; see [`load_discover_mixes`]). A server that fails only
/// drops its playlists.
pub async fn load_home_playlists(servers: &[ServerConfig], settings: &AppSettings) -> Vec<HomePlaylist> {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return Vec::new();
    };
    let mixes = load_discover_mixes(servers, settings).await;
    let lists = join_all(ctx.servers.iter().cloned().map(|server| async move {
        server_call(NavidromeClient::new(server).get_playlists())
            .await
            .unwrap_or_default()
    }))
    .await;
    combine_playlists(&mixes.mixes, lists.into_iter().flatten().collect())
}

/// "Songs you might like" (from the mixes' similar-songs requests; free once
/// the mixes have loaded).
pub async fn load_songs_you_might_like(servers: &[ServerConfig], settings: &AppSettings) -> Vec<DiscoverSong> {
    load_discover_mixes(servers, settings).await.songs_like
}

/// "New in your library": newest albums with an "Added …" reason. Reads the
/// shared snapshot only (no requests once any other section has loaded).
pub async fn load_new_in_library(servers: &[ServerConfig], settings: &AppSettings) -> Vec<DiscoverAlbum> {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return Vec::new();
    };
    let snapshot = ctx.snapshot().await;
    stage::new_in_library(&snapshot.newest, ctx.now, NEW_LIMIT)
}

/// Albums the user already plays a lot (frequent list, ≥ [`HEAVY_PLAYS`]).
fn heavy_albums(snapshot: &LibrarySnapshot) -> HashSet<String> {
    snapshot
        .frequent
        .iter()
        .filter(|a| a.play_count.unwrap_or(0) >= HEAVY_PLAYS)
        .map(album_key)
        .collect()
}

/// Albums of the "Songs you might like" picks, one per album, skipping
/// albums in `skip` (already played a lot, or already picked). The reason
/// names the artist the song was found through. Pure.
pub fn albums_from_songs(songs: &[DiscoverSong], skip: &HashSet<String>) -> Vec<DiscoverAlbum> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for pick in songs {
        let song = &pick.song;
        let (Some(album_id), Some(name)) = (song.album_id.as_ref(), song.album.as_ref()) else {
            continue;
        };
        if album_id.trim().is_empty() || name.trim().is_empty() {
            continue;
        }
        let key = format!("{}::{album_id}", song.server_id);
        if skip.contains(&key) || !seen.insert(key) {
            continue;
        }
        let Some(because) = pick.because_artist.as_deref() else {
            continue;
        };
        out.push(DiscoverAlbum {
            album: Album {
                id: album_id.clone(),
                name: name.clone(),
                artist: song.artist.clone().unwrap_or_default(),
                artist_id: song.artist_id.clone(),
                cover_art: song.cover_art.clone(),
                year: song.year,
                genre: song.genre.clone(),
                server_id: song.server_id.clone(),
                ..Default::default()
            },
            reason: because_reason(because),
            kind: ReasonKind::BecauseYouPlay,
        });
    }
    out
}

/// "Because you play" picks first, then albums from the song picks; one
/// album per artist where possible, no duplicates, at most `limit`. Pure.
pub fn merge_album_picks(because: Vec<DiscoverAlbum>, from_songs: Vec<DiscoverAlbum>, limit: usize) -> Vec<DiscoverAlbum> {
    let mut seen = HashSet::new();
    let mut artists = HashSet::new();
    let mut out = Vec::new();
    let mut spare = Vec::new();
    for pick in because.into_iter().chain(from_songs) {
        if !seen.insert(album_key(&pick.album)) {
            continue;
        }
        if artists.insert(normalize_name(&pick.album.artist)) {
            out.push(pick);
        } else {
            spare.push(pick);
        }
    }
    out.extend(spare);
    out.truncate(limit);
    out
}

/// "Albums you might like": in-library albums by artists similar to the ones
/// the user plays, the least-played album of each first ("Because you play
/// X"), then albums of the "Songs you might like" picks. Albums the user
/// already plays a lot are left out.
///
/// Server requests (cold): ≤ 2 `getArtistInfo2` (shared with the off-server
/// shelf) and ≤ [`ALBUMS_LIKE_ARTISTS`] `getArtist`, all cached by the
/// client; plus the mixes' requests (shared). Cached for the day.
pub async fn load_albums_you_might_like(servers: &[ServerConfig], settings: &AppSettings) -> Vec<DiscoverAlbum> {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return Vec::new();
    };
    let key = ctx.section_key("albums-like");
    if let Some(cached) = cache::get::<Vec<DiscoverAlbum>>(&key) {
        return cached;
    }
    let snapshot = ctx.snapshot().await;
    let heavy = heavy_albums(&snapshot);
    let favourites: HashSet<String> = top_artists(&snapshot)
        .iter()
        .take(10)
        .map(|a| normalize_name(&a.name))
        .collect();

    // Seed → its in-library similar artists (not favourites), ≤ 3 each.
    let seeds = pick_similar_seeds(&snapshot, ctx.seed("similar"));
    let similar = join_all(seeds.iter().cloned().map(|seed| {
        let server = ctx.server(&seed.server_id);
        let favourites = &favourites;
        async move {
            let Some(server) = server else {
                return Vec::new();
            };
            let Ok(info) = server_call(NavidromeClient::new(server).get_artist_info2(
                &seed.artist_id,
                offserver::SIMILAR_COUNT,
                true,
            ))
            .await
            else {
                return Vec::new();
            };
            info.similar
                .iter()
                .filter(|a| a.in_library() && !favourites.contains(&normalize_name(&a.name)))
                .filter_map(|a| a.id.clone().map(|id| (seed.clone(), id)))
                .take(ALBUMS_LIKE_PER_SEED)
                .collect::<Vec<_>>()
        }
    }))
    .await;
    // Round-robin the seeds, so both get a turn before the cap.
    let longest = similar.iter().map(Vec::len).max().unwrap_or(0);
    let mut artists = Vec::new();
    for i in 0..longest {
        for list in &similar {
            if let Some(entry) = list.get(i) {
                artists.push(entry.clone());
            }
        }
    }
    artists.truncate(ALBUMS_LIKE_ARTISTS);
    let because = join_all(artists.into_iter().map(|(seed, artist_id)| {
        let server = ctx.server(&seed.server_id);
        let heavy = &heavy;
        async move {
            let (_, albums) = server_call(NavidromeClient::new(server?).get_artist(&artist_id))
                .await
                .ok()?;
            let fresh: Vec<Album> = albums
                .into_iter()
                .filter(|a| !heavy.contains(&album_key(a)))
                .collect();
            stage::pick_because_album(&fresh, &seed.name)
        }
    }))
    .await;

    let songs = load_discover_mixes(servers, settings).await.songs_like;
    let from_songs = albums_from_songs(&songs, &heavy);
    let picks = merge_album_picks(because.into_iter().flatten().collect(), from_songs, ALBUMS_LIKE_SIZE);
    if snapshot.servers_ok > 0 {
        cache::put(&key, &picks);
    }
    picks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Song;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_793_232_000;

    fn iso(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
    }

    fn album(server: &str, id: &str, artist: &str, played_days_ago: Option<i64>) -> Album {
        Album {
            id: id.into(),
            name: format!("Album {id}"),
            artist: artist.into(),
            server_id: server.into(),
            played: played_days_ago.map(|d| iso(NOW - d * DAY)),
            ..Default::default()
        }
    }

    #[test]
    fn recent_merges_servers_newest_first() {
        let s1 = vec![album("s1", "a", "A", Some(0)), album("s1", "b", "B", Some(10))];
        let s2 = vec![
            album("s2", "c", "C", Some(3)),
            album("s2", "d", "D", None),
            album("s2", "c", "C", Some(3)),
        ];
        let picks = recently_played(vec![s1, s2], NOW, 10);
        let ids: Vec<&str> = picks.iter().map(|p| p.album.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "c", "b", "d"], "dated newest first, undated last, no repeats");
        assert_eq!(picks[0].reason, "Played today");
        assert_eq!(picks[1].reason, "Played 3 days ago");
        assert_eq!(picks[3].reason, "Recently played");
        assert!(picks.iter().all(|p| p.kind == ReasonKind::RecentlyPlayed));
        assert_eq!(recently_played(vec![], NOW, 10), vec![]);
        let one = recently_played(vec![vec![album("s1", "a", "A", Some(1))]], NOW, 1);
        assert_eq!(one[0].reason, "Played yesterday");
    }

    #[test]
    fn epoch_played_dates_count_as_unknown() {
        let mut a = album("s1", "a", "A", None);
        a.played = Some("1970-01-01T00:00:00Z".into());
        assert_eq!(recently_played(vec![vec![a]], NOW, 5)[0].reason, "Recently played");
    }

    fn playlist(id: &str, name: &str, changed_days_ago: Option<i64>) -> Playlist {
        Playlist {
            id: id.into(),
            name: name.into(),
            server_id: "s1".into(),
            changed: changed_days_ago.map(|d| iso(NOW - d * DAY)),
            ..Default::default()
        }
    }

    fn mix(id: &str) -> Mix {
        Mix {
            id: id.into(),
            kind: super::super::types::MixKind::Discovery,
            name: format!("Mix {id}"),
            blurb: String::new(),
            songs: vec![],
            covers: vec![],
            song_count: 0,
            total_seconds: 0,
            length_label: String::new(),
            freshness: String::new(),
            generated_on: "2026-10-29".into(),
        }
    }

    #[test]
    fn playlists_follow_mixes_most_recent_first() {
        let combined = combine_playlists(
            &[mix("discovery"), mix("deep-cuts")],
            vec![
                playlist("old", "Old", Some(300)),
                playlist("new", "New", Some(1)),
                playlist("undated-b", "beta", None),
                playlist("undated-a", "Alpha", None),
            ],
        );
        let keys: Vec<String> = combined.iter().map(HomePlaylist::key).collect();
        assert_eq!(
            keys,
            vec![
                "mix:discovery",
                "mix:deep-cuts",
                "playlist:s1:new",
                "playlist:s1:old",
                "playlist:s1:undated-a",
                "playlist:s1:undated-b",
            ]
        );
        assert_eq!(combined[2].name(), "New");
        assert!(combine_playlists(&[], vec![]).is_empty());
    }

    fn song_pick(id: &str, album_id: &str, artist: &str, because: &str) -> DiscoverSong {
        DiscoverSong {
            song: Song {
                id: id.into(),
                title: id.into(),
                album: Some(format!("Album {album_id}")),
                album_id: Some(album_id.into()),
                artist: Some(artist.into()),
                cover_art: Some(format!("al-{album_id}")),
                server_id: "s1".into(),
                ..Default::default()
            },
            reason: format!("Similar to {because}"),
            kind: ReasonKind::SimilarTo,
            because_artist: Some(because.into()),
        }
    }

    #[test]
    fn albums_from_song_picks() {
        let songs = vec![
            song_pick("1", "al1", "Ilse Varga", "Mira Okafor"),
            song_pick("2", "al1", "Ilse Varga", "Mira Okafor"),
            song_pick("3", "al2", "Pell Avenue", "Junie Marlow"),
            song_pick("4", "heavy", "Pell Avenue", "Junie Marlow"),
        ];
        let skip: HashSet<String> = ["s1::heavy".to_string()].into_iter().collect();
        let albums = albums_from_songs(&songs, &skip);
        let ids: Vec<&str> = albums.iter().map(|a| a.album.id.as_str()).collect();
        assert_eq!(ids, vec!["al1", "al2"]);
        assert_eq!(albums[0].reason, "Because you play Mira Okafor");
        assert_eq!(albums[0].album.cover_art.as_deref(), Some("al-al1"));
        assert_eq!(albums[1].album.artist, "Pell Avenue");
    }

    #[test]
    fn album_picks_spread_artists_and_cap() {
        let pick = |id: &str, artist: &str| DiscoverAlbum {
            album: album("s1", id, artist, None),
            reason: String::new(),
            kind: ReasonKind::BecauseYouPlay,
        };
        let merged = merge_album_picks(
            vec![pick("a1", "A"), pick("b1", "B")],
            vec![pick("a2", "A"), pick("a1", "A"), pick("c1", "C")],
            4,
        );
        let ids: Vec<&str> = merged.iter().map(|a| a.album.id.as_str()).collect();
        assert_eq!(ids, vec!["a1", "b1", "c1", "a2"]);
    }

    #[test]
    fn loaders_without_active_servers_are_empty() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let settings = AppSettings::default();
        let servers = vec![ServerConfig {
            id: "off".into(),
            name: "off".into(),
            url: "https://off.example".into(),
            username: "u".into(),
            password: "p".into(),
            active: false,
        }];
        runtime.block_on(async {
            assert!(load_recently_played(&servers, &settings).await.is_empty());
            assert!(load_home_playlists(&servers, &settings).await.is_empty());
            assert!(load_songs_you_might_like(&servers, &settings).await.is_empty());
            assert!(load_albums_you_might_like(&servers, &settings).await.is_empty());
            assert!(load_new_in_library(&servers, &settings).await.is_empty());
        });
    }
}
