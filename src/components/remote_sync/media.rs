//! The sync server's media cache (its HTTP.md): covers resized to WebP
//! buckets, lyrics from the music server or LRCLIB in one shape, and
//! prewarming an album's worth ahead of time.
//!
//! What each sync server offers comes from its `/health` (`features`), kept
//! per music server in one table. `art_url` is the one place a cover URL is
//! routed through the sync server (`NavidromeClient::get_cover_art_display_url`
//! asks it); `lyrics_url` is the lyrics provider's. Without a feature, while
//! the server is unreachable, or after a run of failed covers, both say no
//! and the Subsonic URLs are used, so nothing visible depends on the sync
//! server being up.

use crate::api::{NavidromeClient, ServerConfig};
use dioxus::prelude::*;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Cover buckets when `/health` lists none (HTTP.md).
const DEFAULT_SIZES: [u32; 5] = [64, 128, 256, 512, 1024];
/// This many failed covers within `FAILURE_WINDOW_MS` pause the sync server's
/// covers for `DOWN_MS`; covers come from the music server meanwhile.
const FAILURES_TO_PAUSE: usize = 3;
const FAILURE_WINDOW_MS: f64 = 30_000.0;
const DOWN_MS: f64 = 60_000.0;
/// `/health` gives up after this long.
const HEALTH_TIMEOUT_MS: u64 = 5_000;
/// Prewarm at most this often per music server.
const PREWARM_GAP_MS: f64 = 10_000.0;

/// What a sync server's `/health` says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Health {
    pub version: String,
    pub protocol: u32,
    /// Cover buckets, when it serves covers.
    pub art_sizes: Option<Vec<u32>>,
    pub lyrics: bool,
    /// Most items one prewarm takes, when it prewarms.
    pub prewarm_max: Option<usize>,
}

/// Reads `/health`. `None` when it isn't a healthy rustysound-sync.
pub fn parse_health(value: &Value) -> Option<Health> {
    if value.get("status")?.as_str()? != "ok" {
        return None;
    }
    let features = value.get("features");
    let feature = |name: &str| features.and_then(|f| f.get(name)).filter(|f| f.get("v").and_then(Value::as_u64) == Some(1));
    let art_sizes = feature("art")
        .filter(|art| {
            // Only WebP is asked for; a server without it is treated as having no covers.
            art.get("formats")
                .and_then(Value::as_array)
                .is_none_or(|formats| formats.iter().any(|f| f.as_str() == Some("webp")))
        })
        .map(|art| {
            let mut sizes: Vec<u32> = art
                .get("sizes")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|s| s.as_u64()).map(|s| s as u32).collect())
                .unwrap_or_default();
            sizes.sort_unstable();
            sizes.dedup();
            if sizes.is_empty() {
                DEFAULT_SIZES.to_vec()
            } else {
                sizes
            }
        });
    Some(Health {
        version: value.get("version").and_then(Value::as_str).unwrap_or_default().to_string(),
        protocol: value.get("protocol").and_then(Value::as_u64).unwrap_or(0) as u32,
        art_sizes,
        lyrics: feature("lyrics").is_some(),
        prewarm_max: feature("prewarm").map(|p| p.get("max_items").and_then(Value::as_u64).unwrap_or(500) as usize),
    })
}

/// The bucket a cover of `size` px is asked for in: the smallest one at least
/// that big, else the largest. Asking in buckets keeps URLs few and stable,
/// so the browser's cache and the server's hit more often.
pub fn art_bucket(size: u32, sizes: &[u32]) -> u32 {
    let sizes = if sizes.is_empty() { &DEFAULT_SIZES[..] } else { sizes };
    sizes
        .iter()
        .copied()
        .find(|bucket| *bucket >= size)
        .unwrap_or_else(|| sizes.iter().copied().max().unwrap_or(512))
}

