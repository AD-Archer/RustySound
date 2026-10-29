//! Recommendations from outside the library. Sources, in order:
//! 1. the server's own `getArtistInfo2?includeNotPresent=true` (no config),
//! 2. Last.fm (API key; username optional),
//! 3. ListenBrainz fresh releases (username),
//! then iTunes Search fills in art, previews and links.
//! Every source is optional; failures only shrink the list.

pub mod itunes;
pub mod lastfm;
pub mod listenbrainz;

use super::http::{server_call, Budget};
use super::library::{pick_similar_seeds, ArtistSeed, LibrarySnapshot};
use super::reasons::similar_reason;
use super::types::{
    DiscoveryConfig, DiscoverySource, OffServerItem, OffServerKind, OffServerSection,
    OffServerStatus, SourceStatus,
};
use super::util::{normalize_name, SeededRng};
use crate::api::models::ServerConfig;
use crate::api::navidrome::{ArtistInfo, NavidromeClient};
use futures_util::future::join_all;
use std::collections::HashSet;

/// Max items in the section.
pub const OFF_SERVER_LIMIT: usize = 12;
/// Per-load request caps.
pub const LASTFM_BUDGET: u32 = 6;
pub const LISTENBRAINZ_BUDGET: u32 = 1;
pub const ITUNES_BUDGET: u32 = 8;
/// Server `search3` calls used to confirm a release isn't already in the library.
pub const PRESENCE_CHECK_BUDGET: u32 = 6;
/// Similar artists requested from the server per seed.
pub const SIMILAR_COUNT: u32 = 15;

/// Stable dedupe key for an item.
pub fn item_id(kind: OffServerKind, artist: &str, title: &str) -> String {
    match kind {
        OffServerKind::Artist => format!("artist:{}", normalize_name(artist)),
        OffServerKind::Album => format!("album:{}:{}", normalize_name(artist), normalize_name(title)),
        OffServerKind::Track => format!("track:{}:{}", normalize_name(artist), normalize_name(title)),
    }
}

fn new_item(
    kind: OffServerKind,
    title: &str,
    artist: &str,
    reason: String,
    because: Option<&str>,
    source: DiscoverySource,
) -> OffServerItem {
    OffServerItem {
        id: item_id(kind, artist, title),
        kind,
        title: title.to_string(),
        artist: artist.to_string(),
        reason,
        because_artist: because.map(str::to_string),
        source,
        art_url: None,
        preview_url: None,
        preview_title: None,
        external_url: None,
        release_date: None,
    }
}

/// Off-library similar artists from the server's `getArtistInfo2`. Pure.
pub fn navidrome_items(seed: &ArtistSeed, info: &ArtistInfo) -> Vec<OffServerItem> {
    info.similar
        .iter()
        .filter(|a| !a.in_library())
        .map(|a| {
            new_item(
                OffServerKind::Artist,
                &a.name,
                &a.name,
                similar_reason(&seed.name),
                Some(&seed.name),
                DiscoverySource::Navidrome,
            )
        })
        .collect()
}

/// Last.fm similar artists that aren't in the library. Pure.
pub fn lastfm_artist_items(
    seed: &str,
    artists: &[lastfm::LfmArtist],
    library_artists: &HashSet<String>,
) -> Vec<OffServerItem> {
    artists
        .iter()
        .filter(|a| !library_artists.contains(&normalize_name(&a.name)))
        .map(|a| {
            let mut item = new_item(
                OffServerKind::Artist,
                &a.name,
                &a.name,
                similar_reason(seed),
                Some(seed),
                DiscoverySource::LastFm,
            );
            item.art_url = a.image.clone();
            item.external_url = a.url.clone();
            item
        })
        .collect()
}

