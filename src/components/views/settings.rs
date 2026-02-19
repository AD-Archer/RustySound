use crate::api::*;
use crate::cache_service::{
    apply_settings as apply_cache_settings, clear_all as clear_cache_storage,
    stats as current_cache_stats,
};
use crate::components::{export_ios_audio_log_txt, AppView, Icon, Navigation, VolumeSignal};
use crate::db::{save_settings, AppSettings, ArtworkDownloadPreference};
use crate::offline_audio::{
    clear_downloads, download_stats, refresh_downloaded_cache, run_auto_download_pass,
};
use dioxus::prelude::*;
use std::collections::HashSet;

fn resolve_server_name(name: &str, url: &str) -> String {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        url.trim().to_string()
    } else {
        trimmed_name.to_string()
    }
}

fn is_local_http_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".local") {
        return true;
    }
    if host.starts_with("10.") || host.starts_with("192.168.") {
        return true;
    }
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some(octet) = rest.split('.').next() {
            if let Ok(value) = octet.parse::<u8>() {
                return (16..=31).contains(&value);
            }
        }
    }
    false
}

fn normalize_server_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if !trimmed.starts_with("http://") {
        return trimmed;
    }

    let host_port = trimmed.trim_start_matches("http://");
    let host = host_port
        .split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default();

    if is_local_http_host(host) {
        trimmed
    } else {
        trimmed.replacen("http://", "https://", 1)
    }
}

fn lyrics_provider_label(provider_key: &str) -> &'static str {
    LyricsProvider::from_key(provider_key)
        .map(|provider| provider.label())
        .unwrap_or("Unknown")
}

fn artwork_pref_key(pref: ArtworkDownloadPreference) -> &'static str {
    match pref {
        ArtworkDownloadPreference::ServerOnly => "server_only",
        ArtworkDownloadPreference::Id3Only => "id3_only",
        ArtworkDownloadPreference::PreferServer => "prefer_server",
        ArtworkDownloadPreference::PreferId3 => "prefer_id3",
    }
}

fn parse_artwork_pref(value: &str) -> ArtworkDownloadPreference {
    match value {
        "server_only" => ArtworkDownloadPreference::ServerOnly,
        "id3_only" => ArtworkDownloadPreference::Id3Only,
        "prefer_id3" => ArtworkDownloadPreference::PreferId3,
        _ => ArtworkDownloadPreference::PreferServer,
    }
}

#[derive(Clone)]
struct ScanResultEntry {
    server_name: String,
    status: ScanStatus,
}

const SMART_CACHE_MIN_ALBUMS_PER_SERVER: u32 = 24;
const SMART_CACHE_MIN_RANDOM_SONGS_PER_SERVER: u32 = 30;
const SMART_CACHE_MIN_PLAYLISTS_PER_SERVER: usize = 4;
const SMART_CACHE_MIN_ALBUM_DETAILS_PER_SERVER: usize = 6;
const SMART_CACHE_MIN_LYRICS_LIMIT: usize = 36;
const SMART_CACHE_MIN_ARTWORK_LIMIT: usize = 160;
const SMART_CACHE_MAX_ALBUMS_PER_SERVER: u32 = 120;
const SMART_CACHE_MAX_RANDOM_SONGS_PER_SERVER: u32 = 220;
const SMART_CACHE_MAX_PLAYLISTS_PER_SERVER: usize = 16;
const SMART_CACHE_MAX_ALBUM_DETAILS_PER_SERVER: usize = 30;
const SMART_CACHE_MAX_LYRICS_LIMIT: usize = 600;
const SMART_CACHE_MAX_ARTWORK_LIMIT: usize = 4800;
const SMART_CACHE_SONG_ART_SIZES: [u32; 3] = [80, 120, 160];
const SMART_CACHE_ALBUM_ART_SIZES: [u32; 4] = [120, 160, 300, 500];
const SMART_CACHE_PLAYLIST_ART_SIZES: [u32; 3] = [120, 160, 300];

fn smart_cache_albums_per_server(cache_size_mb: u32) -> u32 {
    (SMART_CACHE_MIN_ALBUMS_PER_SERVER + cache_size_mb.clamp(25, 2048) / 20).clamp(
        SMART_CACHE_MIN_ALBUMS_PER_SERVER,
        SMART_CACHE_MAX_ALBUMS_PER_SERVER,
    )
}

fn smart_cache_random_songs_per_server(cache_size_mb: u32) -> u32 {
    (SMART_CACHE_MIN_RANDOM_SONGS_PER_SERVER + cache_size_mb.clamp(25, 2048) / 12).clamp(
        SMART_CACHE_MIN_RANDOM_SONGS_PER_SERVER,
        SMART_CACHE_MAX_RANDOM_SONGS_PER_SERVER,
    )
}

fn smart_cache_playlists_per_server(cache_size_mb: u32) -> usize {
    (SMART_CACHE_MIN_PLAYLISTS_PER_SERVER + (cache_size_mb.clamp(25, 2048) / 180) as usize).clamp(
        SMART_CACHE_MIN_PLAYLISTS_PER_SERVER,
        SMART_CACHE_MAX_PLAYLISTS_PER_SERVER,
    )
}

fn smart_cache_album_details_per_server(cache_size_mb: u32) -> usize {
    (SMART_CACHE_MIN_ALBUM_DETAILS_PER_SERVER + (cache_size_mb.clamp(25, 2048) / 60) as usize)
        .clamp(
            SMART_CACHE_MIN_ALBUM_DETAILS_PER_SERVER,
            SMART_CACHE_MAX_ALBUM_DETAILS_PER_SERVER,
        )
}

fn smart_cache_lyrics_limit(cache_size_mb: u32) -> usize {
    (SMART_CACHE_MIN_LYRICS_LIMIT + (cache_size_mb.clamp(25, 2048) / 3) as usize)
        .clamp(SMART_CACHE_MIN_LYRICS_LIMIT, SMART_CACHE_MAX_LYRICS_LIMIT)
}

fn smart_cache_artwork_limit(cache_size_mb: u32) -> usize {
    (SMART_CACHE_MIN_ARTWORK_LIMIT + (cache_size_mb.clamp(25, 2048) * 5) as usize)
        .clamp(SMART_CACHE_MIN_ARTWORK_LIMIT, SMART_CACHE_MAX_ARTWORK_LIMIT)
}

fn push_cover_art_request(
    client: &NavidromeClient,
    server: &ServerConfig,
    cover_art_id: &str,
    sizes: &[u32],
    output: &mut Vec<String>,
    seen_requests: &mut HashSet<String>,
    limit: usize,
) {
    for size in sizes {
        if output.len() >= limit {
            return;
        }
        let request_key = format!("{}:{}:{}", server.id, cover_art_id, size);
        if !seen_requests.insert(request_key) {
            continue;
        }
        output.push(client.get_cover_art_url(cover_art_id, *size));
    }
}

fn song_cache_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

fn is_id3_cover_art_id(cover_art_id: &str) -> bool {
    cover_art_id.trim().to_ascii_lowercase().starts_with("mf-")
}

fn include_cover_for_pref(cover_art_id: &str, pref: ArtworkDownloadPreference) -> bool {
    match pref {
        ArtworkDownloadPreference::ServerOnly => !is_id3_cover_art_id(cover_art_id),
        ArtworkDownloadPreference::Id3Only => is_id3_cover_art_id(cover_art_id),
        ArtworkDownloadPreference::PreferServer | ArtworkDownloadPreference::PreferId3 => true,
    }
}

fn collect_song_cover_urls(
    server: &ServerConfig,
    songs: &[Song],
    output: &mut Vec<String>,
    seen_requests: &mut HashSet<String>,
    limit: usize,
    pref: ArtworkDownloadPreference,
) {
    let client = NavidromeClient::new(server.clone());
    for song in songs {
        if output.len() >= limit {
            break;
        }
        if let Some(cover) = song
            .cover_art
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                song.album_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
            })
        {
            if include_cover_for_pref(cover, pref) {
                push_cover_art_request(
                    &client,
                    server,
                    cover,
                    &SMART_CACHE_SONG_ART_SIZES,
                    output,
                    seen_requests,
                    limit,
                );
            }
        }
    }
}

