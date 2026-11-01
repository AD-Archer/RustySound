//! Home and Discover data: recently played, playlists plus generated mixes,
//! songs and albums you might like, new albums, rewind shelves and
//! recommendations from outside the library. No UI lives here.
//!
//! # Home entry points (library-first shelves; see `home.rs`)
//! - [`load_recently_played`]: the Cover Flow stage (fresh, 1 request per server).
//! - [`load_new_in_library`], [`load_discover_rewind`]: snapshot reads.
//! - [`load_home_playlists`]: the user's playlists plus the generated mixes.
//! - [`load_songs_you_might_like`], [`load_albums_you_might_like`]: in-library
//!   picks from the server's similarity data.
//! - [`load_discover_off_server`]: the bottom "outside your server" shelf.
//!
//! # Entry points
//! - [`load_discover_mixes`], then [`load_discover_off_server`]. They share
//!   one library snapshot in memory with the Home loaders, so later calls
//!   reuse the first one's requests.
//! - [`load_mix`]: one generated mix by id, for the mix page.
//! - [`load_discover_rewind`]: the Rewind segment for a chosen window (3, 6 or
//!   12 months). Reuses the snapshot, so switching windows costs no requests.
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

mod cache;
mod home;
mod http;
mod library;
mod mixes;
mod offserver;
mod reasons;
mod rewind;
mod stage;
mod types;
mod util;

pub use home::{
    load_albums_you_might_like, load_home_playlists, load_new_in_library, load_recently_played,
    load_songs_you_might_like,
};
pub use mixes::{mix_id_from_slug, mix_slug};
pub use rewind::DEFAULT_REWIND_MONTHS;
pub use types::*;
// Shared with the listening aggregation (`api::listening`).
pub(crate) use cache::servers_signature;
pub(crate) use http::server_call;
pub(crate) use util::{normalize_name, parse_timestamp};

use crate::api::models::{ServerConfig, Song};
use crate::api::navidrome::NavidromeClient;
use crate::db::AppSettings;
use futures_util::future::join_all;
use library::{recent_artists, top_artists, ArtistSeed, LibrarySnapshot};
use mixes::MixContext;
use std::collections::{HashMap, HashSet};
use util::{day_seed, SeededRng};

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

    /// A client for the active server with this id.
    fn client(&self, id: &str) -> Option<NavidromeClient> {
        NavidromeClient::for_id(&self.servers, id)
    }
}

/// Start fresh picks for today: bumps the day's seed and drops cached Discover
/// sections, album lists, the playlists list and the recently-played fetch,
/// so the next loader calls refetch and reshuffle.
/// Synchronous and cheap; makes no network requests itself.
pub fn refresh_discover(servers: &[ServerConfig]) {
    let active: Vec<ServerConfig> = servers.iter().filter(|s| s.active).cloned().collect();
    cache::bump_salt(&util::today_key(), &active);
    home::clear_recent_memo();
    for server in &active {
        let _ = crate::cache_service::remove_by_prefix(&format!("api:getPlaylists:v1:{}", server.id));
    }
}

/// Generated mixes, plus "Songs you might like" from the same requests.
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
        let client = ctx.client(&seed.server_id);
        async move {
            match client {
                Some(client) => server_call(client.get_similar_songs2(&seed.artist_id, count))
                    .await
                    .unwrap_or_default(),
                None => Vec::new(),
            }
        }
    };
    let discovery_future = join_all(discovery_seeds.iter().cloned().map(|s| similar(s, 30)));
    let radio_future = join_all(radio_seeds.iter().cloned().map(|s| similar(s, 40)));
    let deep_future = join_all(deep_albums.iter().cloned().map(|album| {
        let client = ctx.client(&album.server_id);
        async move { server_call(client?.get_album(&album.id)).await.ok() }
    }));
    let (discovery_lists, radio_lists, deep_results) =
        futures_util::join!(discovery_future, radio_future, deep_future);

    // "Songs you might like" comes from the same similar-songs lists.
    let similar_lists: Vec<(ArtistSeed, Vec<Song>)> = discovery_seeds
        .iter()
        .cloned()
        .zip(discovery_lists.iter().cloned())
        .chain(radio_seeds.iter().cloned().zip(radio_lists.iter().cloned()))
        .collect();
    let songs_like = mixes::select_songs_like(&mix_ctx, &similar_lists, mixes::SONGS_LIKE_SIZE);

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
        songs_like,
        generated_on: ctx.date.clone(),
    };
    if snapshot.servers_ok > 0 {
        cache::put(&key, &section);
    }
    section
}

/// What [`load_mix`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum MixLookup {
    Found(Mix),
    /// No such mix today (not enough data, or an unknown id).
    Missing,
    /// No active server to build it from.
    NoServer,
}

