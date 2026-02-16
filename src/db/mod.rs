use crate::api::{default_lyrics_provider_order, models::ServerConfig};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use gloo_storage::{errors::StorageError, LocalStorage, Storage};

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

#[cfg(target_arch = "wasm32")]
const SETTINGS_KEY: &str = "rustysound.app_settings";
#[cfg(target_arch = "wasm32")]
const PLAYBACK_KEY: &str = "rustysound.playback_state";
#[cfg(target_arch = "wasm32")]
const SERVERS_KEY: &str = "rustysound.servers";

/// Repeat mode for playback
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

/// App settings stored in the database
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub volume: f64,
    pub last_server_id: Option<String>,
    pub theme: String,
    pub crossfade_enabled: bool,
    pub crossfade_duration: u32, // seconds
    pub replay_gain: bool,
    #[serde(default)]
    pub shuffle_enabled: bool,
    #[serde(default)]
    pub repeat_mode: RepeatMode,
    #[serde(default)]
    pub cache_enabled: bool,
    #[serde(default)]
    pub cache_size_mb: u32,
    #[serde(default)]
    pub cache_expiry_hours: u32,
    #[serde(default)]
    pub cache_images_enabled: bool,
    #[serde(default = "default_lyrics_provider_order")]
    pub lyrics_provider_order: Vec<String>,
    #[serde(default = "default_lyrics_request_timeout_secs")]
    pub lyrics_request_timeout_secs: u32,
    #[serde(default)]
    pub lyrics_offset_ms: i32,
    #[serde(default)]
    pub lyrics_unsynced_mode: bool,
}

