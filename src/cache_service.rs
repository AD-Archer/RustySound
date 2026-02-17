use crate::cache::{CacheEntry, CacheStats, SimpleCache};
use crate::db::AppSettings;
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
struct RuntimeCacheConfig {
    enabled: bool,
    cache_images_enabled: bool,
    cache_expiry_hours: u32,
    cache_size_mb: u32,
}

impl Default for RuntimeCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_images_enabled: true,
            cache_expiry_hours: 24,
            cache_size_mb: 100,
        }
    }
}

static CACHE: Lazy<Mutex<SimpleCache>> = Lazy::new(|| {
    let loaded = SimpleCache::load_from_storage().unwrap_or_default();
    Mutex::new(loaded)
});
static CACHE_CONFIG: Lazy<Mutex<RuntimeCacheConfig>> =
    Lazy::new(|| Mutex::new(RuntimeCacheConfig::default()));

fn effective_expiry_hours(override_hours: Option<u32>) -> u32 {
    let config = CACHE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    override_hours
        .unwrap_or(config.cache_expiry_hours)
        .clamp(1, 24 * 30)
}

fn can_cache(include_images: bool) -> bool {
    let config = CACHE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
    config.enabled && (!include_images || config.cache_images_enabled)
}

pub fn is_enabled(include_images: bool) -> bool {
    can_cache(include_images)
}

fn save_cache(cache: &SimpleCache) {
    cache.save_to_storage();
}

pub fn apply_settings(settings: &AppSettings) {
    {
        let mut config = CACHE_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
        config.enabled = settings.cache_enabled;
        config.cache_images_enabled = settings.cache_images_enabled;
        config.cache_expiry_hours = settings.cache_expiry_hours.clamp(1, 24 * 30);
        config.cache_size_mb = settings.cache_size_mb.clamp(25, 2048);
    }

    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.resize_max_size_mb(settings.cache_size_mb.clamp(25, 2048));
    save_cache(&cache);
}

pub fn get_json<T>(key: &str) -> Option<T>
where
    T: DeserializeOwned,
{
    if !can_cache(false) {
        return None;
    }

    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = cache.get(key)?.data.clone();
    drop(cache);
    serde_json::from_slice::<T>(&bytes).ok()
}

pub fn put_json<T>(key: impl Into<String>, value: &T, expiry_hours: Option<u32>) -> bool
where
    T: Serialize,
{
    if !can_cache(false) {
        return false;
    }

    let Ok(bytes) = serde_json::to_vec(value) else {
        return false;
    };
    let expiry = Duration::from_secs(effective_expiry_hours(expiry_hours) as u64 * 3600);
    let entry = CacheEntry::new(bytes, "application/json".to_string(), expiry);

    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.put(key.into(), entry);
    save_cache(&cache);
    true
}

pub fn get_bytes(key: &str, include_images: bool) -> Option<Vec<u8>> {
    if !can_cache(include_images) {
        return None;
    }

    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.get(key).map(|entry| entry.data.clone())
}

pub fn put_bytes(
    key: impl Into<String>,
    bytes: Vec<u8>,
    content_type: impl Into<String>,
    expiry_hours: Option<u32>,
    include_images: bool,
) -> bool {
    if !can_cache(include_images) {
        return false;
    }

    let expiry = Duration::from_secs(effective_expiry_hours(expiry_hours) as u64 * 3600);
    let entry = CacheEntry::new(bytes, content_type.into(), expiry);

    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.put(key.into(), entry);
    save_cache(&cache);
    true
}

pub fn remove_by_prefix(prefix: &str) -> usize {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let removed = cache.remove_prefix(prefix);
    if removed > 0 {
        save_cache(&cache);
    }
    removed
}

pub fn clear_all() {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.clear();
    save_cache(&cache);
}

pub fn stats() -> CacheStats {
    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.stats()
}
