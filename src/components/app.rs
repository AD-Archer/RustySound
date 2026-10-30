use crate::api::*;
use crate::cache_service::apply_settings as apply_cache_settings;
use crate::components::ui::env::{
    appearance_attr, platform_name, stage_theme, use_layout_provider,
};
use crate::components::ui::{
    EdgeSwipe, LayoutSignal, MainScroll, PlayerBar, Sidebar, SongChange, TabBar, TrackMenuController,
    TrackMenuHost, TrackMenuRequest, WindowDragRegion,
};
use crate::components::{
    ios_audio_log_snapshot, ios_diag_log, AddIntent, AddMenuController,
    AddToMenuOverlay, AppView, AudioController, AudioState, Icon,
    IsPlayingSignal, Navigation, PlaybackPositionSignal, PreviewPlaybackSignal, SeekRequestSignal,
    ShuffleEnabledSignal, SongDetailsController, SongDetailsOverlay,
    SongDetailsState, VolumeSignal,
};
use crate::db::{
    initialize_database, load_playback_state, load_servers, load_settings, save_playback_state,
    save_servers, save_settings, save_temporary_queue_snapshot, AppSettings, PlaybackState,
    QueueItem, TemporaryQueueSnapshot,
};
use crate::offline_audio::{prune_temporary_queue_prefetch_downloads, run_auto_download_pass};
use chrono::Utc;
#[cfg(all(feature = "desktop", target_os = "macos"))]
use dioxus::desktop::use_muda_event_handler;
use dioxus_router::components::Outlet;
// Re-export RepeatMode for other components
pub use crate::db::RepeatMode;
use dioxus::prelude::*;
use dioxus_router::use_navigator;

const AUTO_DOWNLOAD_POLL_INTERVAL_MS: u64 = 5 * 60 * 1000;

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

/// Focuses (and selects) the Search field once the Search view has mounted.
/// Polls briefly, because the route change renders a frame or two later.
fn focus_global_search_input() {
    let _ = document::eval(
        r#"
(() => {
  let tries = 0;
  const attempt = () => {
    const input = document.getElementById("global-search-input");
    if (input && typeof input.focus === "function") {
      input.focus();
      if (typeof input.select === "function") input.select();
      return;
    }
    if (++tries < 40) setTimeout(attempt, 25);
  };
  attempt();
  return true;
})();
        "#,
    );
}

/// Keydown listener for the Find shortcut: Cmd+F on Apple platforms, Ctrl+F
/// elsewhere. Multi-line editors (textarea, contenteditable) keep the key.
/// The script never resolves, so its channel stays open for `dioxus.send`.
const FIND_SHORTCUT_JS: &str = r#"
const isApple = /Mac|iPhone|iPad|iPod/i.test(navigator.platform || navigator.userAgent || "");
if (window.__rsFindShortcut) document.removeEventListener("keydown", window.__rsFindShortcut);
const handler = (event) => {
  if (!event || event.defaultPrevented || event.isComposing) return;
  if ((event.key || "").toLowerCase() !== "f" || event.altKey || event.shiftKey) return;
  const primary = isApple ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary) return;
  const target = event.target;
  if (target && target.closest && target.closest("textarea, [contenteditable]:not([contenteditable='false'])")) return;
  event.preventDefault();
  dioxus.send(true);
};
window.__rsFindShortcut = handler;
document.addEventListener("keydown", handler);
await new Promise(() => {});
"#;

/// Route for a macOS menu item id (app menu, Edit > Find, Go).
#[cfg(all(feature = "desktop", target_os = "macos"))]
fn menu_route(id: &str) -> Option<AppView> {
    Some(match id {
        "rustysound-open-settings" => AppView::SettingsView {},
        "rustysound-go-home" => AppView::HomeView {},
        "rustysound-go-search" | "rustysound-find" => AppView::SearchView {},
        "rustysound-go-albums" => AppView::Albums {},
        "rustysound-go-artists" => AppView::ArtistsView {},
        "rustysound-go-playlists" => AppView::PlaylistsView {},
        "rustysound-go-songs" => AppView::SongsView {},
        "rustysound-go-queue" => AppView::QueueView {},
        "rustysound-go-downloads" => AppView::DownloadsView {},
        "rustysound-go-favorites" => AppView::FavoritesView {},
        "rustysound-go-radio" => AppView::RadioView {},
        "rustysound-go-stats" => AppView::StatsView {},
        _ => return None,
    })
}

/// Player button pressed by a Playback menu item.
#[cfg(all(feature = "desktop", target_os = "macos"))]
fn playback_menu_button(id: &str) -> Option<&'static str> {
    Some(match id {
        "rustysound-play-pause" => "play-pause-btn",
        "rustysound-next" => "next-btn",
        "rustysound-previous" => "prev-btn",
        "rustysound-shuffle" => "shuffle-btn",
        "rustysound-repeat" => "repeat-btn",
        _ => return None,
    })
}

/// Clicks a player bar button by id (the ids MediaSession presses), so menu
/// items run exactly the bar's actions. Falls back to the Now Playing copy
/// (`np-<id>`) when the bar's button is not mounted.
#[cfg(all(feature = "desktop", target_os = "macos"))]
fn press_player_button(id: &str) {
    let _ = document::eval(&format!(
        r#"(() => {{
  const button = document.getElementById("{id}") || document.getElementById("np-{id}");
  if (button && !button.disabled) button.click();
  return true;
}})();"#
    ));
}