/// One music server's sync server, as far as media goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    /// The sync server's base URL, normalised.
    pub base: String,
    pub health: Health,
    /// Recent cover failures (local clock, ms).
    failures: Vec<f64>,
    /// Covers skip the sync server until then.
    down_until: f64,
    last_prewarm: f64,
}

impl Endpoint {
    pub fn new(base: String, health: Health) -> Self {
        Self { base, health, failures: Vec::new(), down_until: 0.0, last_prewarm: f64::MIN }
    }

    /// The cover buckets to use now, or `None` to use the music server.
    pub fn art_sizes(&self, now: f64) -> Option<&[u32]> {
        if now < self.down_until {
            return None;
        }
        self.health.art_sizes.as_deref()
    }

    /// A cover failed (5xx, network). True when this pauses the sync
    /// server's covers.
    pub fn record_failure(&mut self, now: f64) -> bool {
        if now < self.down_until {
            return false;
        }
        self.failures.retain(|at| now - at < FAILURE_WINDOW_MS);
        self.failures.push(now);
        if self.failures.len() >= FAILURES_TO_PAUSE {
            self.failures.clear();
            self.down_until = now + DOWN_MS;
            return true;
        }
        false
    }

    /// The socket dropped: assume the server is down until `/health` says
    /// otherwise (the next connect checks again).
    pub fn mark_unreachable(&mut self) -> bool {
        let changed = self.down_until != f64::INFINITY;
        self.down_until = f64::INFINITY;
        changed
    }
}

static ENDPOINTS: Lazy<Mutex<HashMap<String, Endpoint>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Bumped whenever covers should switch between the sync server and the
/// music server; `cover_art_src` reads it, so artwork re-renders then.
static REVISION: GlobalSignal<u64> = Signal::global(|| 0);

fn bump() {
    if dioxus::core::Runtime::try_current().is_some() {
        *REVISION.write() += 1;
    }
}

/// Subscribes the rendering component to cover-route changes.
pub fn subscribe() {
    if dioxus::core::Runtime::try_current().is_some() {
        let _ = REVISION();
    }
}

fn with_endpoint<T>(server_id: &str, f: impl FnOnce(&mut Endpoint) -> T) -> Option<T> {
    let mut table = ENDPOINTS.lock().unwrap_or_else(|e| e.into_inner());
    table.get_mut(server_id).map(f)
}

/// Records what `/health` said for `server_id` (or that it didn't answer).
pub fn set_health(server_id: &str, base: &str, health: Option<Health>) {
    let mut table = ENDPOINTS.lock().unwrap_or_else(|e| e.into_inner());
    let before = table.get(server_id).cloned();
    match health {
        Some(health) => {
            let mut entry = Endpoint::new(base.to_string(), health);
            if let Some(old) = before.as_ref().filter(|old| old.base == base) {
                entry.last_prewarm = old.last_prewarm;
            }
            table.insert(server_id.to_string(), entry);
        }
        None => {
            table.remove(server_id);
        }
    }
    let changed = before.as_ref().map(|e| (&e.base, &e.health, e.down_until))
        != table.get(server_id).map(|e| (&e.base, &e.health, e.down_until));
    drop(table);
    if changed {
        bump();
    }
}

/// The sync server for `server_id` stopped answering (the socket dropped).
pub fn mark_unreachable(server_id: &str) {
    if with_endpoint(server_id, Endpoint::mark_unreachable) == Some(true) {
        bump();
    }
}

/// A cover from the sync server failed to load.
pub fn note_art_failure(server_id: &str) {
    let now = crate::time::now_ms();
    if with_endpoint(server_id, |e| e.record_failure(now)) == Some(true) {
        bump();
    }
}

/// The login and server every media request carries (HTTP.md "Logins"):
/// the same token and salt as `/rest` calls, one salt per session, so the
/// URLs stay stable and the browser caches covers.
fn login_query(server: &ServerConfig) -> String {
    let (token, salt) = NavidromeClient::new(server.clone()).sync_auth();
    format!(
        "server={}&u={}&t={token}&s={salt}",
        urlencoding::encode(&server.url),
        urlencoding::encode(&server.username)
    )
}

