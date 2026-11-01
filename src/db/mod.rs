use crate::api::{
    default_lyrics_provider_order,
    models::{ServerConfig, Song},
    normalize_lyrics_provider_order, LyricsProvider,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::storage::app_data_dir;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_storage::{errors::StorageError, LocalStorage, Storage};

pub mod profiles;

/// Error type for database operations on native platforms
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct DbError(String);

#[cfg(not(target_arch = "wasm32"))]
impl DbError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl std::error::Error for DbError {}

// What each profile stores (`profiles::scoped_key`): settings rows on
// native, `rustysound.<key>` in localStorage on web.
const SETTINGS_KEY: &str = "app_settings";
const PLAYBACK_KEY: &str = "playback_state";
#[cfg(target_arch = "wasm32")]
const SERVERS_KEY: &str = "servers";
const TEMP_QUEUE_SNAPSHOTS_KEY: &str = "temporary_queue_snapshots";
const TEMP_QUEUE_SNAPSHOT_LIMIT: usize = 1;

/// The active profile's localStorage key for `key`.
#[cfg(target_arch = "wasm32")]
fn web_key(key: &str) -> String {
    format!("rustysound.{}", profiles::active_key(key))
}

/// The active profile's settings row `key`, if saved.
#[cfg(not(target_arch = "wasm32"))]
fn read_row(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [profiles::active_key(key)],
        |row: &rusqlite::Row| row.get(0),
    )
    .ok()
}

/// Saves the active profile's settings row `key`.
#[cfg(not(target_arch = "wasm32"))]
fn write_row(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [profiles::active_key(key).as_str(), value],
    )
    .map(|_| ())
    .map_err(|e| DbError::new(e.to_string()))
}

/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkDownloadPreference {
    ServerOnly,
    Id3Only,
    #[default]
    PreferServer,
    PreferId3,
}

// Downloads (native only) and the smart cache use these; the smart cache
// switches to them in refactor chunk D2, until then they're native only.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
impl ArtworkDownloadPreference {
    /// Whether to fetch the cover with this id. Navidrome's `mf-` ids are
    /// the art embedded in the file (ID3); other ids are the server's art.
    pub fn includes(self, cover_art_id: &str) -> bool {
        let id3 = cover_art_id.trim().to_ascii_lowercase().starts_with("mf-");
        match self {
            Self::ServerOnly => !id3,
            Self::Id3Only => id3,
            Self::PreferServer | Self::PreferId3 => true,
        }
    }

    /// The cover ids to fetch for a song, in order: its own cover, then its
    /// album's, each only when [`Self::includes`] it.
    pub fn song_covers(self, song: &Song) -> Vec<String> {
        let mut covers = Vec::new();
        for id in [song.cover_art.as_deref(), song.album_id.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
        {
            if !id.is_empty() && self.includes(id) && !covers.iter().any(|c: &String| c == id) {
                covers.push(id.to_string());
            }
        }
        covers
    }
}

/// Volume levelling (ReplayGain) mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VolumeLevelling {
    Off,
    Track,
    Album,
    /// Album gain while consecutive songs come from the same album, else track gain.
    #[default]
    Auto,
}

