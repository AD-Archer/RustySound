//! Palettes remembered per cover: in memory for the session and in a small
//! persistent store (localStorage on the web, a JSON file in the cache
//! folder on Mac and iPhone), so a cover is sampled once, ever.

use super::colour::ArtPalette;
use std::collections::HashMap;
use std::sync::Mutex;

/// Bumped when the palette rules change, so old entries are dropped.
const VERSION: u32 = 1;
/// Entries kept on disk; the oldest go first.
const LIMIT: usize = 400;
const STORAGE_KEY: &str = "rustysound.art-palettes";

/// The cache key for a cover: its server and cover id. Size is left out on
/// purpose (every size of a cover has the same colours).
pub fn palette_key(server_id: &str, cover_art: &str) -> String {
    format!("v{VERSION}:{}:{}", server_id.trim(), cover_art.trim())
}

/// Insertion-ordered map of key to palette, capped at `LIMIT`.
#[derive(Default)]
pub struct PaletteStore {
    order: Vec<String>,
    map: HashMap<String, ArtPalette>,
}

impl PaletteStore {
    pub fn get(&self, key: &str) -> Option<ArtPalette> {
        self.map.get(key).copied()
    }

    pub fn insert(&mut self, key: String, palette: ArtPalette) {
        if self.map.insert(key.clone(), palette).is_none() {
            self.order.push(key);
        }
        while self.order.len() > LIMIT {
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }

    /// `[[key, "five hex colours"], …]`, oldest first.
    pub fn to_json(&self) -> String {
        let rows: Vec<(&str, String)> = self
            .order
            .iter()
            .filter_map(|key| Some((key.as_str(), self.map.get(key)?.to_compact())))
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string())
    }

    /// Reads `to_json` output, skipping rows of an older version.
    pub fn from_json(raw: &str) -> PaletteStore {
        let rows: Vec<(String, String)> = serde_json::from_str(raw).unwrap_or_default();
        let prefix = format!("v{VERSION}:");
        let mut store = PaletteStore::default();
        for (key, line) in rows {
            if !key.starts_with(&prefix) {
                continue;
            }
            if let Some(palette) = ArtPalette::from_compact(&line) {
                store.insert(key, palette);
            }
        }
        store
    }
}

static STORE: Mutex<Option<PaletteStore>> = Mutex::new(None);

fn with_store<T>(f: impl FnOnce(&mut PaletteStore) -> T) -> T {
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    let store = guard.get_or_insert_with(|| PaletteStore::from_json(&load().unwrap_or_default()));
    f(store)
}

/// The palette for `key`, from memory (the store is read from disk on
/// first use: one small file).
pub fn lookup(key: &str) -> Option<ArtPalette> {
    with_store(|store| store.get(key))
}

/// Loads the store, so the first lookup doesn't read the file.
#[cfg(not(target_arch = "wasm32"))]
pub fn warm() {
    with_store(|_| ());
}

/// Remembers a palette and returns the store's JSON for [`persist`].
pub fn remember(key: String, palette: ArtPalette) -> String {
    with_store(|store| {
        store.insert(key, palette);
        store.to_json()
    })
}

#[cfg(target_arch = "wasm32")]
fn load() -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(STORAGE_KEY).ok()?
}

/// Writes the store's JSON. Small (tens of kilobytes at most).
#[cfg(target_arch = "wasm32")]
pub fn persist(json: String) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(STORAGE_KEY, &json);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file() -> Option<std::path::PathBuf> {
    crate::storage::app_cache_dir().map(|dir| dir.join(format!("{STORAGE_KEY}.json")))
}

#[cfg(not(target_arch = "wasm32"))]
fn load() -> Option<String> {
    std::fs::read_to_string(file()?).ok()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn persist(json: String) {
    if let Some(path) = file() {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::super::colour::Rgb;
    use super::*;

    fn palette(n: u8) -> ArtPalette {
        let c = Rgb(n, n, n);
        ArtPalette { glow: c, tint: c, stage: c, light: c, dark: c }
    }

    #[test]
    fn keys_name_the_cover_not_the_size() {
        assert_eq!(palette_key("srv-1", "al-9_ab12cd34"), "v1:srv-1:al-9_ab12cd34");
        assert_eq!(palette_key(" srv-1 ", "al-9 "), palette_key("srv-1", "al-9"));
        assert_ne!(palette_key("srv-1", "al-9"), palette_key("srv-2", "al-9"), "per server");
        assert_ne!(palette_key("srv-1", "al-9"), palette_key("srv-1", "al-10"));
    }

    #[test]
    fn store_round_trips_and_drops_old_versions() {
        let mut store = PaletteStore::default();
        store.insert(palette_key("s", "a"), palette(10));
        store.insert(palette_key("s", "b"), palette(20));
        let json = store.to_json();
        let back = PaletteStore::from_json(&json);
        assert_eq!(back.get(&palette_key("s", "a")), Some(palette(10)));
        assert_eq!(back.get(&palette_key("s", "b")), Some(palette(20)));

        let old = json.replace("v1:", "v0:");
        assert!(PaletteStore::from_json(&old).get(&palette_key("s", "a")).is_none());
        assert!(PaletteStore::from_json("not json").map.is_empty());
    }

    #[test]
    fn store_keeps_the_newest_entries() {
        let mut store = PaletteStore::default();
        for i in 0..(LIMIT + 5) {
            store.insert(format!("v1:s:{i}"), palette((i % 250) as u8));
        }
        assert_eq!(store.order.len(), LIMIT);
        assert!(store.get("v1:s:0").is_none());
        assert!(store.get(&format!("v1:s:{}", LIMIT + 4)).is_some());
        // Re-inserting a key doesn't duplicate it.
        store.insert(format!("v1:s:{}", LIMIT + 4), palette(1));
        assert_eq!(store.order.len(), LIMIT);
    }
}
