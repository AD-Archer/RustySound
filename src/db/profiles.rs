//! Profiles: several people on one device, each with their own servers,
//! settings, queue, offline changes, recent searches and downloads list.
//!
//! * The list of profiles, and which one is active, lives outside every
//!   profile (native: the `profiles` row of the settings table; web:
//!   localStorage `rustysound.profiles`).
//! * Everything a profile owns is stored under [`scoped_key`]. The Default
//!   profile keeps the keys the app used before profiles, so existing data
//!   already is the Default profile: the migration only writes the list,
//!   running it twice changes nothing, and an older build still finds its
//!   data. Other profiles prefix their keys with `profile.<id>.`.
//! * Native servers carry a `profile_id` column instead (`db::mod`).
//! * Downloaded audio is shared (files are files); each profile keeps its own
//!   index of what it downloaded (`offline_audio`).
//! * The API response cache is keyed by server id, and servers belong to a
//!   profile, so cached favourites and history follow the profile too.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

/// The profile every install starts with. It owns the pre-profile keys.
pub const DEFAULT_PROFILE_ID: &str = "default";
const DEFAULT_PROFILE_NAME: &str = "Default";
/// Where the list is kept: a settings row (native) or `rustysound.profiles` (web).
const PROFILES_KEY: &str = "profiles";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub created_at_ms: i64,
}

impl Profile {
    /// The letter on the profile's avatar.
    pub fn initial(&self) -> String {
        self.name
            .trim()
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_else(|| "?".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileList {
    pub profiles: Vec<Profile>,
    pub active: String,
}

impl Default for ProfileList {
    fn default() -> Self {
        Self { profiles: Vec::new(), active: DEFAULT_PROFILE_ID.to_string() }.migrated()
    }
}

impl ProfileList {
    /// Puts the Default profile first when it's missing and points `active`
    /// at a profile that exists. The migration from before profiles; safe to
    /// run on every load.
    pub fn migrated(mut self) -> Self {
        if !self.profiles.iter().any(|p| p.id == DEFAULT_PROFILE_ID) {
            self.profiles.insert(
                0,
                Profile {
                    id: DEFAULT_PROFILE_ID.to_string(),
                    name: DEFAULT_PROFILE_NAME.to_string(),
                    created_at_ms: 0,
                },
            );
        }
        let mut seen = std::collections::HashSet::new();
        self.profiles.retain(|p| !p.id.trim().is_empty() && seen.insert(p.id.clone()));
        if !self.profiles.iter().any(|p| p.id == self.active) {
            self.active = DEFAULT_PROFILE_ID.to_string();
        }
        self
    }

    pub fn active_profile(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active)
            .unwrap_or(&self.profiles[0])
    }

    /// Profiles only show once there is more than one.
    pub fn is_shared(&self) -> bool {
        self.profiles.len() > 1
    }

    /// Adds a profile called `name` ("Profile 2" when blank) and returns it.
    pub fn add(&mut self, name: &str, now_ms: i64) -> Profile {
        let name = match name.trim() {
            "" => format!("Profile {}", self.profiles.len() + 1),
            trimmed => trimmed.to_string(),
        };
        let profile = Profile {
            id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
            name,
            created_at_ms: now_ms,
        };
        self.profiles.push(profile.clone());
        profile
    }

    /// Removes a profile. The Default profile and the active one stay.
    pub fn remove(&mut self, id: &str) -> bool {
        if id == DEFAULT_PROFILE_ID || id == self.active {
            return false;
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        self.profiles.len() != before
    }
}

/// The key `key` is stored under for `profile_id` (see the module docs).
pub fn scoped_key(profile_id: &str, key: &str) -> String {
    if profile_id == DEFAULT_PROFILE_ID || profile_id.is_empty() {
        key.to_string()
    } else {
        format!("profile.{profile_id}.{key}")
    }
}

/// A per-profile file name: `download_index.json` for Default,
/// `profile.<id>.download_index.json` for the others.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn scoped_file_name(profile_id: &str, file_name: &str) -> String {
    scoped_key(profile_id, file_name)
}

static ACTIVE: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(load_profiles().active));

/// The active profile's id. Read from storage the first time, so code that
/// loads data before the app shell starts gets the right profile.
pub fn active_profile_id() -> String {
    ACTIVE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// [`scoped_key`] for the active profile.
pub fn active_key(key: &str) -> String {
    scoped_key(&active_profile_id(), key)
}

/// The saved list, migrated. Never fails: unreadable storage is one Default profile.
pub fn load_profiles() -> ProfileList {
    read_list().unwrap_or_default().migrated()
}

/// Saves the list and makes its `active` profile the one storage reads.
pub fn save_profiles(list: &ProfileList) -> bool {
    let list = list.clone().migrated();
    let saved = write_list(&list);
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = list.active;
    saved
}

#[cfg(not(target_arch = "wasm32"))]
fn read_list() -> Option<ProfileList> {
    let conn = super::get_db_connection().ok()?;
    let json: String = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [PROFILES_KEY], |row| row.get(0))
        .ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(not(target_arch = "wasm32"))]
fn write_list(list: &ProfileList) -> bool {
    let (Ok(conn), Ok(json)) = (super::get_db_connection(), serde_json::to_string(list)) else {
        return false;
    };
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        [PROFILES_KEY, json.as_str()],
    )
    .is_ok()
}