/// App settings stored in the database. Fields that were dropped
/// (`last_server_id`, `replay_gain`, `home_layout_json`,
/// `home_feed_load_profile`, `discover_rewind_months`) may still be in saved
/// settings; serde ignores unknown keys, so those files load as before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub volume: f64,
    pub theme: String,
    pub crossfade_enabled: bool,
    pub crossfade_duration: u32, // seconds
    /// Start the next song with no gap (preloaded one song ahead).
    #[serde(default = "default_gapless_playback")]
    pub gapless_playback: bool,
    #[serde(default)]
    pub volume_levelling: VolumeLevelling,
    /// Added to every ReplayGain value, in dB.
    #[serde(default)]
    pub replay_gain_preamp_db: f64,
    #[serde(default)]
    pub shuffle_enabled: bool,
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    #[serde(default)]
    pub cache_enabled: bool,
    #[serde(default)]
    pub cache_size_mb: u32,
    #[serde(default = "default_cache_expiry_days", alias = "cache_expiry_hours")]
    pub cache_expiry_days: i32,
    #[serde(default)]
    pub cache_expiry_in_days: bool,
    #[serde(default)]
    pub cache_images_enabled: bool,
    #[serde(default)]
    pub offline_mode: bool,
    #[serde(default = "default_lyrics_provider_order")]
    pub lyrics_provider_order: Vec<String>,
    /// Set once the server provider has been put at the front of a saved
    /// order, so moving it down later sticks.
    #[serde(default)]
    pub lyrics_server_provider_added: bool,
    #[serde(default = "default_lyrics_request_timeout_secs")]
    pub lyrics_request_timeout_secs: u32,
    #[serde(default)]
    pub lyrics_offset_ms: i32,
    #[serde(default)]
    pub lyrics_unsynced_mode: bool,
    #[serde(default = "default_lyrics_screenshot_mode")]
    pub lyrics_screenshot_mode: bool,
    #[serde(default)]
    pub lyrics_screenshot_timestamps: bool,
    #[serde(default = "default_lyrics_theme")]
    pub lyrics_default_theme: String,
    #[serde(default = "default_bookmark_limit")]
    pub bookmark_limit: u32,
    #[serde(default = "default_bookmark_auto_save")]
    pub bookmark_auto_save: bool,
    #[serde(default)]
    pub bookmark_autoplay_on_launch: bool,
    #[serde(default = "default_downloads_enabled")]
    pub downloads_enabled: bool,
    #[serde(default)]
    pub auto_downloads_enabled: bool,
    #[serde(default = "default_auto_download_tier")]
    pub auto_download_tier: u8,
    #[serde(default = "default_auto_download_album_count")]
    pub auto_download_album_count: u32,
    #[serde(default = "default_auto_download_playlist_count")]
    pub auto_download_playlist_count: u32,
    #[serde(default = "default_download_limit_count")]
    pub download_limit_count: u32,
    #[serde(default = "default_download_limit_mb")]
    pub download_limit_mb: u32,
    #[serde(default = "default_artwork_download_preference")]
    pub artwork_download_preference: ArtworkDownloadPreference,
    #[serde(default)]
    pub custom_css: String,
    // Appearance (agent G1)
    /// Light/dark choice, separate from the theme: "system" (default), "light" or "dark".
    #[serde(default = "default_appearance")]
    pub appearance: String,
    // Discovery (Discover screen). All optional; empty strings mean "not set".
    /// Last.fm API key. Enables Last.fm similar artists/albums/tracks off-server.
    #[serde(default)]
    pub lastfm_api_key: String,
    /// Last.fm username. Seeds discovery from the user's Last.fm top artists.
    #[serde(default)]
    pub lastfm_username: String,
    /// ListenBrainz username. Enables "fresh releases" from artists the user listens to.
    #[serde(default)]
    pub listenbrainz_username: String,
    /// Show recommendations from outside the user's servers.
    #[serde(default = "default_discovery_offserver_enabled")]
    pub discovery_offserver_enabled: bool,
    /// Resolve 30-second previews (iTunes) for off-server items.
    #[serde(default = "default_discovery_previews_enabled")]
    pub discovery_previews_enabled: bool,
    // Discover (agent D)
    /// JSON map of Discover mix id to the server playlist it was saved as:
    /// `{"<mix id>": {"server_id": "...", "playlist_id": "..."}}`. Re-saving a
    /// mix replaces that playlist instead of making a new one.
    #[serde(default = "default_discover_saved_mix_playlists")]
    pub discover_saved_mix_playlists: String,
    /// Playback speed (Now Playing menu), one of `PLAYBACK_SPEEDS`. Radio
    /// always plays at 1x.
    #[serde(default = "default_playback_rate")]
    pub playback_rate: f64,
    /// Save the play queue to the server and offer queues saved by other
    /// devices ("Continue from iPhone").
    #[serde(default = "default_queue_sync_enabled")]
    pub queue_sync_enabled: bool,
    /// The 10-band equalizer (`crate::equalizer`). Off by default.
    #[serde(default)]
    pub equalizer: crate::equalizer::EqSettings,
    /// Keep the music going: near the end of any queue, add similar songs
    /// (`audio_manager::autoplay`).
    #[serde(default = "default_autoplay_enabled")]
    pub autoplay_enabled: bool,
}

fn default_autoplay_enabled() -> bool {
    true
}

fn default_queue_sync_enabled() -> bool {
    true
}

fn default_playback_rate() -> f64 {
    1.0
}

fn default_gapless_playback() -> bool {
    true
}

fn default_discover_saved_mix_playlists() -> String {
    "{}".to_string()
}

fn default_discovery_offserver_enabled() -> bool {
    false
}

fn default_discovery_previews_enabled() -> bool {
    true
}

fn default_lyrics_request_timeout_secs() -> u32 {
    4
}

fn default_lyrics_screenshot_mode() -> bool {
    true
}

fn default_lyrics_theme() -> String {
    "cover".to_string()
}

fn default_cache_expiry_days() -> i32 {
    30
}

fn default_bookmark_limit() -> u32 {
    10
}

fn default_bookmark_auto_save() -> bool {
    true
}

fn default_downloads_enabled() -> bool {
    true
}

fn default_auto_download_tier() -> u8 {
    1
}

fn default_auto_download_album_count() -> u32 {
    2
}

fn default_auto_download_playlist_count() -> u32 {
    2
}

fn default_download_limit_count() -> u32 {
    500
}

fn default_download_limit_mb() -> u32 {
    4096
}

fn default_artwork_download_preference() -> ArtworkDownloadPreference {
    ArtworkDownloadPreference::PreferServer
}

fn default_appearance() -> String {
    "system".to_string()
}

