//! Discover data: Cover Flow picks, generated mixes, rewind shelves and
//! recommendations from outside the library. No UI lives here.
//!
//! # Entry points
//! - [`load_discover`]: everything at once (stage → mixes → off-server).
//! - Progressive: [`load_discover_stage`], then [`load_discover_mixes`], then
//!   [`load_discover_off_server`]. Call them in that order; they share one
//!   library snapshot in memory, so later calls reuse the first one's requests.
//! - [`refresh_discover`]: user-initiated "new picks" (new seed for today and
//!   refetch). Call it before the loaders.
//! - [`save_mix_as_playlist`]: turn a [`Mix`] into a real server playlist.
//!
//! # Guarantees
//! - Loaders never fail. A missing source just means fewer items; see
//!   [`OffServerSection::status`] for why.
//! - Results are stable for a local calendar day (seeded by date and cached
//!   per day) until [`refresh_discover`] is called.
//! - Only `active` servers are used. Items carry their `server_id`.

// The Discover view (phase 2) consumes this API; until then most of it is unused.
#![allow(dead_code)]

mod cache;
mod http;
mod library;
mod mixes;
mod offserver;
mod reasons;
mod rewind;
mod stage;
mod types;
mod util;

pub use types::*;

use crate::api::models::{ServerConfig, Song};
use crate::api::navidrome::NavidromeClient;
use crate::db::AppSettings;
use futures_util::future::join_all;
use http::server_call;
use library::{pick_similar_seeds, recent_artists, top_artists, ArtistSeed, LibrarySnapshot};
use mixes::MixContext;
use std::collections::{HashMap, HashSet};
use util::{day_seed, normalize_name, SeededRng};

/// Per-load context shared by the section loaders.
struct LoadContext {
    servers: Vec<ServerConfig>,
    config: DiscoveryConfig,
    date: String,
    salt: u32,
    now: i64,
    servers_sig: String,
}

impl LoadContext {
    /// `None` when no server is active (every section is then empty).
    fn new(servers: &[ServerConfig], settings: &AppSettings) -> Option<Self> {
        let servers: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
        if servers.is_empty() {
            return None;
        }
        let date = util::today_key();
        let salt = cache::current_salt(&date);
        Some(Self {
            servers_sig: cache::servers_signature(&servers),
            servers,
            config: DiscoveryConfig::from_settings(settings),
            salt,
            now: util::now_epoch_secs(),
            date,
        })
    }

    fn seed(&self, scope: &str) -> u64 {
        day_seed(&self.date, self.salt, &format!("{}#{scope}", self.servers_sig))
    }

    fn section_key(&self, section: &str) -> String {
        cache::section_key(
            section,
            &self.date,
            self.salt,
            &self.servers_sig,
            &self.config.signature(),
        )
    }

    async fn snapshot(&self) -> LibrarySnapshot {
        let key = format!("discover-snapshot:{}:{}:{}", self.date, self.salt, self.servers_sig);
        library::load_snapshot(&self.servers, &key).await
    }

    fn mix_context(&self) -> MixContext {
        MixContext {
            date: self.date.clone(),
            today: self.date.clone(),
            seed: self.seed("mixes"),
            now: self.now,
        }
    }

    fn server(&self, id: &str) -> Option<ServerConfig> {
        self.servers.iter().find(|s| s.id == id).cloned()
    }
}

/// Load the whole Discover feed: stage, then mixes, then off-server.
///
/// `servers` may include inactive servers; only active ones are used.
/// `refresh = true` first calls [`refresh_discover`] (new picks for today).
/// Never fails; with no active servers every section is empty.
///
/// Cost on a cold day (no cache), per load: ~4 + up to 12 server requests per
/// active server, plus at most 6 Last.fm, 1 ListenBrainz and 8 iTunes requests.
/// A second load on the same day is served from cache (0 requests).
pub async fn load_discover(servers: &[ServerConfig], settings: &AppSettings, refresh: bool) -> DiscoverFeed {
    if refresh {
        refresh_discover(servers);
    }
    let stage = load_discover_stage(servers, settings).await;
    let mixes = load_discover_mixes(servers, settings).await;
    let off_server = load_discover_off_server(servers, settings).await;
    DiscoverFeed {
        stage,
        mixes,
        off_server,
    }
}

