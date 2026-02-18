// Native controller: bootstrap bridge, poll playback snapshots, and handle remote actions.
{
    // One-time setup: bootstrap audio bridge and poll playback state.
    {
        let servers = servers.clone();
        let queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let repeat_mode = repeat_mode.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        let app_settings = app_settings.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_ended_song = last_ended_song.clone();
        let mut repeat_one_replayed_song = repeat_one_replayed_song.clone();
        let preview_playback = preview_playback.clone();

        use_effect(move || {
            ensure_native_audio_bridge();
            audio_state.write().is_initialized.set(true);

            spawn(async move {
                let mut paused_streak: u8 = 0;
                let mut playing_streak: u8 = 0;
                loop {
                    native_delay_ms(250).await;

                    let Some(snapshot) = native_audio_snapshot().await else {
                        continue;
                    };

                    let mut effective_duration = *audio_state.peek().duration.peek();
                    if snapshot.duration.is_finite() && snapshot.duration > 0.0 {
                        effective_duration = snapshot.duration;
                        audio_state.write().duration.set(snapshot.duration);
                    }

                    let mut current_time = snapshot.current_time.max(0.0);
                    if effective_duration.is_finite() && effective_duration > 0.0 {
                        current_time = current_time.min(effective_duration);
                    }
                    playback_position.set(current_time);
                    audio_state.write().current_time.set(current_time);

                    if !snapshot.paused
                        && app_settings.peek().bookmark_auto_save
                        && !*preview_playback.peek()
                    {
                        if let Some(song) = now_playing.peek().clone() {
                            if can_save_server_bookmark(&song) {
                                let position_ms = (current_time * 1000.0).round().max(0.0) as u64;
                                if position_ms > 1500 {
                                    let should_save = match last_bookmark.peek().clone() {
                                        Some((id, pos)) => {
                                            id != song.id || position_ms.abs_diff(pos) >= 15_000
                                        }
                                        None => true,
                                    };
                                    if should_save {
                                        last_bookmark.set(Some((song.id.clone(), position_ms)));
                                        let servers_snapshot = servers.peek().clone();
                                        let bookmark_limit =
                                            app_settings.peek().bookmark_limit.clamp(1, 5000)
                                                as usize;
                                        let song_id = song.id.clone();
                                        let server_id = song.server_id.clone();
                                        spawn(async move {
                                            if let Some(server) = servers_snapshot
                                                .iter()
                                                .find(|s| s.id == server_id)
                                                .cloned()
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
                    }

                    let has_selected_song = now_playing.peek().is_some();
                    let desired_playing_before_sync = *is_playing.peek();
                    if has_selected_song {
                        if snapshot.paused {
                            paused_streak = paused_streak.saturating_add(1);
                            playing_streak = 0;
                        } else {
                            playing_streak = playing_streak.saturating_add(1);
                            paused_streak = 0;
                        }

                        // Debounced sync from native player to UI state:
                        // avoid immediate false flips while a track is starting.
                        if *is_playing.peek() && paused_streak >= 3 && !snapshot.ended {
                            // Source switches can briefly report paused at t=0.
                            // Keep the requested play state during startup to avoid
                            // requiring extra user clicks after skip/end/select.
                            if current_time > 0.35 {
                                is_playing.set(false);
                            }
                        } else if !*is_playing.peek() && playing_streak >= 2 {
                            is_playing.set(true);
                        }
                    } else {
                        paused_streak = 0;
                        playing_streak = 0;
                        if *is_playing.peek() {
                            is_playing.set(false);
                        }
                    }

                    let currently_playing = *is_playing.peek();

                    let ended_action = matches!(snapshot.action.as_deref(), Some("ended"));

                    if let Some(action) = snapshot.action.as_deref() {
                        if now_playing.peek().is_none() {
                            continue;
                        }
                        let current_is_radio = now_playing
                            .peek()
                            .as_ref()
                            .map(|song| song.server_name == "Radio")
                            .unwrap_or(false);
                        let queue_snapshot = queue.peek().clone();
                        let idx = *queue_index.peek();
                        let repeat = *repeat_mode.peek();
                        let shuffle = *shuffle_enabled.peek();
                        let servers_snapshot = servers.peek().clone();
                        let resume_after_skip = if matches!(action, "next" | "previous") {
                            true
                        } else {
                            desired_playing_before_sync
                        };

                        if let Some(raw_seek) = action.strip_prefix("seek:") {
                            if let Ok(target) = raw_seek.parse::<f64>() {
                                let mut clamped = target.max(0.0);
                                if effective_duration.is_finite() && effective_duration > 0.0 {
                                    clamped = clamped.min(effective_duration);
                                }
                                playback_position.set(clamped);
                                audio_state.write().current_time.set(clamped);
                            }
                            continue;
                        }

                        match action {
                            "toggle_play" | "playpause" => {
                                is_playing.set(!currently_playing);
                            }
                            "play" => {
                                is_playing.set(true);
                            }
                            "pause" => {
                                is_playing.set(false);
                            }
                            "next" => {
                                if current_is_radio {
                                    continue;
                                }
                                let len = queue_snapshot.len();
                                if len == 0 {
                                    spawn_shuffle_queue(
                                        servers_snapshot,
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        now_playing.peek().clone(),
                                        Some(resume_after_skip),
                                    );
                                } else if repeat == RepeatMode::Off && shuffle {
                                    if idx < len.saturating_sub(1) {
                                        if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                            queue_index.set(idx + 1);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else {
                                        spawn_shuffle_queue(
                                            servers_snapshot,
                                            queue.clone(),
                                            queue_index.clone(),
                                            now_playing.clone(),
                                            is_playing.clone(),
                                            now_playing.peek().clone(),
                                            Some(resume_after_skip),
                                        );
                                    }
                                } else if idx < len.saturating_sub(1) {
                                    if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                        queue_index.set(idx + 1);
                                        now_playing.set(Some(song));
                                        is_playing.set(resume_after_skip);
                                    }
                                } else if repeat == RepeatMode::All {
                                    if let Some(song) = queue_snapshot.first().cloned() {
                                        queue_index.set(0);
                                        now_playing.set(Some(song));
                                        is_playing.set(resume_after_skip);
                                    }
                                } else if len <= 1 {
                                    spawn_shuffle_queue(
                                        servers_snapshot,
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        now_playing.peek().clone(),
                                        Some(resume_after_skip),
                                    );
                                } else {
                                    native_audio_command(serde_json::json!({
                                        "type": "seek",
                                        "position": 0.0
                                    }));
                                    is_playing.set(false);
                                }
                            }
                            "previous" => {
                                if current_is_radio {
                                    continue;
                                }
                                let len = queue_snapshot.len();
                                if len > 0 {
                                    if idx > 0 {
                                        if let Some(song) = queue_snapshot.get(idx - 1).cloned() {
                                            queue_index.set(idx - 1);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else if repeat == RepeatMode::All {
                                        let last_idx = len.saturating_sub(1);
                                        if let Some(song) = queue_snapshot.get(last_idx).cloned() {
                                            queue_index.set(last_idx);
                                            now_playing.set(Some(song));
                                            is_playing.set(resume_after_skip);
                                        }
                                    } else {
                                        native_audio_command(serde_json::json!({
                                            "type": "seek",
                                            "position": 0.0
                                        }));
                                        if resume_after_skip {
                                            native_audio_command(serde_json::json!({
                                                "type": "play"
                                            }));
                                        }
                                    }
                                }
                            }
                            "ended" => {}
                            _ => {}
                        }
                    }

                    if snapshot.ended || ended_action {
                        let current_song = now_playing.peek().clone();
                        let current_id = current_song.as_ref().map(|s| s.id.clone());
                        if *last_ended_song.peek() == current_id {
                            continue;
                        }
                        last_ended_song.set(current_id.clone());

                        let queue_snapshot = queue.peek().clone();
                        let idx = *queue_index.peek();
                        let repeat = *repeat_mode.peek();
                        let shuffle = *shuffle_enabled.peek();
                        let servers_snapshot = servers.peek().clone();

                        if repeat != RepeatMode::One && repeat_one_replayed_song.peek().is_some() {
                            repeat_one_replayed_song.set(None);
                        }

                        if let Some(song) = current_song.clone() {
                            if *preview_playback.peek() {
                                continue;
                            }
                            scrobble_song(&servers_snapshot, &song, true);
                        }

                        if repeat == RepeatMode::One {
                            if let Some(song_id) = current_id.clone() {
                                if repeat_one_replayed_song.peek().as_ref() != Some(&song_id) {
                                    repeat_one_replayed_song.set(Some(song_id));
                                    native_audio_command(serde_json::json!({
                                        "type": "seek",
                                        "position": 0.0
                                    }));
                                    if *is_playing.peek() {
                                        native_audio_command(serde_json::json!({
                                            "type": "play"
                                        }));
                                    }
                                } else {
                                    // Repeat-one should replay exactly once, then stop.
                                    repeat_one_replayed_song.set(None);
                                    is_playing.set(false);
                                }
                            } else {
                                is_playing.set(false);
                            }
                            continue;
                        }

                        let len = queue_snapshot.len();
                        if len == 0 {
                            is_playing.set(false);
                            continue;
                        }

                        let should_shuffle = repeat == RepeatMode::Off && shuffle;
                        if should_shuffle {
                            if idx < len.saturating_sub(1) {
                                if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                    queue_index.set(idx + 1);
                                    now_playing.set(Some(song));
                                    is_playing.set(true);
                                }
                            } else {
                                spawn_shuffle_queue(
                                    servers_snapshot,
                                    queue.clone(),
                                    queue_index.clone(),
                                    now_playing.clone(),
                                    is_playing.clone(),
                                    current_song,
                                    Some(true),
                                );
                            }
                            continue;
                        }

                        if idx < len.saturating_sub(1) {
                            if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                queue_index.set(idx + 1);
                                now_playing.set(Some(song));
                                is_playing.set(true);
                            }
                        } else if repeat == RepeatMode::All {
                            if let Some(song) = queue_snapshot.first().cloned() {
                                queue_index.set(0);
                                now_playing.set(Some(song));
                                is_playing.set(true);
                            }
                        } else {
                            is_playing.set(false);
                        }
                    } else if last_ended_song.peek().is_some() {
                        last_ended_song.set(None);
                    }
                }
            });
        });
    }

}