/// Orders saved before the server provider existed get it at the front,
/// once: the user's own lyrics need no third-party call.
fn add_server_lyrics_provider(settings: &mut AppSettings) {
    if settings.lyrics_server_provider_added {
        return;
    }
    let has_server = settings
        .lyrics_provider_order
        .iter()
        .any(|key| LyricsProvider::from_key(key) == Some(LyricsProvider::Server));
    if !has_server {
        settings
            .lyrics_provider_order
            .insert(0, LyricsProvider::Server.key().to_string());
    }
    settings.lyrics_server_provider_added = true;
}

fn migrate_settings(mut settings: AppSettings) -> AppSettings {
    add_server_lyrics_provider(&mut settings);
    let normalized = normalize_lyrics_provider_order(&settings.lyrics_provider_order);
    let legacy = |keys: [&str; 3]| {
        std::iter::once("server")
            .chain(keys)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let legacy_default_v1 = legacy(["netease", "lrclib", "genius"]);
    let legacy_default_v2 = legacy(["lrclib", "netease", "genius"]);
    let legacy_default_v3 = legacy(["genius", "lrclib", "netease"]);

    settings.lyrics_provider_order = if normalized == legacy_default_v1
        || normalized == legacy_default_v2
        || normalized == legacy_default_v3
    {
        default_lyrics_provider_order()
    } else {
        normalized
    };

    // Legacy settings stored this field in hours; migrate once to day-based semantics.
    if !settings.cache_expiry_in_days {
        if settings.cache_expiry_days < 0 {
            settings.cache_expiry_days = -1;
        } else {
            let hours = settings.cache_expiry_days.max(1);
            settings.cache_expiry_days = ((hours + 23) / 24).clamp(1, 3650);
        }
        settings.cache_expiry_in_days = true;
    } else {
        settings.cache_expiry_days = settings.cache_expiry_days.clamp(-1, 3650);
    }

    settings.crossfade_duration = settings.crossfade_duration.clamp(1, 12);
    settings.replay_gain_preamp_db = if settings.replay_gain_preamp_db.is_finite() {
        settings.replay_gain_preamp_db.clamp(-12.0, 12.0)
    } else {
        0.0
    };
    settings.auto_download_tier = settings.auto_download_tier.clamp(1, 3);
    settings.auto_download_album_count = settings.auto_download_album_count.clamp(0, 25);
    settings.auto_download_playlist_count = settings.auto_download_playlist_count.clamp(0, 25);
    settings.download_limit_count = settings.download_limit_count.clamp(25, 20000);
    settings.download_limit_mb = settings.download_limit_mb.clamp(256, 131072);
    settings
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            theme: "rusty".to_string(),
            crossfade_enabled: false,
            crossfade_duration: 3,
            gapless_playback: default_gapless_playback(),
            volume_levelling: VolumeLevelling::default(),
            replay_gain_preamp_db: 0.0,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::Off,
            cache_enabled: true,
            cache_size_mb: 100,
            cache_expiry_days: default_cache_expiry_days(),
            cache_expiry_in_days: true,
            cache_images_enabled: true,
            offline_mode: false,
            lyrics_provider_order: default_lyrics_provider_order(),
            lyrics_server_provider_added: true,
            lyrics_request_timeout_secs: default_lyrics_request_timeout_secs(),
            lyrics_offset_ms: 0,
            lyrics_unsynced_mode: false,
            lyrics_screenshot_mode: default_lyrics_screenshot_mode(),
            lyrics_screenshot_timestamps: false,
            lyrics_default_theme: default_lyrics_theme(),
            bookmark_limit: default_bookmark_limit(),
            bookmark_auto_save: default_bookmark_auto_save(),
            bookmark_autoplay_on_launch: false,
            downloads_enabled: default_downloads_enabled(),
            auto_downloads_enabled: false,
            auto_download_tier: default_auto_download_tier(),
            auto_download_album_count: default_auto_download_album_count(),
            auto_download_playlist_count: default_auto_download_playlist_count(),
            download_limit_count: default_download_limit_count(),
            download_limit_mb: default_download_limit_mb(),
            artwork_download_preference: default_artwork_download_preference(),
            custom_css: String::new(),
            appearance: default_appearance(),
            lastfm_api_key: String::new(),
            lastfm_username: String::new(),
            listenbrainz_username: String::new(),
            discovery_offserver_enabled: default_discovery_offserver_enabled(),
            discovery_previews_enabled: default_discovery_previews_enabled(),
            discover_saved_mix_playlists: default_discover_saved_mix_playlists(),
            playback_rate: default_playback_rate(),
            queue_sync_enabled: default_queue_sync_enabled(),
            equalizer: crate::equalizer::EqSettings::default(),
            autoplay_enabled: default_autoplay_enabled(),
        }
    }
}