/// Start fresh picks for today: bumps the day's seed and drops cached Discover
/// sections and album lists, so the next loader calls refetch and reshuffle.
/// Synchronous and cheap; makes no network requests itself.
pub fn refresh_discover(servers: &[ServerConfig]) {
    let active: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    cache::bump_salt(&util::today_key(), &active);
}

/// Cover Flow items plus the "New in your library" and "Rewind" album shelves.
///
/// Server requests (cold): 3 album lists + 1 song list per server, plus ≤ 2
/// `getArtistInfo2` and ≤ 2 `getArtist` for "Because you play …". No external
/// requests. Sections are empty when the server lacks the data (e.g. Rewind
/// needs Navidrome's `played` dates).
pub async fn load_discover_stage(servers: &[ServerConfig], settings: &AppSettings) -> StageSection {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return StageSection {
            generated_on: util::today_key(),
            ..Default::default()
        };
    };
    let key = ctx.section_key("stage");
    if let Some(cached) = cache::get::<StageSection>(&key) {
        return cached;
    }

    let snapshot = ctx.snapshot().await;
    let rewind_albums = rewind::select_rewind_albums(&snapshot.frequent, ctx.now, ctx.seed("rewind-albums"), 12);
    let new_albums = stage::new_in_library(&snapshot.newest, ctx.now, 20);
    let because = because_you_play(&ctx, &snapshot).await;

    let section = StageSection {
        stage: stage::compose_stage(
            &rewind_albums[..rewind_albums.len().min(3)],
            &new_albums[..new_albums.len().min(6)],
            &because,
        ),
        new_in_library: new_albums,
        rewind_albums,
        generated_on: ctx.date.clone(),
    };
    if snapshot.servers_ok > 0 {
        cache::put(&key, &section);
    }
    section
}

/// "Because you play X": for each similar-artist seed, the first in-library
/// similar artist that isn't already a favourite, and their least-played album.
async fn because_you_play(ctx: &LoadContext, snapshot: &LibrarySnapshot) -> Vec<DiscoverAlbum> {
    let favourites: HashSet<String> = top_artists(snapshot)
        .iter()
        .take(10)
        .map(|a| normalize_name(&a.name))
        .collect();
    let seeds = pick_similar_seeds(snapshot, ctx.seed("similar"));
    let picks = join_all(seeds.into_iter().map(|seed| {
        let server = ctx.server(&seed.server_id);
        let favourites = &favourites;
        async move {
            let client = NavidromeClient::new(server?);
            let info = server_call(client.get_artist_info2(
                &seed.artist_id,
                offserver::SIMILAR_COUNT,
                true,
            ))
            .await
            .ok()?;
            let similar = info
                .similar
                .iter()
                .find(|a| a.in_library() && !favourites.contains(&normalize_name(&a.name)))?;
            let (_, albums) = server_call(client.get_artist(similar.id.as_deref()?)).await.ok()?;
            stage::pick_because_album(&albums, &seed.name)
        }
    }))
    .await;
    picks.into_iter().flatten().collect()
}

