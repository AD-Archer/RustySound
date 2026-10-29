//! Discover caching. Two layers:
//! 1. an in-memory memo (always on, so mixes stay stable within a session even
//!    when the user disabled the disk cache), and
//! 2. the app's persistent cache (`cache_service`), when enabled.
//!
//! Keys embed the local date and a per-day "salt" that [`bump_salt`] changes on
//! an explicit refresh, so a day's results never reshuffle on their own.

use crate::api::models::ServerConfig;
use crate::cache_service::{get_json, put_json, remove_by_prefix};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// Prefix for every Discover section entry in the persistent cache.
pub const SECTION_PREFIX: &str = "view:discover:v1:";
const SALT_PREFIX: &str = "view:discover-salt:v1:";
/// Section results live a day; the date in the key retires them anyway.
const SECTION_TTL_HOURS: u32 = 24;

static MEMO: Lazy<Mutex<HashMap<String, Vec<u8>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static SALTS: Lazy<Mutex<HashMap<String, u32>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Sorted active-server signature, mirroring the home feed's cache keys.
pub fn servers_signature(servers: &[ServerConfig]) -> String {
    let mut ids: Vec<&str> = servers.iter().map(|s| s.id.as_str()).collect();
    ids.sort_unstable();
    ids.join("|")
}

/// Full cache key for one section.
pub fn section_key(section: &str, date: &str, salt: u32, servers: &str, config: &str) -> String {
    format!("{SECTION_PREFIX}{section}:{date}:{salt}:{servers}:{config}")
}

/// Read from memo, then the persistent cache.
pub fn get<T: DeserializeOwned>(key: &str) -> Option<T> {
    {
        let memo = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(bytes) = memo.get(key) {
            if let Ok(value) = serde_json::from_slice::<T>(bytes) {
                return Some(value);
            }
        }
    }
    get_json::<T>(key)
}

/// Write to memo and the persistent cache.
pub fn put<T: Serialize>(key: &str, value: &T) {
    if let Ok(bytes) = serde_json::to_vec(value) {
        let mut memo = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        // Keep the memo small: only Discover entries, a few days at most.
        if memo.len() > 64 {
            memo.clear();
        }
        memo.insert(key.to_string(), bytes);
    }
    let _ = put_json(key.to_string(), value, Some(SECTION_TTL_HOURS));
}

/// Today's salt (0 until the user refreshes).
pub fn current_salt(date: &str) -> u32 {
    if let Some(salt) = SALTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(date)
        .copied()
    {
        return salt;
    }
    let salt = get_json::<u32>(&format!("{SALT_PREFIX}{date}")).unwrap_or(0);
    SALTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(date.to_string(), salt);
    salt
}

/// Start a new generation for `date`: new salt, drop Discover entries and the
/// album-list caches the library snapshot reads, so the next load refetches.
pub fn bump_salt(date: &str, servers: &[ServerConfig]) -> u32 {
    let next = current_salt(date).wrapping_add(1);
    SALTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(date.to_string(), next);
    let _ = put_json(format!("{SALT_PREFIX}{date}"), &next, Some(48));
    MEMO.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = remove_by_prefix(SECTION_PREFIX);
    for server in servers {
        let _ = remove_by_prefix(&format!("api:getAlbumList2:v1:{}:", server.id));
    }
    next
}

/// Memo-only put/get for intermediate data (library snapshot) that must not
/// touch the disk cache.
pub fn memo_put<T: Serialize>(key: &str, value: &T) {
    if let Ok(bytes) = serde_json::to_vec(value) {
        let mut memo = MEMO.lock().unwrap_or_else(|e| e.into_inner());
        if memo.len() > 64 {
            memo.clear();
        }
        memo.insert(key.to_string(), bytes);
    }
}

/// See [`memo_put`].
pub fn memo_get<T: DeserializeOwned>(key: &str) -> Option<T> {
    let memo = MEMO.lock().unwrap_or_else(|e| e.into_inner());
    memo.get(key)
        .and_then(|bytes| serde_json::from_slice::<T>(bytes).ok())
}