fn collect_album_cover_urls(
    server: &ServerConfig,
    albums: &[Album],
    output: &mut Vec<String>,
    seen_requests: &mut HashSet<String>,
    limit: usize,
    pref: ArtworkDownloadPreference,
) {
    let client = NavidromeClient::new(server.clone());
    for album in albums {
        if output.len() >= limit {
            break;
        }
        if let Some(cover) = album
            .cover_art
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if !include_cover_for_pref(cover, pref) {
                continue;
            }
            push_cover_art_request(
                &client,
                server,
                cover,
                &SMART_CACHE_ALBUM_ART_SIZES,
                output,
                seen_requests,
                limit,
            );
        }
    }
}

fn collect_playlist_cover_urls(
    server: &ServerConfig,
    playlists: &[Playlist],
    output: &mut Vec<String>,
    seen_requests: &mut HashSet<String>,
    limit: usize,
    pref: ArtworkDownloadPreference,
) {
    let client = NavidromeClient::new(server.clone());
    for playlist in playlists {
        if output.len() >= limit {
            break;
        }
        if let Some(cover) = playlist
            .cover_art
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            if !include_cover_for_pref(cover, pref) {
                continue;
            }
            push_cover_art_request(
                &client,
                server,
                cover,
                &SMART_CACHE_PLAYLIST_ART_SIZES,
                output,
                seen_requests,
                limit,
            );
        }
    }
}

fn push_unique_songs(
    target: &mut Vec<Song>,
    seen: &mut HashSet<String>,
    incoming: impl IntoIterator<Item = Song>,
) {
    for song in incoming {
        let key = song_cache_key(&song);
        if seen.insert(key) {
            target.push(song);
        }
    }
}

fn dedupe_trim_urls(urls: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut output = Vec::<String>::new();
    for url in urls {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            output.push(trimmed.to_string());
        }
        if output.len() >= limit {
            break;
        }
    }
    output
}