/// `server_key` in cover paths: the saved server's id, in the characters
/// the sync server takes.
fn server_key(server: &ServerConfig) -> String {
    let key: String = server
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-._~".contains(*c))
        .take(64)
        .collect();
    if key.is_empty() { "server".to_string() } else { key }
}

/// The sync server's URL for a cover, at the bucket for `size` px, when this
/// server's covers should come from there. The one decision point for
/// cover routing.
pub fn art_url(server: &ServerConfig, cover_id: &str, size: u32) -> Option<String> {
    if server.is_jellyfin() || cover_id.trim().is_empty() || crate::cache_service::is_offline_mode() {
        return None;
    }
    let now = crate::time::now_ms();
    let (base, bucket) = {
        let table = ENDPOINTS.lock().unwrap_or_else(|e| e.into_inner());
        let endpoint = table.get(&server.id)?;
        let bucket = art_bucket(size, endpoint.art_sizes(now)?);
        (endpoint.base.clone(), bucket)
    };
    Some(format!(
        "{base}/art/{}/{}?size={bucket}&fmt=webp&{}",
        server_key(server),
        urlencoding::encode(cover_id),
        login_query(server)
    ))
}

/// Whether `url` is a sync-server cover (`art_url`), for the fallback.
pub fn is_art_url(url: &str) -> bool {
    url.contains("/art/") && url.contains("fmt=webp") && url.contains("&server=")
}

/// The sync server's `/lyrics` URL for a song, when it serves lyrics.
pub fn lyrics_url(server: &ServerConfig, song_id: &str, artist: &str, title: &str, album: &str, duration: Option<u32>) -> Option<String> {
    if server.is_jellyfin() || crate::cache_service::is_offline_mode() {
        return None;
    }
    let base = {
        let table = ENDPOINTS.lock().unwrap_or_else(|e| e.into_inner());
        let endpoint = table.get(&server.id).filter(|e| e.health.lyrics)?;
        endpoint.base.clone()
    };
    let mut url = format!("{base}/lyrics?id={}", urlencoding::encode(song_id));
    for (key, value) in [("artist", artist), ("title", title), ("album", album)] {
        if !value.trim().is_empty() {
            url.push_str(&format!("&{key}={}", urlencoding::encode(value)));
        }
    }
    if let Some(duration) = duration.filter(|d| *d > 0) {
        url.push_str(&format!("&duration={duration}"));
    }
    url.push('&');
    url.push_str(&login_query(server));
    Some(url)
}

/// `GET {sync_url}/health`, with a five-second limit.
pub async fn probe_health(sync_url: &str) -> Result<Health, String> {
    let url = format!("{}/health", super::logic::normalize_sync_url(sync_url));
    let request = async move {
        let response = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|_| "Can't reach the sync server.".to_string())?;
        if !response.status().is_success() {
            return Err(format!("The sync server answered HTTP {}.", response.status().as_u16()));
        }
        let value: Value = response
            .json()
            .await
            .map_err(|_| "That address isn't a RustySound sync server.".to_string())?;
        parse_health(&value).ok_or_else(|| "That address isn't a RustySound sync server.".to_string())
    };
    let timer = crate::components::ui::env::sleep_ms(HEALTH_TIMEOUT_MS);
    match futures_util::future::select(Box::pin(request), Box::pin(timer)).await {
        futures_util::future::Either::Left((result, _)) => result,
        futures_util::future::Either::Right(_) => Err("The sync server took too long to answer.".to_string()),
    }
}

