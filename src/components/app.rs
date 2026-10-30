use crate::api::*;
use crate::cache_service::{
    apply_settings as apply_cache_settings, get_json as cache_get_json, put_json as cache_put_json,
    remove_by_prefix as cache_remove_prefix,
};
use crate::components::views::home_layout::HomeFeedLoadProfile;
use crate::components::ui::env::{
    appearance_attr, platform_name, stage_theme, use_layout_provider,
};
use crate::components::ui::{
    LayoutSignal, MainScroll, PlayerBar, Sidebar, SongChange, TabBar, TrackMenuController,
    TrackMenuHost, TrackMenuRequest,
};
use crate::components::{
    ios_audio_log_snapshot, ios_diag_log, view_instance_key, AddIntent, AddMenuController,
    AddToMenuOverlay, AppView, AudioController, AudioState, HomeRefreshSignal, Icon,
    IsPlayingSignal, Navigation, PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal,
    ShuffleEnabledSignal, SidebarOpenSignal, SongDetailsController, SongDetailsOverlay,
    SongDetailsState, VolumeSignal,
};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, save_temporary_queue_snapshot, AppSettings, PlaybackState,
    QueueItem, TemporaryQueueSnapshot,
};
use crate::diagnostics::{log_perf, PerfTimer};
use crate::offline_audio::{prune_temporary_queue_prefetch_downloads, run_auto_download_pass};
use chrono::{DateTime, NaiveDateTime, Utc};
#[cfg(target_arch = "wasm32")]
use dioxus::core::{Runtime, RuntimeGuard};
#[cfg(all(feature = "desktop", target_os = "macos"))]
use dioxus::desktop::use_muda_event_handler;
use dioxus_router::components::Outlet;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::window;
// Re-export RepeatMode for other components
pub use crate::db::RepeatMode;
use dioxus::prelude::*;
use dioxus_router::use_navigator;

#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_THRESHOLD: f64 = 100.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_VERTICAL_SLOP: f64 = 72.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_EDGE_ZONE: f64 = 28.0;
const HOME_INIT_QUICK_PICK_LIMIT_STANDARD: usize = 72;
const HOME_INIT_SECTION_BASE_COUNT_STANDARD: usize = 9;
const HOME_INIT_SECTION_LOAD_STEP_STANDARD: usize = 6;
const HOME_INIT_SECTION_FETCH_LIMIT_STANDARD: usize = 180;
const HOME_INIT_WARMUP_FLAG_CACHE_HOURS: u32 = 24 * 365;
const AUTO_DOWNLOAD_POLL_INTERVAL_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy)]
struct HomeInitFetchBudget {
    quick_pick_limit: usize,
    section_base_count: usize,
    section_load_step: usize,
    section_cache_count: usize,
    section_fetch_limit: usize,
    random_fetch_limit: usize,
    album_preview_limit: u32,
}

fn home_init_fetch_budget(profile: HomeFeedLoadProfile) -> HomeInitFetchBudget {
    match profile {
        HomeFeedLoadProfile::Conservative => {
            let section_base_count = 6usize;
            let section_load_step = 4usize;
            let section_fetch_limit = 60usize;
            HomeInitFetchBudget {
                quick_pick_limit: 24,
                section_base_count,
                section_load_step,
                section_cache_count: section_fetch_limit,
                section_fetch_limit,
                random_fetch_limit: section_fetch_limit,
                album_preview_limit: section_base_count as u32,
            }
        }
        HomeFeedLoadProfile::Standard => {
            let section_base_count = HOME_INIT_SECTION_BASE_COUNT_STANDARD;
            let section_load_step = HOME_INIT_SECTION_LOAD_STEP_STANDARD;
            let section_fetch_limit = HOME_INIT_SECTION_FETCH_LIMIT_STANDARD;
            HomeInitFetchBudget {
                quick_pick_limit: HOME_INIT_QUICK_PICK_LIMIT_STANDARD,
                section_base_count,
                section_load_step,
                section_cache_count: section_fetch_limit,
                section_fetch_limit,
                random_fetch_limit: section_fetch_limit,
                album_preview_limit: section_base_count as u32,
            }
        }
        HomeFeedLoadProfile::Super => {
            let section_base_count = 12usize;
            let section_load_step = 8usize;
            let section_fetch_limit = 360usize;
            HomeInitFetchBudget {
                quick_pick_limit: 144,
                section_base_count,
                section_load_step,
                section_cache_count: section_fetch_limit,
                section_fetch_limit,
                random_fetch_limit: section_fetch_limit,
                album_preview_limit: section_base_count as u32,
            }
        }
    }
}

fn home_init_profile_from_settings(settings: &AppSettings) -> HomeFeedLoadProfile {
    HomeFeedLoadProfile::from_storage(&settings.home_feed_load_profile)
}

fn home_init_profile_cache_key(profile: HomeFeedLoadProfile) -> &'static str {
    profile.as_storage()
}

fn queue_snapshot_signature(
    queue: &[Song],
    queue_index: usize,
    now_playing: Option<&Song>,
) -> String {
    let mut signature = String::new();
    signature.push_str(&queue.len().to_string());
    signature.push('|');
    signature.push_str(&queue_index.to_string());
    signature.push('|');
    if let Some(song) = now_playing {
        signature.push_str(song.server_id.trim());
        signature.push(':');
        signature.push_str(song.id.trim());
    }
    signature.push('|');
    for song in queue {
        signature.push_str(song.server_id.trim());
        signature.push(':');
        signature.push_str(song.id.trim());
        signature.push(';');
    }
    signature
}

#[cfg(all(feature = "desktop", target_os = "macos"))]
fn focus_global_search_input() {
    let _ = document::eval(
        r#"
(() => {
  setTimeout(() => {
    const input = document.getElementById("global-search-input");
    if (input && typeof input.focus === "function") {
      input.focus();
      if (typeof input.select === "function") {
        input.select();
      }
    }
  }, 50);
  return true;
})();
        "#,
    );
}

fn loading_progress_percent(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * 100.0).round() as u32
}

const BOOT_MARK: Asset = asset!("/assets/favicon-96x96.png");

/// Full-screen stage overlay shown while the app boots or warms its caches.
#[component]
fn BootOverlay(
    title: String,
    message: String,
    progress: f32,
    stage: String,
    logs: Vec<String>,
    testid: String,
) -> Element {
    let percent = loading_progress_percent(progress);
    rsx! {
        div {
            class: "rs-boot",
            role: "status",
            aria_live: "polite",
            "data-testid": "{testid}",
            div { class: "rs-boot__inner",
                img {
                    class: "rs-boot__mark",
                    src: BOOT_MARK,
                    alt: "",
                    width: "64",
                    height: "64",
                }
                h1 { class: "rs-boot__title", "{title}" }
                p { class: "rs-boot__msg", "{message}" }
                div { class: "rs-boot__progress",
                    div {
                        class: "rs-track",
                        role: "progressbar",
                        aria_label: "{title}",
                        aria_valuemin: "0",
                        aria_valuemax: "100",
                        aria_valuenow: "{percent}",
                        div {
                            class: "rs-track__fill rs-boot__fill",
                            style: "width:{percent}%",
                        }
                    }
                    div { class: "rs-boot__meta",
                        span { class: "rs-truncate", "{stage}" }
                        span { class: "num", "{percent}%" }
                    }
                }
                if !logs.is_empty() {
                    div { class: "rs-boot__log",
                        for (index, line) in logs.iter().enumerate() {
                            div { key: "{index}", "{line}" }
                        }
                    }
                }
            }
        }
    }
}

fn normalize_volume(mut value: f64) -> f64 {
    if !value.is_finite() {
        return 0.8;
    }
    let mut passes = 0;
    while value > 1.0 && passes < 4 {
        value /= 100.0;
        passes += 1;
    }
    value.clamp(0.0, 1.0)
}

fn home_init_warmup_cache_key(
    active_servers: &[ServerConfig],
    profile: HomeFeedLoadProfile,
) -> String {
    let mut ids: Vec<String> = active_servers
        .iter()
        .map(|server| server.id.clone())
        .collect();
    ids.sort();
    format!(
        "view:home:warmup:v3:{}:{}",
        home_init_profile_cache_key(profile),
        ids.join("|")
    )
}

fn home_init_cache_prefix(active_servers: &[ServerConfig], profile: HomeFeedLoadProfile) -> String {
    let mut ids: Vec<String> = active_servers
        .iter()
        .map(|server| server.id.clone())
        .collect();
    ids.sort();
    format!(
        "view:home:v4:{}:{}",
        home_init_profile_cache_key(profile),
        ids.join("|")
    )
}

