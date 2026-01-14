//! Simple cache system for RustySound
//! Provides caching for API responses and images

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Cache entry with expiration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub data: Vec<u8>,
    pub content_type: String,
    pub timestamp: SystemTime,
    pub expiry: Duration,
}

impl CacheEntry {
    pub fn new(data: Vec<u8>, content_type: String, expiry: Duration) -> Self {
        Self {
            data,
            content_type,
            timestamp: SystemTime::now(),
            expiry,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.timestamp.elapsed().unwrap_or(Duration::MAX) > self.expiry
    }

    pub fn size_bytes(&self) -> usize {
        self.data.len() + self.content_type.len() + std::mem::size_of::<SystemTime>() + std::mem::size_of::<Duration>()
    }
}

/// Simple LRU cache with size limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimpleCache {
    entries: HashMap<String, CacheEntry>,
    max_size_bytes: usize,
    current_size_bytes: usize,
}

impl SimpleCache {
    pub fn new(max_size_mb: u32) -> Self {
        Self {
            entries: HashMap::new(),
            max_size_bytes: (max_size_mb as usize) * 1024 * 1024,
            current_size_bytes: 0,
        }
    }

    pub fn get(&self, key: &str) -> Option<&CacheEntry> {
        self.entries.get(key).filter(|entry| !entry.is_expired())
    }

    pub fn put(&mut self, key: String, entry: CacheEntry) {
        // Remove expired entries first
        self.clean_expired();

        let entry_size = entry.size_bytes();

        // If adding this entry would exceed max size, remove oldest entries
        while self.current_size_bytes + entry_size > self.max_size_bytes && !self.entries.is_empty() {
            // Remove the first entry (simple FIFO eviction)
            if let Some((key_to_remove, entry_to_remove)) = self.entries.iter().next() {
                let key_to_remove = key_to_remove.clone();
                let size_to_remove = entry_to_remove.size_bytes();
                self.entries.remove(&key_to_remove);
                self.current_size_bytes = self.current_size_bytes.saturating_sub(size_to_remove);
            }
        }

        // Remove existing entry if it exists
        if let Some(old_entry) = self.entries.remove(&key) {
            self.current_size_bytes = self.current_size_bytes.saturating_sub(old_entry.size_bytes());
        }

        // Add new entry
        self.entries.insert(key, entry);
        self.current_size_bytes += entry_size;
    }

    pub fn remove(&mut self, key: &str) -> bool {
        if let Some(entry) = self.entries.remove(key) {
            self.current_size_bytes = self.current_size_bytes.saturating_sub(entry.size_bytes());
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size_bytes = 0;
    }

    pub fn clean_expired(&mut self) {
        let expired_keys: Vec<String> = self.entries
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size_bytes = self.current_size_bytes.saturating_sub(entry.size_bytes());
            }
        }
    }

    pub fn size_bytes(&self) -> usize {
        self.current_size_bytes
    }

    pub fn max_size_bytes(&self) -> usize {
        self.max_size_bytes
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.entries.len(),
            total_size_bytes: self.size_bytes(),
            max_size_bytes: self.max_size_bytes,
        }
    }
}

/// Cache utility functions for common operations
pub mod utils {
    use super::*;
    use std::time::Duration;

    /// Get cache expiry duration from hours
    pub fn expiry_from_hours(hours: u32) -> Duration {
        Duration::from_secs(hours as u64 * 3600)
    }

    /// Cache an image with default expiry
    #[allow(dead_code)]
    pub fn cache_image(cache: &mut SimpleCache, key: String, data: Vec<u8>, content_type: String, expiry_hours: u32) {
        let expiry = expiry_from_hours(expiry_hours);
        let entry = CacheEntry::new(data, content_type, expiry);
        cache.put(key, entry);
    }

    /// Get cached image data
    pub fn get_cached_image<'a>(cache: &'a SimpleCache, key: &str) -> Option<&'a [u8]> {
        cache.get(key).map(|entry| entry.data.as_slice())
    }

    /// Cache API response
    pub fn cache_api_response(cache: &mut SimpleCache, key: String, data: Vec<u8>, expiry_hours: u32) {
        let expiry = expiry_from_hours(expiry_hours);
        let entry = CacheEntry::new(data, "application/json".to_string(), expiry);
        cache.put(key, entry);
    }

    /// Get cached API response
    pub fn get_cached_api_response<'a>(cache: &'a SimpleCache, key: &str) -> Option<&'a [u8]> {
        cache.get(key).map(|entry| entry.data.as_slice())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size_bytes: usize,
    pub max_size_bytes: usize,
}

impl Default for SimpleCache {
    fn default() -> Self {
        Self::new(100) // 100MB default
    }
}

/// Cache key generation utilities
pub mod keys {
    use base64::{Engine as _, engine::general_purpose};

    pub fn album_cover(album_id: &str, server_id: &str, size: u32) -> String {
        format!("album_cover:{server_id}:{album_id}:{size}")
    }

    pub fn artist_cover(artist_id: &str, server_id: &str, size: u32) -> String {
        format!("artist_cover:{server_id}:{artist_id}:{size}")
    }

    pub fn song_cover(song_id: &str, server_id: &str, size: u32) -> String {
        format!("song_cover:{server_id}:{song_id}:{size}")
    }

    pub fn api_response(endpoint: &str, params: &str) -> String {
        let combined = format!("{endpoint}:{params}");
        format!("api:{}", general_purpose::URL_SAFE_NO_PAD.encode(combined))
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_impl {
    use super::*;
    use wasm_bindgen::JsValue;
    use web_sys::{window, Storage};

    impl SimpleCache {
        pub fn load_from_storage() -> Option<Self> {
            if let Some(storage) = Self::get_local_storage() {
                if let Ok(Some(data)) = storage.get_item("rustysound_cache") {
                    if let Ok(cache) = serde_json::from_str::<SimpleCache>(&data) {
                        return Some(cache);
                    }
                }
            }
            None
        }

        pub fn save_to_storage(&self) {
            if let Ok(data) = serde_json::to_string(self) {
                if let Some(storage) = Self::get_local_storage() {
                    let _ = storage.set_item("rustysound_cache", &data);
                }
            }
        }

        fn get_local_storage() -> Option<Storage> {
            window()?.local_storage().ok()?
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native_impl {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    impl SimpleCache {
        pub fn load_from_storage() -> Option<Self> {
            Self::get_cache_file_path()
                .and_then(|path| fs::read_to_string(path).ok())
                .and_then(|data| serde_json::from_str::<SimpleCache>(&data).ok())
        }

        pub fn save_to_storage(&self) {
            if let Some(path) = Self::get_cache_file_path() {
                if let Ok(data) = serde_json::to_string(self) {
                    let _ = fs::write(path, data);
                }
            }
        }

        fn get_cache_file_path() -> Option<PathBuf> {
            dirs::cache_dir()
                .map(|dir: PathBuf| dir.join("rustysound"))
                .map(|dir: PathBuf| {
                    let _ = fs::create_dir_all(&dir);
                    dir.join("cache.json")
                })
        }
    }
}