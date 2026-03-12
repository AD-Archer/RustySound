use crate::api::*;
use crate::cache_service::{apply_settings as apply_cache_settings, put_json as cache_put_json};
use crate::components::{
    view_label, AddIntent, AddMenuController, AddToMenuOverlay, AppView, AudioController,
    AudioState, Icon, Navigation, PlaybackPositionSignal, Player, PreviewPlaybackSignal,
    SeekRequestSignal, Sidebar, SidebarOpenSignal, SongDetailsController, SongDetailsOverlay,
    SongDetailsState, VolumeSignal,
};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, AppSettings, PlaybackState, QueueItem,
};
use crate::diagnostics::{log_perf, PerfTimer};
use crate::offline_audio::run_auto_download_pass;
#[cfg(target_arch = "wasm32")]
use dioxus::core::{Runtime, RuntimeGuard};
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

#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_THRESHOLD: f64 = 100.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_VERTICAL_SLOP: f64 = 72.0;
#[cfg(target_arch = "wasm32")]
const HISTORY_SWIPE_EDGE_ZONE: f64 = 28.0;
const HOME_INIT_QUICK_PICK_LIMIT: usize = 8;
const HOME_INIT_SECTION_BASE_COUNT: usize = 9;
const HOME_INIT_SECTION_LOAD_STEP: usize = 6;
const HOME_INIT_SECTION_CACHE_COUNT: usize =
    HOME_INIT_SECTION_BASE_COUNT + HOME_INIT_SECTION_LOAD_STEP;
const HOME_INIT_SECTION_FETCH_LIMIT: usize = 30;
const HOME_INIT_RANDOM_FETCH_LIMIT: usize = HOME_INIT_SECTION_FETCH_LIMIT;
const HOME_INIT_ALBUM_PREVIEW_LIMIT: u32 = HOME_INIT_SECTION_BASE_COUNT as u32;
const HOME_INIT_WARMUP_FLAG_CACHE_HOURS: u32 = 24 * 365;

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

fn home_init_warmup_cache_key(active_servers: &[ServerConfig]) -> String {
    let mut ids: Vec<String> = active_servers
        .iter()
        .map(|server| server.id.clone())
        .collect();
    ids.sort();
    format!("view:home:warmup:v1:{}", ids.join("|"))
}

fn home_init_cache_prefix(active_servers: &[ServerConfig]) -> String {
    let mut ids: Vec<String> = active_servers
        .iter()
        .map(|server| server.id.clone())
        .collect();
    ids.sort();
    format!("view:home:v2:{}", ids.join("|"))
}

fn home_init_server_signature(active_servers: &[ServerConfig]) -> String {
    let mut signature_parts: Vec<String> = active_servers
        .iter()
        .map(|server| format!("{}|{}|{}", server.id, server.url, server.username))
        .collect();
    signature_parts.sort();
    signature_parts.join("||")
}

fn home_init_song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
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

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_home_init_native_activity_songs_for_servers(
    active_servers: &[ServerConfig],
    sort: NativeSongSortField,
    limit: u32,
) -> Vec<Song> {
    let mut songs = Vec::<Song>::new();
    let end = limit.saturating_sub(1) as usize;

    for server in active_servers.iter().cloned() {
        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_native_songs(sort, NativeSortOrder::Desc, 0, end)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            home_init_fetch_yield().await;
            fetched = client
                .get_native_songs(sort, NativeSortOrder::Desc, 0, end)
                .await
                .unwrap_or_default();
        }
        songs.append(&mut fetched);
        if songs.len() >= limit as usize {
            break;
        }
        home_init_fetch_yield().await;
    }

    dedupe_home_init_songs(songs, limit as usize)
}