#[derive(Clone)]
pub struct HomeFeedState {
    pub recent_albums: Signal<Option<Vec<Album>>>,
    pub most_played_albums: Signal<Option<Vec<Album>>>,
    pub recently_played_songs: Signal<Option<Vec<Song>>>,
    pub most_played_songs: Signal<Option<Vec<Song>>>,
    pub random_songs: Signal<Option<Vec<Song>>>,
    pub quick_picks: Signal<Option<Vec<Song>>>,
    pub warmup_enabled: Signal<bool>,
    pub progress: Signal<f32>,
    pub status: Signal<Option<String>>,
}

#[derive(Debug, Default, Clone)]
struct CachedHomeFeedSnapshot {
    recent_albums: Option<Vec<Album>>,
    most_played_albums: Option<Vec<Album>>,
    recent_songs: Option<Vec<Song>>,
    most_played_songs: Option<Vec<Song>>,
    random_songs: Option<Vec<Song>>,
    quick_picks: Option<Vec<Song>>,
}

impl CachedHomeFeedSnapshot {
    fn has_full_snapshot(&self) -> bool {
        self.recent_albums.is_some()
            && self.most_played_albums.is_some()
            && self.recent_songs.is_some()
            && self.most_played_songs.is_some()
            && self.random_songs.is_some()
            && self.quick_picks.is_some()
    }
}

#[derive(Debug, Default, Clone)]
struct HomeFeedSnapshot {
    recent_albums: Vec<Album>,
    most_played_albums: Vec<Album>,
    recent_songs: Vec<Song>,
    most_played_songs: Vec<Song>,
    random_songs: Vec<Song>,
    quick_picks: Vec<Song>,
}

impl HomeFeedSnapshot {
    fn summary(&self) -> HomeInitSummary {
        HomeInitSummary {
            recent_albums: self.recent_albums.len(),
            most_played_albums: self.most_played_albums.len(),
            recent_songs: self.recent_songs.len(),
            most_played_songs: self.most_played_songs.len(),
            random_songs: self.random_songs.len(),
            quick_picks: self.quick_picks.len(),
        }
    }
}

fn load_cached_home_feed_snapshot(
    active_servers: &[ServerConfig],
    profile: HomeFeedLoadProfile,
) -> CachedHomeFeedSnapshot {
    let cache_prefix = home_init_cache_prefix(active_servers, profile);
    CachedHomeFeedSnapshot {
        recent_albums: cache_get_json::<Vec<Album>>(&format!("{cache_prefix}:recent_albums")),
        most_played_albums: cache_get_json::<Vec<Album>>(&format!(
            "{cache_prefix}:most_played_albums"
        )),
        recent_songs: cache_get_json::<Vec<Song>>(&format!("{cache_prefix}:recent_songs")),
        most_played_songs: cache_get_json::<Vec<Song>>(&format!(
            "{cache_prefix}:most_played_songs"
        )),
        random_songs: cache_get_json::<Vec<Song>>(&format!("{cache_prefix}:random_songs")),
        quick_picks: cache_get_json::<Vec<Song>>(&format!("{cache_prefix}:quick_picks")),
    }
}

fn apply_cached_home_feed_snapshot(home_feed: &HomeFeedState, snapshot: &CachedHomeFeedSnapshot) {
    let mut recent_albums = home_feed.recent_albums;
    let mut most_played_albums = home_feed.most_played_albums;
    let mut recently_played_songs = home_feed.recently_played_songs;
    let mut most_played_songs = home_feed.most_played_songs;
    let mut random_songs = home_feed.random_songs;
    let mut quick_picks = home_feed.quick_picks;

    recent_albums.set(snapshot.recent_albums.clone());
    most_played_albums.set(snapshot.most_played_albums.clone());
    recently_played_songs.set(snapshot.recent_songs.clone());
    most_played_songs.set(snapshot.most_played_songs.clone());
    random_songs.set(snapshot.random_songs.clone());
    quick_picks.set(snapshot.quick_picks.clone());
    ios_diag_log(
        "home.feed.apply",
        &format!(
            "cached recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
            snapshot
                .recent_albums
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
            snapshot
                .most_played_albums
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
            snapshot
                .recent_songs
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
            snapshot
                .most_played_songs
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
            snapshot
                .random_songs
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
            snapshot
                .quick_picks
                .as_ref()
                .map(|items| items.len().to_string())
                .unwrap_or_else(|| "miss".to_string()),
        ),
    );
}

fn apply_home_feed_snapshot(home_feed: &HomeFeedState, snapshot: &HomeFeedSnapshot) {
    let mut recent_albums = home_feed.recent_albums;
    let mut most_played_albums = home_feed.most_played_albums;
    let mut recently_played_songs = home_feed.recently_played_songs;
    let mut most_played_songs = home_feed.most_played_songs;
    let mut random_songs = home_feed.random_songs;
    let mut quick_picks = home_feed.quick_picks;

    recent_albums.set(Some(snapshot.recent_albums.clone()));
    most_played_albums.set(Some(snapshot.most_played_albums.clone()));
    recently_played_songs.set(Some(snapshot.recent_songs.clone()));
    most_played_songs.set(Some(snapshot.most_played_songs.clone()));
    random_songs.set(Some(snapshot.random_songs.clone()));
    quick_picks.set(Some(snapshot.quick_picks.clone()));
    let summary = snapshot.summary();
    ios_diag_log(
        "home.feed.apply",
        &format!(
            "fresh recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
            summary.recent_albums,
            summary.most_played_albums,
            summary.recent_songs,
            summary.most_played_songs,
            summary.random_songs,
            summary.quick_picks
        ),
    );
}

fn set_empty_home_feed_snapshot(home_feed: &HomeFeedState) {
    let mut recent_albums = home_feed.recent_albums;
    let mut most_played_albums = home_feed.most_played_albums;
    let mut recently_played_songs = home_feed.recently_played_songs;
    let mut most_played_songs = home_feed.most_played_songs;
    let mut random_songs = home_feed.random_songs;
    let mut quick_picks = home_feed.quick_picks;

    recent_albums.set(Some(Vec::new()));
    most_played_albums.set(Some(Vec::new()));
    recently_played_songs.set(Some(Vec::new()));
    most_played_songs.set(Some(Vec::new()));
    random_songs.set(Some(Vec::new()));
    quick_picks.set(Some(Vec::new()));
    ios_diag_log(
        "home.feed.apply",
        "set empty Home feed snapshot for no-server state",
    );
}

fn home_init_has_cached_payload(
    active_servers: &[ServerConfig],
    profile: HomeFeedLoadProfile,
) -> bool {
    load_cached_home_feed_snapshot(active_servers, profile).has_full_snapshot()
}

fn home_init_is_warmed(active_servers: &[ServerConfig], profile: HomeFeedLoadProfile) -> bool {
    let warmup_key = home_init_warmup_cache_key(active_servers, profile);
    let has_warmup_flag = cache_get_json::<bool>(&warmup_key).unwrap_or(false);
    let has_cached_payload = home_init_has_cached_payload(active_servers, profile);

    if has_warmup_flag && !has_cached_payload {
        ios_diag_log(
            "home.init.gate",
            "warmup flag present but cached payload missing; rerunning warmup",
        );
    }

    has_warmup_flag && has_cached_payload
}

fn home_init_server_signature(
    active_servers: &[ServerConfig],
    profile: HomeFeedLoadProfile,
) -> String {
    let mut signature_parts: Vec<String> = active_servers
        .iter()
        .map(|server| format!("{}|{}|{}", server.id, server.url, server.username))
        .collect();
    signature_parts.sort();
    format!(
        "{}::{}",
        home_init_profile_cache_key(profile),
        signature_parts.join("||")
    )
}

fn home_init_song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn parse_home_init_played_timestamp(value: &str) -> Option<i64> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }

    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }

    None
}

fn compare_home_init_optional_i64_desc(
    left: Option<i64>,
    right: Option<i64>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(l), Some(r)) => r.cmp(&l),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_home_init_optional_u32_desc(
    left: Option<u32>,
    right: Option<u32>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(l), Some(r)) => r.cmp(&l),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_home_init_song_title(left: &Song, right: &Song) -> std::cmp::Ordering {
    left.title.to_lowercase().cmp(&right.title.to_lowercase())
}

fn sort_home_init_songs(songs: &mut [Song], sort: NativeSongSortField) {
    songs.sort_by(|left, right| {
        let left_played = left
            .played
            .as_deref()
            .and_then(parse_home_init_played_timestamp);
        let right_played = right
            .played
            .as_deref()
            .and_then(parse_home_init_played_timestamp);

        match sort {
            NativeSongSortField::PlayDate => {
                compare_home_init_optional_i64_desc(left_played, right_played)
                    .then_with(|| {
                        compare_home_init_optional_u32_desc(left.play_count, right.play_count)
                    })
                    .then_with(|| compare_home_init_song_title(left, right))
            }
            NativeSongSortField::PlayCount => {
                compare_home_init_optional_u32_desc(left.play_count, right.play_count)
                    .then_with(|| compare_home_init_optional_i64_desc(left_played, right_played))
                    .then_with(|| compare_home_init_song_title(left, right))
            }
        }
    });
}