/// The top album of an off-library artist, recommended because of `seed`. Pure.
pub fn lastfm_album_item(seed: &str, albums: &[lastfm::LfmAlbum]) -> Option<OffServerItem> {
    let album = albums.first()?;
    let mut item = new_item(
        OffServerKind::Album,
        &album.name,
        &album.artist,
        similar_reason(seed),
        Some(seed),
        DiscoverySource::LastFm,
    );
    item.art_url = album.image.clone();
    item.external_url = album.url.clone();
    Some(item)
}

/// Last.fm similar tracks by off-library artists, one per artist. Pure.
pub fn lastfm_track_items(
    seed_title: &str,
    seed_artist: &str,
    tracks: &[lastfm::LfmTrack],
    library_artists: &HashSet<String>,
) -> Vec<OffServerItem> {
    let mut seen_artists = HashSet::new();
    tracks
        .iter()
        .filter(|t| {
            let key = normalize_name(&t.artist);
            !library_artists.contains(&key) && seen_artists.insert(key)
        })
        .map(|t| {
            let mut item = new_item(
                OffServerKind::Track,
                &t.name,
                &t.artist,
                format!("Similar to {seed_title} by {seed_artist}"),
                Some(seed_artist),
                DiscoverySource::LastFm,
            );
            item.art_url = t.image.clone();
            item.external_url = t.url.clone();
            item
        })
        .collect()
}

/// ListenBrainz fresh releases → album items. Pure.
pub fn fresh_items(releases: &[listenbrainz::FreshRelease]) -> Vec<OffServerItem> {
    releases
        .iter()
        .map(|r| {
            let kind_label = match r.primary_type.as_deref() {
                Some("Album") => "album",
                Some("EP") => "EP",
                Some("Single") => "single",
                _ => "release",
            };
            let mut item = new_item(
                OffServerKind::Album,
                &r.title,
                &r.artist,
                format!("New {kind_label} · You listen to {}", r.artist),
                Some(&r.artist),
                DiscoverySource::ListenBrainz,
            );
            item.art_url = r.cover_url.clone();
            item.external_url = r.external_url();
            item.release_date = r.release_date.clone();
            item
        })
        .collect()
}

/// Round-robin across source lists (in the given priority), dropping
/// duplicates (same kind + names) and capping at `limit`. Pure.
pub fn merge(lists: Vec<Vec<OffServerItem>>, limit: usize) -> Vec<OffServerItem> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let longest = lists.iter().map(Vec::len).max().unwrap_or(0);
    'outer: for i in 0..longest {
        for list in &lists {
            if let Some(item) = list.get(i) {
                if seen.insert(item.id.clone()) {
                    out.push(item.clone());
                    if out.len() >= limit {
                        break 'outer;
                    }
                }
            }
        }
    }
    out
}

/// Normalised names of every artist on every active server (cached 24 h by
/// the client). `None` unless every server answered: callers then skip sources
/// that need the check, rather than recommend something already owned.
async fn library_artist_names(servers: &[ServerConfig]) -> Option<HashSet<String>> {
    if servers.is_empty() {
        return None;
    }
    let results = join_all(servers.iter().cloned().map(|server| async move {
        server_call(NavidromeClient::new(server).get_artists()).await
    }))
    .await;
    let mut names = HashSet::new();
    for artists in results {
        names.extend(artists.ok()?.into_iter().map(|a| normalize_name(&a.name)));
    }
    Some(names)
}