/// Checks `/health` for `server` and records the answer. Returns it for the
/// Settings status line.
pub async fn refresh(server: &ServerConfig) -> Result<Health, String> {
    let base = super::logic::normalize_sync_url(&server.sync_url);
    if base.is_empty() || server.is_jellyfin() {
        set_health(&server.id, "", None);
        return Err("No sync server set.".to_string());
    }
    let result = probe_health(&base).await;
    set_health(&server.id, &base, result.clone().ok());
    result
}

/// One song for `/prewarm`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PrewarmSong {
    pub id: String,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration: u32,
}

/// The body of a prewarm for a queue: its covers (each once, in the buckets
/// the player and lists use) and the first songs' lyrics.
pub fn prewarm_body(covers: &[String], songs: &[PrewarmSong], max_items: usize, lyrics: bool) -> Value {
    let mut unique: Vec<&String> = Vec::new();
    for cover in covers {
        if !cover.trim().is_empty() && !unique.contains(&cover) {
            unique.push(cover);
        }
    }
    unique.truncate(max_items.min(100));
    let songs: Vec<&PrewarmSong> = if lyrics { songs.iter().take(max_items.min(20)).collect() } else { Vec::new() };
    serde_json::json!({ "covers": unique, "sizes": [128, 512], "fmt": "webp", "songs": songs })
}

