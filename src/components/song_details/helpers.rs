// Shared helper functions for lyrics, queue mutation, favorites, ratings, and related-song loading.

fn song_cover_url(song: &Song, servers: &[ServerConfig], size: u32) -> Option<String> {
    let server = servers.iter().find(|server| server.id == song.server_id)?;
    let cover_art = song.cover_art.as_ref()?;
    let client = NavidromeClient::new(server.clone());
    Some(client.get_cover_art_url(cover_art, size))
}

async fn fetch_first_available_lyrics(
    query: LyricsQuery,
    provider_order: Vec<String>,
    timeout_seconds: u32,
) -> Result<LyricsResult, String> {
    let providers = normalize_lyrics_provider_order(&provider_order);
    if providers.is_empty() {
        return Err("No lyrics providers configured.".to_string());
    }

    let mut errors = Vec::<String>::new();
    for provider in providers {
        let result = fetch_lyrics_with_fallback(&query, &[provider.clone()], timeout_seconds).await;
        match result {
            Ok(lyrics) => return Ok(lyrics),
            Err(error) => errors.push(format!("{provider} failed: {error}")),
        }
    }

    if errors.is_empty() {
        Err("No lyrics providers configured.".to_string())
    } else {
        Err(errors.join(" | "))
    }
}

fn build_mini_lyrics_preview(
    lyrics: Option<Result<LyricsResult, String>>,
    sync_lyrics: bool,
    current_time: f64,
    offset_seconds: f64,
) -> Option<MiniLyricsPreviewData> {
    let lyrics = lyrics?.ok()?;

    if sync_lyrics && !lyrics.synced_lines.is_empty() {
        let active_index = active_lyric_index(&lyrics.synced_lines, current_time + offset_seconds)
            .unwrap_or(0)
            .min(lyrics.synced_lines.len().saturating_sub(1));
        let previous = active_index
            .checked_sub(1)
            .and_then(|index| lyrics.synced_lines.get(index))
            .map(|line| line.text.trim().to_string());
        let current = lyrics
            .synced_lines
            .get(active_index)
            .map(|line| line.text.trim().to_string())
            .unwrap_or_default();
        let next = lyrics
            .synced_lines
            .get(active_index.saturating_add(1))
            .map(|line| line.text.trim().to_string());
        return Some(MiniLyricsPreviewData {
            previous,
            current,
            next,
        });
    }

    let lines = lyrics
        .plain_lyrics
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    Some(MiniLyricsPreviewData {
        previous: None,
        current: lines.first().cloned().unwrap_or_default(),
        next: lines.get(1).cloned(),
    })
}

fn pick_display_lyrics(
    sync_lyrics: bool,
    primary: Option<Result<LyricsResult, String>>,
    lrclib_upgrade: Option<Result<Option<LyricsResult>, String>>,
    cached_synced: Option<LyricsResult>,
) -> Option<Result<LyricsResult, String>> {
    if sync_lyrics {
        if let Some(Ok(Some(upgrade))) = lrclib_upgrade.as_ref() {
            if !upgrade.synced_lines.is_empty() {
                return Some(Ok(upgrade.clone()));
            }
        }
        if let Some(cached_synced) = cached_synced {
            if !cached_synced.synced_lines.is_empty() {
                return Some(Ok(cached_synced));
            }
        }
    }

    if let Some(primary_result) = primary.clone() {
        match primary_result {
            Ok(lyrics) => return Some(Ok(lyrics)),
            Err(error) => {
                if let Some(Ok(Some(upgrade))) = lrclib_upgrade {
                    return Some(Ok(upgrade));
                }
                return Some(Err(error));
            }
        }
    }

    if let Some(Ok(Some(upgrade))) = lrclib_upgrade {
        return Some(Ok(upgrade));
    }

    None
}