fn dedupe_home_init_songs(songs: Vec<Song>, limit: usize) -> Vec<Song> {
    let mut seen = std::collections::HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in songs {
        let key = home_init_song_key(&song);
        if seen.insert(key) {
            output.push(song);
        }
        if output.len() >= limit {
            break;
        }
    }
    output
}

fn merge_home_init_song_lists(
    primary: Vec<Song>,
    supplemental: Vec<Song>,
    limit: usize,
) -> Vec<Song> {
    let mut merged = primary;
    merged.extend(supplemental);
    dedupe_home_init_songs(merged, limit)
}

async fn fetch_home_init_albums_for_servers(
    active_servers: &[ServerConfig],
    album_type: &str,
    limit: u32,
) -> Vec<Album> {
    let mut albums = Vec::<Album>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_albums(album_type, limit, 0)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            home_init_fetch_yield().await;
            fetched = client
                .get_albums(album_type, limit, 0)
                .await
                .unwrap_or_default();
        }
        albums.append(&mut fetched);
        if albums.len() >= limit as usize {
            break;
        }
        home_init_fetch_yield().await;
    }
    albums.truncate(limit as usize);
    albums
}

async fn fetch_home_init_random_songs_for_servers(
    active_servers: &[ServerConfig],
    limit: u32,
) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client.get_random_songs(limit).await.unwrap_or_default();
        if fetched.is_empty() {
            home_init_fetch_yield().await;
            fetched = client.get_random_songs(limit).await.unwrap_or_default();
        }
        songs.append(&mut fetched);
        if songs.len() >= limit as usize {
            break;
        }
        home_init_fetch_yield().await;
    }
    dedupe_home_init_songs(songs, limit as usize)
}

async fn fetch_home_init_sorted_songs_for_servers(
    active_servers: &[ServerConfig],
    sort: NativeSongSortField,
    limit: u32,
) -> Vec<Song> {
    if active_servers.is_empty() {
        return Vec::new();
    }

    let per_server_song_target = ((limit as usize).max(30) / active_servers.len()).max(20);
    let mut songs = Vec::<Song>::new();
    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_native_songs(sort, NativeSortOrder::Desc, 0, per_server_song_target)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            home_init_fetch_yield().await;
            fetched = client
                .get_native_songs(sort, NativeSortOrder::Desc, 0, per_server_song_target)
                .await
                .unwrap_or_default();
        }
        songs.append(&mut fetched);
        home_init_fetch_yield().await;
    }

    sort_home_init_songs(&mut songs, sort);
    dedupe_home_init_songs(songs, limit as usize)
}
#[cfg(not(target_arch = "wasm32"))]
async fn home_init_fetch_yield() {
    tokio::task::yield_now().await;
}

#[cfg(target_arch = "wasm32")]
async fn home_init_fetch_yield() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn loading_log_poll_sleep() {
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
}