/// Generated mixes ("For You") and Rewind songs.
///
/// Server requests (cold): the shared snapshot, plus ≤ 3 `getSimilarSongs2`
/// (Discovery), ≤ 2 `getSimilarSongs2` (artist radios) and ≤ 3 `getAlbum`
/// (Deep Cuts). No external requests. A mix is omitted when the data can't
/// support it (fewer than 8 songs), so the list may be empty.
pub async fn load_discover_mixes(servers: &[ServerConfig], settings: &AppSettings) -> MixesSection {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return MixesSection {
            generated_on: util::today_key(),
            ..Default::default()
        };
    };
    let key = ctx.section_key("mixes");
    if let Some(cached) = cache::get::<MixesSection>(&key) {
        return cached;
    }

    let snapshot = ctx.snapshot().await;
    let mix_ctx = ctx.mix_context();

    // Seeds.
    let top: Vec<ArtistSeed> = top_artists(&snapshot)
        .into_iter()
        .filter(|a| !library::is_various_artists(&a.name))
        .collect();
    let mut recent = recent_artists(&snapshot);
    if recent.is_empty() {
        recent = top.clone();
    }
    let discovery_seeds = SeededRng::new(ctx.seed("discovery-seeds")).pick_ordered(&recent, 6, 3);
    let taken: HashSet<String> = discovery_seeds.iter().map(|s| normalize_name(&s.name)).collect();
    let radio_pool: Vec<ArtistSeed> = top
        .iter()
        .filter(|a| !taken.contains(&normalize_name(&a.name)))
        .cloned()
        .collect();
    let radio_seeds = SeededRng::new(ctx.seed("radio-seeds")).pick_ordered(&radio_pool, 5, 2);
    let deep_albums = SeededRng::new(ctx.seed("deep-cuts")).pick_ordered(&snapshot.frequent, 10, 3);

    let similar = |seed: ArtistSeed, count: u32| {
        let server = ctx.server(&seed.server_id);
        async move {
            match server {
                Some(server) => server_call(
                    NavidromeClient::new(server).get_similar_songs2(&seed.artist_id, count),
                )
                .await
                .unwrap_or_default(),
                None => Vec::new(),
            }
        }
    };
    let discovery_future = join_all(discovery_seeds.iter().cloned().map(|s| similar(s, 30)));
    let radio_future = join_all(radio_seeds.iter().cloned().map(|s| similar(s, 40)));
    let deep_future = join_all(deep_albums.iter().cloned().map(|album| {
        let server = ctx.server(&album.server_id);
        async move {
            let server = server?;
            server_call(NavidromeClient::new(server).get_album(&album.id)).await.ok()
        }
    }));
    let (discovery_lists, radio_lists, deep_results) =
        futures_util::join!(discovery_future, radio_future, deep_future);

    let mut mixes = Vec::new();
    mixes.extend(mixes::build_discovery_mix(&mix_ctx, &discovery_seeds, discovery_lists));
    mixes.extend(mixes::build_deep_cuts(
        &mix_ctx,
        deep_results.into_iter().flatten().collect(),
    ));
    mixes.extend(mixes::build_rewind_mix(&mix_ctx, &snapshot.top_songs));
    for (seed, songs) in radio_seeds.iter().zip(radio_lists) {
        mixes.extend(mixes::build_artist_radio(&mix_ctx, seed, songs));
    }

    let section = MixesSection {
        mixes,
        rewind_songs: rewind::select_rewind_songs(&snapshot.top_songs, ctx.now, 20),
        generated_on: ctx.date.clone(),
    };
    if snapshot.servers_ok > 0 {
        cache::put(&key, &section);
    }
    section
}