#[cfg(target_arch = "wasm32")]
fn derive_home_init_web_song_sections(
    mut pool: Vec<Song>,
    section_limit: usize,
) -> (Vec<Song>, Vec<Song>, Vec<Song>) {
    if pool.is_empty() || section_limit == 0 {
        return (Vec::new(), Vec::new(), Vec::new());
    }

    let recent = dedupe_home_init_songs(pool.clone(), section_limit);

    if pool.len() > 1 {
        let step = (section_limit / 2).max(1).min(pool.len().saturating_sub(1));
        pool.rotate_left(step);
    }
    let most_played = dedupe_home_init_songs(pool.clone(), section_limit);

    if pool.len() > 1 {
        let step = (section_limit / 3).max(1).min(pool.len().saturating_sub(1));
        pool.rotate_left(step);
    }
    let random = dedupe_home_init_songs(pool, HOME_INIT_RANDOM_FETCH_LIMIT);

    (recent, most_played, random)
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_home_init_similar_songs_for_seeds(
    active_servers: &[ServerConfig],
    seeds: &[Song],
    per_seed: u32,
    total_limit: usize,
) -> Vec<Song> {
    if seeds.is_empty() || per_seed == 0 {
        return Vec::new();
    }

    let seed_keys = seeds
        .iter()
        .map(home_init_song_key)
        .collect::<std::collections::HashSet<_>>();
    let mut similar = Vec::<Song>::new();

    for seed in seeds.iter().take(8).cloned() {
        let Some(server) = active_servers
            .iter()
            .find(|server| server.id == seed.server_id)
            .cloned()
        else {
            continue;
        };

        let client = NavidromeClient::new(server);
        let mut fetched = client
            .get_similar_songs2(&seed.id, per_seed)
            .await
            .unwrap_or_default();
        if fetched.is_empty() {
            fetched = client
                .get_similar_songs(&seed.id, per_seed)
                .await
                .unwrap_or_default();
        }
        similar.append(&mut fetched);
        home_init_fetch_yield().await;
    }

    similar.retain(|song| !seed_keys.contains(&home_init_song_key(song)));

    dedupe_home_init_songs(similar, total_limit)
}

#[cfg(not(target_arch = "wasm32"))]
async fn build_home_init_quick_picks_mix(
    active_servers: &[ServerConfig],
    most_played_songs: &[Song],
    limit: usize,
) -> Vec<Song> {
    if limit == 0 {
        return Vec::new();
    }

    let anchors = dedupe_home_init_songs(most_played_songs.to_vec(), 8);
    let similar =
        fetch_home_init_similar_songs_for_seeds(active_servers, &anchors, 4, limit * 3).await;
    let random =
        fetch_home_init_random_songs_for_servers(active_servers, (limit as u32).saturating_mul(2))
            .await;

    let mut anchor_iter = anchors.into_iter();
    let mut similar_iter = similar.into_iter();
    let mut random_iter = random.into_iter();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut mixed = Vec::<Song>::new();

    loop {
        let mut progressed = false;

        if let Some(song) = anchor_iter.next() {
            progressed = true;
            let key = home_init_song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit {
            break;
        }

        if let Some(song) = similar_iter.next() {
            progressed = true;
            let key = home_init_song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit {
            break;
        }

        if let Some(song) = random_iter.next() {
            progressed = true;
            let key = home_init_song_key(&song);
            if seen.insert(key) {
                mixed.push(song);
            }
        }

        if mixed.len() >= limit || !progressed {
            break;
        }
    }

    mixed.truncate(limit);
    mixed
}

#[cfg(target_arch = "wasm32")]
async fn build_home_init_quick_picks_mix(
    active_servers: &[ServerConfig],
    _most_played_songs: &[Song],
    limit: usize,
) -> Vec<Song> {
    if limit == 0 {
        return Vec::new();
    }
    fetch_home_init_random_songs_for_servers(active_servers, limit as u32).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn home_init_fetch_yield() {
    tokio::task::yield_now().await;
}

#[cfg(target_arch = "wasm32")]
async fn home_init_fetch_yield() {
    gloo_timers::future::TimeoutFuture::new(0).await;
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

async fn initialize_home_cache(active_servers: &[ServerConfig]) -> HomeInitSummary {
    let init_start = PerfTimer::now();
    let warmup_key = home_init_warmup_cache_key(active_servers);
    let _ = cache_put_json(warmup_key, &true, Some(HOME_INIT_WARMUP_FLAG_CACHE_HOURS));

    let cache_prefix = home_init_cache_prefix(active_servers);
    let recent_cache_key = format!("{cache_prefix}:recent_albums");
    let most_played_album_cache_key = format!("{cache_prefix}:most_played_albums");
    let recent_song_cache_key = format!("{cache_prefix}:recent_songs");
    let most_played_song_cache_key = format!("{cache_prefix}:most_played_songs");
    let random_song_cache_key = format!("{cache_prefix}:random_songs");
    let quick_pick_cache_key = format!("{cache_prefix}:quick_picks");

    eprintln!(
        "[app-init] starting home cache warmup for {} server(s)",
        active_servers.len()
    );

    #[cfg(not(target_arch = "wasm32"))]
    let (recent_played, most_played_song_items, random_song_items) = {
        let recent_played = fetch_home_init_native_activity_songs_for_servers(
            active_servers,
            NativeSongSortField::PlayDate,
            HOME_INIT_SECTION_FETCH_LIMIT as u32,
        )
        .await;
        let recent_cached: Vec<Song> = recent_played
            .iter()
            .take(HOME_INIT_SECTION_CACHE_COUNT)
            .cloned()
            .collect();
        let _ = cache_put_json(recent_song_cache_key.clone(), &recent_cached, Some(3));
        home_init_fetch_yield().await;

        let most_played_song_items = fetch_home_init_native_activity_songs_for_servers(
            active_servers,
            NativeSongSortField::PlayCount,
            HOME_INIT_SECTION_FETCH_LIMIT as u32,
        )
        .await;
        let most_played_cached: Vec<Song> = most_played_song_items
            .iter()
            .take(HOME_INIT_SECTION_CACHE_COUNT)
            .cloned()
            .collect();
        let _ = cache_put_json(
            most_played_song_cache_key.clone(),
            &most_played_cached,
            Some(6),
        );
        home_init_fetch_yield().await;

        let random_song_items = fetch_home_init_random_songs_for_servers(
            active_servers,
            HOME_INIT_RANDOM_FETCH_LIMIT as u32,
        )
        .await;
        let _ = cache_put_json(random_song_cache_key.clone(), &random_song_items, Some(2));
        home_init_fetch_yield().await;

        (recent_played, most_played_song_items, random_song_items)
    };

    #[cfg(target_arch = "wasm32")]
    let (recent_played, most_played_song_items, random_song_items) = {
        let pool_size = (HOME_INIT_SECTION_FETCH_LIMIT * 3).max(HOME_INIT_RANDOM_FETCH_LIMIT * 2);
        let song_pool =
            fetch_home_init_random_songs_for_servers(active_servers, pool_size as u32).await;
        let (recent_played, most_played_song_items, random_song_items) =
            derive_home_init_web_song_sections(song_pool, HOME_INIT_SECTION_FETCH_LIMIT);

        let recent_cached: Vec<Song> = recent_played
            .iter()
            .take(HOME_INIT_SECTION_CACHE_COUNT)
            .cloned()
            .collect();
        let most_played_cached: Vec<Song> = most_played_song_items
            .iter()
            .take(HOME_INIT_SECTION_CACHE_COUNT)
            .cloned()
            .collect();
        let _ = cache_put_json(recent_song_cache_key.clone(), &recent_cached, Some(3));
        let _ = cache_put_json(
            most_played_song_cache_key.clone(),
            &most_played_cached,
            Some(6),
        );
        let _ = cache_put_json(random_song_cache_key.clone(), &random_song_items, Some(2));
        home_init_fetch_yield().await;

        (recent_played, most_played_song_items, random_song_items)
    };

    let mut quick = build_home_init_quick_picks_mix(
        active_servers,
        &most_played_song_items,
        HOME_INIT_QUICK_PICK_LIMIT,
    )
    .await;
    if quick.is_empty() {
        quick = dedupe_home_init_songs(random_song_items.clone(), HOME_INIT_QUICK_PICK_LIMIT);
    }
    if quick.is_empty() {
        quick = fetch_home_init_random_songs_for_servers(
            active_servers,
            HOME_INIT_QUICK_PICK_LIMIT as u32,
        )
        .await;
    }
    let _ = cache_put_json(quick_pick_cache_key, &quick, Some(3));
    home_init_fetch_yield().await;

    let recent_albums =
        fetch_home_init_albums_for_servers(active_servers, "newest", HOME_INIT_ALBUM_PREVIEW_LIMIT)
            .await;
    let _ = cache_put_json(recent_cache_key, &recent_albums, Some(6));
    home_init_fetch_yield().await;

    let most_played_albums = fetch_home_init_albums_for_servers(
        active_servers,
        "frequent",
        HOME_INIT_SECTION_FETCH_LIMIT as u32,
    )
    .await;
    let most_played_cached: Vec<Album> = most_played_albums
        .iter()
        .take(HOME_INIT_SECTION_CACHE_COUNT)
        .cloned()
        .collect();
    let _ = cache_put_json(most_played_album_cache_key, &most_played_cached, Some(6));

    let summary = HomeInitSummary {
        recent_albums: recent_albums.len(),
        most_played_albums: most_played_albums.len(),
        recent_songs: recent_played.len(),
        most_played_songs: most_played_song_items.len(),
        random_songs: random_song_items.len(),
        quick_picks: quick.len(),
    };

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

    summary
}

#[component]
pub fn AppShell() -> Element {
    let mut servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_route::<AppView>();
    let now_playing = use_signal(|| None::<Song>);
    let queue = use_signal(Vec::<Song>::new);
    let mut queue_index = use_signal(|| 0usize);
    let is_playing = use_signal(|| false);
    let mut volume = use_signal(|| 0.8f64);
    let mut app_settings = use_signal(AppSettings::default);
    let mut playback_position = use_signal(|| 0.0f64);
    let mut last_playback_save = use_signal(|| None::<(String, String, u64, usize, usize)>);
    let mut db_initialized = use_signal(|| false);
    let mut settings_loaded = use_signal(|| false);
    let mut shuffle_enabled = use_signal(|| false);
    let mut repeat_mode = use_signal(|| RepeatMode::Off);
    let mut auto_download_bootstrap_done = use_signal(|| false);
    let mut home_init_in_progress = use_signal(|| false);
    let mut home_init_status = use_signal(|| None::<String>);
    let mut home_init_signature = use_signal(|| None::<String>);
    let mut home_init_generation = use_signal(|| 0u64);
    let audio_state = use_signal(AudioState::default);
    let preview_playback = use_signal(|| false);
    let sidebar_open = use_signal(|| false);
    let navigation = Navigation::new();
    let seek_request = use_signal(|| None::<(String, f64)>);
    let mut resume_bookmark_loaded = use_signal(|| false);
    #[cfg(target_arch = "wasm32")]
    let swipe_start = use_signal(|| None::<(f64, f64, i8)>);
    let swipe_hint = use_signal(|| None::<(i8, f64)>);
    let add_menu_intent = use_signal(|| None::<AddIntent>);
    let add_menu = AddMenuController::new(add_menu_intent.clone());
    let song_details_state = use_signal(SongDetailsState::default);
    let song_details = SongDetailsController::new(song_details_state.clone());

    // Provide state via context
    use_context_provider(|| servers);
    use_context_provider(|| current_view);
    use_context_provider(|| navigation.clone());
    use_context_provider(|| add_menu.clone());
    use_context_provider(|| song_details.clone());

    #[cfg(target_arch = "wasm32")]
    let nav_for_swipe = navigation.clone();
    #[cfg(target_arch = "wasm32")]
    let sidebar_open_for_swipe = sidebar_open.clone();

    // Browser-only: queue cover-art image requests to avoid provider/CDN rate-limit bursts.
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

      if (!img.getAttribute('loading')) originalSetAttribute.call(img, 'loading', 'lazy');
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

  window.__rustyCoverArtEnqueue = (url) => {
    if (!isCoverArtUrl(url)) return false;
    const img = new Image();
    enqueue(img, url, true, true);
    return true;
  };

  Element.prototype.setAttribute = function(name, value) {
    if (this instanceof HTMLImageElement && name === 'src' && isCoverArtUrl(value)) {
      enqueue(this, value, true, false);
      return;
    }
    return originalSetAttribute.call(this, name, value);
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
    use_context_provider(|| is_playing);
    use_context_provider(|| VolumeSignal(volume));
    use_context_provider(|| app_settings);
    use_context_provider(|| PlaybackPositionSignal(playback_position));
    use_context_provider(|| SeekRequestSignal(seek_request));
    use_context_provider(|| SidebarOpenSignal(sidebar_open));
    use_context_provider(|| PreviewPlaybackSignal(preview_playback));
    use_context_provider(|| shuffle_enabled);
    use_context_provider(|| repeat_mode);
    use_context_provider(|| audio_state);

    // Initialize database and load saved state on mount
    use_effect(move || {
        spawn(async move {
            // Initialize DB
            if let Err(_e) = initialize_database().await {
                #[cfg(not(target_arch = "wasm32"))]
                eprintln!("Failed to initialize database: {}", _e);
                db_initialized.set(true);
                settings_loaded.set(true);
                apply_cache_settings(&app_settings());
                return;
            }
            db_initialized.set(true);

            // Load servers
            if let Ok(saved_servers) = load_servers().await {
                servers.set(saved_servers);
            }

            // Load settings
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

            // Load playback state (but don't auto-play)
            if let Ok(state) = load_playback_state().await {
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
        if settings_snapshot.offline_mode {
            home_init_in_progress.set(false);
            home_init_status.set(None);
            home_init_signature.set(None);
            return;
        }

        let active_servers: Vec<ServerConfig> = servers()
            .into_iter()
            .filter(|server| server.active)
            .collect();
        if active_servers.is_empty() {
            home_init_in_progress.set(false);
            home_init_status.set(None);
            home_init_signature.set(None);
            return;
        }

        let signature = home_init_server_signature(&active_servers);
        if home_init_signature().as_deref() == Some(signature.as_str()) {
            return;
        }

        home_init_signature.set(Some(signature));
        home_init_generation.with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *home_init_generation.peek();

        home_init_in_progress.set(true);
        home_init_status.set(Some("App initializing, please wait".to_string()));

        let mut home_init_in_progress = home_init_in_progress.clone();
        let mut home_init_status = home_init_status.clone();
        let home_init_generation = home_init_generation.clone();
        spawn(async move {
            let _ = initialize_home_cache(&active_servers).await;

            if *home_init_generation.peek() != generation {
                return;
            }

            home_init_in_progress.set(false);
            home_init_status.set(None);
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
                queue.set(vec![song.clone()]);
                queue_index.set(0);
                now_playing.set(Some(song.clone()));
                playback_position.set(position);
                seek_request.set(Some((song.id.clone(), position)));
                is_playing.set(true);
            }

            resume_bookmark_loaded.set(true);
        });
    });

    // Auto-save servers when they change
    use_effect(move || {
        let current_servers = servers();
        if db_initialized() && !current_servers.is_empty() {
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

    // Auto-save playback position periodically
    use_effect(move || {
        let song = now_playing();
        let pos = playback_position();
        let q = queue();
        let idx = queue_index();
        let previewing = preview_playback();

        if db_initialized() && song.is_some() && !previewing {
            let current = song.as_ref().expect("checked is_some");
            let song_id = current.id.clone();
            let server_id = current.server_id.clone();
            let position_ms = (pos.max(0.0) * 1000.0).round() as u64;
            let queue_len = q.len();

            let should_save = match last_playback_save() {
                Some((prev_song_id, prev_server_id, prev_pos_ms, prev_idx, prev_queue_len)) => {
                    prev_song_id != song_id
                        || prev_server_id != server_id
                        || prev_idx != idx
                        || prev_queue_len != queue_len
                        || position_ms.abs_diff(prev_pos_ms) >= 1500
                }
                None => true,
            };

            if !should_save {
                return;
            }

            last_playback_save.set(Some((song_id, server_id, position_ms, idx, queue_len)));

            let state = PlaybackState {
                song_id: song.as_ref().map(|s| s.id.clone()),
                server_id: song.as_ref().map(|s| s.server_id.clone()),
                position: pos,
                queue: q
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
        }
    });

    let view = use_route::<AppView>();
    let sidebar_signal = sidebar_open.clone();
    let can_go_back = navigation.can_go_back();
    let song_details_open = song_details_state().is_open;
    let is_startup_bootstrapping = !db_initialized() || !settings_loaded();
    let is_home_initializing = home_init_in_progress() && matches!(&view, AppView::HomeView {});
    let home_init_status_text = home_init_status()
        .unwrap_or_else(|| "Caching songs and albums for Home to avoid cold loads.".to_string());
    let offline_mode_enabled = app_settings().offline_mode;
    let app_container_class = if sidebar_open() {
        "app-container sidebar-open-mobile flex min-h-screen text-white overflow-hidden"
    } else {
        "app-container flex min-h-screen text-white overflow-hidden"
    };
    let swipe_hint_state = swipe_hint();

    rsx! {
        div { class: "{app_container_class}",
            if sidebar_open() && !song_details_open {
                div {
                    class: "fixed inset-0 bg-black/60 backdrop-blur-sm z-30 2xl:hidden",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                }
            }

            // Sidebar
            Sidebar { sidebar_open: sidebar_signal, overlay_mode: false }

            // Main content area
            div { class: "flex-1 flex flex-col overflow-hidden",
                header { class: "mobile-safe-top 2xl:hidden border-b border-zinc-800/60 bg-zinc-950/80 backdrop-blur-xl",
                    div { class: "flex items-center justify-between px-4 py-3",
                        div { class: "flex items-center gap-1",
                            button {
                                class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                                aria_label: "Open menu",
                                onclick: {
                                    let mut sidebar_open = sidebar_open.clone();
                                    move |_| sidebar_open.set(true)
                                },
                                Icon {
                                    name: "menu".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                            if can_go_back {
                                button {
                                    class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                                    aria_label: "Go back",
                                    onclick: {
                                        let navigation = navigation.clone();
                                        move |_| {
                                            let _ = navigation.go_back();
                                        }
                                    },
                                    Icon {
                                        name: "arrow-left".to_string(),
                                        class: "w-5 h-5".to_string(),
                                    }
                                }
                            }
                        }
                        div { class: "flex flex-col items-center text-center",
                            span { class: "text-xs uppercase tracking-widest text-zinc-500",
                                "RustySound"
                            }
                            span { class: "text-sm font-semibold text-white", "{view_label(&view)}" }
                        }
                        button {
                            class: "p-2 rounded-lg text-zinc-300 hover:text-white hover:bg-zinc-800/60 transition-colors",
                            aria_label: "Open queue",
                            onclick: {
                                let nav = navigation.clone();
                                move |_| nav.navigate_to(AppView::QueueView {})
                            },
                            Icon {
                                name: "bars".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                }

                // Main scrollable content
                main {
                    class: "flex-1 overflow-y-auto main-scroll",
                    div {
                        class: "page-shell",
                        if offline_mode_enabled {
                            div { class: "mb-4 rounded-xl border border-amber-500/40 bg-amber-500/10 p-3 flex flex-wrap items-center justify-between gap-3",
                                div {
                                    p { class: "text-sm font-medium text-amber-200", "Offline mode is currently enabled." }
                                    p { class: "text-xs text-amber-100/80", "Only downloaded and cached content is available." }
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg border border-amber-400/60 text-amber-100 hover:text-white hover:border-amber-300 transition-colors text-sm",
                                    onclick: {
                                        let mut app_settings = app_settings.clone();
                                        move |_| {
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
                                        }
                                    },
                                    "Disable Offline Mode"
                                }
                            }
                        }
                        Outlet::<AppView> {}
                    }
                }
            }

            // Fixed bottom player
            Player {}
        }

        if let Some((direction, progress)) = swipe_hint_state {
            if progress > 0.0 {
                div {
                    class: if direction > 0 {
                        "swipe-hint swipe-hint--back 2xl:hidden"
                    } else {
                        "swipe-hint swipe-hint--forward 2xl:hidden"
                    },
                    style: if direction > 0 {
                        format!(
                            "opacity: {}; transform: translateY(-50%) translateX({}px) scale({});",
                            0.2 + progress.min(1.0) * 0.8,
                            -12.0 + progress.min(1.0) * 12.0,
                            0.86 + progress.min(1.0) * 0.18
                        )
                    } else {
                        format!(
                            "opacity: {}; transform: translateY(-50%) translateX({}px) scale({});",
                            0.2 + progress.min(1.0) * 0.8,
                            12.0 - progress.min(1.0) * 12.0,
                            0.86 + progress.min(1.0) * 0.18
                        )
                    },
                    div {
                        class: "w-10 h-10 rounded-full border border-emerald-400/50 bg-zinc-950/80 text-emerald-300 shadow-lg backdrop-blur flex items-center justify-center",
                        Icon {
                            name: "arrow-left".to_string(),
                            class: if direction > 0 { "w-5 h-5".to_string() } else { "w-5 h-5 rotate-180".to_string() },
                        }
                    }
                }
            }
        }

        AddToMenuOverlay { controller: add_menu.clone() }

        SongDetailsOverlay { controller: song_details.clone() }

        if song_details_open {
            if sidebar_open() {
                div {
                    class: "fixed inset-0 bg-black/60 backdrop-blur-sm z-[115]",
                    onclick: {
                        let mut sidebar_open = sidebar_open.clone();
                        move |_| sidebar_open.set(false)
                    },
                }
            }
            Sidebar { sidebar_open: sidebar_open, overlay_mode: true }
        }

        if is_startup_bootstrapping {
            div { class: "fixed inset-0 z-[210] bg-zinc-950/95 backdrop-blur-sm flex items-center justify-center px-6",
                div { class: "max-w-md text-center space-y-3",
                    div { class: "flex items-center justify-center",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-emerald-400 animate-spin".to_string(),
                        }
                    }
                    h2 { class: "text-lg font-semibold text-white", "Preparing RustySound" }
                    p { class: "text-sm text-zinc-400",
                        "Loading servers and settings, then warming local cache for faster navigation."
                    }
                }
            }
        }

        if is_home_initializing {
            div { class: "fixed inset-0 z-[210] bg-zinc-950/95 backdrop-blur-sm flex items-center justify-center px-6",
                div { class: "max-w-md text-center space-y-3",
                    div { class: "flex items-center justify-center",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-emerald-400 animate-spin".to_string(),
                        }
                    }
                    h2 { class: "text-lg font-semibold text-white", "App initializing, please wait" }
                    p { class: "text-sm text-zinc-400", "{home_init_status_text}" }
                }
            }
        }

        // Audio controller - manages playback separately from UI
        AudioController {}
    }
}