#[cfg(target_arch = "wasm32")]
async fn loading_log_poll_sleep() {
    gloo_timers::future::TimeoutFuture::new(350).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn auto_download_poll_sleep() {
    tokio::time::sleep(std::time::Duration::from_millis(
        AUTO_DOWNLOAD_POLL_INTERVAL_MS,
    ))
    .await;
}

#[cfg(target_arch = "wasm32")]
async fn auto_download_poll_sleep() {
    gloo_timers::future::TimeoutFuture::new(AUTO_DOWNLOAD_POLL_INTERVAL_MS as u32).await;
}

#[derive(Debug, Default, Clone, Copy)]
struct HomeInitSummary {
    recent_albums: usize,
    most_played_albums: usize,
    recent_songs: usize,
    most_played_songs: usize,
    random_songs: usize,
    quick_picks: usize,
}

async fn initialize_home_cache(
    active_servers: &[ServerConfig],
    persist_cache: bool,
    profile: HomeFeedLoadProfile,
    mut progress: Signal<f32>,
    mut status: Signal<Option<String>>,
) -> HomeFeedSnapshot {
    let init_start = PerfTimer::now();
    let budget = home_init_fetch_budget(profile);
    let warmup_key = home_init_warmup_cache_key(active_servers, profile);

    let cache_prefix = home_init_cache_prefix(active_servers, profile);
    let recent_cache_key = format!("{cache_prefix}:recent_albums");
    let most_played_album_cache_key = format!("{cache_prefix}:most_played_albums");
    let recent_song_cache_key = format!("{cache_prefix}:recent_songs");
    let most_played_song_cache_key = format!("{cache_prefix}:most_played_songs");
    let random_song_cache_key = format!("{cache_prefix}:random_songs");
    let quick_pick_cache_key = format!("{cache_prefix}:quick_picks");

    eprintln!(
        "[app-init] starting home cache warmup for {} server(s) with profile={} (base={} step={} fetch={})",
        active_servers.len(),
        home_init_profile_cache_key(profile),
        budget.section_base_count,
        budget.section_load_step,
        budget.section_fetch_limit
    );
    let mut set_stage = |progress_value: f32, message: &str| {
        progress.set(progress_value.clamp(0.0, 1.0));
        status.set(Some(message.to_string()));
    };
    set_stage(0.08, "Fetching recently played songs");
    ios_diag_log(
        "home.init",
        &format!("start warmup servers={}", active_servers.len()),
    );

    let pool_size = (budget.section_fetch_limit * 3).max(budget.random_fetch_limit * 2);
    ios_diag_log(
        "home.init",
        &format!(
            "fetch native recent songs limit={}",
            budget.section_fetch_limit
        ),
    );
    let mut recent_played = fetch_home_init_sorted_songs_for_servers(
        active_servers,
        NativeSongSortField::PlayDate,
        budget.section_fetch_limit as u32,
    )
    .await;

    set_stage(0.18, "Fetching most played songs");
    ios_diag_log(
        "home.init",
        &format!(
            "fetch native most played songs limit={}",
            budget.section_fetch_limit
        ),
    );
    let mut most_played_song_items = fetch_home_init_sorted_songs_for_servers(
        active_servers,
        NativeSongSortField::PlayCount,
        budget.section_fetch_limit as u32,
    )
    .await;

    set_stage(0.28, "Fetching random songs");
    ios_diag_log(
        "home.init",
        &format!("fetch random fallback song pool size={pool_size}"),
    );
    let song_pool =
        fetch_home_init_random_songs_for_servers(active_servers, pool_size as u32).await;
    let random_song_items = dedupe_home_init_songs(song_pool.clone(), budget.random_fetch_limit);

    if recent_played.len() < budget.section_fetch_limit {
        let mut fallback_recent = song_pool.clone();
        sort_home_init_songs(&mut fallback_recent, NativeSongSortField::PlayDate);
        recent_played =
            merge_home_init_song_lists(recent_played, fallback_recent, budget.section_fetch_limit);
    }

    if most_played_song_items.len() < budget.section_fetch_limit {
        let mut fallback_most_played = song_pool;
        sort_home_init_songs(&mut fallback_most_played, NativeSongSortField::PlayCount);
        most_played_song_items = merge_home_init_song_lists(
            most_played_song_items,
            fallback_most_played,
            budget.section_fetch_limit,
        );
    }

    let recent_cached: Vec<Song> = recent_played
        .iter()
        .take(budget.section_cache_count)
        .cloned()
        .collect();
    let most_played_cached: Vec<Song> = most_played_song_items
        .iter()
        .take(budget.section_cache_count)
        .cloned()
        .collect();
    if persist_cache {
        let _ = cache_put_json(recent_song_cache_key.clone(), &recent_cached, Some(3));
        let _ = cache_put_json(
            most_played_song_cache_key.clone(),
            &most_played_cached,
            Some(6),
        );
        let _ = cache_put_json(random_song_cache_key.clone(), &random_song_items, Some(2));
    }
    ios_diag_log(
        "home.init",
        &format!(
            "song sections cached recent={} most_played={} random={} (native recent={} native most_played={})",
            recent_played.len(),
            most_played_song_items.len(),
            random_song_items.len(),
            recent_played
                .iter()
                .filter(|song| song.played.is_some())
                .count(),
            most_played_song_items
                .iter()
                .filter(|song| song.play_count.is_some())
                .count()
        ),
    );
    set_stage(0.42, "Song sections cached");
    home_init_fetch_yield().await;

    set_stage(0.58, "Building Home quick picks");
    ios_diag_log("home.init", "building quick picks");
    let mut quick = dedupe_home_init_songs(most_played_song_items.clone(), budget.quick_pick_limit);
    if quick.is_empty() {
        quick = dedupe_home_init_songs(random_song_items.clone(), budget.quick_pick_limit);
    }
    if quick.is_empty() {
        quick = fetch_home_init_random_songs_for_servers(
            active_servers,
            budget.quick_pick_limit as u32,
        )
        .await;
    }
    if persist_cache {
        let _ = cache_put_json(quick_pick_cache_key, &quick, Some(3));
    }
    ios_diag_log(
        "home.init",
        &format!("quick picks cached count={}", quick.len()),
    );
    set_stage(0.7, "Quick picks cached");
    home_init_fetch_yield().await;

    set_stage(0.82, "Fetching recent albums");
    ios_diag_log("home.init", "fetching recent albums");
    let recent_albums =
        fetch_home_init_albums_for_servers(active_servers, "newest", budget.album_preview_limit)
            .await;
    if persist_cache {
        let _ = cache_put_json(recent_cache_key, &recent_albums, Some(6));
    }
    ios_diag_log(
        "home.init",
        &format!("recent albums cached count={}", recent_albums.len()),
    );
    set_stage(0.9, "Recent albums cached");
    home_init_fetch_yield().await;

    set_stage(0.96, "Fetching most played albums");
    ios_diag_log("home.init", "fetching most played albums");
    let most_played_albums = fetch_home_init_albums_for_servers(
        active_servers,
        "frequent",
        budget.section_fetch_limit as u32,
    )
    .await;
    let most_played_cached: Vec<Album> = most_played_albums
        .iter()
        .take(budget.section_cache_count)
        .cloned()
        .collect();
    if persist_cache {
        let _ = cache_put_json(most_played_album_cache_key, &most_played_cached, Some(6));
        let _ = cache_put_json(warmup_key, &true, Some(HOME_INIT_WARMUP_FLAG_CACHE_HOURS));
    }

    let snapshot = HomeFeedSnapshot {
        recent_albums,
        most_played_albums,
        recent_songs: recent_played,
        most_played_songs: most_played_song_items,
        random_songs: random_song_items,
        quick_picks: quick,
    };
    let summary = snapshot.summary();

    log_perf(
        "app.home_init.total",
        init_start,
        &format!(
            "servers={} recent_albums={} frequent_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
            active_servers.len(),
            summary.recent_albums,
            summary.most_played_albums,
            summary.recent_songs,
            summary.most_played_songs,
            summary.random_songs,
            summary.quick_picks
        ),
    );
    eprintln!(
        "[app-init] home cache warmup complete | recent_albums={} frequent_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
        summary.recent_albums,
        summary.most_played_albums,
        summary.recent_songs,
        summary.most_played_songs,
        summary.random_songs,
        summary.quick_picks
    );
    ios_diag_log(
        "home.init",
        &format!(
            "complete recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
            summary.recent_albums,
            summary.most_played_albums,
            summary.recent_songs,
            summary.most_played_songs,
            summary.random_songs,
            summary.quick_picks
        ),
    );
    set_stage(1.0, "Home cache ready");

    snapshot
}

#[component]
pub fn AppShell() -> Element {
    let mut servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_route::<AppView>();
    let router_navigator = use_navigator();
    let mut current_view_signal = use_signal(|| current_view.clone());
    let pending_navigation_target = use_signal(|| None::<AppView>);
    let outlet_key = view_instance_key(&current_view);
    let now_playing = use_signal(|| None::<Song>);
    let queue = use_signal(Vec::<Song>::new);
    let mut queue_index = use_signal(|| 0usize);
    let is_playing = use_signal(|| false);
    let mut volume = use_signal(|| 0.8f64);
    let mut app_settings = use_signal(AppSettings::default);
    let mut playback_position = use_signal(|| 0.0f64);
    let mut last_playback_save = use_signal(|| None::<(String, String, u64, usize, usize)>);
    let mut last_queue_snapshot_signature = use_signal(String::new);
    let mut db_initialized = use_signal(|| false);
    let mut servers_loaded = use_signal(|| false);
    let mut settings_loaded = use_signal(|| false);
    let mut shuffle_enabled = use_signal(|| false);
    let mut repeat_mode = use_signal(|| RepeatMode::Off);
    let mut auto_download_bootstrap_done = use_signal(|| false);
    let mut auto_download_poll_generation = use_signal(|| 0u64);
    let mut home_init_in_progress = use_signal(|| false);
    let home_init_status = use_signal(|| None::<String>);
    let home_init_progress = use_signal(|| 0.0f32);
    let mut home_init_signature = use_signal(|| None::<String>);
    let mut home_init_generation = use_signal(|| 0u64);
    let home_manual_refresh_generation = use_signal(|| 0u64);
    let mut home_manual_refresh_applied = use_signal(|| 0u64);
    let mut startup_bootstrap_progress = use_signal(|| 0.0f32);
    let mut startup_bootstrap_status = use_signal(|| "Initializing database".to_string());
    let mut ios_loading_log_lines = use_signal(Vec::<String>::new);
    let mut ios_loading_log_poll_generation = use_signal(|| 0u64);
    let audio_state = use_signal(AudioState::default);
    let preview_playback = use_signal(|| false);
    let mut sidebar_open = use_signal(|| false);
    // Resize-aware layout; only flips re-render the shell.
    let layout_signal = use_layout_provider();
    use_context_provider(|| LayoutSignal(layout_signal));
    let track_menu_request = use_signal(|| None::<TrackMenuRequest>);
    let track_menu_change = use_signal(|| None::<SongChange>);
    use_context_provider(|| TrackMenuController::new(track_menu_request, track_menu_change));
    let mut main_scroll_top = use_signal(|| 0.0f64);
    let mut main_scroll_height = use_signal(|| 0.0f64);
    let mut main_viewport_top = use_signal(|| 0.0f64);
    use_context_provider(|| MainScroll {
        top: main_scroll_top,
        height: main_scroll_height,
        viewport_top: main_viewport_top,
    });
    // The inner transport signals, taken once. Reading the outer AudioState
    // signal would re-render the shell on every playback tick, because the
    // native controllers write through `audio_state.write()`.
    let (transport_loading_signal, transport_label_signal) = use_hook(move || {
        let state = audio_state.peek();
        (state.is_transport_loading, state.transport_loading_label)
    });
    use_effect({
        let current_view = current_view.clone();
        move || {
            if current_view_signal() != current_view {
                current_view_signal.set(current_view.clone());
            }
        }
    });

    let navigation = Navigation::new(
        router_navigator,
        current_view_signal,
        pending_navigation_target,
    );
    use_effect({
        let navigation = navigation;
        move || {
            let current_view = current_view_signal();
            let pending_target = pending_navigation_target();
            if pending_target.is_some() && matches!(current_view, AppView::HomeView {}) {
                eprintln!(
                    "[nav.refresh.effect] current={} pending={}",
                    current_view,
                    pending_target
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "<none>".to_string())
                );
                navigation.resume_pending_navigation();
            }
        }
    });
    let seek_request = use_signal(|| None::<(String, f64)>);
    let mut resume_bookmark_loaded = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let swipe_start = use_signal(|| None::<(f64, f64, i8)>);
    let swipe_hint = use_signal(|| None::<(i8, f64)>);
    let add_menu_intent = use_signal(|| None::<AddIntent>);
    let add_menu = AddMenuController::new(add_menu_intent.clone());
    let song_details_state = use_signal(SongDetailsState::default);
    let song_details = SongDetailsController::new(song_details_state.clone());
    {
        let song_details = song_details.clone();
        use_effect(move || {
            if sidebar_open() {
                let mut song_details = song_details.clone();
                song_details.close();
                sidebar_open.set(false);
            }
        });
    }
    let mut home_feed = HomeFeedState {
        recent_albums: use_signal(|| None::<Vec<Album>>),
        most_played_albums: use_signal(|| None::<Vec<Album>>),
        recently_played_songs: use_signal(|| None::<Vec<Song>>),
        most_played_songs: use_signal(|| None::<Vec<Song>>),
        random_songs: use_signal(|| None::<Vec<Song>>),
        quick_picks: use_signal(|| None::<Vec<Song>>),
        warmup_enabled: use_signal(|| true),
        progress: home_init_progress,
        status: home_init_status,
    };

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| navigation.clone());
    use_context_provider(|| add_menu.clone());
    use_context_provider(|| song_details.clone());
    use_context_provider(|| home_feed.clone());
    use_context_provider(|| HomeRefreshSignal(home_manual_refresh_generation));

    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        let navigation = navigation.clone();
        use_muda_event_handler(move |event| match event.id().as_ref() {
            "rustysound-open-settings" => {
                navigation.navigate_to(AppView::SettingsView {});
            }
            "rustysound-go-home" => {
                navigation.navigate_to(AppView::HomeView {});
            }
            "rustysound-go-search" => {
                navigation.navigate_to(AppView::SearchView {});
                focus_global_search_input();
            }
            "rustysound-go-albums" => {
                navigation.navigate_to(AppView::Albums {});
            }
            "rustysound-go-artists" => {
                navigation.navigate_to(AppView::ArtistsView {});
            }
            "rustysound-go-playlists" => {
                navigation.navigate_to(AppView::PlaylistsView {});
            }
            "rustysound-go-songs" => {
                navigation.navigate_to(AppView::SongsView {});
            }
            "rustysound-go-queue" => {
                navigation.navigate_to(AppView::QueueView {});
            }
            "rustysound-go-downloads" => {
                navigation.navigate_to(AppView::DownloadsView {});
            }
            _ => {}
        });
    }

    #[cfg(target_arch = "wasm32")]
    let nav_for_swipe = navigation.clone();
    #[cfg(target_arch = "wasm32")]
    let sidebar_open_for_swipe = sidebar_open.clone();

    // Browser-only: queue bulk cover-art prefetches to avoid provider/CDN rate-limit bursts.
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let _ = js_sys::eval(
            r#"
(() => {
  if (typeof window === 'undefined') return;
  if (window.__rustyCoverArtThrottleInstalled) return;
  window.__rustyCoverArtThrottleInstalled = true;

  const originalSetAttribute = Element.prototype.setAttribute;
  const queue = [];
  const state = new WeakMap();
  let active = 0;
  const isMobile = /Mobi|Android|iPhone|iPad|iPod/i.test(navigator.userAgent || '');
  const MAX_CONCURRENT = isMobile ? 2 : 4;
  const RETRY_DELAYS_MS = [1200, 2600];

  function isCoverArtUrl(value) {
    return typeof value === 'string' && value.includes('/rest/getCoverArt?');
  }

  function getState(img) {
    let current = state.get(img);
    if (!current) {
      current = { queued: false, loading: false, targetUrl: '', retries: 0, prefetch: false };
      state.set(img, current);
    }
    return current;
  }

  function enqueue(img, url, resetRetries, prefetch) {
    const current = getState(img);
    current.targetUrl = url;
    current.prefetch = !!prefetch;
    if (resetRetries) current.retries = 0;
    if (current.queued || current.loading) return;
    current.queued = true;
    queue.push(img);
    pump();
  }

  function finish(img) {
    const current = getState(img);
    current.loading = false;
    active = Math.max(0, active - 1);
    if (current.targetUrl && img.getAttribute('src') !== current.targetUrl) {
      enqueue(img, current.targetUrl, false, current.prefetch);
    }
    pump();
  }

  function pump() {
    while (active < MAX_CONCURRENT && queue.length > 0) {
      const img = queue.shift();
      if (!(img instanceof HTMLImageElement)) continue;

      const current = getState(img);
      current.queued = false;
      const url = current.targetUrl;
      if (!url) continue;

      active += 1;
      current.loading = true;

      if (!img.getAttribute('decoding')) originalSetAttribute.call(img, 'decoding', 'async');
      if (!img.getAttribute('fetchpriority')) originalSetAttribute.call(img, 'fetchpriority', 'low');

      let settled = false;
      let timeout = null;

      const cleanup = () => {
        if (settled) return false;
        settled = true;
        img.removeEventListener('load', onLoad);
        img.removeEventListener('error', onError);
        if (timeout) clearTimeout(timeout);
        timeout = null;
        return true;
      };

      const onLoad = () => {
        if (!cleanup()) return;
        current.retries = 0;
        finish(img);
      };

      const onError = () => {
        if (!cleanup()) return;
        if (current.retries < RETRY_DELAYS_MS.length) {
          const delay = RETRY_DELAYS_MS[current.retries++];
          current.loading = false;
          active = Math.max(0, active - 1);
          setTimeout(() => enqueue(img, current.targetUrl, false, current.prefetch), delay);
          return;
        }
        finish(img);
      };

      img.addEventListener('load', onLoad, { once: true });
      img.addEventListener('error', onError, { once: true });
      timeout = setTimeout(onError, 15000);

      originalSetAttribute.call(img, 'src', url);
    }
  }

  // Only bulk prefetches (Settings cache warm-up) go through the queue. On-screen
  // images load directly: Artwork marks them lazy, and queuing them let offscreen
  // or unmounted images hold every slot until the timeout, leaving grids blank.
  window.__rustyCoverArtEnqueue = (url) => {
    if (!isCoverArtUrl(url)) return false;
    const img = new Image();
    enqueue(img, url, true, true);
    return true;
  };
})();
            "#,
        );
    });

    // Global pointer listeners so back swipe works anywhere on the screen (PWA-like)
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let Some(win) = window() else {
            return;
        };

        let runtime = Runtime::current();
        let mut swipe_start = swipe_start.clone();
        let mut swipe_hint = swipe_hint.clone();
        let nav = nav_for_swipe.clone();
        let sidebar_open_for_swipe = sidebar_open_for_swipe.clone();

        let runtime_down = runtime.clone();
        let down_cb = Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
            let _guard = RuntimeGuard::new(runtime_down.clone());
            if e.pointer_type() != "touch" || sidebar_open_for_swipe() {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            let viewport_width = window()
                .and_then(|w| w.inner_width().ok())
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            if viewport_width <= 0.0 {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            let x = e.client_x() as f64;
            let y = e.client_y() as f64;
            let direction = if x <= HISTORY_SWIPE_EDGE_ZONE {
                1
            } else if x >= viewport_width - HISTORY_SWIPE_EDGE_ZONE {
                -1
            } else {
                0
            };

            if direction == 0 {
                swipe_start.set(None);
                swipe_hint.set(None);
                return;
            }

            swipe_start.set(Some((x, y, direction)));
            swipe_hint.set(Some((direction, 0.0)));
        }) as Box<dyn FnMut(_)>);
        let move_cb = {
            let mut swipe_start = swipe_start.clone();
            let nav = nav.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_move = runtime.clone();
            Closure::wrap(Box::new(move |e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_move.clone());
                if let Some((start_x, start_y, direction)) = swipe_start() {
                    let delta_x = e.client_x() as f64 - start_x;
                    let delta_y = e.client_y() as f64 - start_y;
                    if delta_y.abs() > HISTORY_SWIPE_VERTICAL_SLOP {
                        swipe_start.set(None);
                        swipe_hint.set(None);
                        return;
                    }

                    let travel = if direction > 0 {
                        delta_x.max(0.0)
                    } else {
                        (-delta_x).max(0.0)
                    };
                    let progress = (travel / HISTORY_SWIPE_THRESHOLD).clamp(0.0, 1.2);
                    swipe_hint.set(Some((direction, progress)));

                    if progress >= 1.0 {
                        if direction > 0 && nav.can_go_back() {
                            nav.go_back();
                        } else if direction < 0 && nav.can_go_forward() {
                            nav.go_forward();
                        }
                        swipe_start.set(None);
                        swipe_hint.set(None);
                    }
                }
            }) as Box<dyn FnMut(_)>)
        };
        let up_cb = {
            let mut swipe_start = swipe_start.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_up = runtime.clone();
            Closure::wrap(Box::new(move |_e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_up.clone());
                swipe_start.set(None);
                swipe_hint.set(None);
            }) as Box<dyn FnMut(_)>)
        };
        let cancel_cb = {
            let mut swipe_start = swipe_start.clone();
            let mut swipe_hint = swipe_hint.clone();
            let runtime_cancel = runtime.clone();
            Closure::wrap(Box::new(move |_e: web_sys::PointerEvent| {
                let _guard = RuntimeGuard::new(runtime_cancel.clone());
                swipe_start.set(None);
                swipe_hint.set(None);
            }) as Box<dyn FnMut(_)>)
        };

        let _ =
            win.add_event_listener_with_callback("pointerdown", down_cb.as_ref().unchecked_ref());
        let _ =
            win.add_event_listener_with_callback("pointermove", move_cb.as_ref().unchecked_ref());
        let _ = win.add_event_listener_with_callback("pointerup", up_cb.as_ref().unchecked_ref());
        let _ = win
            .add_event_listener_with_callback("pointercancel", cancel_cb.as_ref().unchecked_ref());

        down_cb.forget();
        move_cb.forget();
        up_cb.forget();
        cancel_cb.forget();
    });
    use_context_provider(|| now_playing);
    use_context_provider(|| queue);
    use_context_provider(|| queue_index);
    use_context_provider(|| IsPlayingSignal(is_playing));
    use_context_provider(|| VolumeSignal(volume));
    use_context_provider(|| app_settings);
    use_context_provider(|| PlaybackPositionSignal(playback_position));

    // Inject user-defined custom CSS into the document whenever it changes
    use_effect(move || {
        let raw = app_settings().custom_css.clone();
        let escaped = raw
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\0', "");
        let js = format!(
            r#"(function(){{var e=document.getElementById('rs-custom-css');if(!e){{e=document.createElement('style');e.id='rs-custom-css';document.head.appendChild(e);}}e.textContent="{}";}})();"#,
            escaped
        );
        let _ = document::eval(&js);
    });
    use_context_provider(|| SeekRequestSignal(seek_request));
    use_context_provider(|| SidebarOpenSignal(sidebar_open));
    use_context_provider(|| PreviewPlaybackSignal(preview_playback));
    use_context_provider(|| ShuffleEnabledSignal(shuffle_enabled));
    use_context_provider(|| repeat_mode);
    use_context_provider(|| audio_state);

    // Mirror the root attributes onto <html> so the page background, overscroll
    // area and scrollbars follow the theme too.
    use_effect(move || {
        let layout = layout_signal();
        let (theme, appearance) = {
            let settings = app_settings.read();
            (stage_theme(&settings.theme), appearance_attr(&settings))
        };
        let js = format!(
            "(function(){{var r=document.documentElement;r.setAttribute('data-theme','{theme}');r.setAttribute('data-appearance','{appearance}');r.setAttribute('data-layout','{}');r.setAttribute('data-platform','{}');}})();",
            layout.attr(),
            platform_name()
        );
        let _ = document::eval(&js);
    });

    // Initialize database and load saved state on mount
    use_effect(move || {
        startup_bootstrap_progress.set(0.08);
        startup_bootstrap_status.set("Initializing database".to_string());
        spawn(async move {
            // Initialize DB
            startup_bootstrap_progress.set(0.16);
            startup_bootstrap_status.set("Initializing database".to_string());
            if let Err(_e) = initialize_database().await {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to initialize database: {}", _e);
                db_initialized.set(true);
                servers_loaded.set(true);
                settings_loaded.set(true);
                startup_bootstrap_progress.set(1.0);
                startup_bootstrap_status.set("Startup ready".to_string());
                apply_cache_settings(&app_settings());
                return;
            }
            db_initialized.set(true);

            // Load servers
            startup_bootstrap_progress.set(0.42);
            startup_bootstrap_status.set("Loading saved servers".to_string());
            if let Ok(saved_servers) = load_servers().await {
                servers.set(saved_servers);
            }
            servers_loaded.set(true);

            // Load settings
            startup_bootstrap_progress.set(0.72);
            startup_bootstrap_status.set("Loading app settings".to_string());
            if let Ok(mut settings) = load_settings().await {
                let original_volume = settings.volume;
                settings.volume = normalize_volume(settings.volume);
                apply_cache_settings(&settings);
                volume.set(settings.volume);
                shuffle_enabled.set(settings.shuffle_enabled);
                repeat_mode.set(settings.repeat_mode);
                let normalized_settings = settings.clone();
                app_settings.set(settings);
                if (normalized_settings.volume - original_volume).abs() > f64::EPSILON {
                    let _ = save_settings(normalized_settings.clone()).await;
                }
            } else {
                apply_cache_settings(&app_settings());
            }
            settings_loaded.set(true);
            startup_bootstrap_progress.set(1.0);
            startup_bootstrap_status.set("Startup ready".to_string());

            // Load playback state (but don't auto-play)
            if let Ok(state) = load_playback_state().await {
                ios_diag_log(
                    "app.playback.restore",
                    &format!(
                        "queue_idx={} queue_len={} position={:.3} song_id={:?}",
                        state.queue_index,
                        state.queue.len(),
                        state.position,
                        state.song_id
                    ),
                );
                queue_index.set(state.queue_index);
                playback_position.set(state.position);
                // Note: We don't restore the full queue/song here since we'd need to re-fetch song details
                // That would require knowing which server each song came from
            }
        });
    });

    // Warm Home cache whenever the active server set changes.
    use_effect(move || {
        if !db_initialized() || !settings_loaded() {
            return;
        }

        let settings_snapshot = app_settings();
        let cache_enabled = settings_snapshot.cache_enabled;
        let home_profile = home_init_profile_from_settings(&settings_snapshot);
        if settings_snapshot.offline_mode {
            ios_diag_log("home.init.gate", "skip warmup: offline mode enabled");
            home_init_in_progress.set(false);
            home_feed.progress.set(1.0);
            home_feed.status.set(Some(
                "Offline mode: using cached Home data only".to_string(),
            ));
            home_init_signature.set(None);
            return;
        }

        let active_servers: Vec<ServerConfig> = servers()
            .into_iter()
            .filter(|server| server.active)
            .collect();
        if active_servers.is_empty() {
            ios_diag_log("home.init.gate", "skip warmup: no active servers");
            home_init_in_progress.set(false);
            home_feed.warmup_enabled.set(false);
            home_feed.progress.set(1.0);
            home_feed.status.set(Some("No active servers".to_string()));
            set_empty_home_feed_snapshot(&home_feed);
            home_init_signature.set(None);
            return;
        }

        let manual_refresh_generation = home_manual_refresh_generation();
        let manual_refresh_requested = manual_refresh_generation != home_manual_refresh_applied();
        if manual_refresh_requested {
            home_manual_refresh_applied.set(manual_refresh_generation);
            home_init_signature.set(None);
            if cache_enabled {
                let cache_prefix = home_init_cache_prefix(&active_servers, home_profile);
                let warmup_cache_key = home_init_warmup_cache_key(&active_servers, home_profile);
                let mut removed_entries = cache_remove_prefix(&format!("{cache_prefix}:"));
                removed_entries += cache_remove_prefix(&warmup_cache_key);
                for server in &active_servers {
                    for album_type in [
                        "newest",
                        "frequent",
                        "recent",
                        "alphabeticalByName",
                        "highest",
                        "random",
                    ] {
                        removed_entries += cache_remove_prefix(&format!(
                            "api:getAlbumList2:v1:{}:{}:",
                            server.id, album_type
                        ));
                    }
                }
                ios_diag_log(
                    "home.init.gate",
                    &format!(
                        "manual refresh invalidated cache entries={removed_entries} active_servers={} profile={}",
                        active_servers.len(),
                        home_init_profile_cache_key(home_profile)
                    ),
                );
            }
            ios_diag_log(
                "home.init.gate",
                &format!(
                    "manual refresh requested generation={manual_refresh_generation}; bypassing warmup gate profile={}",
                    home_init_profile_cache_key(home_profile)
                ),
            );
        }

        let warmup_key = home_init_warmup_cache_key(&active_servers, home_profile);
        let warmup_enabled = if cache_enabled {
            cache_get_json::<bool>(&warmup_key).unwrap_or(true)
        } else {
            true
        };
        home_feed.warmup_enabled.set(warmup_enabled);

        if cache_enabled {
            let cached_snapshot = load_cached_home_feed_snapshot(&active_servers, home_profile);
            apply_cached_home_feed_snapshot(&home_feed, &cached_snapshot);
            ios_diag_log(
                "home.feed.cache",
                &format!(
                    "profile={} recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={} full_cache_hit={}",
                    home_init_profile_cache_key(home_profile),
                    cached_snapshot
                        .recent_albums
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot
                        .most_played_albums
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot
                        .recent_songs
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot
                        .most_played_songs
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot
                        .random_songs
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot
                        .quick_picks
                        .as_ref()
                        .map(|items| items.len().to_string())
                        .unwrap_or_else(|| "miss".to_string()),
                    cached_snapshot.has_full_snapshot()
                ),
            );

            match (
                cached_snapshot.recent_albums.as_ref().map(Vec::len),
                cached_snapshot.most_played_albums.as_ref().map(Vec::len),
            ) {
                (Some(recent_count), Some(most_played_count)) => {
                    home_feed.progress.set(1.0);
                    home_feed.status.set(Some(format!(
                        "Home ready from cache (recent {} | most played {})",
                        recent_count, most_played_count
                    )));
                }
                (Some(recent_count), None) => {
                    home_feed.progress.set(0.56);
                    home_feed.status.set(Some(format!(
                        "Recent albums cached ({recent_count}), fetching most played albums"
                    )));
                }
                (None, Some(most_played_count)) => {
                    home_feed.progress.set(0.32);
                    home_feed.status.set(Some(format!(
                        "Most played albums cached ({most_played_count}), fetching recent albums"
                    )));
                }
                (None, None) => {
                    home_feed.progress.set(0.08);
                    home_feed.status.set(Some("Loading Home feed".to_string()));
                }
            }
        } else {
            home_feed.recent_albums.set(None);
            home_feed.most_played_albums.set(None);
            home_feed.recently_played_songs.set(None);
            home_feed.most_played_songs.set(None);
            home_feed.random_songs.set(None);
            home_feed.quick_picks.set(None);
            home_feed.progress.set(0.08);
            home_feed
                .status
                .set(Some("Cache disabled: loading Home feed live".to_string()));
            ios_diag_log(
                "home.feed.cache",
                "cache disabled: skipping persistent Home cache hydrate",
            );
        }

        let signature = home_init_server_signature(&active_servers, home_profile);
        if !manual_refresh_requested && home_init_signature().as_deref() == Some(signature.as_str())
        {
            ios_diag_log("home.init.gate", "skip warmup: signature unchanged");
            return;
        }

        home_init_signature.set(Some(signature));
        if !manual_refresh_requested
            && cache_enabled
            && home_init_is_warmed(&active_servers, home_profile)
        {
            ios_diag_log("home.init.gate", "skip warmup: cache already warmed");
            home_init_in_progress.set(false);
            home_feed.progress.set(1.0);
            return;
        }

        home_init_generation.with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *home_init_generation.peek();

        home_init_in_progress.set(true);
        home_feed.progress.set(0.04);
        home_feed.status.set(Some(if manual_refresh_requested {
            "Refreshing Home feed".to_string()
        } else if cache_enabled {
            format!(
                "Starting Home cache warmup ({})",
                home_init_profile_cache_key(home_profile)
            )
        } else {
            format!(
                "Loading Home feed live ({})",
                home_init_profile_cache_key(home_profile)
            )
        }));
        ios_diag_log(
            "home.init.gate",
            &format!(
                "starting warmup generation={} active_servers={} manual_refresh={manual_refresh_requested} profile={}",
                generation,
                active_servers.len(),
                home_init_profile_cache_key(home_profile)
            ),
        );

        let mut home_init_in_progress = home_init_in_progress.clone();
        let mut home_init_status = home_init_status.clone();
        let mut home_init_progress = home_init_progress.clone();
        let home_feed = home_feed.clone();
        let home_init_generation = home_init_generation.clone();
        spawn(async move {
            let snapshot = initialize_home_cache(
                &active_servers,
                cache_enabled,
                home_profile,
                home_init_progress.clone(),
                home_init_status.clone(),
            )
            .await;

            if *home_init_generation.peek() != generation {
                ios_diag_log(
                    "home.init.gate",
                    &format!("discard warmup generation={} (superseded)", generation),
                );
                return;
            }

            apply_home_feed_snapshot(&home_feed, &snapshot);
            home_init_in_progress.set(false);
            let summary = snapshot.summary();
            home_init_progress.set(1.0);
            home_init_status.set(Some(format!(
                "Home ready (recent albums {} | most played albums {})",
                summary.recent_albums, summary.most_played_albums
            )));
            ios_diag_log(
                "home.init.gate",
                &format!("warmup generation={} finished", generation),
            );
        });
    });

    // While startup/home-init overlays are visible, keep a short iOS log tail fresh.
    use_effect(move || {
        if !cfg!(all(not(target_arch = "wasm32"), target_os = "ios")) {
            ios_loading_log_lines.set(Vec::new());
            return;
        }

        let should_poll = !db_initialized() || !settings_loaded() || home_init_in_progress();

        if !should_poll {
            ios_loading_log_lines.set(Vec::new());
            ios_loading_log_poll_generation
                .with_mut(|generation| *generation = generation.saturating_add(1));
            return;
        }

        ios_loading_log_poll_generation
            .with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *ios_loading_log_poll_generation.peek();
        let mut ios_loading_log_lines = ios_loading_log_lines.clone();
        let ios_loading_log_poll_generation = ios_loading_log_poll_generation.clone();
        spawn(async move {
            loop {
                ios_loading_log_lines.set(ios_audio_log_snapshot(8));
                loading_log_poll_sleep().await;
                if *ios_loading_log_poll_generation.peek() != generation {
                    break;
                }
            }
        });
    });

    // Run one startup auto-download pass when enabled.
    use_effect(move || {
        if auto_download_bootstrap_done() {
            return;
        }
        if !db_initialized() || !settings_loaded() {
            return;
        }

        let settings_snapshot = app_settings();
        if !settings_snapshot.downloads_enabled || !settings_snapshot.auto_downloads_enabled {
            if !settings_snapshot.auto_downloads_enabled {
                let _ = prune_temporary_queue_prefetch_downloads(5);
            }
            auto_download_bootstrap_done.set(true);
            return;
        }

        let active_servers: Vec<ServerConfig> = servers()
            .into_iter()
            .filter(|server| server.active)
            .collect();
        if active_servers.is_empty() {
            return;
        }

        auto_download_bootstrap_done.set(true);
        spawn(async move {
            let _ = run_auto_download_pass(&active_servers, &settings_snapshot).await;
        });
    });

    // Keep checking tracked playlists so newly added songs are picked up automatically.
    use_effect(move || {
        auto_download_poll_generation
            .with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *auto_download_poll_generation.peek();

        if !db_initialized() || !settings_loaded() {
            return;
        }

        let settings_snapshot = app_settings();
        if !settings_snapshot.downloads_enabled || !settings_snapshot.auto_downloads_enabled {
            return;
        }

        let active_servers: Vec<ServerConfig> = servers()
            .into_iter()
            .filter(|server| server.active)
            .collect();
        if active_servers.is_empty() {
            return;
        }

        let auto_download_poll_generation = auto_download_poll_generation.clone();
        spawn(async move {
            loop {
                auto_download_poll_sleep().await;
                if *auto_download_poll_generation.peek() != generation {
                    break;
                }

                let _ = run_auto_download_pass(&active_servers, &settings_snapshot).await;

                if *auto_download_poll_generation.peek() != generation {
                    break;
                }
            }
        });
    });

    // Resume from the most recent bookmark on startup.
    use_effect(move || {
        if resume_bookmark_loaded() {
            return;
        }
        if !settings_loaded() {
            return;
        }
        if now_playing().is_some() {
            resume_bookmark_loaded.set(true);
            return;
        }

        let bookmark_autoplay_on_launch = app_settings().bookmark_autoplay_on_launch;
        if !bookmark_autoplay_on_launch {
            resume_bookmark_loaded.set(true);
            return;
        }
        let servers_snapshot = servers();
        if servers_snapshot.is_empty() {
            return;
        }

        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut seek_request = seek_request.clone();
        let mut resume_bookmark_loaded = resume_bookmark_loaded.clone();
        spawn(async move {
            let mut candidates: Vec<Bookmark> = Vec::new();

            for server in servers_snapshot.iter().filter(|s| s.active).cloned() {
                let client = NavidromeClient::new(server.clone());
                if let Ok(mut bookmarks) = client.get_bookmarks().await {
                    for bm in bookmarks.iter_mut() {
                        if bm.entry.server_id.is_empty() {
                            bm.entry.server_id = server.id.clone();
                        }
                        if bm.entry.server_name.is_empty() {
                            bm.entry.server_name = server.name.clone();
                        }
                    }
                    candidates.extend(
                        bookmarks
                            .into_iter()
                            .filter(|bookmark| !bookmark.entry.id.trim().is_empty()),
                    );
                }
            }

            candidates.sort_by(|a, b| {
                b.changed
                    .cmp(&a.changed)
                    .then_with(|| b.created.cmp(&a.created))
            });

            let mut resumed_song: Option<(Song, f64)> = None;
            for bookmark in candidates.into_iter() {
                let Some(server) = servers_snapshot
                    .iter()
                    .find(|server| server.id == bookmark.server_id)
                    .cloned()
                else {
                    continue;
                };

                let client = NavidromeClient::new(server);
                if let Ok(song) = client.get_song(&bookmark.entry.id).await {
                    let position = bookmark.position as f64 / 1000.0;
                    resumed_song = Some((song, position));
                    break;
                }
            }

            if let Some((song, position)) = resumed_song {
                if now_playing.peek().is_some() {
                    ios_diag_log(
                        "app.resume_bookmark",
                        "skip restore: now_playing changed while bookmark lookup was in flight",
                    );
                    resume_bookmark_loaded.set(true);
                    return;
                }
                ios_diag_log(
                    "app.resume_bookmark",
                    &format!(
                        "song_id={} position={position:.3} queue_reset=true autoplay=false",
                        song.id
                    ),
                );
                queue.set(vec![song.clone()]);
                queue_index.set(0);
                now_playing.set(Some(song.clone()));
                playback_position.set(position);
                seek_request.set(Some((song.id.clone(), position)));
                is_playing.set(false);
            } else {
                ios_diag_log("app.resume_bookmark", "no bookmark candidate restored");
            }

            resume_bookmark_loaded.set(true);
        });
    });

    // Auto-save servers when they change
    use_effect(move || {
        let current_servers = servers();
        if db_initialized() && servers_loaded() {
            spawn(async move {
                let _ = save_servers(current_servers).await;
            });
        }
    });

    // Auto-save settings when volume, shuffle, or repeat changes
    use_effect(move || {
        let vol = volume();
        let vol = normalize_volume(vol);
        let shuffle = shuffle_enabled();
        let repeat = repeat_mode();
        let mut settings = app_settings();

        if db_initialized() {
            let changed = (settings.volume - vol).abs() > 0.01
                || settings.shuffle_enabled != shuffle
                || settings.repeat_mode != repeat;

            if changed {
                settings.volume = vol;
                settings.shuffle_enabled = shuffle;
                settings.repeat_mode = repeat;
                app_settings.set(settings.clone());
                spawn(async move {
                    let _ = save_settings(settings).await;
                });
            }
        }
    });

    // Normalize volume if any writer pushes it out of range
    use_effect(move || {
        let vol = volume();
        let normalized = normalize_volume(vol);
        if (vol - normalized).abs() > f64::EPSILON {
            volume.set(normalized);
        }
    });

    // Auto-save playback position periodically.
    // Runs on every position tick, so it must stay cheap: it reads the queue
    // length (no clone) and only builds the queue list when it actually saves.
    use_effect(move || {
        let pos = playback_position();
        let idx = queue_index();
        let previewing = preview_playback();
        let ready = db_initialized();
        let queue_len = queue.read().len();
        let song_key = now_playing
            .read()
            .as_ref()
            .map(|song| (song.id.clone(), song.server_id.clone()));

        if !ready || previewing {
            return;
        }
        let Some((song_id, server_id)) = song_key else {
            return;
        };
        let position_ms = (pos.max(0.0) * 1000.0).round() as u64;

        let should_save = match &*last_playback_save.peek() {
            Some((prev_song_id, prev_server_id, prev_pos_ms, prev_idx, prev_queue_len)) => {
                *prev_song_id != song_id
                    || *prev_server_id != server_id
                    || *prev_idx != idx
                    || *prev_queue_len != queue_len
                    || position_ms.abs_diff(*prev_pos_ms) >= 1500
            }
            None => true,
        };
        if !should_save {
            return;
        }

        last_playback_save.set(Some((
            song_id.clone(),
            server_id.clone(),
            position_ms,
            idx,
            queue_len,
        )));

        let state = PlaybackState {
            song_id: Some(song_id),
            server_id: Some(server_id),
            position: pos,
            queue: queue
                .peek()
                .iter()
                .map(|s| QueueItem {
                    song_id: s.id.clone(),
                    server_id: s.server_id.clone(),
                })
                .collect(),
            queue_index: idx,
        };
        spawn(async move {
            let _ = save_playback_state(state).await;
        });
    });

    // Save temporary queue snapshots so queue can be restored after tab/app close.
    use_effect(move || {
        if !db_initialized() || !settings_loaded() {
            return;
        }
        if preview_playback() {
            return;
        }

        let queue_snapshot = queue();
        if queue_snapshot.is_empty() {
            return;
        }

        let clamped_queue_index = queue_index().min(queue_snapshot.len().saturating_sub(1));
        let now_playing_snapshot = now_playing();
        let signature = queue_snapshot_signature(
            &queue_snapshot,
            clamped_queue_index,
            now_playing_snapshot.as_ref(),
        );
        if signature == *last_queue_snapshot_signature.peek() {
            return;
        }
        last_queue_snapshot_signature.set(signature);

        let saved_at_epoch_ms = Utc::now().timestamp_millis();
        let snapshot = TemporaryQueueSnapshot {
            id: format!("queue-{saved_at_epoch_ms}"),
            saved_at_epoch_ms,
            queue: queue_snapshot,
            queue_index: clamped_queue_index,
            now_playing: now_playing_snapshot,
            playback_position: playback_position.peek().max(0.0),
        };

        spawn(async move {
            let _ = save_temporary_queue_snapshot(snapshot).await;
        });
    });

    let view = use_route::<AppView>();
    let layout = layout_signal();
    let compact = layout.is_compact();
    let is_startup_bootstrapping = !db_initialized() || !settings_loaded();
    let is_home_initializing = home_init_in_progress() && matches!(&view, AppView::HomeView {});
    let show_ios_loading_logs = cfg!(all(not(target_arch = "wasm32"), target_os = "ios"));
    let boot_logs = if show_ios_loading_logs && (is_startup_bootstrapping || is_home_initializing) {
        ios_loading_log_lines()
    } else {
        Vec::new()
    };
    let (offline_mode_enabled, theme_attr, appearance) = {
        let settings = app_settings.read();
        (
            settings.offline_mode,
            stage_theme(&settings.theme),
            appearance_attr(&settings),
        )
    };
    let is_transport_loading = transport_loading_signal();
    let show_transport_loading_overlay =
        is_transport_loading && !is_startup_bootstrapping && !is_home_initializing;
    let transport_loading_label = if show_transport_loading_overlay {
        transport_label_signal().unwrap_or_else(|| "Loading playback…".to_string())
    } else {
        String::new()
    };
    let has_song = now_playing.read().is_some();
    let player_on = !compact || has_song;
    let swipe_hint_state = swipe_hint();

    rsx! {
        div {
            class: "rs-app",
            "data-platform": platform_name(),
            "data-theme": theme_attr,
            "data-appearance": appearance,
            "data-layout": layout.attr(),
            "data-player": if player_on { "on" } else { "off" },

            if !compact {
                Sidebar {}
            }

            div { class: "rs-content",
                main {
                    class: "rs-main main-scroll",
                    onmounted: move |evt| async move {
                        if let Ok(rect) = evt.get_client_rect().await {
                            main_viewport_top.set(rect.origin.y);
                            main_scroll_height.set(rect.size.height);
                        }
                    },
                    onscroll: move |evt| {
                        // Quantised so windowed lists re-render per 32px, not per event.
                        let top = (evt.scroll_top() / 32.0).floor() * 32.0;
                        if top != *main_scroll_top.peek() {
                            main_scroll_top.set(top);
                        }
                        let height = evt.client_height() as f64;
                        if (height - *main_scroll_height.peek()).abs() > 1.0 {
                            main_scroll_height.set(height);
                        }
                    },
                    if offline_mode_enabled {
                        div { class: "rs-banner", role: "status", "data-testid": "offline-banner",
                            Icon { name: "offline".to_string(), class: "rs-ic".to_string() }
                            div { class: "rs-banner__text",
                                p { class: "rs-banner__title", "Offline mode is on" }
                                p { class: "rs-banner__sub", "Only downloaded and cached music is available." }
                            }
                            button {
                                r#type: "button",
                                class: "rs-pill rs-pill--mini",
                                onclick: move |_| {
                                    let mut settings = app_settings();
                                    if !settings.offline_mode {
                                        return;
                                    }
                                    settings.offline_mode = false;
                                    apply_cache_settings(&settings);
                                    let settings_clone = settings.clone();
                                    app_settings.set(settings);
                                    spawn(async move {
                                        let _ = save_settings(settings_clone).await;
                                    });
                                },
                                "Go Online"
                            }
                        }
                    }
                    div { key: "{outlet_key}", class: "rs-outlet",
                        Outlet::<AppView> {}
                    }
                }
                PlayerBar {}
            }

            if compact {
                TabBar {}
            }

            if let Some((direction, progress)) = swipe_hint_state {
                if progress > 0.0 {
                    div {
                        class: if direction > 0 { "rs-swipe-hint rs-swipe-hint--back" } else { "rs-swipe-hint rs-swipe-hint--forward" },
                        aria_hidden: "true",
                        style: format!(
                            "opacity:{:.2};transform:translateY(-50%) translateX({:.1}px) scale({:.2});",
                            0.2 + progress.min(1.0) * 0.8,
                            if direction > 0 { -12.0 + progress.min(1.0) * 12.0 } else { 12.0 - progress.min(1.0) * 12.0 },
                            0.86 + progress.min(1.0) * 0.18
                        ),
                        Icon {
                            name: if direction > 0 { "chevron-left".to_string() } else { "chevron-right".to_string() },
                            class: "rs-ic".to_string(),
                        }
                    }
                }
            }

            AddToMenuOverlay { controller: add_menu.clone() }

            SongDetailsOverlay { controller: song_details.clone() }

            TrackMenuHost {}

            if show_transport_loading_overlay {
                div { class: "rs-capsule", role: "status", "data-testid": "transport-loading",
                    span { class: "rs-spinner", aria_hidden: "true" }
                    span { class: "rs-truncate", "{transport_loading_label}" }
                }
            }

            if is_startup_bootstrapping {
                BootOverlay {
                    title: "Starting RustySound".to_string(),
                    message: "Loading your servers and settings.".to_string(),
                    progress: startup_bootstrap_progress(),
                    stage: startup_bootstrap_status(),
                    logs: boot_logs.clone(),
                    testid: "boot-overlay".to_string(),
                }
            } else if is_home_initializing {
                BootOverlay {
                    title: "Getting Discover ready".to_string(),
                    message: "Caching songs and albums so Discover opens instantly.".to_string(),
                    progress: home_init_progress(),
                    stage: home_init_status().unwrap_or_else(|| "Caching your library".to_string()),
                    logs: boot_logs.clone(),
                    testid: "home-init-overlay".to_string(),
                }
            }

            // Audio controller - manages playback separately from UI
            AudioController {}
        }
    }
}