/// Recommendations from outside the library ("New" segment).
///
/// Uses, in order: the server's similar artists (`getArtistInfo2`,
/// `includeNotPresent=true`, no config), Last.fm (needs `lastfm_api_key`),
/// ListenBrainz fresh releases (needs `listenbrainz_username`), then iTunes
/// for art/previews/links. Empty when `discovery_offserver_enabled` is off or
/// offline mode is on. Caps per load: 6 Last.fm, 1 ListenBrainz, 8 iTunes,
/// ≤ 2 `getArtistInfo2` + ≤ 6 `search3` + 1 `getArtists` per server; every
/// external request times out after 6 s.
pub async fn load_discover_off_server(servers: &[ServerConfig], settings: &AppSettings) -> OffServerSection {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        let config = DiscoveryConfig::from_settings(settings);
        let mut section = OffServerSection {
            generated_on: util::today_key(),
            ..Default::default()
        };
        if !config.offserver_enabled {
            section.status.navidrome = SourceStatus::Disabled;
            section.status.lastfm = SourceStatus::Disabled;
            section.status.listenbrainz = SourceStatus::Disabled;
            section.status.itunes = SourceStatus::Disabled;
        }
        return section;
    };
    let key = ctx.section_key("offserver");
    if let Some(cached) = cache::get::<OffServerSection>(&key) {
        return cached;
    }

    let snapshot = ctx.snapshot().await;
    let section = offserver::load(
        &ctx.servers,
        &snapshot,
        &ctx.config,
        ctx.seed("similar"),
        &ctx.date,
    )
    .await;
    // Don't pin a day's worth of nothing when every source failed.
    let any_ok = [
        &section.status.navidrome,
        &section.status.lastfm,
        &section.status.listenbrainz,
    ]
    .iter()
    .any(|s| matches!(s, SourceStatus::Ok(_)));
    if snapshot.servers_ok > 0 && (any_ok || !section.items.is_empty()) {
        cache::put(&key, &section);
    }
    section
}

/// Where [`save_mix_as_playlist`] writes.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveMixTarget {
    /// Create a new playlist. `None` name ⇒ "<Mix name> · <Mon D>".
    New { name: Option<String> },
    /// Replace the songs of an existing playlist (e.g. re-saving today's mix).
    Replace { server_id: String, playlist_id: String },
}

/// Result of [`save_mix_as_playlist`].
#[derive(Debug, Clone, PartialEq)]
pub struct SavedMix {
    pub server_id: String,
    /// The playlist id (the server may not return one on create).
    pub playlist_id: Option<String>,
    pub saved_songs: usize,
    /// Songs left out because they live on another server (playlists are per server).
    pub skipped_songs: usize,
}

/// Which server a mix should be saved to and which song ids go in: the active
/// server holding most of the mix's songs. Pure.
pub fn plan_mix_save(mix: &Mix, servers: &[ServerConfig]) -> Result<(String, Vec<String>, usize), String> {
    let active: HashSet<&str> = servers.iter().filter(|s| s.active).map(|s| s.id.as_str()).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for song in &mix.songs {
        if active.contains(song.server_id.as_str()) {
            *counts.entry(song.server_id.as_str()).or_insert(0) += 1;
        }
    }
    let server_id = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(id, _)| id.to_string())
        .ok_or_else(|| "None of this mix's songs are on an active server.".to_string())?;
    let ids = song_ids_on(&mix.songs, &server_id);
    let skipped = mix.songs.len() - ids.len();
    Ok((server_id, ids, skipped))
}

fn song_ids_on(songs: &[Song], server_id: &str) -> Vec<String> {
    songs
        .iter()
        .filter(|s| s.server_id == server_id)
        .map(|s| s.id.clone())
        .collect()
}

/// Default playlist name: "Discovery Mix · Oct 29".
pub fn default_playlist_name(mix: &Mix) -> String {
    match chrono::NaiveDate::parse_from_str(&mix.generated_on, "%Y-%m-%d") {
        Ok(date) => format!("{} · {}", mix.name, date.format("%b %-d")),
        Err(_) => mix.name.clone(),
    }
}