#[cfg(target_arch = "wasm32")]
fn read_list() -> Option<ProfileList> {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::get(format!("rustysound.{PROFILES_KEY}")).ok()
}

#[cfg(target_arch = "wasm32")]
fn write_list(list: &ProfileList) -> bool {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::set(format!("rustysound.{PROFILES_KEY}"), list).is_ok()
}

/// Writes the list on first launch with profiles (the Default profile).
/// Later launches find it and change nothing.
pub fn migrate() {
    if read_list().is_none() {
        let _ = write_list(&ProfileList::default());
    }
}

/// Deletes what a removed profile stored: its settings rows or keys and its
/// servers. Its downloads index goes through `offline_audio`.
pub fn delete_profile_data(profile_id: &str) {
    if profile_id == DEFAULT_PROFILE_ID || profile_id.is_empty() {
        return;
    }
    let prefix = scoped_key(profile_id, "");
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(conn) = super::get_db_connection() {
        let _ = conn.execute(
            "DELETE FROM settings WHERE substr(key, 1, length(?1)) = ?1",
            [prefix.as_str()],
        );
        let _ = conn.execute("DELETE FROM servers WHERE profile_id = ?1", [profile_id]);
    }
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_storage::{LocalStorage, Storage};
        let storage = LocalStorage::raw();
        let full = format!("rustysound.{prefix}");
        let keys: Vec<String> = (0..storage.length().unwrap_or(0))
            .filter_map(|i| storage.key(i).ok().flatten())
            .filter(|key| key.starts_with(&full))
            .collect();
        for key in keys {
            let _ = storage.remove_item(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_owns_the_old_keys_and_others_are_prefixed() {
        assert_eq!(scoped_key(DEFAULT_PROFILE_ID, "app_settings"), "app_settings");
        assert_eq!(scoped_key("ab12", "app_settings"), "profile.ab12.app_settings");
        assert_eq!(scoped_file_name("ab12", "download_index.json"), "profile.ab12.download_index.json");
        assert_ne!(scoped_key("a", "x"), scoped_key("b", "x"));
    }

    #[test]
    fn migrating_adds_default_once_and_fixes_the_active_profile() {
        let empty = ProfileList { profiles: Vec::new(), active: "gone".into() }.migrated();
        assert_eq!(empty.profiles.len(), 1);
        assert_eq!(empty.active, DEFAULT_PROFILE_ID);
        assert!(!empty.is_shared());
        // Running it again is harmless.
        assert_eq!(empty.clone().migrated(), empty);

        let mut list = empty;
        let kid = list.add("  Sam ", 5);
        assert_eq!(kid.name, "Sam");
        assert_eq!(kid.initial(), "S");
        list.active = kid.id.clone();
        let again = list.clone().migrated();
        assert_eq!(again, list, "a valid list survives the migration unchanged");
        assert!(again.is_shared());
        assert_eq!(again.active_profile().name, "Sam");
    }

    #[test]
    fn blank_names_get_a_number_and_default_and_active_stay() {
        let mut list = ProfileList::default();
        let second = list.add("", 0);
        assert_eq!(second.name, "Profile 2");
        assert!(!list.remove(DEFAULT_PROFILE_ID));
        list.active = second.id.clone();
        assert!(!list.remove(&second.id), "the active profile can't be removed");
        list.active = DEFAULT_PROFILE_ID.into();
        assert!(list.remove(&second.id));
        assert!(!list.is_shared());
    }

    #[test]
    fn duplicate_ids_collapse() {
        let p = Profile { id: "x".into(), name: "X".into(), created_at_ms: 0 };
        let list = ProfileList { profiles: vec![p.clone(), p], active: "x".into() }.migrated();
        assert_eq!(list.profiles.iter().filter(|p| p.id == "x").count(), 1);
    }
}
