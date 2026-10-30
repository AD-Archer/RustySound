//! Smart Cache Warm-up: prefetches album/playlist metadata, lyrics and artwork
//! for the active servers, throttled so a server isn't flooded. Moved from the
//! old monolithic settings view unchanged, except that it reports through
//! signals instead of closing over the view's state.

use crate::api::*;
use crate::db::{AppSettings, ArtworkDownloadPreference};
use dioxus::prelude::*;
use std::collections::HashSet;

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

fn set_server_progress(mut progress: Signal<u8>, done: f64, total: f64) {
    progress.set(((done / total) * 60.0).round().clamp(0.0, 60.0) as u8);
}

/// Runs the warm-up. `progress` goes 0..=100; `status` narrates each step.
/// Returns the summary line.
pub(super) async fn run_smart_cache(
    active_servers: Vec<ServerConfig>,
    settings: AppSettings,
    mut progress: Signal<u8>,
    mut status: Signal<Option<String>>,
) -> String {
    let provider_order = normalize_lyrics_provider_order(&settings.lyrics_provider_order);
    let timeout_seconds = settings.lyrics_request_timeout_secs.clamp(1, 20);
    let cache_images_enabled = settings.cache_images_enabled;
    let artwork_pref = settings.artwork_download_preference;
    let cache_size_mb = settings.cache_size_mb.clamp(25, 2048);
    let albums_per_server = smart_cache_albums_per_server(cache_size_mb);
    let random_songs_per_server = smart_cache_random_songs_per_server(cache_size_mb);
    let playlists_per_server = smart_cache_playlists_per_server(cache_size_mb);
    let album_details_per_server = smart_cache_album_details_per_server(cache_size_mb);
    let lyrics_limit = smart_cache_lyrics_limit(cache_size_mb);
    let artwork_limit = smart_cache_artwork_limit(cache_size_mb);

    progress.set(0);
    status.set(Some(format!(
        "Planning {lyrics_limit} lyric lookups and up to {artwork_limit} artwork variants…"
    )));

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
        status.set(Some(format!("Loading metadata for {}…", server.name)));

        let newest_albums = client
            .get_albums("newest", albums_per_server, 0)
            .await
            .unwrap_or_default();
        collect_album_cover_urls(&server, &newest_albums, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);

        let frequent_albums = client
            .get_albums("frequent", albums_per_server, 0)
            .await
            .unwrap_or_default();
        collect_album_cover_urls(&server, &frequent_albums, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);

        for album in newest_albums
            .iter()
            .chain(frequent_albums.iter())
            .take(album_details_per_server)
        {
            if let Ok((_, songs)) = client.get_album(&album.id).await {
                warmed_album_details += 1;
                collect_song_cover_urls(&server, &songs, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
                push_unique_songs(&mut collected_songs, &mut seen_song_keys, songs);
            }
            smart_cache_pause(20).await;
        }
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);

        let playlists = client.get_playlists().await.unwrap_or_default();
        collect_playlist_cover_urls(&server, &playlists, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
        for playlist in playlists.iter().take(playlists_per_server) {
            if let Ok((_, songs)) = client.get_playlist(&playlist.id).await {
                warmed_playlists += 1;
                collect_song_cover_urls(&server, &songs, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
                push_unique_songs(&mut collected_songs, &mut seen_song_keys, songs);
            }
            smart_cache_pause(20).await;
        }
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);

        if let Ok((_, starred_albums, starred_songs)) = client.get_starred().await {
            collect_album_cover_urls(&server, &starred_albums, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
            collect_song_cover_urls(&server, &starred_songs, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
            push_unique_songs(&mut collected_songs, &mut seen_song_keys, starred_songs);
        }
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);

        let random_songs = client
            .get_random_songs(random_songs_per_server)
            .await
            .unwrap_or_default();
        collect_song_cover_urls(&server, &random_songs, &mut cover_urls, &mut seen_cover_requests, artwork_limit, artwork_pref);
        push_unique_songs(&mut collected_songs, &mut seen_song_keys, random_songs);
        server_steps_done += 1.0;
        set_server_progress(progress, server_steps_done, total_server_steps);
    }

    let mut lyric_candidates = collected_songs.clone();
    lyric_candidates.truncate(lyrics_limit);
    let lyric_total = lyric_candidates.len();
    if lyric_total > 0 {
        for (index, song) in lyric_candidates.into_iter().enumerate() {
            status.set(Some(format!("Warming lyrics ({}/{})…", index + 1, lyric_total)));
            let query = LyricsQuery::from_song(&song);
            if !query.title.trim().is_empty()
                && fetch_lyrics_with_fallback(&query, &provider_order, timeout_seconds)
                    .await
                    .is_ok()
            {
                warmed_lyrics += 1;
            }
            progress.set(
                (60.0 + ((index as f64 + 1.0) / lyric_total as f64) * 25.0)
                    .round()
                    .clamp(60.0, 85.0) as u8,
            );
            smart_cache_pause(35).await;
        }
    } else {
        progress.set(85);
    }

    let mut artwork_warning = None;
    if cache_images_enabled {
        let unique_cover_urls = dedupe_trim_urls(cover_urls, artwork_limit);
        if !unique_cover_urls.is_empty() {
            status.set(Some(format!("Queueing artwork ({} URLs)…", unique_cover_urls.len())));
            match warm_cover_art_urls(&unique_cover_urls) {
                Ok(queued) => queued_artwork = queued,
                Err(error) => artwork_warning = Some(error),
            }
        }
    }

    progress.set(100);
    let mut summary = format!(
        "Done: {} songs, {} album details, {} playlists, {} lyrics, {} artwork requests queued (limit {}).",
        collected_songs.len(),
        warmed_album_details,
        warmed_playlists,
        warmed_lyrics,
        queued_artwork,
        artwork_limit
    );
    if let Some(warning) = artwork_warning {
        summary.push_str(&format!(" Artwork prefetch warning: {warning}"));
    }
    summary
}