/// One generated mix by id, for the mix page (`/mix/<slug>`).
///
/// Reads today's mixes ([`load_discover_mixes`]: the in-memory memo, then the
/// disk cache, so it matches Home exactly and costs no requests once Home
/// has loaded); otherwise regenerates them from the same day seed. An artist
/// radio that isn't in today's list (a link from another day, or from before
/// a refresh) is rebuilt from the server and artist ids in its id, with
/// 1 `getArtist` and 1 `getSimilarSongs2`, then cached for the day.
pub async fn load_mix(servers: &[ServerConfig], settings: &AppSettings, id: &str) -> MixLookup {
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return MixLookup::NoServer;
    };
    let section = load_discover_mixes(servers, settings).await;
    if let Some(mix) = section.mixes.into_iter().find(|m| m.id == id) {
        return MixLookup::Found(mix);
    }
    let Some((server_id, artist_id)) = mixes::radio_parts(id) else {
        return MixLookup::Missing;
    };
    let key = ctx.section_key(&format!("mix:{id}"));
    if let Some(mix) = cache::get::<Mix>(&key) {
        return MixLookup::Found(mix);
    }
    let Some(client) = ctx.client(server_id) else {
        return MixLookup::Missing;
    };
    let (artist, similar) = futures_util::join!(
        server_call(client.get_artist(artist_id)),
        server_call(client.get_similar_songs2(artist_id, 40)),
    );
    let (Ok((artist, _)), Ok(songs)) = (artist, similar) else {
        return MixLookup::Missing;
    };
    let seed = ArtistSeed {
        server_id: server_id.to_string(),
        artist_id: artist_id.to_string(),
        name: artist.name.clone(),
        plays: 0,
    };
    match mixes::build_artist_radio(&ctx.mix_context(), &seed, songs) {
        Some(mix) => {
            cache::put(&key, &mix);
            MixLookup::Found(mix)
        }
        None => MixLookup::Missing,
    }
}

/// Haven't heard in a while: heavily played albums (≥ 5 plays, ≤ 12) and songs
/// (≥ 3 plays, ≤ 20) not played in the last [`DEFAULT_REWIND_MONTHS`] months.
///
/// A fresh random pick in random order on every call, so Home shows something
/// different each visit. Uses only the shared library snapshot, so after
/// any Home loader it makes no requests. Empty when nothing qualifies,
/// including servers without `played` dates (plain Subsonic).
pub async fn load_discover_rewind(servers: &[ServerConfig], settings: &AppSettings) -> RewindSection {
    let months = DEFAULT_REWIND_MONTHS;
    let Some(ctx) = LoadContext::new(servers, settings) else {
        return RewindSection {
            months,
            generated_on: util::today_key(),
            ..Default::default()
        };
    };
    let snapshot = ctx.snapshot().await;
    let cutoff_days = rewind::cutoff_days_for_months(months);
    let mut rng = util::SeededRng::new(rand::random());
    let mut albums =
        rewind::select_rewind_albums(&snapshot.frequent, ctx.now, cutoff_days, rng.next_u64(), 12);
    let mut songs = rewind::select_rewind_songs(&snapshot.top_songs, ctx.now, cutoff_days, 20);
    rng.shuffle(&mut albums);
    rng.shuffle(&mut songs);
    RewindSection {
        months,
        albums,
        songs,
        generated_on: ctx.date.clone(),
    }
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
            let client = NavidromeClient::for_id(servers, &server_id)
                .ok_or_else(|| "Server not found.".to_string())?;
            let name = name
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| default_playlist_name(mix));
            let comment = format!("{} · made by RustySound Discover on {}", mix.blurb, mix.generated_on);
            let playlist_id = client
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
            let client = ServerConfig::find(servers, &server_id)
                .filter(|s| s.active)
                .cloned()
                .map(NavidromeClient::new)
                .ok_or_else(|| "That playlist's server isn't active.".to_string())?;
            let ids = song_ids_on(&mix.songs, &server_id);
            if ids.is_empty() {
                return Err("None of this mix's songs are on that server.".to_string());
            }
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
            ..Default::default()
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
        let (mixes, off_server) = runtime.block_on(async {
            (
                load_discover_mixes(&servers, &settings).await,
                load_discover_off_server(&servers, &settings).await,
            )
        });
        assert!(mixes.mixes.is_empty());
        assert!(off_server.items.is_empty());
        assert_eq!(off_server.status.external_requests, 0);
        let rewind = runtime.block_on(load_discover_rewind(&servers, &settings));
        assert_eq!(rewind.months, DEFAULT_REWIND_MONTHS);
        assert!(rewind.albums.is_empty() && rewind.songs.is_empty());
        assert_eq!(mixes.generated_on.len(), 10);
    }

    #[test]
    fn settings_map_to_config() {
        let mut settings = AppSettings::default();
        // Music from outside the server is opt-in.
        assert!(!settings.discovery_offserver_enabled && settings.discovery_previews_enabled);
        settings.lastfm_api_key = "  ".into();
        settings.lastfm_username = " rj ".into();
        let config = DiscoveryConfig::from_settings(&settings);
        assert_eq!(config.lastfm_api_key, None);
        assert_eq!(config.lastfm_username.as_deref(), Some("rj"));
        assert!(!config.signature().contains("rj"));
    }
}