/// True when some server's `search3` finds this album by this artist.
async fn album_in_library(servers: &[ServerConfig], artist: &str, title: &str) -> Result<bool, String> {
    let mut last_error = None;
    let mut any_ok = false;
    for server in servers.iter().cloned() {
        match server_call(NavidromeClient::new(server).search(title, 0, 8, 0)).await {
            Ok(result) => {
                any_ok = true;
                if result.albums.iter().any(|album| {
                    super::util::same_name(&album.name, title)
                        && super::util::same_name(&album.artist, artist)
                }) {
                    return Ok(true);
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    // Only claim "not in your library" when every server answered.
    match (any_ok, last_error) {
        (_, Some(error)) => Err(error),
        (true, None) => Ok(false),
        (false, None) => Err("No server to check".to_string()),
    }
}

/// Load the off-server section. Never fails; see `status` for what ran.
pub async fn load(
    servers: &[ServerConfig],
    snapshot: &LibrarySnapshot,
    config: &DiscoveryConfig,
    seed: u64,
    today: &str,
) -> OffServerSection {
    let mut status = OffServerStatus::default();
    if !config.offserver_enabled || crate::cache_service::is_offline_mode() {
        status.navidrome = SourceStatus::Disabled;
        status.lastfm = SourceStatus::Disabled;
        status.listenbrainz = SourceStatus::Disabled;
        status.itunes = SourceStatus::Disabled;
        return OffServerSection {
            items: Vec::new(),
            status,
            generated_on: today.to_string(),
        };
    }

    let seeds = pick_similar_seeds(snapshot, seed);
    let lastfm_budget = Budget::new(LASTFM_BUDGET);
    let lb_budget = Budget::new(LISTENBRAINZ_BUDGET);

    // 1. Server similar artists (same calls/cached keys as the stage section).
    let navidrome_future = join_all(seeds.iter().cloned().map(|seed| {
        let server = servers.iter().find(|s| s.id == seed.server_id).cloned();
        async move {
            let server = server.ok_or_else(|| "Server not active".to_string())?;
            let info = server_call(
                NavidromeClient::new(server).get_artist_info2(&seed.artist_id, SIMILAR_COUNT, true),
            )
            .await?;
            Ok::<_, String>(navidrome_items(&seed, &info))
        }
    }));

    // Artist names across all servers (cached 24 h by the client). Needed to
    // filter Last.fm results, and server-similar artists when several servers
    // are active (server A may not have an artist that server B does).
    let library = if config.lastfm_api_key.is_some() || servers.len() > 1 {
        library_artist_names(servers).await
    } else {
        None
    };

    // 2. Last.fm.
    let lastfm_future = load_lastfm(snapshot, config, library.as_ref(), &seeds, seed, &lastfm_budget);

    // 3. ListenBrainz.
    let lb_future = async {
        match config.listenbrainz_username.as_deref() {
            Some(user) => Some(listenbrainz::fresh_releases(user, &lb_budget).await),
            None => None,
        }
    };

    let (navidrome_results, (lastfm_lists, lastfm_status), lb_result) =
        futures_util::join!(navidrome_future, lastfm_future, lb_future);

    // Navidrome status.
    let mut navidrome_items_all = Vec::new();
    let mut navidrome_error = None;
    for result in navidrome_results {
        match result {
            Ok(items) => navidrome_items_all.extend(items),
            Err(error) => navidrome_error = Some(error),
        }
    }
    if servers.len() > 1 {
        match &library {
            Some(names) => {
                navidrome_items_all.retain(|item| !names.contains(&normalize_name(&item.artist)))
            }
            // Can't confirm across servers: don't recommend what may be owned.
            None => navidrome_items_all.clear(),
        }
    }
    status.navidrome = match (seeds.is_empty(), navidrome_error) {
        (true, _) => SourceStatus::Ok(0),
        (false, Some(error)) if navidrome_items_all.is_empty() => SourceStatus::Failed(error),
        _ => SourceStatus::Ok(navidrome_items_all.len()),
    };
    status.lastfm = lastfm_status;

    // ListenBrainz: confirm each candidate isn't already in the library.
    let mut fresh = Vec::new();
    status.listenbrainz = match lb_result {
        None => SourceStatus::NotConfigured,
        Some(Err(error)) => SourceStatus::Failed(error),
        Some(Ok(releases)) => {
            let known: HashSet<String> = snapshot
                .newest
                .iter()
                .chain(snapshot.frequent.iter())
                .chain(snapshot.recent.iter())
                .map(|a| item_id(OffServerKind::Album, &a.artist, &a.name))
                .collect();
            let candidates: Vec<_> = listenbrainz::select_fresh(releases, today)
                .into_iter()
                .filter(|r| !known.contains(&item_id(OffServerKind::Album, &r.artist, &r.title)))
                .take(PRESENCE_CHECK_BUDGET as usize)
                .collect();
            let checks = join_all(
                candidates
                    .iter()
                    .map(|r| album_in_library(servers, &r.artist, &r.title)),
            )
            .await;
            let confirmed: Vec<_> = candidates
                .into_iter()
                .zip(checks)
                .filter_map(|(release, owned)| matches!(owned, Ok(false)).then_some(release))
                .collect();
            fresh = fresh_items(&confirmed);
            SourceStatus::Ok(fresh.len())
        }
    };

    let mut items = merge(
        vec![
            fresh,
            lastfm_lists.albums,
            lastfm_lists.tracks,
            navidrome_items_all,
            lastfm_lists.artists,
        ],
        OFF_SERVER_LIMIT,
    );

    // 4. iTunes: art, previews, links for the first items that need them.
    let itunes_budget = Budget::new(ITUNES_BUDGET);
    let needs: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.art_url.is_none()
                || item.external_url.is_none()
                || (config.previews_enabled && item.preview_url.is_none())
        })
        .map(|(index, _)| index)
        .take(ITUNES_BUDGET as usize)
        .collect();
    let mut enriched: Vec<OffServerItem> = needs.iter().map(|&i| items[i].clone()).collect();
    let results = join_all(
        enriched
            .iter_mut()
            .map(|item| itunes::enrich(item, config.previews_enabled, &itunes_budget)),
    )
    .await;
    let matched = results.iter().filter(|r| matches!(r, Ok(true))).count();
    status.itunes = if needs.is_empty() {
        SourceStatus::Ok(0)
    } else if results.iter().all(Result::is_err) {
        SourceStatus::Failed(
            results
                .into_iter()
                .find_map(Result::err)
                .unwrap_or_else(|| "iTunes unavailable".to_string()),
        )
    } else {
        SourceStatus::Ok(matched)
    };
    for (index, item) in needs.into_iter().zip(enriched) {
        items[index] = item;
    }

    status.external_requests = lastfm_budget.used() + lb_budget.used() + itunes_budget.used();
    OffServerSection {
        items,
        status,
        generated_on: today.to_string(),
    }
}

#[derive(Default)]
struct LastFmLists {
    artists: Vec<OffServerItem>,
    albums: Vec<OffServerItem>,
    tracks: Vec<OffServerItem>,
}

async fn load_lastfm(
    snapshot: &LibrarySnapshot,
    config: &DiscoveryConfig,
    library: Option<&HashSet<String>>,
    library_seeds: &[ArtistSeed],
    seed: u64,
    budget: &Budget,
) -> (LastFmLists, SourceStatus) {
    let Some(key) = config.lastfm_api_key.as_deref() else {
        return (LastFmLists::default(), SourceStatus::NotConfigured);
    };
    let Some(library) = library else {
        return (
            LastFmLists::default(),
            SourceStatus::Failed("Couldn't read your library's artists".to_string()),
        );
    };

    // Seed artists: the user's Last.fm top artists when a username is set,
    // otherwise the library's top artists.
    let mut rng = SeededRng::new(seed ^ 0x1a5f);
    let seed_names: Vec<String> = match config.lastfm_username.as_deref() {
        Some(user) => match lastfm::top_artists(key, user, 10, budget).await {
            Ok(top) if !top.is_empty() => {
                let names: Vec<String> = top.into_iter().map(|a| a.name).collect();
                rng.pick_ordered(&names, 6, 2)
            }
            Ok(_) => library_seeds.iter().map(|s| s.name.clone()).collect(),
            Err(error) => return (LastFmLists::default(), SourceStatus::Failed(error)),
        },
        None => library_seeds.iter().map(|s| s.name.clone()).collect(),
    };

    // A seed track for track.getSimilar: one of the 10 most played songs.
    let seed_track = {
        let pool: Vec<_> = snapshot
            .top_songs
            .iter()
            .filter(|s| s.artist.is_some())
            .take(10)
            .collect();
        (!pool.is_empty()).then(|| pool[rng.below(pool.len())].clone())
    };

    let similar_future = join_all(
        seed_names
            .iter()
            .map(|name| lastfm::similar_artists(key, name, 20, budget)),
    );
    let track_future = async {
        match &seed_track {
            Some(song) => Some(
                lastfm::similar_tracks(
                    key,
                    song.artist.as_deref().unwrap_or_default(),
                    &song.title,
                    20,
                    budget,
                )
                .await,
            ),
            None => None,
        }
    };
    let (similar_results, track_result) = futures_util::join!(similar_future, track_future);

    let mut lists = LastFmLists::default();
    let mut first_error = None;
    let mut album_seeds: Vec<(String, String)> = Vec::new(); // (seed, off-library artist)
    for (seed_name, result) in seed_names.iter().zip(similar_results) {
        match result {
            Ok(artists) => {
                let items = lastfm_artist_items(seed_name, &artists, &library);
                if let Some(first) = items.first() {
                    album_seeds.push((seed_name.clone(), first.artist.clone()));
                }
                lists.artists.extend(items);
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    if let (Some(song), Some(result)) = (&seed_track, track_result) {
        match result {
            Ok(tracks) => {
                lists.tracks = lastfm_track_items(
                    &song.title,
                    song.artist.as_deref().unwrap_or_default(),
                    &tracks,
                    &library,
                );
            }
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }

    let album_results = join_all(
        album_seeds
            .iter()
            .map(|(_, artist)| lastfm::top_albums(key, artist, 3, budget)),
    )
    .await;
    for ((seed_name, _), result) in album_seeds.iter().zip(album_results) {
        if let Ok(albums) = result {
            lists.albums.extend(lastfm_album_item(seed_name, &albums));
        }
    }

    let contributed = lists.artists.len() + lists.albums.len() + lists.tracks.len();
    let status = match first_error {
        Some(error) if contributed == 0 => SourceStatus::Failed(error),
        _ => SourceStatus::Ok(contributed),
    };
    (lists, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::navidrome::{artist_info_from_response, SubsonicResponse};

    fn seed(name: &str) -> ArtistSeed {
        ArtistSeed {
            server_id: "s1".into(),
            artist_id: "ar-1".into(),
            name: name.into(),
            plays: 10,
        }
    }

    #[test]
    fn navidrome_fixture_yields_only_off_library_artists() {
        let json: SubsonicResponse =
            serde_json::from_str(include_str!("../fixtures/navidrome_getartistinfo2.json")).unwrap();
        let info = artist_info_from_response("s1", json).unwrap();
        assert_eq!(info.similar.len(), 3, "blank names dropped");
        assert!(info.similar[0].in_library());
        assert_eq!(info.similar[0].cover_art.as_deref(), Some("ar-2"), "cache-bust suffix stripped");
        assert!(!info.similar[1].in_library());
        assert!(info.similar[1].image_url.is_none(), "broken off-server image dropped");

        let items = navidrome_items(&seed("Mira Okafor"), &info);
        let names: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(names, vec!["Ilse Varga", "Delta Pines"]);
        assert_eq!(items[0].reason, "Similar to Mira Okafor");
        assert_eq!(items[0].source, DiscoverySource::Navidrome);
        assert!(items.iter().all(|i| i.preview_url.is_none()));
    }

    #[test]
    fn navidrome_error_envelope_is_an_error() {
        let json: SubsonicResponse = serde_json::from_str(
            r#"{"subsonic-response":{"status":"failed","error":{"code":70,"message":"Artist not found"}}}"#,
        )
        .unwrap();
        assert_eq!(artist_info_from_response("s1", json).unwrap_err(), "Artist not found");
    }

    #[test]
    fn lastfm_items_skip_library_artists() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/lastfm_artist_getsimilar.json")).unwrap();
        let artists = lastfm::parse_similar_artists(&value).unwrap();
        let library: HashSet<String> = [normalize_name("Cyndi Lauper")].into_iter().collect();
        let items = lastfm_artist_items("Cher", &artists, &library);
        let names: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(names, vec!["Sonny & Cher", "Bonnie Tyler"]);
        assert_eq!(items[0].external_url.as_deref(), Some("https://www.last.fm/music/Sonny+&+Cher"));
    }

    #[test]
    fn merge_interleaves_dedupes_and_caps() {
        let a = |n: &str, src| new_item(OffServerKind::Artist, n, n, String::new(), None, src);
        let lists = vec![
            vec![a("X", DiscoverySource::ListenBrainz)],
            vec![],
            vec![a("Y", DiscoverySource::Navidrome), a("Z", DiscoverySource::Navidrome)],
            vec![a("the y", DiscoverySource::LastFm), a("W", DiscoverySource::LastFm)],
        ];
        let merged = merge(lists.clone(), 10);
        let names: Vec<&str> = merged.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(names, vec!["X", "Y", "Z", "W"], "'the y' duplicates 'Y'");
        assert_eq!(merge(lists, 2).len(), 2);
        assert!(merge(vec![vec![], vec![]], 5).is_empty(), "no sources ⇒ empty, not an error");
    }

    #[test]
    fn fresh_release_reason() {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/listenbrainz_fresh_releases_live.json"
        ))
        .unwrap();
        let releases = listenbrainz::select_fresh(listenbrainz::parse_fresh_releases(&value), "2026-10-29");
        let items = fresh_items(&releases);
        assert_eq!(items[0].kind, OffServerKind::Album);
        assert!(items[0].reason.starts_with("New album · You listen to "));
        assert_eq!(items[0].release_date.as_deref(), Some("2026-09-29"));
    }

    /// Hits the real iTunes, ListenBrainz and Last.fm endpoints through the
    /// production code paths. Run with `cargo test live_ -- --ignored`.
    #[test]
    #[ignore]
    fn live_external_sources() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let budget = Budget::new(4);
            let mut artist = new_item(
                OffServerKind::Artist,
                "Radiohead",
                "Radiohead",
                String::new(),
                None,
                DiscoverySource::Navidrome,
            );
            assert_eq!(itunes::enrich(&mut artist, true, &budget).await, Ok(true));
            assert!(artist.preview_url.as_deref().unwrap().starts_with("https://"));
            assert!(artist.art_url.as_deref().unwrap().contains("600x600"));

            let releases = listenbrainz::fresh_releases("rob", &budget).await.unwrap();
            assert!(!releases.is_empty());

            let err = lastfm::similar_artists("invalid", "Cher", 2, &budget).await.unwrap_err();
            assert_eq!(err, "Invalid Last.fm API key");
            assert_eq!(budget.used(), 3);
        });
    }

    #[test]
    fn disabled_config_returns_empty_without_network() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let config = DiscoveryConfig {
            offserver_enabled: false,
            ..Default::default()
        };
        let section = runtime.block_on(load(&[], &LibrarySnapshot::default(), &config, 1, "2026-10-29"));
        assert!(section.items.is_empty());
        assert_eq!(section.status.lastfm, SourceStatus::Disabled);
        assert_eq!(section.status.external_requests, 0);
    }

    #[test]
    fn no_seeds_and_no_keys_degrades_to_empty() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let config = DiscoveryConfig {
            offserver_enabled: true,
            previews_enabled: true,
            ..Default::default()
        };
        let section = runtime.block_on(load(&[], &LibrarySnapshot::default(), &config, 1, "2026-10-29"));
        assert!(section.items.is_empty());
        assert_eq!(section.status.lastfm, SourceStatus::NotConfigured);
        assert_eq!(section.status.listenbrainz, SourceStatus::NotConfigured);
        assert_eq!(section.status.navidrome, SourceStatus::Ok(0));
        assert_eq!(section.status.external_requests, 0);
    }
}