/// Playback state for resuming
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PlaybackState {
    pub song_id: Option<String>,
    pub server_id: Option<String>,
    pub position: f64, // seconds
    pub queue: Vec<QueueItem>,
    pub queue_index: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItem {
    pub song_id: String,
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TemporaryQueueSnapshot {
    pub id: String,
    pub saved_at_epoch_ms: i64,
    pub queue: Vec<Song>,
    pub queue_index: usize,
    pub now_playing: Option<Song>,
    pub playback_position: f64,
}

fn snapshot_signature(snapshot: &TemporaryQueueSnapshot) -> String {
    let mut signature = String::new();
    signature.push_str(&snapshot.queue_index.to_string());
    signature.push('|');
    if let Some(song) = snapshot.now_playing.as_ref() {
        signature.push_str(song.server_id.trim());
        signature.push(':');
        signature.push_str(song.id.trim());
    }
    signature.push('|');
    for song in &snapshot.queue {
        signature.push_str(song.server_id.trim());
        signature.push(':');
        signature.push_str(song.id.trim());
        signature.push(';');
    }
    signature
}

fn normalize_queue_snapshots(
    snapshots: Vec<TemporaryQueueSnapshot>,
) -> Vec<TemporaryQueueSnapshot> {
    let mut snapshots: Vec<TemporaryQueueSnapshot> = snapshots
        .into_iter()
        .filter_map(|mut snapshot| {
            if snapshot.queue.is_empty() {
                return None;
            }

            snapshot.queue_index = snapshot
                .queue_index
                .min(snapshot.queue.len().saturating_sub(1));
            snapshot.playback_position = snapshot.playback_position.max(0.0);
            if snapshot.id.trim().is_empty() {
                snapshot.id = format!("queue-{}", snapshot.saved_at_epoch_ms.max(0));
            }
            Some(snapshot)
        })
        .collect();

    snapshots.sort_by(|left, right| {
        right
            .saved_at_epoch_ms
            .cmp(&left.saved_at_epoch_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    snapshots.truncate(TEMP_QUEUE_SNAPSHOT_LIMIT);
    snapshots
}

fn upsert_queue_snapshot(
    existing: Vec<TemporaryQueueSnapshot>,
    mut incoming: TemporaryQueueSnapshot,
) -> Vec<TemporaryQueueSnapshot> {
    if incoming.id.trim().is_empty() {
        incoming.id = format!("queue-{}", incoming.saved_at_epoch_ms.max(0));
    }
    incoming.queue_index = incoming
        .queue_index
        .min(incoming.queue.len().saturating_sub(1));
    incoming.playback_position = incoming.playback_position.max(0.0);

    let incoming_signature = snapshot_signature(&incoming);
    let mut snapshots = normalize_queue_snapshots(existing);
    if let Some(position) = snapshots
        .iter()
        .position(|snapshot| snapshot_signature(snapshot) == incoming_signature)
    {
        let mut existing_snapshot = snapshots.remove(position);
        existing_snapshot.saved_at_epoch_ms = incoming.saved_at_epoch_ms;
        existing_snapshot.queue = incoming.queue;
        existing_snapshot.queue_index = incoming.queue_index;
        existing_snapshot.now_playing = incoming.now_playing;
        existing_snapshot.playback_position = incoming.playback_position;
        snapshots.insert(0, existing_snapshot);
    } else {
        snapshots.insert(0, incoming);
    }

    normalize_queue_snapshots(snapshots)
}

// Database operations for native platforms
// These run directly on desktop/mobile without needing #[server]
// Everything below reads and writes the active profile's data.

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_servers(servers: Vec<ServerConfig>) -> Result<(), DbError> {
    save_servers_now(&servers)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_servers_now(servers: &[ServerConfig]) -> Result<(), DbError> {
    let mut conn = get_db_connection()?;
    save_servers_inner(&mut conn, &profiles::active_profile_id(), servers)
}

#[cfg(not(target_arch = "wasm32"))]
fn save_servers_inner(
    conn: &mut rusqlite::Connection,
    profile_id: &str,
    servers: &[ServerConfig],
) -> Result<(), DbError> {
    let tx = conn
        .transaction()
        .map_err(|e| DbError::new(e.to_string()))?;

    tx.execute("DELETE FROM servers WHERE profile_id = ?1", [profile_id])
        .map_err(|e| DbError::new(e.to_string()))?;

    for server in servers {
        tx.execute(
            "INSERT OR REPLACE INTO servers (profile_id, id, name, url, username, password, active, kind, user_id, access_token)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                profile_id,
                &server.id,
                &server.name,
                &server.url,
                &server.username,
                &server.password,
                if server.active { "1" } else { "0" },
                server.kind.as_str(),
                &server.user_id,
                &server.access_token,
            ],
        )
        .map_err(|e| DbError::new(e.to_string()))?;
    }

    tx.commit().map_err(|e| DbError::new(e.to_string()))
}

#[cfg(target_arch = "wasm32")]
pub async fn save_servers(servers: Vec<ServerConfig>) -> Result<(), StorageError> {
    save_servers_now(&servers)
}

#[cfg(target_arch = "wasm32")]
pub fn save_servers_now(servers: &[ServerConfig]) -> Result<(), StorageError> {
    LocalStorage::set(web_key(SERVERS_KEY), servers.to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_servers() -> Result<Vec<ServerConfig>, DbError> {
    let conn = get_db_connection()?;
    load_servers_inner(&conn, &profiles::active_profile_id())
}

#[cfg(not(target_arch = "wasm32"))]
fn load_servers_inner(conn: &rusqlite::Connection, profile_id: &str) -> Result<Vec<ServerConfig>, DbError> {
    let mut stmt = conn
        .prepare("SELECT id, name, url, username, password, active, kind, user_id, access_token FROM servers WHERE profile_id = ?1")
        .map_err(|e| DbError::new(e.to_string()))?;

    let servers = stmt
        .query_map([profile_id], |row: &rusqlite::Row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                username: row.get(3)?,
                password: row.get(4)?,
                active: row.get::<_, String>(5)? == "1",
                kind: crate::api::ServerKind::from_stored(&row.get::<_, String>(6)?),
                user_id: row.get(7)?,
                access_token: row.get(8)?,
            })
        })
        .map_err(|e| DbError::new(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(servers)
}

#[cfg(target_arch = "wasm32")]
pub async fn load_servers() -> Result<Vec<ServerConfig>, StorageError> {
    match LocalStorage::get(web_key(SERVERS_KEY)) {
        Ok(servers) => Ok(servers),
        Err(_) => Ok(Vec::new()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_settings(settings: AppSettings) -> Result<(), DbError> {
    let conn = get_db_connection()?;
    let settings_json =
        serde_json::to_string(&settings).map_err(|e| DbError::new(e.to_string()))?;
    write_row(&conn, SETTINGS_KEY, &settings_json)
}

#[cfg(target_arch = "wasm32")]
pub async fn save_settings(settings: AppSettings) -> Result<(), StorageError> {
    LocalStorage::set(web_key(SETTINGS_KEY), settings)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_settings() -> Result<AppSettings, DbError> {
    let conn = get_db_connection()?;
    match read_row(&conn, SETTINGS_KEY) {
        Some(json) => serde_json::from_str(&json)
            .map(migrate_settings)
            .map_err(|e| DbError::new(e.to_string())),
        None => Ok(AppSettings::default()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn load_settings() -> Result<AppSettings, StorageError> {
    match LocalStorage::get(web_key(SETTINGS_KEY)) {
        Ok(settings) => Ok(migrate_settings(settings)),
        Err(_) => Ok(AppSettings::default()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_playback_state(state: PlaybackState) -> Result<(), DbError> {
    let conn = get_db_connection()?;
    let state_json = serde_json::to_string(&state).map_err(|e| DbError::new(e.to_string()))?;
    write_row(&conn, PLAYBACK_KEY, &state_json)
}

#[cfg(target_arch = "wasm32")]
pub async fn save_playback_state(state: PlaybackState) -> Result<(), StorageError> {
    LocalStorage::set(web_key(PLAYBACK_KEY), state)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_playback_state() -> Result<PlaybackState, DbError> {
    let conn = get_db_connection()?;
    match read_row(&conn, PLAYBACK_KEY) {
        Some(json) => serde_json::from_str(&json).map_err(|e| DbError::new(e.to_string())),
        None => Ok(PlaybackState::default()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn load_playback_state() -> Result<PlaybackState, StorageError> {
    match LocalStorage::get(web_key(PLAYBACK_KEY)) {
        Ok(state) => Ok(state),
        Err(_) => Ok(PlaybackState::default()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_temporary_queue_snapshot(
    snapshot: TemporaryQueueSnapshot,
) -> Result<(), DbError> {
    if snapshot.queue.is_empty() {
        return Ok(());
    }

    let mut snapshots = load_temporary_queue_snapshots().await?;
    snapshots = upsert_queue_snapshot(snapshots, snapshot);
    let payload = serde_json::to_string(&snapshots).map_err(|e| DbError::new(e.to_string()))?;
    let conn = get_db_connection()?;
    write_row(&conn, TEMP_QUEUE_SNAPSHOTS_KEY, &payload)
}

#[cfg(target_arch = "wasm32")]
pub async fn save_temporary_queue_snapshot(
    snapshot: TemporaryQueueSnapshot,
) -> Result<(), StorageError> {
    if snapshot.queue.is_empty() {
        return Ok(());
    }

    let existing = load_temporary_queue_snapshots().await.unwrap_or_default();
    let snapshots = upsert_queue_snapshot(existing, snapshot);
    LocalStorage::set(web_key(TEMP_QUEUE_SNAPSHOTS_KEY), snapshots)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_temporary_queue_snapshots() -> Result<Vec<TemporaryQueueSnapshot>, DbError> {
    let conn = get_db_connection()?;
    match read_row(&conn, TEMP_QUEUE_SNAPSHOTS_KEY) {
        Some(json) => {
            let parsed: Vec<TemporaryQueueSnapshot> =
                serde_json::from_str(&json).map_err(|e| DbError::new(e.to_string()))?;
            Ok(normalize_queue_snapshots(parsed))
        }
        None => Ok(Vec::new()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn load_temporary_queue_snapshots() -> Result<Vec<TemporaryQueueSnapshot>, StorageError> {
    match LocalStorage::get(web_key(TEMP_QUEUE_SNAPSHOTS_KEY)) {
        Ok(snapshots) => Ok(normalize_queue_snapshots(snapshots)),
        Err(_) => Ok(Vec::new()),
    }
}

/// A small JSON value kept next to the active profile's settings (native:
/// the `settings` table; web: localStorage `rustysound.<key>`). For
/// bookkeeping that isn't a user setting, like playback sync's queue marks
/// and the offline changes waiting to sync.
#[cfg(not(target_arch = "wasm32"))]
pub fn save_json_value<T: Serialize>(key: &str, value: &T) -> bool {
    let Ok(json) = serde_json::to_string(value) else {
        return false;
    };
    let Ok(conn) = get_db_connection() else {
        return false;
    };
    write_row(&conn, key, &json).is_ok()
}

#[cfg(target_arch = "wasm32")]
pub fn save_json_value<T: Serialize>(key: &str, value: &T) -> bool {
    LocalStorage::set(web_key(key), value).is_ok()
}

/// Reads a value saved with [`save_json_value`].
#[cfg(not(target_arch = "wasm32"))]
pub fn load_json_value<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let conn = get_db_connection().ok()?;
    serde_json::from_str(&read_row(&conn, key)?).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn load_json_value<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    LocalStorage::get(web_key(key)).ok()
}

/// A small JSON value that belongs to the device, not a profile (the
/// Jellyfin install id). Kept under the bare key, where it was before profiles.
/// (Tests use a fixed install id, so they never call these.)
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(test, allow(dead_code))]
pub fn save_device_value<T: Serialize>(key: &str, value: &T) -> bool {
    let (Ok(json), Ok(conn)) = (serde_json::to_string(value), get_db_connection()) else {
        return false;
    };
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [key, json.as_str()],
    )
    .is_ok()
}

#[cfg(target_arch = "wasm32")]
pub fn save_device_value<T: Serialize>(key: &str, value: &T) -> bool {
    LocalStorage::set(format!("rustysound.{key}"), value).is_ok()
}

/// Reads a value saved with [`save_device_value`].
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(test, allow(dead_code))]
pub fn load_device_value<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let conn = get_db_connection().ok()?;
    let json: String = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row: &rusqlite::Row| row.get(0))
        .ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn load_device_value<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    LocalStorage::get(format!("rustysound.{key}")).ok()
}

/// Jellyfin support added the server's kind and its Jellyfin sign-in to the
/// servers table. Existing rows become Subsonic servers.
#[cfg(not(target_arch = "wasm32"))]
fn add_server_kind_columns(conn: &rusqlite::Connection) -> Result<(), DbError> {
    let existing = conn
        .prepare("PRAGMA table_info(servers)")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| DbError::new(e.to_string()))?;
    for (column, default) in [("kind", "subsonic"), ("user_id", ""), ("access_token", "")] {
        if !existing.iter().any(|name| name == column) {
            conn.execute(
                &format!("ALTER TABLE servers ADD COLUMN {column} TEXT NOT NULL DEFAULT '{default}'"),
                [],
            )
            .map_err(|e| DbError::new(e.to_string()))?;
        }
    }
    Ok(())
}

/// Profiles gave every server row a `profile_id`, keyed together with the
/// server id (a backup restored into a second profile keeps its server ids,
/// so downloads and caches line up). The table is rebuilt once; existing
/// rows become the Default profile's. Running it again does nothing.
#[cfg(not(target_arch = "wasm32"))]
fn add_server_profile_column(conn: &mut rusqlite::Connection) -> Result<(), DbError> {
    let has_profile = conn
        .prepare("PRAGMA table_info(servers)")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|e| DbError::new(e.to_string()))?
        .iter()
        .any(|name| name == "profile_id");
    if has_profile {
        return Ok(());
    }
    let tx = conn.transaction().map_err(|e| DbError::new(e.to_string()))?;
    tx.execute_batch(&format!(
        "CREATE TABLE servers_by_profile (
            profile_id TEXT NOT NULL DEFAULT '{default}',
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            username TEXT NOT NULL,
            password TEXT NOT NULL,
            active TEXT NOT NULL DEFAULT '1',
            kind TEXT NOT NULL DEFAULT 'subsonic',
            user_id TEXT NOT NULL DEFAULT '',
            access_token TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (profile_id, id)
        );
        INSERT INTO servers_by_profile (profile_id, id, name, url, username, password, active, kind, user_id, access_token)
            SELECT '{default}', id, name, url, username, password, active, kind, user_id, access_token FROM servers;
        DROP TABLE servers;
        ALTER TABLE servers_by_profile RENAME TO servers;",
        default = profiles::DEFAULT_PROFILE_ID
    ))
    .map_err(|e| DbError::new(e.to_string()))?;
    tx.commit().map_err(|e| DbError::new(e.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn initialize_database() -> Result<(), DbError> {
    let mut conn = get_db_connection()?;

    // Create tables
    conn.execute(
        "CREATE TABLE IF NOT EXISTS servers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            url TEXT NOT NULL,
            username TEXT NOT NULL,
            password TEXT NOT NULL,
            active TEXT NOT NULL DEFAULT '1'
        )",
        [],
    )
    .map_err(|e| DbError::new(e.to_string()))?;
    add_server_kind_columns(&conn)?;
    add_server_profile_column(&mut conn)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| DbError::new(e.to_string()))?;
    profiles::migrate();

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn initialize_database() -> Result<(), StorageError> {
    profiles::migrate();
    Ok(())
}

/// Opens the app database. Also used by the local-music index, whose
/// background scans write here; the busy timeout lets a settings save wait
/// out one of their short transactions instead of failing.
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub(crate) fn get_db_connection() -> Result<rusqlite::Connection, DbError> {
    let data_dir = app_data_dir()
        .ok_or_else(|| DbError::new("Failed to resolve application data directory"))?;
    let db_path = data_dir.join("rustysound.db");

    let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
        DbError::new(format!(
            "Failed to open database at {}: {}",
            db_path.display(),
            e
        ))
    })?;
    let _ = conn.busy_timeout(std::time::Duration::from_secs(3));
    Ok(conn)
}

#[cfg(test)]
mod lyrics_provider_migration_tests {
    use super::*;

    fn order(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| key.to_string()).collect()
    }

    /// Saved settings JSON with the given extra fields (`""` for none).
    fn load(fields: &str) -> AppSettings {
        let base = r#""volume":0.5,"theme":"rusty","crossfade_enabled":false,
            "crossfade_duration":3,"replay_gain":false"#;
        let json = if fields.is_empty() {
            format!("{{{base}}}")
        } else {
            format!("{{{base},{fields}}}")
        };
        migrate_settings(serde_json::from_str(&json).unwrap())
    }

    #[test]
    fn saved_custom_order_gains_server_at_the_front() {
        let settings = load(r#""lyrics_provider_order":["genius","netease","lrclib"]"#);
        assert_eq!(
            settings.lyrics_provider_order,
            order(&["server", "genius", "netease", "lrclib"])
        );
        assert!(settings.lyrics_server_provider_added);
    }

    #[test]
    fn legacy_defaults_become_the_new_default() {
        let settings = load(r#""lyrics_provider_order":["netease","lrclib","genius"]"#);
        assert_eq!(settings.lyrics_provider_order, default_lyrics_provider_order());
        assert_eq!(settings.lyrics_provider_order[0], "server");
    }

    #[test]
    fn server_moved_down_stays_down() {
        // Migrated once already, then the user moved the server to the end.
        let settings = load(
            r#""lyrics_provider_order":["lrclib","genius","netease","server"],
                "lyrics_server_provider_added":true"#,
        );
        assert_eq!(
            settings.lyrics_provider_order,
            order(&["lrclib", "genius", "netease", "server"])
        );
        // And a second load changes nothing.
        let again = migrate_settings(settings.clone());
        assert_eq!(again.lyrics_provider_order, settings.lyrics_provider_order);
    }

    #[test]
    fn migration_runs_once_even_if_reloaded() {
        let first = load(r#""lyrics_provider_order":["lrclib","genius","netease"]"#);
        let second = migrate_settings(first.clone());
        assert_eq!(first.lyrics_provider_order, second.lyrics_provider_order);
        assert_eq!(
            first.lyrics_provider_order.iter().filter(|k| *k == "server").count(),
            1
        );
    }

    #[test]
    fn new_settings_start_with_server_first() {
        let settings = migrate_settings(AppSettings::default());
        assert_eq!(settings.lyrics_provider_order, default_lyrics_provider_order());
        let fresh = load("");
        assert_eq!(fresh.lyrics_provider_order, default_lyrics_provider_order());
    }
}

#[cfg(test)]
mod artwork_preference_tests {
    use super::*;

    #[test]
    fn song_covers_follow_the_preference() {
        let song = Song {
            cover_art: Some("mf-abc".into()),
            album_id: Some(" al-1 ".into()),
            ..Default::default()
        };
        use ArtworkDownloadPreference::*;
        assert_eq!(PreferServer.song_covers(&song), vec!["mf-abc", "al-1"]);
        assert_eq!(ServerOnly.song_covers(&song), vec!["al-1"]);
        assert_eq!(Id3Only.song_covers(&song), vec!["mf-abc"]);
        let same = Song {
            cover_art: Some("al-1".into()),
            album_id: Some("al-1".into()),
            ..Default::default()
        };
        assert_eq!(PreferId3.song_covers(&same), vec!["al-1"]);
        assert!(PreferServer.song_covers(&Song::default()).is_empty());
    }
}

#[cfg(test)]
mod discovery_settings_tests {
    use super::*;

    #[test]
    fn old_settings_load_with_discovery_defaults() {
        // Settings JSON saved before the discovery fields existed, with
        // fields that have since been dropped.
        let legacy = r#"{"volume":0.5,"last_server_id":null,"theme":"rusty",
            "crossfade_enabled":false,"crossfade_duration":3,"replay_gain":false,
            "home_layout_json":"","home_feed_load_profile":"super",
            "discover_rewind_months":12}"#;
        let settings: AppSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(settings.volume, 0.5);
        assert_eq!(settings.lastfm_api_key, "");
        assert_eq!(settings.lastfm_username, "");
        assert_eq!(settings.listenbrainz_username, "");
        assert!(!settings.discovery_offserver_enabled);
        assert!(settings.discovery_previews_enabled);
        assert_eq!(settings.discover_saved_mix_playlists, "{}");
    }

    #[test]
    fn discovery_fields_round_trip() {
        let mut settings = AppSettings::default();
        settings.lastfm_username = "rj".to_string();
        settings.discovery_previews_enabled = false;
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, settings);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod server_kind_migration_tests {
    use super::*;
    use crate::api::ServerKind;

    fn kinds(conn: &rusqlite::Connection) -> Vec<(String, String, String)> {
        conn.prepare("SELECT kind, user_id, access_token FROM servers ORDER BY id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn saved_servers_load_as_subsonic_and_jellyfin_round_trips() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        // The table as it was before Jellyfin support, with one saved server.
        conn.execute(
            "CREATE TABLE servers (id TEXT PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL,
             username TEXT NOT NULL, password TEXT NOT NULL, active TEXT NOT NULL DEFAULT '1')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO servers VALUES ('a', 'Home', 'http://h', 'u', 'p', '1')", []).unwrap();
        add_server_kind_columns(&conn).unwrap();
        // Running it again is harmless.
        add_server_kind_columns(&conn).unwrap();
        assert_eq!(kinds(&conn), vec![("subsonic".into(), String::new(), String::new())]);

        let jelly = ServerConfig {
            id: "b".into(),
            ..ServerConfig::jellyfin("Jelly".into(), "http://j".into(), "u".into(), "uid".into(), "tok".into())
        };
        add_server_profile_column(&mut conn).unwrap();
        save_servers_inner(&mut conn, profiles::DEFAULT_PROFILE_ID, &[jelly]).unwrap();
        assert_eq!(kinds(&conn), vec![("jellyfin".into(), "uid".into(), "tok".into())]);
        assert_eq!(ServerKind::from_stored("jellyfin"), ServerKind::Jellyfin);
        assert_eq!(ServerKind::from_stored("anything"), ServerKind::Subsonic);
    }

    #[test]
    fn servers_move_into_the_default_profile_once_and_profiles_keep_their_own() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE servers (id TEXT PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL,
             username TEXT NOT NULL, password TEXT NOT NULL, active TEXT NOT NULL DEFAULT '1')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO servers VALUES ('a', 'Home', 'http://h', 'u', 'p', '1')", []).unwrap();
        add_server_kind_columns(&conn).unwrap();
        add_server_profile_column(&mut conn).unwrap();
        // Running it again is harmless and keeps the row.
        add_server_profile_column(&mut conn).unwrap();
        let default = load_servers_inner(&conn, profiles::DEFAULT_PROFILE_ID).unwrap();
        assert_eq!(default.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Home"]);
        assert!(load_servers_inner(&conn, "kid").unwrap().is_empty());

        // The same server id can live in two profiles; saving one leaves the other.
        let mut copy = default[0].clone();
        copy.name = "Kid's Home".into();
        save_servers_inner(&mut conn, "kid", &[copy]).unwrap();
        save_servers_inner(&mut conn, profiles::DEFAULT_PROFILE_ID, &default).unwrap();
        assert_eq!(load_servers_inner(&conn, "kid").unwrap()[0].name, "Kid's Home");
        assert_eq!(load_servers_inner(&conn, profiles::DEFAULT_PROFILE_ID).unwrap()[0].name, "Home");
        save_servers_inner(&mut conn, "kid", &[]).unwrap();
        assert_eq!(load_servers_inner(&conn, profiles::DEFAULT_PROFILE_ID).unwrap().len(), 1);
    }

    #[test]
    fn web_saved_servers_without_a_kind_are_subsonic() {
        let old = r#"[{"id":"a","name":"A","url":"http://a","username":"u","password":"p","active":true}]"#;
        let servers: Vec<ServerConfig> = serde_json::from_str(old).unwrap();
        assert_eq!(servers[0].kind, ServerKind::Subsonic);
        assert!(servers[0].access_token.is_empty());
        // Subsonic servers save without the Jellyfin fields.
        let json = serde_json::to_string(&servers[0]).unwrap();
        assert!(json.contains(r#""kind":"subsonic""#) && !json.contains("access_token"));
    }
}