fn loading_progress_percent(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * 100.0).round() as u32
}


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
                    src: crate::components::ui::env::app_mark_src(),
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

#[component]
pub fn AppShell() -> Element {
    let mut servers = use_signal(Vec::<ServerConfig>::new);
    let current_view = use_route::<AppView>();
    let router_navigator = use_navigator();
    let mut current_view_signal = use_signal(|| current_view.clone());
    let pending_navigation_target = use_signal(|| None::<AppView>);
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
    let mut startup_bootstrap_progress = use_signal(|| 0.0f32);
    let mut startup_bootstrap_status = use_signal(|| "Initializing database".to_string());
    let mut ios_loading_log_lines = use_signal(Vec::<String>::new);
    let mut ios_loading_log_poll_generation = use_signal(|| 0u64);
    let audio_state = use_signal(AudioState::default);
    let preview_playback = use_signal(|| false);
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

    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        let navigation = navigation.clone();
        let add_menu = add_menu.clone();
        let song_details = song_details.clone();
        let track_menu = use_context::<TrackMenuController>();
        use_muda_event_handler(move |event| {
            let id = event.id().as_ref();
            if let Some(button) = playback_menu_button(id) {
                press_player_button(button);
                return;
            }
            if id == "rustysound-now-playing" {
                let mut song_details = song_details.clone();
                if song_details.current().is_open {
                    song_details.close();
                } else if let Some(song) = now_playing.peek().clone() {
                    song_details.open(song);
                }
                return;
            }
            let Some(target) = menu_route(id) else {
                return;
            };
            // Menu navigation happens over whatever is open: close Now
            // Playing and the shell's sheets and menus so the new page is
            // not hidden. Page-local menus unmount with the old page.
            let (mut song_details, mut add_menu, mut track_menu) =
                (song_details.clone(), add_menu.clone(), track_menu);
            song_details.close();
            add_menu.close();
            track_menu.close();
            let focus_search = matches!(target, AppView::SearchView {});
            navigation.navigate_to(target);
            if focus_search {
                focus_global_search_input();
            }
        });
    }

    // Cmd+F / Ctrl+F jumps to Search from anywhere (the macOS menu's Find item
    // does the same through muda when it gets the key first).
    {
        let navigation = navigation.clone();
        use_effect(move || {
            let navigation = navigation.clone();
            spawn(async move {
                let mut shortcut = document::eval(FIND_SHORTCUT_JS);
                while shortcut.recv::<bool>().await.is_ok() {
                    navigation.navigate_to(AppView::SearchView {});
                    focus_global_search_input();
                }
            });
        });
    }

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
    use_context_provider(|| PreviewPlaybackSignal(preview_playback));
    use_context_provider(|| ShuffleEnabledSignal(shuffle_enabled));
    use_context_provider(|| repeat_mode);
    use_context_provider(|| audio_state);

    // Mirror theme, appearance and platform onto <html> so the page background,
    // overscroll area and scrollbars follow the theme too. data-layout lives on
    // .rs-app only (no CSS reads it from <html>). `lang` covers the desktop and
    // iOS webviews, whose index page has none; the web index.html sets it too.
    use_effect(move || {
        let (theme, appearance) = {
            let settings = app_settings.read();
            (stage_theme(&settings.theme), appearance_attr(&settings))
        };
        let js = format!(
            "(function(){{var r=document.documentElement;r.lang='en';r.setAttribute('data-theme','{theme}');r.setAttribute('data-appearance','{appearance}');r.setAttribute('data-platform','{}');}})();",
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

    // While the startup overlay is visible, keep a short iOS log tail fresh.
    use_effect(move || {
        if !cfg!(all(not(target_arch = "wasm32"), target_os = "ios")) {
            ios_loading_log_lines.set(Vec::new());
            return;
        }

        let should_poll = !db_initialized() || !settings_loaded();

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

    let layout = layout_signal();
    let compact = layout.is_compact();
    let is_startup_bootstrapping = !db_initialized() || !settings_loaded();
    let show_ios_loading_logs = cfg!(all(not(target_arch = "wasm32"), target_os = "ios"));
    let boot_logs = if show_ios_loading_logs && is_startup_bootstrapping {
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
        is_transport_loading && !is_startup_bootstrapping;
    let transport_loading_label = if show_transport_loading_overlay {
        transport_label_signal().unwrap_or_else(|| "Loading playback…".to_string())
    } else {
        String::new()
    };
    let has_song = now_playing.read().is_some();
    let player_on = !compact || has_song;

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
                if !compact {
                    WindowDragRegion { class: "rs-drag--content".to_string() }
                }
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
                                class: "rs-btn rs-btn--tinted rs-btn--small",
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
                    div { class: "rs-outlet",
                        Outlet::<AppView> {}
                    }
                }
                PlayerBar {}
            }

            if compact {
                TabBar {}
            }

            // Edge swipe back/forward; the script only acts on compact.
            EdgeSwipe {}

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
            }

            // Audio controller - manages playback separately from UI
            AudioController {}
        }
    }
}