/// Asks the sync server to fetch a queue's covers and lyrics ahead of time,
/// at most once every ten seconds per server. Fire and forget.
pub fn prewarm(server: &ServerConfig, covers: Vec<String>, songs: Vec<PrewarmSong>) {
    let now = crate::time::now_ms();
    let plan = with_endpoint(&server.id, |e| {
        let max = e.health.prewarm_max?;
        if now - e.last_prewarm < PREWARM_GAP_MS {
            return None;
        }
        e.last_prewarm = now;
        Some((e.base.clone(), max, e.health.lyrics))
    })
    .flatten();
    let Some((base, max, lyrics)) = plan else {
        return;
    };
    let body = prewarm_body(&covers, &songs, max, lyrics);
    let url = format!("{base}/prewarm?{}", login_query(server));
    spawn(async move {
        let _ = reqwest::Client::new().post(&url).json(&body).send().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn health_json() -> Value {
        json!({
            "status": "ok", "version": "0.1.0", "protocol": 1, "rooms": 2,
            "features": {
                "art": { "v": 1, "formats": ["webp", "jpeg"], "sizes": [64, 128, 256, 512, 1024] },
                "lyrics": { "v": 1 },
                "prewarm": { "v": 1, "max_items": 500 }
            },
            "cache": { "art": { "files": 1830, "bytes": 61234567 } }
        })
    }

    #[test]
    fn health_lists_the_features() {
        let health = parse_health(&health_json()).unwrap();
        assert_eq!(health.version, "0.1.0");
        assert_eq!(health.art_sizes.as_deref(), Some(&[64, 128, 256, 512, 1024][..]));
        assert!(health.lyrics);
        assert_eq!(health.prewarm_max, Some(500));
    }

    #[test]
    fn missing_or_newer_features_are_left_out() {
        let bare = parse_health(&json!({"status": "ok", "version": "0.1.0", "protocol": 1, "features": {}})).unwrap();
        assert_eq!(bare.art_sizes, None);
        assert!(!bare.lyrics);
        let newer = parse_health(&json!({"status": "ok", "features": {"art": {"v": 2}, "lyrics": {"v": 2}}})).unwrap();
        assert_eq!(newer.art_sizes, None, "an incompatible art feature isn't used");
        assert!(!newer.lyrics);
        assert_eq!(parse_health(&json!({"status": "down"})), None);
        assert_eq!(parse_health(&json!({"hello": 1})), None);
    }

    #[test]
    fn covers_are_asked_for_in_buckets() {
        let sizes = [64, 128, 256, 512, 1024];
        assert_eq!(art_bucket(88, &sizes), 128);
        assert_eq!(art_bucket(128, &sizes), 128);
        assert_eq!(art_bucket(600, &sizes), 1024);
        assert_eq!(art_bucket(2400, &sizes), 1024);
        assert_eq!(art_bucket(10, &[]), 64, "default buckets");
    }

    fn endpoint() -> Endpoint {
        Endpoint::new("http://sync".into(), parse_health(&health_json()).unwrap())
    }

    #[test]
    fn a_run_of_failures_falls_back_for_a_minute() {
        let mut e = endpoint();
        assert!(e.art_sizes(0.0).is_some());
        assert!(!e.record_failure(1_000.0));
        assert!(!e.record_failure(2_000.0));
        assert!(e.record_failure(3_000.0), "third failure pauses");
        assert!(e.art_sizes(3_001.0).is_none(), "music server meanwhile");
        assert!(e.art_sizes(3_000.0 + DOWN_MS).is_some(), "tried again after a minute");
    }

    #[test]
    fn scattered_failures_never_fall_back() {
        let mut e = endpoint();
        for i in 0..10 {
            assert!(!e.record_failure(i as f64 * 20_000.0));
        }
        assert!(e.art_sizes(200_000.0).is_some());
    }

    #[test]
    fn an_unreachable_server_falls_back_until_checked_again() {
        let mut e = endpoint();
        assert!(e.mark_unreachable());
        assert!(!e.mark_unreachable(), "once");
        assert!(e.art_sizes(1e15).is_none());
        let no_art = Endpoint::new("http://sync".into(), Health { art_sizes: None, ..Health::default() });
        assert!(no_art.art_sizes(0.0).is_none(), "no feature, no sync covers");
    }

    fn server() -> ServerConfig {
        ServerConfig {
            id: "srv-1".into(),
            name: "Home".into(),
            url: "http://music.test:4533".into(),
            username: "demo".into(),
            password: "demo".into(),
            active: true,
            sync_url: "http://sync.test".into(),
            ..Default::default()
        }
    }

    #[test]
    fn cover_urls_go_through_the_sync_server_only_with_the_feature() {
        let server = server();
        set_health(&server.id, "http://sync.test", Some(parse_health(&health_json()).unwrap()));
        let url = art_url(&server, "al-1_ab", 88).unwrap();
        assert!(url.starts_with("http://sync.test/art/srv-1/al-1_ab?size=128&fmt=webp&server=http%3A%2F%2Fmusic.test%3A4533&u=demo&t="), "{url}");
        assert!(is_art_url(&url));
        // The same salt every time, so the browser cache works.
        assert_eq!(art_url(&server, "al-1_ab", 100), Some(url));
        let lyrics = lyrics_url(&server, "tr-1", "Radiohead", "Reckoner", "", Some(290)).unwrap();
        assert!(lyrics.starts_with("http://sync.test/lyrics?id=tr-1&artist=Radiohead&title=Reckoner&duration=290&server="), "{lyrics}");

        set_health(&server.id, "http://sync.test", Some(Health { lyrics: false, ..Health::default() }));
        assert_eq!(art_url(&server, "al-1_ab", 88), None);
        assert_eq!(lyrics_url(&server, "tr-1", "a", "t", "", None), None);
        set_health(&server.id, "", None);
        assert_eq!(art_url(&server, "al-1_ab", 88), None);
    }

    #[test]
    fn a_prewarm_sends_each_cover_once() {
        let covers = vec!["a".to_string(), "b".to_string(), "a".to_string(), " ".to_string()];
        let songs = vec![PrewarmSong { id: "1".into(), artist: "x".into(), title: "y".into(), album: "z".into(), duration: 100 }];
        let body = prewarm_body(&covers, &songs, 500, true);
        assert_eq!(body["covers"], json!(["a", "b"]));
        assert_eq!(body["songs"][0]["id"], "1");
        assert_eq!(prewarm_body(&covers, &songs, 500, false)["songs"], json!([]));
    }
}