fn active_lyric_index(lines: &[LyricLine], playback_seconds: f64) -> Option<usize> {
    if lines.is_empty() {
        return None;
    }

    let mut active = None;
    for (index, line) in lines.iter().enumerate() {
        if playback_seconds >= line.timestamp_seconds {
            active = Some(index);
        } else {
            break;
        }
    }
    active
}

fn format_timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u32;
    let mins = total / 60;
    let secs = total % 60;
    format!("{mins:02}:{secs:02}")
}

fn sanitize_dom_id(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "lyrics".to_string()
    } else {
        sanitized
    }
}

fn now_millis() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

fn adjusted_queue_index_after_reorder(
    current_index: usize,
    source_index: usize,
    target_index: usize,
) -> usize {
    if source_index == current_index {
        target_index
    } else if source_index < current_index && target_index >= current_index {
        current_index.saturating_sub(1)
    } else if source_index > current_index && target_index <= current_index {
        current_index.saturating_add(1)
    } else {
        current_index
    }
}

fn reorder_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    source_index: usize,
    target_index: usize,
) {
    let current_index = queue_index();
    let mut reordered = false;
    let mut next_index = current_index;

    queue.with_mut(|items| {
        if items.len() < 2
            || source_index >= items.len()
            || target_index >= items.len()
            || source_index == target_index
        {
            return;
        }

        let moved_song = items.remove(source_index);
        let insert_index = target_index;
        items.insert(insert_index, moved_song);
        next_index = adjusted_queue_index_after_reorder(current_index, source_index, insert_index);
        reordered = true;
    });

    if !reordered {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        return;
    }

    let clamped_index = next_index.min(updated_queue.len().saturating_sub(1));
    queue_index.set(clamped_index);
    if now_playing().is_some() {
        now_playing.set(updated_queue.get(clamped_index).cloned());
    }
}

fn remove_queue_entry(
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    remove_index: usize,
) {
    let had_now_playing = now_playing().is_some();
    let was_playing = is_playing();
    let current_index = queue_index();
    let mut removed = false;

    queue.with_mut(|items| {
        if remove_index >= items.len() {
            return;
        }
        items.remove(remove_index);
        removed = true;
    });

    if !removed {
        return;
    }

    let updated_queue = queue();
    if updated_queue.is_empty() {
        queue_index.set(0);
        now_playing.set(None);
        is_playing.set(false);
        return;
    }

    let mut next_index = current_index.min(updated_queue.len().saturating_sub(1));
    if remove_index < current_index {
        next_index = current_index.saturating_sub(1);
    } else if remove_index == current_index {
        next_index = remove_index.min(updated_queue.len().saturating_sub(1));
    }

    queue_index.set(next_index);
    if had_now_playing {
        now_playing.set(updated_queue.get(next_index).cloned());
        is_playing.set(was_playing);
    }
}

async fn fetch_related_candidates(client: &NavidromeClient, song: &Song, count: u32) -> Vec<Song> {
    let mut related = client
        .get_similar_songs2(&song.id, count)
        .await
        .unwrap_or_default();

    if let Some(artist_id) = song.artist_id.as_deref() {
        let by_artist = client
            .get_similar_songs(artist_id, count)
            .await
            .unwrap_or_default();
        related.extend(by_artist);
    }

    if related.len() < count as usize {
        if let Some(artist_name) = song.artist.clone().filter(|name| !name.trim().is_empty()) {
            let top_songs = client
                .get_top_songs(&artist_name, count)
                .await
                .unwrap_or_default();
            related.extend(top_songs);
        }
    }

    if related.len() < count as usize {
        let fallback_query = format!(
            "{} {}",
            song.artist.clone().unwrap_or_default(),
            song.title.clone()
        );
        if let Ok(search) = client.search(&fallback_query, 0, 0, count).await {
            related.extend(search.songs);
        }
    }

    related
}