#[cfg(target_arch = "wasm32")]
async fn smart_cache_pause(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn smart_cache_pause(_ms: u32) {}

#[cfg(target_arch = "wasm32")]
fn warm_cover_art_urls(urls: &[String]) -> Result<usize, String> {
    let payload = serde_json::to_string(urls).map_err(|error| error.to_string())?;
    let script = format!(
        r#"
(() => {{
  const urls = {payload};
  if (!Array.isArray(urls) || urls.length === 0) return 0;
  const enqueue = window.__rustyCoverArtEnqueue;
  let queued = 0;
  for (const raw of urls) {{
    if (typeof raw !== "string" || !raw.includes("/rest/getCoverArt?")) continue;
    if (typeof enqueue === "function") {{
      if (enqueue(raw)) queued++;
    }} else {{
      const img = new Image();
      img.setAttribute("src", raw);
      queued++;
    }}
  }}
  return queued;
}})()
        "#
    );

    let result = js_sys::eval(&script).map_err(|error| format!("{error:?}"))?;
    Ok(result.as_f64().unwrap_or(0.0).round().max(0.0) as usize)
}

#[cfg(not(target_arch = "wasm32"))]
fn warm_cover_art_urls(_urls: &[String]) -> Result<usize, String> {
    Ok(0)
}

#[cfg(target_arch = "wasm32")]
async fn settings_toast_pause(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn settings_toast_pause(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(ms as u64)).await;
}

fn persist_settings_with_toast(
    settings: AppSettings,
    mut saved_toast: Signal<Option<String>>,
    mut saved_toast_nonce: Signal<u64>,
) {
    saved_toast_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
    let nonce = saved_toast_nonce();
    saved_toast.set(Some("Saved".to_string()));

    spawn(async move {
        let _ = save_settings(settings).await;
    });

    spawn(async move {
        settings_toast_pause(1400).await;
        if saved_toast_nonce() == nonce {
            saved_toast.set(None);
        }
    });
}

#[component]
pub fn SettingsView() -> Element {
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut app_settings = use_context::<Signal<AppSettings>>();
    let navigation = use_context::<Navigation>();
    let mut volume = use_context::<VolumeSignal>().0;
    let scan_results = use_signal(|| Vec::<ScanResultEntry>::new());
    let scan_busy = use_signal(|| false);

    let mut server_name = use_signal(String::new);
    let mut server_url = use_signal(String::new);
    let mut server_user = use_signal(String::new);
    let mut server_pass = use_signal(String::new);
    let mut is_testing = use_signal(|| false);
    let mut test_result = use_signal(|| None::<Result<(), String>>);
    let mut editing_server = use_signal(|| None::<ServerConfig>);
    let mut is_testing_connection = use_signal(|| false);
    let mut connection_test_result = use_signal(|| None::<Result<(), String>>);
    let mut save_status = use_signal(|| None::<String>);
    let saved_toast = use_signal(|| None::<String>);
    let saved_toast_nonce = use_signal(|| 0u64);
    let smart_cache_busy = use_signal(|| false);
    let smart_cache_progress = use_signal(|| 0u8);
    let smart_cache_status = use_signal(|| None::<String>);
    let auto_download_busy = use_signal(|| false);
    let download_cache_refresh_busy = use_signal(|| false);
    let auto_download_status = use_signal(|| None::<String>);
    let download_refresh_nonce = use_signal(|| 0u64);
    let ios_log_export_busy = use_signal(|| false);
    let ios_log_export_status = use_signal(|| None::<String>);

    let can_add = use_memo(move || {
        !server_url().trim().is_empty()
            && !server_user().trim().is_empty()
            && !server_pass().trim().is_empty()
            && test_result().is_some_and(|r: Result<(), String>| r.is_ok())
            && editing_server().is_none()
    });

    let on_test = {
        let url = server_url.clone();
        let user = server_user.clone();
        let pass = server_pass.clone();
        move |_| {
            if is_testing() {
                return;
            }
            let url = normalize_server_url(&url());
            let user = user().trim().to_string();
            let pass = pass().trim().to_string();

            is_testing.set(true);
            test_result.set(None);

            spawn(async move {
                let test_server = ServerConfig::new("Test".to_string(), url, user, pass);
                let client = NavidromeClient::new(test_server);
                let result = client.ping().await;

                test_result.set(Some(result.map(|_| ())));
                is_testing.set(false);
            });
        }
    };

    let mut on_edit_server = {
        let mut server_name = server_name.clone();
        let mut server_url = server_url.clone();
        let mut server_user = server_user.clone();
        let mut server_pass = server_pass.clone();
        move |server: ServerConfig| {
            editing_server.set(Some(server.clone()));
            server_name.set(server.name);
            server_url.set(server.url);
            server_user.set(server.username);
            server_pass.set(server.password);
            test_result.set(None);
        }
    };

    let on_cancel_edit = move |_| {
        editing_server.set(None);
        server_name.set(String::new());
        server_url.set(String::new());
        server_user.set(String::new());
        server_pass.set(String::new());
        test_result.set(None);
    };

    let on_save_edit = move |_| {
        if let Some(editing) = editing_server() {
            let url = normalize_server_url(&server_url());
            let name = resolve_server_name(&server_name(), &url);
            let user = server_user().trim().to_string();
            let pass = server_pass().trim().to_string();

            if url.is_empty() || user.is_empty() || pass.is_empty() {
                return;
            }

            servers.with_mut(|list| {
                if let Some(server) = list.iter_mut().find(|s| s.id == editing.id) {
                    server.name = name;
                    server.url = url;
                    server.username = user;
                    server.password = pass;
                }
            });

            editing_server.set(None);
            server_name.set(String::new());
            server_url.set(String::new());
            server_user.set(String::new());
            server_pass.set(String::new());
            test_result.set(None);

            save_status.set(Some("Server updated!".to_string()));
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                spawn(async move {
                    TimeoutFuture::new(2000).await;
                    save_status.set(None);
                });
            }
        }
    };

    let on_add = move |_| {
        let url = normalize_server_url(&server_url());
        let name = resolve_server_name(&server_name(), &url);
        let user = server_user().trim().to_string();
        let pass = server_pass().trim().to_string();

        if url.is_empty() || user.is_empty() || pass.is_empty() {
            return;
        }

        let new_server = ServerConfig::new(name, url, user, pass);
        servers.with_mut(|list| list.push(new_server));

        server_name.set(String::new());
        server_url.set(String::new());
        server_user.set(String::new());
        server_pass.set(String::new());
        test_result.set(None);

        save_status.set(Some("Server added!".to_string()));
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            spawn(async move {
                TimeoutFuture::new(2000).await;
                save_status.set(None);
            });
        }
    };

    let mut on_test_existing = {
        let servers = servers.clone();
        move |server_id: String| {
            if is_testing_connection() {
                return;
            }
            if let Some(server) = servers().iter().find(|s| s.id == server_id).cloned() {
                is_testing_connection.set(true);
                connection_test_result.set(None);

                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = client.ping().await;

                    connection_test_result.set(Some(result.map(|_| ())));
                    is_testing_connection.set(false);
                });
            }
        }
    };

    let on_crossfade_toggle = move |_| {
        let mut settings = app_settings();
        settings.crossfade_enabled = !settings.crossfade_enabled;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        persist_settings_with_toast(
            settings_clone,
            saved_toast.clone(),
            saved_toast_nonce.clone(),
        );
    };

    let on_replay_gain_toggle = move |_| {
        let mut settings = app_settings();
        settings.replay_gain = !settings.replay_gain;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        persist_settings_with_toast(
            settings_clone,
            saved_toast.clone(),
            saved_toast_nonce.clone(),
        );
    };

    let on_crossfade_duration_change = move |e: Event<FormData>| {
        if let Ok(duration) = e.value().parse::<u32>() {
            let mut settings = app_settings();
            settings.crossfade_duration = duration;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_volume_change = move |e: Event<FormData>| {
        if let Ok(vol) = e.value().parse::<f64>() {
            volume.set((vol / 100.0).clamp(0.0, 1.0));
        }
    };

    let on_bookmark_limit_change = move |e: Event<FormData>| {
        if let Ok(limit) = e.value().parse::<u32>() {
            let mut settings = app_settings();
            settings.bookmark_limit = limit.clamp(1, 5000);
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_bookmark_auto_save_toggle = move |_| {
        let mut settings = app_settings();
        settings.bookmark_auto_save = !settings.bookmark_auto_save;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        persist_settings_with_toast(
            settings_clone,
            saved_toast.clone(),
            saved_toast_nonce.clone(),
        );
    };

    let on_bookmark_autoplay_toggle = move |_| {
        let mut settings = app_settings();
        settings.bookmark_autoplay_on_launch = !settings.bookmark_autoplay_on_launch;
        let settings_clone = settings.clone();
        app_settings.set(settings);
        persist_settings_with_toast(
            settings_clone,
            saved_toast.clone(),
            saved_toast_nonce.clone(),
        );
    };

    let on_cache_enabled_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.cache_enabled = !settings.cache_enabled;
            apply_cache_settings(&settings);
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_cache_images_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.cache_images_enabled = !settings.cache_images_enabled;
            apply_cache_settings(&settings);
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_cache_size_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(size_mb) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.cache_size_mb = size_mb.clamp(25, 2048);
                apply_cache_settings(&settings);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_cache_expiry_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(expiry_days) = e.value().parse::<i32>() {
                let mut settings = app_settings();
                settings.cache_expiry_days = expiry_days.clamp(-1, 3650);
                settings.cache_expiry_in_days = true;
                apply_cache_settings(&settings);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_use_recommended_cache = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.cache_enabled = true;
            settings.cache_images_enabled = true;
            settings.cache_size_mb = 100;
            settings.cache_expiry_days = 30;
            settings.cache_expiry_in_days = true;
            apply_cache_settings(&settings);
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_offline_mode_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.offline_mode = !settings.offline_mode;
            apply_cache_settings(&settings);
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_clear_cache = {
        let mut save_status = save_status.clone();
        move |_| {
            clear_cache_storage();
            save_status.set(Some("Cache cleared.".to_string()));
            #[cfg(target_arch = "wasm32")]
            {
                use gloo_timers::future::TimeoutFuture;
                spawn(async move {
                    TimeoutFuture::new(2000).await;
                    save_status.set(None);
                });
            }
        }
    };

    let on_smart_cache = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut smart_cache_busy = smart_cache_busy.clone();
        let mut smart_cache_progress = smart_cache_progress.clone();
        let mut smart_cache_status = smart_cache_status.clone();
        move |_| {
            if smart_cache_busy() {
                return;
            }

            let active_servers: Vec<ServerConfig> = servers()
                .into_iter()
                .filter(|server| server.active)
                .collect();
            if active_servers.is_empty() {
                smart_cache_progress.set(0);
                smart_cache_status.set(Some("No active servers to warm cache.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            let provider_order =
                normalize_lyrics_provider_order(&settings_snapshot.lyrics_provider_order);
            let timeout_seconds = settings_snapshot.lyrics_request_timeout_secs.clamp(1, 20);
            let cache_images_enabled = settings_snapshot.cache_images_enabled;
            let artwork_pref = settings_snapshot.artwork_download_preference;
            let cache_size_mb = settings_snapshot.cache_size_mb.clamp(25, 2048);
            let albums_per_server = smart_cache_albums_per_server(cache_size_mb);
            let random_songs_per_server = smart_cache_random_songs_per_server(cache_size_mb);
            let playlists_per_server = smart_cache_playlists_per_server(cache_size_mb);
            let album_details_per_server = smart_cache_album_details_per_server(cache_size_mb);
            let lyrics_limit = smart_cache_lyrics_limit(cache_size_mb);
            let artwork_limit = smart_cache_artwork_limit(cache_size_mb);

            smart_cache_busy.set(true);
            smart_cache_progress.set(0);
            smart_cache_status.set(Some(format!(
                "Smart cache: planning {lyrics_limit} lyric lookups and up to {artwork_limit} artwork variants..."
            )));

            spawn(async move {
                let total_server_steps = (active_servers.len() as f64 * 6.0).max(1.0);
                let mut server_steps_done = 0.0f64;
                let mut warmed_album_details = 0usize;
                let mut warmed_playlists = 0usize;
                let mut warmed_lyrics = 0usize;
                let mut queued_artwork = 0usize;

                let mut collected_songs = Vec::<Song>::new();
                let mut seen_song_keys = HashSet::<String>::new();
                let mut cover_urls = Vec::<String>::new();
                let mut seen_cover_requests = HashSet::<String>::new();

                for server in active_servers.iter().cloned() {
                    let client = NavidromeClient::new(server.clone());
                    smart_cache_status.set(Some(format!(
                        "Smart cache: loading metadata for {}...",
                        server.name
                    )));

                    let newest_albums = client
                        .get_albums("newest", albums_per_server, 0)
                        .await
                        .unwrap_or_default();
                    collect_album_cover_urls(
                        &server,
                        &newest_albums,
                        &mut cover_urls,
                        &mut seen_cover_requests,
                        artwork_limit,
                        artwork_pref,
                    );
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );

                    let frequent_albums = client
                        .get_albums("frequent", albums_per_server, 0)
                        .await
                        .unwrap_or_default();
                    collect_album_cover_urls(
                        &server,
                        &frequent_albums,
                        &mut cover_urls,
                        &mut seen_cover_requests,
                        artwork_limit,
                        artwork_pref,
                    );
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );

                    for album in newest_albums
                        .iter()
                        .chain(frequent_albums.iter())
                        .take(album_details_per_server)
                    {
                        if let Ok((_, songs)) = client.get_album(&album.id).await {
                            warmed_album_details += 1;
                            collect_song_cover_urls(
                                &server,
                                &songs,
                                &mut cover_urls,
                                &mut seen_cover_requests,
                                artwork_limit,
                                artwork_pref,
                            );
                            push_unique_songs(&mut collected_songs, &mut seen_song_keys, songs);
                        }
                        smart_cache_pause(20).await;
                    }
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );

                    let playlists = client.get_playlists().await.unwrap_or_default();
                    collect_playlist_cover_urls(
                        &server,
                        &playlists,
                        &mut cover_urls,
                        &mut seen_cover_requests,
                        artwork_limit,
                        artwork_pref,
                    );
                    for playlist in playlists.iter().take(playlists_per_server) {
                        if let Ok((_, songs)) = client.get_playlist(&playlist.id).await {
                            warmed_playlists += 1;
                            collect_song_cover_urls(
                                &server,
                                &songs,
                                &mut cover_urls,
                                &mut seen_cover_requests,
                                artwork_limit,
                                artwork_pref,
                            );
                            push_unique_songs(&mut collected_songs, &mut seen_song_keys, songs);
                        }
                        smart_cache_pause(20).await;
                    }
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );

                    if let Ok((_, starred_albums, starred_songs)) = client.get_starred().await {
                        collect_album_cover_urls(
                            &server,
                            &starred_albums,
                            &mut cover_urls,
                            &mut seen_cover_requests,
                            artwork_limit,
                            artwork_pref,
                        );
                        collect_song_cover_urls(
                            &server,
                            &starred_songs,
                            &mut cover_urls,
                            &mut seen_cover_requests,
                            artwork_limit,
                            artwork_pref,
                        );
                        push_unique_songs(&mut collected_songs, &mut seen_song_keys, starred_songs);
                    }
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );

                    let random_songs = client
                        .get_random_songs(random_songs_per_server)
                        .await
                        .unwrap_or_default();
                    collect_song_cover_urls(
                        &server,
                        &random_songs,
                        &mut cover_urls,
                        &mut seen_cover_requests,
                        artwork_limit,
                        artwork_pref,
                    );
                    push_unique_songs(&mut collected_songs, &mut seen_song_keys, random_songs);
                    server_steps_done += 1.0;
                    smart_cache_progress.set(
                        ((server_steps_done / total_server_steps) * 60.0)
                            .round()
                            .clamp(0.0, 60.0) as u8,
                    );
                }

                let mut lyric_candidates = collected_songs.clone();
                lyric_candidates.truncate(lyrics_limit);
                let lyric_total = lyric_candidates.len();
                if lyric_total > 0 {
                    for (index, song) in lyric_candidates.into_iter().enumerate() {
                        smart_cache_status.set(Some(format!(
                            "Smart cache: warming lyrics ({}/{})...",
                            index + 1,
                            lyric_total
                        )));
                        let query = LyricsQuery::from_song(&song);
                        if !query.title.trim().is_empty()
                            && fetch_lyrics_with_fallback(&query, &provider_order, timeout_seconds)
                                .await
                                .is_ok()
                        {
                            warmed_lyrics += 1;
                        }
                        smart_cache_progress.set(
                            (60.0 + ((index as f64 + 1.0) / lyric_total as f64) * 25.0)
                                .round()
                                .clamp(60.0, 85.0) as u8,
                        );
                        smart_cache_pause(35).await;
                    }
                } else {
                    smart_cache_progress.set(85);
                }

                if cache_images_enabled {
                    let unique_cover_urls = dedupe_trim_urls(cover_urls, artwork_limit);
                    if !unique_cover_urls.is_empty() {
                        smart_cache_status.set(Some(format!(
                            "Smart cache: queueing artwork ({} URLs)...",
                            unique_cover_urls.len()
                        )));
                        match warm_cover_art_urls(&unique_cover_urls) {
                            Ok(queued) => queued_artwork = queued,
                            Err(error) => smart_cache_status.set(Some(format!(
                                "Smart cache: artwork prefetch warning: {}",
                                error
                            ))),
                        }
                    }
                }

                smart_cache_progress.set(100);
                smart_cache_busy.set(false);
                smart_cache_status.set(Some(format!(
                    "Smart cache complete: {} songs, {} album details, {} playlists, {} lyrics, {} artwork requests queued (limit {}).",
                    collected_songs.len(),
                    warmed_album_details,
                    warmed_playlists,
                    warmed_lyrics,
                    queued_artwork,
                    artwork_limit
                )));
            });
        }
    };

    let on_downloads_enabled_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.downloads_enabled = !settings.downloads_enabled;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_auto_downloads_enabled_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.auto_downloads_enabled = !settings.auto_downloads_enabled;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_auto_download_tier_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(tier) = e.value().parse::<u8>() {
                let mut settings = app_settings();
                settings.auto_download_tier = tier.clamp(1, 3);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_auto_download_album_count_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(count) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.auto_download_album_count = count.clamp(0, 25);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_auto_download_playlist_count_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(count) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.auto_download_playlist_count = count.clamp(0, 25);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_download_limit_count_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(limit) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.download_limit_count = limit.clamp(25, 20_000);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_download_limit_mb_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(limit_mb) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.download_limit_mb = limit_mb.clamp(256, 131_072);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_use_recommended_downloads = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.downloads_enabled = true;
            settings.auto_downloads_enabled = true;
            settings.auto_download_album_count = 15;
            settings.auto_download_playlist_count = 15;
            settings.download_limit_count = 5000;
            settings.download_limit_mb = 6000;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_artwork_pref_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            let mut settings = app_settings();
            settings.artwork_download_preference = parse_artwork_pref(&e.value());
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_clear_downloads = {
        let mut auto_download_status = auto_download_status.clone();
        let mut download_refresh_nonce = download_refresh_nonce.clone();
        move |_| {
            let removed = clear_downloads();
            auto_download_status.set(Some(format!("Removed {removed} downloaded songs.")));
            download_refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
        }
    };

    let on_run_auto_download = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut auto_download_busy = auto_download_busy.clone();
        let download_cache_refresh_busy = download_cache_refresh_busy.clone();
        let mut auto_download_status = auto_download_status.clone();
        let mut download_refresh_nonce = download_refresh_nonce.clone();
        move |_| {
            if auto_download_busy() || download_cache_refresh_busy() {
                return;
            }

            let active_servers: Vec<ServerConfig> = servers()
                .into_iter()
                .filter(|server| server.active)
                .collect();
            if active_servers.is_empty() {
                auto_download_status.set(Some("No active servers available.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            auto_download_busy.set(true);
            auto_download_status.set(Some("Running auto-download pass...".to_string()));
            spawn(async move {
                match run_auto_download_pass(&active_servers, &settings_snapshot).await {
                    Ok(report) => {
                        auto_download_status.set(Some(format!(
                            "Auto-download complete: {} new, {} skipped, {} failed, {} purged.",
                            report.downloaded, report.skipped, report.failed, report.purged
                        )));
                    }
                    Err(error) => {
                        auto_download_status.set(Some(format!("Auto-download failed: {error}")));
                    }
                }
                download_refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                auto_download_busy.set(false);
            });
        }
    };

    let on_refresh_download_cache = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let auto_download_busy = auto_download_busy.clone();
        let mut download_cache_refresh_busy = download_cache_refresh_busy.clone();
        let mut auto_download_status = auto_download_status.clone();
        let mut download_refresh_nonce = download_refresh_nonce.clone();
        move |_| {
            if auto_download_busy() || download_cache_refresh_busy() {
                return;
            }

            let servers_snapshot = servers();
            if servers_snapshot.is_empty() {
                auto_download_status.set(Some("No servers configured.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            download_cache_refresh_busy.set(true);
            auto_download_status.set(Some(
                "Refreshing downloaded cache (lyrics + artwork)...".to_string(),
            ));
            spawn(async move {
                match refresh_downloaded_cache(&servers_snapshot, &settings_snapshot).await {
                    Ok(report) => {
                        let missing_suffix = if report.missing_servers > 0 {
                            format!(" ({} missing server mappings)", report.missing_servers)
                        } else {
                            String::new()
                        };
                        auto_download_status.set(Some(format!(
                            "Cache refresh complete: {} scanned, {} lyrics warmed ({} attempted), {} artwork refreshed{}.",
                            report.scanned,
                            report.lyrics_warmed,
                            report.lyrics_attempted,
                            report.artwork_refreshed,
                            missing_suffix
                        )));
                    }
                    Err(error) => {
                        auto_download_status.set(Some(format!("Cache refresh failed: {error}")));
                    }
                }
                download_refresh_nonce.with_mut(|nonce| *nonce = nonce.saturating_add(1));
                download_cache_refresh_busy.set(false);
            });
        }
    };

    let on_lyrics_sync_toggle = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.lyrics_unsynced_mode = !settings.lyrics_unsynced_mode;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_lyrics_timeout_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(timeout) = e.value().parse::<u32>() {
                let mut settings = app_settings();
                settings.lyrics_request_timeout_secs = timeout.clamp(1, 20);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_lyrics_offset_change = {
        let mut app_settings = app_settings.clone();
        move |e: Event<FormData>| {
            if let Ok(offset) = e.value().parse::<i32>() {
                let mut settings = app_settings();
                settings.lyrics_offset_ms = offset.clamp(-5000, 5000);
                let settings_clone = settings.clone();
                app_settings.set(settings);
                persist_settings_with_toast(
                    settings_clone,
                    saved_toast.clone(),
                    saved_toast_nonce.clone(),
                );
            }
        }
    };

    let on_lyrics_reset_offset = {
        let mut app_settings = app_settings.clone();
        move |_| {
            let mut settings = app_settings();
            settings.lyrics_offset_ms = 0;
            let settings_clone = settings.clone();
            app_settings.set(settings);
            persist_settings_with_toast(
                settings_clone,
                saved_toast.clone(),
                saved_toast_nonce.clone(),
            );
        }
    };

    let on_start_scan = {
        let servers = servers.clone();
        let mut scan_results = scan_results.clone();
        let mut scan_busy = scan_busy.clone();
        move |_| {
            if scan_busy() {
                return;
            }
            scan_busy.set(true);
            spawn(async move {
                let mut results = Vec::new();
                for server in servers().iter().filter(|s| s.active).cloned() {
                    let client = NavidromeClient::new(server.clone());
                    if let Ok(status) = client.start_scan().await {
                        results.push(ScanResultEntry {
                            server_name: server.name.clone(),
                            status,
                        });
                    }
                }
                scan_results.set(results);
                scan_busy.set(false);
            });
        }
    };

    let on_refresh_scan = {
        let servers = servers.clone();
        let mut scan_results = scan_results.clone();
        let mut scan_busy = scan_busy.clone();
        move |_| {
            if scan_busy() {
                return;
            }
            scan_busy.set(true);
            spawn(async move {
                let mut results = Vec::new();
                for server in servers().iter().filter(|s| s.active).cloned() {
                    let client = NavidromeClient::new(server.clone());
                    if let Ok(status) = client.get_scan_status().await {
                        results.push(ScanResultEntry {
                            server_name: server.name.clone(),
                            status,
                        });
                    }
                }
                scan_results.set(results);
                scan_busy.set(false);
            });
        }
    };

    let on_export_ios_audio_log = {
        let mut ios_log_export_busy = ios_log_export_busy.clone();
        let mut ios_log_export_status = ios_log_export_status.clone();
        move |_| {
            if ios_log_export_busy() {
                return;
            }
            ios_log_export_busy.set(true);
            match export_ios_audio_log_txt() {
                Ok(path) => {
                    ios_log_export_status.set(Some(format!("Share sheet opened for: {path}")));
                }
                Err(error) => {
                    ios_log_export_status.set(Some(format!("Export failed: {error}")));
                }
            }
            ios_log_export_busy.set(false);
        }
    };

    let server_list = servers();
    let settings = app_settings();
    let current_volume = volume();
    let lyrics_provider_order = normalize_lyrics_provider_order(&settings.lyrics_provider_order);
    let lyrics_sync_enabled = !settings.lyrics_unsynced_mode;
    let cache_stats = current_cache_stats();
    let cache_used_mb = cache_stats.total_size_bytes as f64 / (1024.0 * 1024.0);
    let cache_max_mb = cache_stats.max_size_bytes as f64 / (1024.0 * 1024.0);
    let cache_usage_percent = if cache_stats.max_size_bytes > 0 {
        ((cache_stats.total_size_bytes as f64 / cache_stats.max_size_bytes as f64) * 100.0)
            .clamp(0.0, 100.0)
    } else {
        0.0
    };
    let cache_usage_bar_width = format!("{cache_usage_percent:.1}%");
    let cache_usage_label = format!(
        "Cache usage: {} entries | {:.1}MB / {:.1}MB ({:.0}% full)",
        cache_stats.entry_count, cache_used_mb, cache_max_mb, cache_usage_percent
    );
    let smart_cache_percent = smart_cache_progress();
    let smart_cache_progress_style = format!("width: {}%", smart_cache_percent);
    let _download_refresh = download_refresh_nonce();
    let download_snapshot = download_stats();
    let downloaded_size_mb = download_snapshot.total_size_bytes as f64 / (1024.0 * 1024.0);
    let download_limit_mb = settings.download_limit_mb.max(1) as f64;
    let download_size_usage_percent =
        ((downloaded_size_mb / download_limit_mb) * 100.0).clamp(0.0, 100.0);
    let download_limit_count = settings.download_limit_count.max(1) as usize;
    let download_count_usage_percent =
        ((download_snapshot.song_count as f64 / download_limit_count as f64) * 100.0)
            .clamp(0.0, 100.0);
    let download_usage_label = format!(
        "Downloads: {} songs | {:.1}MB / {:.0}MB ({:.0}% size, {:.0}% count)",
        download_snapshot.song_count,
        downloaded_size_mb,
        download_limit_mb,
        download_size_usage_percent,
        download_count_usage_percent
    );
    let download_size_usage_bar_width = format!("{download_size_usage_percent:.1}%");
    let favorite_tier_limit = match settings.auto_download_tier {
        3 => 150,
        2 => 100,
        _ => 50,
    };
    let download_actions_busy = auto_download_busy() || download_cache_refresh_busy();

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header",
                h1 { class: "page-title", "Settings" }
                p { class: "page-subtitle", "Manage your servers and playback preferences" }
            }

            // Save status notification
            if let Some(status) = save_status() {
                div { class: "fixed top-4 right-4 px-4 py-2 bg-emerald-500/20 border border-emerald-500/50 rounded-lg text-emerald-400 text-sm",
                    "{status}"
                }
            }
            if let Some(message) = saved_toast() {
                div { class: "fixed top-16 right-4 px-4 py-2 bg-cyan-500/20 border border-cyan-500/50 rounded-lg text-cyan-200 text-sm shadow-lg",
                    "{message}"
                }
            }

            // Playback Settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-6", "Playback Settings" }

                div { class: "space-y-6",
                    // Volume control
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-3",
                            "Default Volume"
                        }
                        div { class: "flex items-center gap-4",
                            Icon {
                                name: if current_volume > 0.5 { "volume-2".to_string() } else if current_volume > 0.0 { "volume-1".to_string() } else { "volume-x".to_string() },
                                class: "w-5 h-5 text-zinc-400".to_string(),
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: (current_volume * 100.0).round() as i32,
                                class: "flex-1 h-2 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-emerald-500",
                                oninput: on_volume_change,
                                onchange: on_volume_change,
                            }
                            span { class: "text-sm text-zinc-400 w-12 text-right",
                                "{(current_volume * 100.0).round() as i32}%"
                            }
                        }
                    }

                    // Crossfade toggle
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Crossfade" }
                            p { class: "text-sm text-zinc-400", "Smoothly transition between songs" }
                        }
                        button {
                            class: if settings.crossfade_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_crossfade_toggle,
                            div { class: if settings.crossfade_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    // Crossfade duration (show only if crossfade is enabled)
                    if settings.crossfade_enabled {
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Crossfade Duration"
                            }
                            div { class: "flex items-center gap-4",
                                input {
                                    r#type: "range",
                                    min: "1",
                                    max: "12",
                                    value: settings.crossfade_duration,
                                    class: "flex-1 h-2 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-emerald-500",
                                    oninput: on_crossfade_duration_change,
                                }
                                span { class: "text-sm text-zinc-400 w-16 text-right",
                                    "{settings.crossfade_duration} seconds"
                                }
                            }
                        }
                    }

                    // Replay Gain toggle
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Replay Gain" }
                            p { class: "text-sm text-zinc-400", "Normalize volume across tracks" }
                        }
                        button {
                            class: if settings.replay_gain { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_replay_gain_toggle,
                            div { class: if settings.replay_gain { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }
                }
            }

            // Bookmark settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Bookmark Settings" }
                p { class: "text-sm text-zinc-400 mb-5",
                    "Bookmarks remember your listening position so you can quickly continue where you left off."
                }

                div { class: "space-y-5",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Auto-save bookmarks" }
                            p { class: "text-sm text-zinc-400", "Automatically save playback position while listening and when switching songs." }
                        }
                        button {
                            class: if settings.bookmark_auto_save { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_bookmark_auto_save_toggle,
                            div { class: if settings.bookmark_auto_save { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Resume bookmark on launch" }
                            p { class: "text-sm text-zinc-400", "Automatically queue and play your latest bookmark when the app starts." }
                        }
                        button {
                            class: if settings.bookmark_autoplay_on_launch { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_bookmark_autoplay_toggle,
                            div { class: if settings.bookmark_autoplay_on_launch { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-2",
                            "Bookmark Limit"
                        }
                        p { class: "text-xs text-zinc-500 mb-3",
                            "Keep only the newest bookmarks per server user. Oldest bookmarks are deleted when this limit is exceeded."
                        }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "5000",
                            value: settings.bookmark_limit,
                            class: "w-full max-w-xs px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                            oninput: on_bookmark_limit_change,
                        }
                    }
                }
            }

            // Cache settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                div { class: "flex items-center justify-between gap-3 mb-3",
                    h2 { class: "text-lg font-semibold text-white", "Cache" }
                    button {
                        class: "px-3 py-2 rounded-lg border border-emerald-500/40 text-emerald-300 hover:text-white hover:border-emerald-400/70 transition-colors text-sm",
                        onclick: on_use_recommended_cache,
                        "Use Recommended"
                    }
                }
                p { class: "text-sm text-zinc-400 mb-5",
                    "Control metadata, artwork, and lyrics caching. Native apps also prefetch now playing + next songs for offline continuity."
                }

                div { class: "space-y-5",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Enable cache" }
                            p { class: "text-sm text-zinc-400", "Store song/artist/playlist/favorites metadata and lyrics locally." }
                        }
                        button {
                            class: if settings.cache_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_cache_enabled_toggle,
                            div { class: if settings.cache_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Cache album artwork" }
                            p { class: "text-sm text-zinc-400", "Cache image responses for faster repeat views and fewer artwork requests." }
                        }
                        button {
                            class: if settings.cache_images_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_cache_images_toggle,
                            div { class: if settings.cache_images_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Max cache size (MB)"
                            }
                            input {
                                r#type: "number",
                                min: "25",
                                max: "2048",
                                value: settings.cache_size_mb,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_cache_size_change,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Cache expiry (days)"
                            }
                            input {
                                r#type: "number",
                                min: "-1",
                                max: "3650",
                                value: settings.cache_expiry_days,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_cache_expiry_change,
                            }
                            p { class: "text-xs text-zinc-500 mt-2",
                                "Use -1 to disable automatic expiry."
                            }
                        }
                    }

                    div { class: "space-y-2 pt-1",
                        div { class: "flex items-center justify-between gap-3",
                            p { class: "text-xs text-zinc-500", "{cache_usage_label}" }
                            button {
                                class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-rose-500/60 transition-colors text-sm",
                                onclick: on_clear_cache,
                                "Clear cache"
                            }
                        }
                        div { class: "w-full h-2 rounded-full bg-zinc-700/70 overflow-hidden",
                            div {
                                class: "h-full bg-emerald-500/80 transition-all",
                                style: "width: {cache_usage_bar_width}",
                            }
                        }
                    }

                    div { class: "space-y-2 pt-2 border-t border-zinc-800/80",
                        div { class: "flex items-center justify-between gap-3",
                            div {
                                p { class: "font-medium text-white", "Smart Cache Warm-up" }
                                p { class: "text-sm text-zinc-400", "Prefetch albums, songs, playlists, lyrics, and queue artwork with request throttling." }
                            }
                            button {
                                class: if smart_cache_busy() {
                                    "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-400 cursor-not-allowed text-sm"
                                } else {
                                    "px-3 py-2 rounded-lg border border-emerald-500/40 text-emerald-300 hover:text-white hover:border-emerald-400/70 transition-colors text-sm"
                                },
                                disabled: smart_cache_busy(),
                                onclick: on_smart_cache,
                                if smart_cache_busy() {
                                    "Warming..."
                                } else {
                                    "Run Smart Cache"
                                }
                            }
                        }
                        if let Some(status) = smart_cache_status() {
                            p { class: "text-xs text-zinc-500", "{status}" }
                        }
                        if smart_cache_busy() {
                            div { class: "w-full h-2 rounded-full bg-zinc-700/70 overflow-hidden",
                                div {
                                    class: "h-full bg-emerald-500/80 transition-all",
                                    style: "{smart_cache_progress_style}",
                                }
                            }
                            p { class: "text-xs text-zinc-500", "{smart_cache_percent}% complete" }
                        }
                    }

                }
            }

            // Downloads settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                div { class: "flex items-center justify-between gap-3 mb-4",
                    div {
                        h2 { class: "text-lg font-semibold text-white", "Downloads" }
                        p { class: "text-sm text-zinc-400",
                            "Store songs and lyrics offline, manage auto-download limits, and control artwork source preference."
                        }
                    }
                    div { class: "flex flex-wrap items-center gap-2",
                        button {
                            class: "px-3 py-2 rounded-lg border border-emerald-500/40 text-emerald-300 hover:text-white hover:border-emerald-400/70 transition-colors text-sm",
                            onclick: on_use_recommended_downloads,
                            "Use Recommended"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-sm",
                            onclick: move |_| navigation.navigate_to(AppView::DownloadsView {}),
                            "Open Downloads Page"
                        }
                    }
                }

                div { class: "space-y-5",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Enable downloads" }
                            p { class: "text-sm text-zinc-400", "Allow manual and automatic audio downloads." }
                        }
                        button {
                            class: if settings.downloads_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_downloads_enabled_toggle,
                            div { class: if settings.downloads_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Auto downloads" }
                            p { class: "text-sm text-zinc-400", "Fetch favorite songs and recent albums/playlists automatically." }
                        }
                        button {
                            class: if settings.auto_downloads_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_auto_downloads_enabled_toggle,
                            div { class: if settings.auto_downloads_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Favorite tier" }
                            select {
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                value: settings.auto_download_tier.to_string(),
                                oninput: on_auto_download_tier_change,
                                option { value: "1", "Tier 1 (50 favorites)" }
                                option { value: "2", "Tier 2 (100 favorites)" }
                                option { value: "3", "Tier 3 (150 favorites)" }
                            }
                            p { class: "text-xs text-zinc-500 mt-2",
                                "Current tier keeps up to {favorite_tier_limit} favorite songs."
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Artwork source preference" }
                            select {
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                value: artwork_pref_key(settings.artwork_download_preference),
                                oninput: on_artwork_pref_change,
                                option { value: "server_only", "Server artwork only" }
                                option { value: "id3_only", "ID3 artwork only" }
                                option { value: "prefer_server", "Prefer server artwork" }
                                option { value: "prefer_id3", "Prefer ID3 artwork" }
                            }
                            p { class: "text-xs text-zinc-500 mt-2",
                                "Large artwork warm-ups can trigger server rate limits if repeated frequently."
                            }
                        }
                    }

                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Auto albums" }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "25",
                                value: settings.auto_download_album_count,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_auto_download_album_count_change,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Auto playlists" }
                            input {
                                r#type: "number",
                                min: "0",
                                max: "25",
                                value: settings.auto_download_playlist_count,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_auto_download_playlist_count_change,
                            }
                        }
                    }

                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Download limit (songs)" }
                            input {
                                r#type: "number",
                                min: "25",
                                max: "20000",
                                value: settings.download_limit_count,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_download_limit_count_change,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2", "Download limit (MB)" }
                            input {
                                r#type: "number",
                                min: "256",
                                max: "131072",
                                value: settings.download_limit_mb,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_download_limit_mb_change,
                            }
                        }
                    }

                    div { class: "space-y-2",
                        p { class: "text-xs text-zinc-500", "{download_usage_label}" }
                        div { class: "h-2 w-full rounded-full bg-zinc-700/70 overflow-hidden",
                            div {
                                class: "h-full bg-cyan-500/80 transition-all",
                                style: "width: {download_size_usage_bar_width}",
                            }
                        }
                    }

                    div { class: "flex flex-wrap items-center gap-3",
                        button {
                            class: if download_actions_busy {
                                "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-sm"
                            } else {
                                "px-3 py-2 rounded-lg border border-emerald-500/40 text-emerald-300 hover:text-white hover:border-emerald-400/70 transition-colors text-sm"
                            },
                            disabled: download_actions_busy,
                            onclick: on_run_auto_download,
                            if auto_download_busy() {
                                "Running auto-download..."
                            } else {
                                "Run Auto-Download Now"
                            }
                        }
                        button {
                            class: if download_actions_busy {
                                "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-sm"
                            } else {
                                "px-3 py-2 rounded-lg border border-cyan-500/40 text-cyan-300 hover:text-white hover:border-cyan-400/70 transition-colors text-sm"
                            },
                            disabled: download_actions_busy,
                            onclick: on_refresh_download_cache,
                            if download_cache_refresh_busy() {
                                "Refreshing cache..."
                            } else {
                                "Refresh Downloaded Cache"
                            }
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-rose-500/50 text-rose-300 hover:text-white hover:border-rose-400 transition-colors text-sm",
                            onclick: on_clear_downloads,
                            "Clear Downloads"
                        }
                    }

                    if let Some(status) = auto_download_status() {
                        p { class: "text-xs text-zinc-500", "{status}" }
                    }
                }
            }

            // Diagnostics
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Diagnostics" }
                p { class: "text-sm text-zinc-400 mb-4",
                    "Export iOS audio diagnostics as a .txt file and save it to Files or share it directly."
                }

                div { class: "flex flex-wrap items-center gap-3",
                    button {
                        class: if ios_log_export_busy() {
                            "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-500 cursor-not-allowed text-sm"
                        } else {
                            "px-3 py-2 rounded-lg border border-cyan-500/40 text-cyan-300 hover:text-white hover:border-cyan-400/70 transition-colors text-sm"
                        },
                        disabled: ios_log_export_busy(),
                        onclick: on_export_ios_audio_log,
                        if ios_log_export_busy() {
                            "Preparing export..."
                        } else {
                            "Export iOS Audio Log (.txt)"
                        }
                    }
                }

                if let Some(status) = ios_log_export_status() {
                    p { class: "text-xs text-zinc-500 mt-3", "{status}" }
                }
                p { class: "text-xs text-zinc-600 mt-2",
                    "This action is available in native iOS builds only."
                }
            }

            // Lyrics settings
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Lyrics Sync" }
                p { class: "text-sm text-zinc-400 mb-5",
                    "Configure provider priority, lookup timeout, and sync behavior for the song menu lyrics panel. Changes are auto-saved."
                }
                p { class: "text-xs text-zinc-500 mb-5",
                    "Web note: browser CORS blocks direct Netease and Genius requests in web builds. Keep LRCLIB first on web. Desktop supports all providers."
                }

                div { class: "space-y-5",
                    div { class: "flex items-center justify-between",
                        div {
                            p { class: "font-medium text-white", "Sync lyrics" }
                            p { class: "text-sm text-zinc-400", "Enable timeline-synced lyrics and tap-to-seek from the lyrics tab (default: ON)" }
                        }
                        button {
                            class: if lyrics_sync_enabled { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                            onclick: on_lyrics_sync_toggle,
                            div { class: if lyrics_sync_enabled { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                        }
                    }

                    div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Provider timeout (seconds)"
                            }
                            input {
                                r#type: "number",
                                min: "1",
                                max: "20",
                                value: settings.lyrics_request_timeout_secs,
                                class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                oninput: on_lyrics_timeout_change,
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Sync offset (ms)"
                            }
                            div { class: "flex items-center gap-2",
                                input {
                                    r#type: "number",
                                    min: "-5000",
                                    max: "5000",
                                    step: "50",
                                    value: settings.lyrics_offset_ms,
                                    class: "w-full px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-900 text-white focus:outline-none focus:border-emerald-500/50",
                                    oninput: on_lyrics_offset_change,
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors text-sm",
                                    onclick: on_lyrics_reset_offset,
                                    "Reset"
                                }
                            }
                        }
                    }

                    div { class: "space-y-2",
                        p { class: "text-sm font-medium text-zinc-300", "Provider priority" }
                        for (index, provider) in lyrics_provider_order.iter().enumerate() {
                            div { class: "flex items-center justify-between gap-3 px-3 py-2 rounded-xl border border-zinc-700/60 bg-zinc-900/40",
                                div { class: "flex items-center gap-2 min-w-0",
                                    span { class: "text-xs text-zinc-500 w-6", "{index + 1}." }
                                    span { class: "text-sm text-white truncate", "{lyrics_provider_label(provider)}" }
                                }
                                div { class: "flex items-center gap-2",
                                    button {
                                        class: "px-2 py-1 rounded border border-zinc-700 text-zinc-400 hover:text-white text-xs disabled:opacity-40",
                                        disabled: index == 0,
                                        onclick: {
                                            let provider = provider.clone();
                                            let mut app_settings = app_settings.clone();
                                            move |_| {
                                                let mut settings = app_settings();
                                                let mut order = normalize_lyrics_provider_order(
                                                    &settings.lyrics_provider_order,
                                                );
                                                if let Some(position) =
                                                    order.iter().position(|entry| entry == &provider)
                                                {
                                                    if position > 0 {
                                                        order.swap(position, position - 1);
                                                        settings.lyrics_provider_order = order;
                                                        let settings_clone = settings.clone();
                                                        app_settings.set(settings);
                                                        persist_settings_with_toast(settings_clone, saved_toast.clone(), saved_toast_nonce.clone());
                                                    }
                                                }
                                            }
                                        },
                                        "Up"
                                    }
                                    button {
                                        class: "px-2 py-1 rounded border border-zinc-700 text-zinc-400 hover:text-white text-xs disabled:opacity-40",
                                        disabled: index + 1 >= lyrics_provider_order.len(),
                                        onclick: {
                                            let provider = provider.clone();
                                            let mut app_settings = app_settings.clone();
                                            move |_| {
                                                let mut settings = app_settings();
                                                let mut order = normalize_lyrics_provider_order(
                                                    &settings.lyrics_provider_order,
                                                );
                                                if let Some(position) =
                                                    order.iter().position(|entry| entry == &provider)
                                                {
                                                    if position + 1 < order.len() {
                                                        order.swap(position, position + 1);
                                                        settings.lyrics_provider_order = order;
                                                        let settings_clone = settings.clone();
                                                        app_settings.set(settings);
                                                        persist_settings_with_toast(settings_clone, saved_toast.clone(), saved_toast_nonce.clone());
                                                    }
                                                }
                                            }
                                        },
                                        "Down"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Quick Scan Section
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Quick Scan" }

                div { class: "space-y-4",
                    p { class: "text-sm text-zinc-400",
                        "Trigger a quick scan on your connected servers and keep an eye on the status."
                    }
                    div { class: "flex flex-wrap gap-3",
                        button {
                            class: if scan_busy() { "px-4 py-2 rounded-xl bg-emerald-500/60 text-white cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-xl bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center gap-2" },
                            disabled: scan_busy(),
                            onclick: on_start_scan,
                            if scan_busy() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 text-white animate-spin".to_string(),
                                }
                                "Scanning..."
                            } else {
                                Icon {
                                    name: "search".to_string(),
                                    class: "w-4 h-4 text-white".to_string(),
                                }
                                "Start Quick Scan"
                            }
                        }
                        button {
                            class: if scan_busy() { "px-4 py-2 rounded-xl bg-zinc-700/40 text-zinc-300 cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-xl bg-zinc-700/60 text-white hover:bg-zinc-700 transition-colors flex items-center gap-2" },
                            disabled: scan_busy(),
                            onclick: on_refresh_scan,
                            if scan_busy() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 text-white animate-spin".to_string(),
                                }
                                "Refreshing..."
                            } else {
                                Icon {
                                    name: "repeat".to_string(),
                                    class: "w-4 h-4 text-white".to_string(),
                                }
                                "Refresh Status"
                            }
                        }
                    }

                    {
                        if scan_results().is_empty() {
                            rsx! {
                                p { class: "text-sm text-zinc-500", "No scan status available yet." }
                            }
                        } else {
                            rsx! {
                                div { class: "space-y-3",
                                    for entry in scan_results() {
                                        div { class: "p-4 bg-zinc-900/50 border border-zinc-800/70 rounded-2xl space-y-1",
                                            span { class: "text-sm text-zinc-500", "{entry.server_name}" }
                                            p { class: "text-sm text-white", "Status: {entry.status.status}" }
                                            if let Some(task) = entry.status.current_task.as_ref() {
                                                span { class: "text-xs text-zinc-500", "Task: {task}" }
                                            }
                                            if let Some(seconds) = entry.status.seconds_remaining {
                                                span { class: "text-xs text-zinc-500", "{seconds}s remaining" }
                                            }
                                            if let Some(elapsed) = entry.status.seconds_elapsed {
                                                span { class: "text-xs text-zinc-500", "Elapsed: {elapsed}s" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add/Edit server form
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-4",
                    if editing_server().is_some() {
                        "Edit Server"
                    } else {
                        "Add Server"
                    }
                }

                div { class: "grid gap-4",
                    // Server name
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-2",
                            "Server Name"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "My Navidrome Server",
                            value: server_name,
                            oninput: move |e| {
                                server_name.set(e.value());
                                test_result.set(None);
                            },
                        }
                    }

                    // URL
                    div {
                        label { class: "block text-sm font-medium text-zinc-400 mb-2",
                            "Server URL"
                        }
                        input {
                            class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "https://navidrome.example.com",
                            value: server_url,
                            oninput: move |e| {
                                server_url.set(e.value());
                                test_result.set(None);
                            },
                        }
                        p { class: "text-xs text-zinc-500 mt-2",
                            "Remote HTTP URLs are automatically upgraded to HTTPS. Local network hosts keep HTTP."
                        }
                    }

                    // Username & Password
                    div { class: "grid grid-cols-1 sm:grid-cols-2 gap-4",
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Username"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                placeholder: "admin",
                                value: server_user,
                                oninput: move |e| {
                                    server_user.set(e.value());
                                    test_result.set(None);
                                },
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium text-zinc-400 mb-2",
                                "Password"
                            }
                            input {
                                class: "w-full px-4 py-3 bg-zinc-900/50 border border-zinc-700/50 rounded-xl text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                r#type: "password",
                                placeholder: "••••••••",
                                value: server_pass,
                                oninput: move |e| {
                                    server_pass.set(e.value());
                                    test_result.set(None);
                                },
                            }
                        }
                    }

                    // Test result
                    {
                        match test_result() {
                            Some(Ok(())) => rsx! {
                                div { class: "flex items-center gap-2 text-emerald-400 text-sm",
                                    Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection successful!"
                                }
                            },
                            Some(Err(e)) => rsx! {
                                div { class: "flex items-center gap-2 text-red-400 text-sm",
                                    Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                                    "Failed: {e}"
                                }
                            },
                            None => rsx! {},
                        }
                    }

                    // Buttons
                    div { class: "flex flex-col sm:flex-row gap-3 pt-2",
                        button {
                            class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-zinc-700/50 text-zinc-300 hover:text-white hover:bg-zinc-700 transition-colors flex items-center gap-2",
                            disabled: is_testing(),
                            onclick: on_test,
                            if is_testing() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                            } else {
                                Icon {
                                    name: "server".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                            }
                            "Test Connection"
                        }
                        if editing_server().is_some() {
                            button {
                                class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                onclick: on_save_edit,
                                Icon {
                                    name: "check".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Save Changes"
                            }
                            button {
                                class: "w-full sm:w-auto px-4 py-2 rounded-xl bg-zinc-700/50 text-zinc-300 hover:text-white hover:bg-zinc-700 transition-colors flex items-center gap-2",
                                onclick: on_cancel_edit,
                                Icon {
                                    name: "x".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Cancel"
                            }
                        } else {
                            button {
                                class: if can_add() { "w-full sm:w-auto px-6 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2" } else { "w-full sm:w-auto px-6 py-2 rounded-xl bg-zinc-700/50 text-zinc-500 cursor-not-allowed flex items-center gap-2" },
                                disabled: !can_add(),
                                onclick: on_add,
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Add Server"
                            }
                        }
                    }
                }
            }

            // Connected servers
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-4", "Connected Servers" }
                p { class: "text-sm text-amber-200/80 bg-amber-500/10 border border-amber-500/40 rounded-xl px-3 py-2 mb-4",
                    "Playlists can only be managed when a single server is active. Enabling one server will automatically disable the others."
                }

                if server_list.is_empty() {
                    div { class: "flex flex-col items-center justify-center py-12 text-center",
                        Icon {
                            name: "server".to_string(),
                            class: "w-12 h-12 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "No servers connected yet" }
                        p { class: "text-zinc-500 text-sm",
                            "Add a Navidrome server above to get started"
                        }
                    }
                } else {
                    div { class: "space-y-3",
                        for server in server_list {
                            ServerCard {
                                server: server.clone(),
                                on_toggle: {
                                    let server_id = server.id.clone();
                                    move |_| {
                                        servers
                                            .with_mut(|list| {
                                                let new_state = list
                                                    .iter()
                                                    .find(|s| s.id == server_id)
                                                    .map(|s| !s.active)
                                                    .unwrap_or(false);

                                                if new_state {
                                                    for srv in list.iter_mut() {
                                                        srv.active = false;
                                                    }
                                                }

                                                if let Some(s) =
                                                    list.iter_mut().find(|s| s.id == server_id)
                                                {
                                                    s.active = new_state;
                                                }
                                            });
                                    }
                                },
                                on_edit: {
                                    let server = server.clone();
                                    move |_| on_edit_server(server.clone())
                                },
                                on_test: {
                                    let server_id = server.id.clone();
                                    move |_| on_test_existing(server_id.clone())
                                },
                                on_remove: {
                                    let server_id = server.id.clone();
                                    move |_| {
                                        servers
                                            .with_mut(|list| {
                                                list.retain(|s| s.id != server_id);
                                            });
                                    }
                                },
                                is_testing: is_testing_connection(),
                            }
                        }
                    }

                    // Connection test result for existing servers
                    {
                        match connection_test_result() {
                            Some(Ok(())) => rsx! {
                                div { class: "mt-4 flex items-center gap-2 text-emerald-400 text-sm",
                                    Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection test successful!"
                                }
                            },
                            Some(Err(e)) => rsx! {
                                div { class: "mt-4 flex items-center gap-2 text-red-400 text-sm",
                                    Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
                                    "Connection test failed: {e}"
                                }
                            },
                            None => rsx! {},
                        }
                    }
                }
            }

            // Offline mode card
            section { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 p-6",
                h2 { class: "text-lg font-semibold text-white mb-3", "Offline Mode" }
                p { class: "text-sm text-zinc-400 mb-5",
                    "When enabled, RustySound uses downloaded and cached content only. New network requests are blocked until you turn it off."
                }
                div { class: "flex items-center justify-between",
                    div {
                        p { class: "font-medium text-white", "Use offline content only" }
                        p { class: "text-sm text-zinc-400", "Disable this to return to live server access." }
                    }
                    button {
                        class: if settings.offline_mode { "w-12 h-6 bg-emerald-500 rounded-full relative transition-colors" } else { "w-12 h-6 bg-zinc-700 rounded-full relative transition-colors" },
                        onclick: on_offline_mode_toggle,
                        div { class: if settings.offline_mode { "w-5 h-5 bg-white rounded-full absolute top-0.5 right-0.5 transition-all" } else { "w-5 h-5 bg-zinc-400 rounded-full absolute top-0.5 left-0.5 transition-all" } }
                    }
                }
            }
        }
    }
}

#[component]
fn ServerCard(
    server: ServerConfig,
    on_toggle: EventHandler<MouseEvent>,
    on_remove: EventHandler<MouseEvent>,
    on_edit: EventHandler<MouseEvent>,
    on_test: EventHandler<MouseEvent>,
    is_testing: bool,
) -> Element {
    let initials: String = server
        .name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();

    rsx! {
        div { class: "p-4 rounded-xl bg-zinc-900/50 border border-zinc-700/30",
            // Server info row
            div { class: "flex items-center gap-4 mb-3",
                // Icon
                div { class: "w-12 h-12 rounded-xl bg-gradient-to-br from-emerald-600 to-teal-700 flex items-center justify-center text-white font-bold shadow-lg flex-shrink-0",
                    "{initials}"
                }
                // Info
                div { class: "min-w-0 flex-1",
                    p { class: "font-medium text-white truncate", "{server.name}" }
                    p { class: "text-sm text-zinc-400 truncate", "{server.url}" }
                    p { class: "text-xs text-zinc-500", "User: {server.username}" }
                }
            }
            // Action buttons row
            div { class: "flex items-center justify-between gap-2",
                // Status and toggle
                div { class: "flex items-center gap-2",
                    div { class: if server.active { "text-xs text-emerald-400" } else { "text-xs text-zinc-500" },
                        if server.active {
                            "Active"
                        } else {
                            "Inactive"
                        }
                    }
                    button {
                        class: if server.active { "px-3 py-1.5 rounded-lg bg-emerald-500/20 text-emerald-400 text-sm hover:bg-emerald-500/30 transition-colors" } else { "px-3 py-1.5 rounded-lg bg-zinc-700/50 text-zinc-400 text-sm hover:bg-zinc-700 transition-colors" },
                        onclick: move |e| on_toggle.call(e),
                        if server.active {
                            "Disable"
                        } else {
                            "Enable"
                        }
                    }
                }
                // Action buttons
                div { class: "flex items-center gap-1",
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-blue-400 hover:bg-blue-500/10 transition-colors",
                        disabled: is_testing,
                        onclick: move |e| on_test.call(e),
                        title: "Test Connection",
                        Icon {
                            name: "server".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-amber-400 hover:bg-amber-500/10 transition-colors",
                        onclick: move |e| on_edit.call(e),
                        title: "Edit Server",
                        Icon {
                            name: "settings".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-500 hover:text-red-400 hover:bg-red-500/10 transition-colors",
                        onclick: move |e| on_remove.call(e),
                        title: "Remove Server",
                        Icon {
                            name: "trash".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                    }
                }
            }
        }
    }
}