fn default_lyrics_request_timeout_secs() -> u32 {
    4
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            volume: 0.8,
            last_server_id: None,
            theme: "dark".to_string(),
            crossfade_enabled: false,
            crossfade_duration: 3,
            replay_gain: false,
            shuffle_enabled: false,
            repeat_mode: RepeatMode::Off,
            cache_enabled: true,
            cache_size_mb: 100,
            cache_expiry_hours: 24,
            cache_images_enabled: true,
            lyrics_provider_order: default_lyrics_provider_order(),
            lyrics_request_timeout_secs: default_lyrics_request_timeout_secs(),
            lyrics_offset_ms: 0,
            lyrics_unsynced_mode: false,
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

// Database operations for native platforms
// These run directly on desktop/mobile without needing #[server]

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_servers(servers: Vec<ServerConfig>) -> Result<(), DbError> {
    let conn = get_db_connection()?;

    // Clear existing servers and insert new ones
    conn.execute("DELETE FROM servers", [])
        .map_err(|e| DbError::new(e.to_string()))?;

    for server in servers {
        conn.execute(
            "INSERT INTO servers (id, name, url, username, password, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            [
                &server.id,
                &server.name,
                &server.url,
                &server.username,
                &server.password,
                &(if server.active { "1" } else { "0" }).to_string(),
            ],
        ).map_err(|e| DbError::new(e.to_string()))?;
    }

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn save_servers(servers: Vec<ServerConfig>) -> Result<(), StorageError> {
    LocalStorage::set(SERVERS_KEY, servers).map_err(|e| e)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_servers() -> Result<Vec<ServerConfig>, DbError> {
    let conn = get_db_connection()?;

    let mut stmt = conn
        .prepare("SELECT id, name, url, username, password, active FROM servers")
        .map_err(|e| DbError::new(e.to_string()))?;

    let servers = stmt
        .query_map([], |row: &rusqlite::Row| {
            Ok(ServerConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                username: row.get(3)?,
                password: row.get(4)?,
                active: row.get::<_, String>(5)? == "1",
            })
        })
        .map_err(|e| DbError::new(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(servers)
}

#[cfg(target_arch = "wasm32")]
pub async fn load_servers() -> Result<Vec<ServerConfig>, StorageError> {
    match LocalStorage::get(SERVERS_KEY) {
        Ok(servers) => Ok(servers),
        Err(_) => Ok(Vec::new()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_settings(settings: AppSettings) -> Result<(), DbError> {
    let conn = get_db_connection()?;

    let settings_json =
        serde_json::to_string(&settings).map_err(|e| DbError::new(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('app_settings', ?1)",
        [&settings_json],
    )
    .map_err(|e| DbError::new(e.to_string()))?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn save_settings(settings: AppSettings) -> Result<(), StorageError> {
    LocalStorage::set(SETTINGS_KEY, settings).map_err(|e| e)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_settings() -> Result<AppSettings, DbError> {
    let conn = get_db_connection()?;

    let result: Result<String, rusqlite::Error> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'app_settings'",
        [],
        |row: &rusqlite::Row| row.get(0),
    );

    match result {
        Ok(json) => serde_json::from_str(&json).map_err(|e| DbError::new(e.to_string())),
        Err(_) => Ok(AppSettings::default()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn load_settings() -> Result<AppSettings, StorageError> {
    match LocalStorage::get(SETTINGS_KEY) {
        Ok(settings) => Ok(settings),
        Err(_) => Ok(AppSettings::default()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save_playback_state(state: PlaybackState) -> Result<(), DbError> {
    let conn = get_db_connection()?;

    let state_json = serde_json::to_string(&state).map_err(|e| DbError::new(e.to_string()))?;

    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('playback_state', ?1)",
        [&state_json],
    )
    .map_err(|e| DbError::new(e.to_string()))?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn save_playback_state(state: PlaybackState) -> Result<(), StorageError> {
    LocalStorage::set(PLAYBACK_KEY, state).map_err(|e| e)
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn load_playback_state() -> Result<PlaybackState, DbError> {
    let conn = get_db_connection()?;

    let result: Result<String, rusqlite::Error> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'playback_state'",
        [],
        |row: &rusqlite::Row| row.get(0),
    );

    match result {
        Ok(json) => serde_json::from_str(&json).map_err(|e| DbError::new(e.to_string())),
        Err(_) => Ok(PlaybackState::default()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn load_playback_state() -> Result<PlaybackState, StorageError> {
    match LocalStorage::get(PLAYBACK_KEY) {
        Ok(state) => Ok(state),
        Err(_) => Ok(PlaybackState::default()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn initialize_database() -> Result<(), DbError> {
    let conn = get_db_connection()?;

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

    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| DbError::new(e.to_string()))?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub async fn initialize_database() -> Result<(), StorageError> {
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn get_db_connection() -> Result<rusqlite::Connection, DbError> {
    use std::path::PathBuf;

    // Get data directory
    let data_dir = dirs_next().unwrap_or_else(|| PathBuf::from("."));
    let db_path = data_dir.join("rustysound.db");

    rusqlite::Connection::open(&db_path)
        .map_err(|e| DbError::new(format!("Failed to open database: {}", e)))
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
fn dirs_next() -> Option<std::path::PathBuf> {
    // Use proper application data directory for each platform
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let data_dir = std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("com.adarcher.rustysound");
            std::fs::create_dir_all(&data_dir).ok()?;
            Some(data_dir)
        } else {
            None
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(app_data) = std::env::var("APPDATA") {
            let data_dir = std::path::PathBuf::from(app_data).join("RustySound");
            std::fs::create_dir_all(&data_dir).ok()?;
            Some(data_dir)
        } else {
            None
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let data_dir = std::path::PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("rustysound");
            std::fs::create_dir_all(&data_dir).ok()?;
            Some(data_dir)
        } else {
            None
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        // Fallback for other platforms
        if let Ok(home) = std::env::var("HOME") {
            let data_dir = std::path::PathBuf::from(home).join(".rustysound");
            std::fs::create_dir_all(&data_dir).ok()?;
            Some(data_dir)
        } else {
            let data_dir = std::path::PathBuf::from(".rustysound");
            std::fs::create_dir_all(&data_dir).ok()?;
            Some(data_dir)
        }
    }
}