fn toggle_song_favorite(
    song: Song,
    should_star: bool,
    servers: Signal<Vec<ServerConfig>>,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
) {
    let Some(server) = servers()
        .iter()
        .find(|entry| entry.id == song.server_id)
        .cloned()
    else {
        return;
    };

    let song_id = song.id.clone();
    let song_server_id = song.server_id.clone();

    spawn(async move {
        let client = NavidromeClient::new(server);
        let result = if should_star {
            client.star(&song_id, "song").await
        } else {
            client.unstar(&song_id, "song").await
        };

        if result.is_ok() {
            let new_starred = if should_star {
                Some("local".to_string())
            } else {
                None
            };
            now_playing.with_mut(|current| {
                if let Some(current_song) = current.as_mut() {
                    if current_song.id == song_id && current_song.server_id == song_server_id {
                        current_song.starred = new_starred.clone();
                    }
                }
            });
            queue.with_mut(|items| {
                for entry in items.iter_mut() {
                    if entry.id == song_id && entry.server_id == song_server_id {
                        entry.starred = new_starred.clone();
                    }
                }
            });
        }
    });
}

fn set_now_playing_rating(
    servers: Signal<Vec<ServerConfig>>,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
    rating: u32,
) {
    let Some(song) = now_playing() else {
        return;
    };
    let Some(server) = servers()
        .iter()
        .find(|entry| entry.id == song.server_id)
        .cloned()
    else {
        return;
    };

    let song_id = song.id.clone();
    let song_server_id = song.server_id.clone();
    let normalized_rating = rating.min(5);

    spawn(async move {
        let client = NavidromeClient::new(server);
        if client.set_rating(&song_id, normalized_rating).await.is_ok() {
            let new_rating = if normalized_rating == 0 {
                None
            } else {
                Some(normalized_rating)
            };
            now_playing.with_mut(|current| {
                if let Some(current_song) = current.as_mut() {
                    if current_song.id == song_id && current_song.server_id == song_server_id {
                        current_song.user_rating = new_rating;
                    }
                }
            });
            queue.with_mut(|items| {
                for entry in items.iter_mut() {
                    if entry.id == song_id && entry.server_id == song_server_id {
                        entry.user_rating = new_rating;
                    }
                }
            });
        }
    });
}

async fn load_related_songs(maybe_song: Option<Song>, servers: Vec<ServerConfig>) -> Vec<Song> {
    let Some(song) = maybe_song else {
        return Vec::new();
    };

    let Some(server) = servers
        .into_iter()
        .find(|server| server.id == song.server_id)
    else {
        return Vec::new();
    };

    let client = NavidromeClient::new(server);
    let related = fetch_related_candidates(&client, &song, 30).await;

    let mut unique = Vec::<Song>::new();
    for candidate in related {
        if candidate.id == song.id && candidate.server_id == song.server_id {
            continue;
        }
        if unique.iter().any(|existing| {
            existing.id == candidate.id && existing.server_id == candidate.server_id
        }) {
            continue;
        }
        unique.push(candidate);
        if unique.len() >= 30 {
            break;
        }
    }

    unique
}

async fn build_queue_from_seed(seed_song: Song, servers: Vec<ServerConfig>) -> Vec<Song> {
    let Some(server) = servers
        .into_iter()
        .find(|server| server.id == seed_song.server_id)
    else {
        return vec![seed_song];
    };

    let client = NavidromeClient::new(server);
    let mut queue = vec![seed_song.clone()];

    let related = fetch_related_candidates(&client, &seed_song, 60).await;
    for candidate in related {
        if queue.iter().any(|existing| {
            existing.id == candidate.id && existing.server_id == candidate.server_id
        }) {
            continue;
        }
        queue.push(candidate);
        if queue.len() >= 60 {
            break;
        }
    }

    if queue.len() <= 1 {
        let random_songs = client.get_random_songs(40).await.unwrap_or_default();
        for candidate in random_songs {
            if queue.iter().any(|existing| {
                existing.id == candidate.id && existing.server_id == candidate.server_id
            }) {
                continue;
            }
            queue.push(candidate);
            if queue.len() >= 40 {
                break;
            }
        }
    }

    queue
}