/// Save a mix as a real playlist on the server ("create playlist for you").
///
/// Uses `createPlaylist` (new) or `updatePlaylist` (replace). Playlists live on
/// one server: songs from other servers are skipped and counted. Invalidates
/// the playlist caches, so playlist views refetch.
pub async fn save_mix_as_playlist(
    servers: &[ServerConfig],
    mix: &Mix,
    target: SaveMixTarget,
) -> Result<SavedMix, String> {
    match target {
        SaveMixTarget::New { name } => {
            let (server_id, ids, skipped) = plan_mix_save(mix, servers)?;
            let server = servers
                .iter()
                .find(|s| s.id == server_id)
                .cloned()
                .ok_or_else(|| "Server not found.".to_string())?;
            let name = name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| default_playlist_name(mix));
            let comment = format!("{} · made by RustySound Discover on {}", mix.blurb, mix.generated_on);
            let playlist_id = NavidromeClient::new(server)
                .create_playlist(&name, Some(&comment), &ids)
                .await?;
            Ok(SavedMix {
                server_id,
                playlist_id,
                saved_songs: ids.len(),
                skipped_songs: skipped,
            })
        }
        SaveMixTarget::Replace { server_id, playlist_id } => {
            let server = servers
                .iter()
                .find(|s| s.id == server_id && s.active)
                .cloned()
                .ok_or_else(|| "That playlist's server isn't active.".to_string())?;
            let ids = song_ids_on(&mix.songs, &server_id);
            if ids.is_empty() {
                return Err("None of this mix's songs are on that server.".to_string());
            }
            let client = NavidromeClient::new(server);
            client.refresh_playlist_cache();
            let (_, existing) = client.get_playlist(&playlist_id).await?;
            client
                .reorder_playlist(&playlist_id, &ids, existing.len())
                .await?;
            Ok(SavedMix {
                server_id,
                playlist_id: Some(playlist_id),
                saved_songs: ids.len(),
                skipped_songs: mix.songs.len() - ids.len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool) -> ServerConfig {
        ServerConfig {
            id: id.into(),
            name: id.into(),
            url: format!("https://{id}.example"),
            username: "u".into(),
            password: "p".into(),
            active,
        }
    }

    fn mix_with(servers: &[&str]) -> Mix {
        let songs: Vec<Song> = servers
            .iter()
            .enumerate()
            .map(|(i, s)| Song {
                id: format!("song{i}"),
                server_id: s.to_string(),
                ..Default::default()
            })
            .collect();
        Mix {
            id: "discovery".into(),
            kind: MixKind::Discovery,
            name: "Discovery Mix".into(),
            blurb: "x".into(),
            song_count: songs.len(),
            songs,
            covers: vec![],
            total_seconds: 0,
            length_label: String::new(),
            freshness: String::new(),
            generated_on: "2026-10-29".into(),
        }
    }

    #[test]
    fn save_plan_picks_majority_active_server() {
        let servers = vec![server("a", true), server("b", true), server("c", false)];
        let mix = mix_with(&["a", "b", "b", "c", "c", "c"]);
        let (server_id, ids, skipped) = plan_mix_save(&mix, &servers).unwrap();
        assert_eq!(server_id, "b", "inactive c is ignored even with most songs");
        assert_eq!(ids, vec!["song1", "song2"]);
        assert_eq!(skipped, 4);
        assert!(plan_mix_save(&mix_with(&["c"]), &servers).is_err());
    }

    #[test]
    fn default_name_has_short_date() {
        assert_eq!(default_playlist_name(&mix_with(&["a"])), "Discovery Mix · Oct 29");
    }

    #[test]
    fn no_active_servers_gives_empty_feed_without_requests() {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let settings = AppSettings::default();
        let servers = vec![server("off", false)];
        let feed = runtime.block_on(async {
            DiscoverFeed {
                stage: load_discover_stage(&servers, &settings).await,
                mixes: load_discover_mixes(&servers, &settings).await,
                off_server: load_discover_off_server(&servers, &settings).await,
            }
        });
        assert!(feed.stage.stage.is_empty());
        assert!(feed.mixes.mixes.is_empty());
        assert!(feed.off_server.items.is_empty());
        assert_eq!(feed.off_server.status.external_requests, 0);
        assert_eq!(feed.stage.generated_on.len(), 10);
    }

    #[test]
    fn settings_map_to_config() {
        let mut settings = AppSettings::default();
        assert!(settings.discovery_offserver_enabled && settings.discovery_previews_enabled);
        settings.lastfm_api_key = "  ".into();
        settings.lastfm_username = " rj ".into();
        let config = DiscoveryConfig::from_settings(&settings);
        assert_eq!(config.lastfm_api_key, None);
        assert_eq!(config.lastfm_username.as_deref(), Some("rj"));
        assert!(!config.signature().contains("rj"));
    }
}
