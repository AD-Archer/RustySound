// Native controller: sync queue selection, source loading, metadata, and prefetch flow.
{
    // Keep queue_index aligned when now_playing changes and the song is in the queue.
    {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let song = now_playing();
            let queue_list = queue();
            if let Some(song) = song {
                if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
                    if pos != queue_index() {
                        queue_index.set(pos);
                    }
                }
            }
        });
    }

    // Update source + metadata and persist bookmark when songs change.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let volume = volume.clone();
        let queue = queue.clone();
        let mut queue_index = queue_index.clone();
        #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
        let repeat_mode = repeat_mode.clone();
        #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
        let shuffle_enabled = shuffle_enabled.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut seek_request = seek_request.clone();
        let mut last_song_id = last_song_id.clone();
        let mut last_src = last_src.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_song_for_bookmark = last_song_for_bookmark.clone();
        let preview_playback = preview_playback.clone();

        use_effect(move || {
            let song = now_playing();
            let song_id = song.as_ref().map(|s| s.id.clone());
            let previous_song_id = last_song_id.peek().clone();
            if song_id != previous_song_id {
                ios_diag_log(
                    "track.sync.song",
                    &format!(
                        "previous={previous_song_id:?} next={song_id:?} queue_idx={} queue_len={}",
                        queue_index(),
                        queue().len()
                    ),
                );
            }
            let previous_song = last_song_for_bookmark.peek().clone();

            if let Some(prev) = previous_song {
                if Some(prev.id.clone()) != song_id {
                    let position_ms = (playback_position.peek().max(0.0) * 1000.0).round() as u64;
                    if position_ms > 1500 && can_save_server_bookmark(&prev) {
                        if app_settings.peek().bookmark_auto_save && !*preview_playback.peek() {
                            let servers_snapshot = servers.peek().clone();
                            let bookmark_limit =
                                app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                            let song_id = prev.id.clone();
                            let server_id = prev.server_id.clone();
                            spawn(async move {
                                last_bookmark.set(Some((song_id.clone(), position_ms)));
                                if let Some(server) =
                                    servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                                {
                                    let client = NavidromeClient::new(server);
                                    let _ = client
                                        .create_bookmark_with_limit(
                                            &song_id,
                                            position_ms,
                                            None,
                                            Some(bookmark_limit),
                                        )
                                        .await;
                                }
                            });
                        }
                    }
                }
            }

            if song_id != *last_song_id.peek() {
                last_song_id.set(song_id.clone());
                last_song_for_bookmark.set(song.clone());
            }

            let Some(song) = song else {
                #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
                {
                    let queue_snapshot = queue();
                    let queue_idx = queue_index();
                    let servers_snapshot = servers.peek().clone();
                    let offline_mode = app_settings.peek().offline_mode;
                    let repeat = repeat_mode();
                    let shuffle = shuffle_enabled();
                    let plan_items = queue_snapshot
                        .iter()
                        .map(|entry| IosPlaybackPlanItem {
                            song_id: entry.id.clone(),
                            src: resolve_stream_url(entry, &servers_snapshot, offline_mode),
                            meta: song_metadata(entry, &servers_snapshot),
                        })
                        .collect::<Vec<_>>();
                    ios_update_playback_plan(plan_items, queue_idx, repeat, shuffle);
                }

                let should_clear = last_src.peek().is_some()
                    || last_song_id.peek().is_some()
                    || *is_playing.peek();
                if should_clear {
                    ios_diag_log("track.sync.command", "clear (no now_playing)");
                    native_audio_command(serde_json::json!({ "type": "clear" }));
                }
                last_src.set(None);
                is_playing.set(false);
                audio_state.write().playback_error.set(None);
                return;
            };

            let servers_snapshot = servers.peek().clone();
            let offline_mode = app_settings.peek().offline_mode;
            #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
            {
                let queue_snapshot = queue();
                let queue_idx = queue_index();
                let repeat = repeat_mode();
                let shuffle = shuffle_enabled();
                let plan_items = queue_snapshot
                    .iter()
                    .map(|entry| IosPlaybackPlanItem {
                        song_id: entry.id.clone(),
                        src: resolve_stream_url(entry, &servers_snapshot, offline_mode),
                        meta: song_metadata(entry, &servers_snapshot),
                    })
                    .collect::<Vec<_>>();
                ios_update_playback_plan(plan_items, queue_idx, repeat, shuffle);
            }

            if let Some(url) = resolve_stream_url(&song, &servers_snapshot, offline_mode) {
                let requested_seek = seek_request.peek().clone().and_then(|(song_id, position)| {
                    if song_id == song.id {
                        Some(position)
                    } else {
                        None
                    }
                });

                let should_reload = Some(url.clone()) != *last_src.peek();
                let metadata = song_metadata(&song, &servers_snapshot);
                let known_duration = if song.duration > 0 {
                    song.duration as f64
                } else {
                    0.0
                };
                let mut target_start = requested_seek.unwrap_or(0.0).max(0.0);
                if known_duration > 0.0 {
                    target_start = target_start.min(known_duration);
                }

                if should_reload {
                    ios_diag_log(
                        "track.sync.command",
                        &format!(
                            "load song_id={} play={} seek={target_start:.3} src_prefix={}",
                            song.id,
                            *is_playing.peek(),
                            url.chars().take(80).collect::<String>()
                        ),
                    );
                    last_src.set(Some(url.clone()));
                    playback_position.set(target_start);
                    audio_state.write().current_time.set(target_start);
                    audio_state.write().playback_error.set(None);
                    if known_duration > 0.0 {
                        audio_state.write().duration.set(known_duration);
                    } else {
                        audio_state.write().duration.set(0.0);
                    }
                    native_audio_command(serde_json::json!({
                        "type": "load",
                        "src": url,
                        "song_id": song.id,
                        "position": target_start,
                        "volume": volume.peek().clamp(0.0, 1.0),
                        "play": *is_playing.peek(),
                        "meta": metadata,
                    }));
                } else if let Some(target_pos) = requested_seek {
                    ios_diag_log(
                        "track.sync.command",
                        &format!("seek song_id={} target={target_pos:.3}", song.id),
                    );
                    native_audio_command(serde_json::json!({
                        "type": "seek",
                        "position": target_pos,
                    }));
                } else {
                    ios_diag_log(
                        "track.sync.command",
                        &format!("metadata song_id={}", song.id),
                    );
                    native_audio_command(serde_json::json!({
                        "type": "metadata",
                        "meta": metadata,
                    }));
                }

                if let Some(target_pos) = requested_seek {
                    let mut clamped_pos = target_pos.max(0.0);
                    let current_duration = *audio_state.peek().duration.peek();
                    if current_duration > 0.0 {
                        clamped_pos = clamped_pos.min(current_duration);
                    }
                    playback_position.set(clamped_pos);
                    audio_state.write().current_time.set(clamped_pos);
                    seek_request.set(None);
                }

                if !*preview_playback.peek() {
                    scrobble_song(&servers_snapshot, &song, false);
                }
            } else {
                if offline_mode {
                    let queue_snapshot = queue();
                    if !queue_snapshot.is_empty() {
                        let current_idx = queue_snapshot
                            .iter()
                            .position(|entry| entry.id == song.id && entry.server_id == song.server_id)
                            .unwrap_or_else(|| queue_index().min(queue_snapshot.len().saturating_sub(1)));

                        let fallback_next = queue_snapshot
                            .iter()
                            .enumerate()
                            .skip(current_idx.saturating_add(1))
                            .find(|(_, entry)| is_song_downloaded(entry))
                            .or_else(|| {
                                queue_snapshot
                                    .iter()
                                    .enumerate()
                                    .take(current_idx.saturating_add(1))
                                    .find(|(_, entry)| is_song_downloaded(entry))
                            });

                        if let Some((next_idx, next_song)) = fallback_next {
                            ios_diag_log(
                                "track.sync.command",
                                &format!(
                                    "offline-skip-unavailable current={} next={} next_idx={next_idx}",
                                    song.id, next_song.id
                                ),
                            );
                            queue_index.set(next_idx);
                            now_playing.set(Some(next_song.clone()));
                            is_playing.set(true);
                            audio_state.write().playback_error.set(None);
                            return;
                        }
                    }
                }

                ios_diag_log(
                    "track.sync.command",
                    &format!("clear (unresolved url) song_id={}", song.id),
                );
                native_audio_command(serde_json::json!({ "type": "clear" }));
                last_src.set(None);
                is_playing.set(false);
                let message = if song.server_name == "Radio" {
                    "No station found: this station has no stream URL.".to_string()
                } else {
                    "Unable to load this audio source.".to_string()
                };
                audio_state.write().playback_error.set(Some(message));
            }
        });
    }

    // Prefetch current + next two songs to local audio cache for brief offline continuity.
    {
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let preview_playback = preview_playback.clone();
        use_effect(move || {
            if *preview_playback.peek() {
                return;
            }
            if !app_settings.peek().cache_enabled {
                return;
            }

            let queue_snapshot = queue();
            let current_index = queue_index();
            let mut seeds = Vec::<Song>::new();

            if let Some(current) = now_playing() {
                seeds.push(current);
            }

            for candidate in queue_snapshot.into_iter().skip(current_index).take(3) {
                if seeds.iter().any(|existing| {
                    existing.id == candidate.id && existing.server_id == candidate.server_id
                }) {
                    continue;
                }
                seeds.push(candidate);
            }

            if seeds.is_empty() {
                return;
            }

            let servers_snapshot = servers();
            let settings_snapshot = app_settings();
            spawn(async move {
                for song in seeds {
                    let _ = prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot).await;
                }
            });
        });
    }

}
