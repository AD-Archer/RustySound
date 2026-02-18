// Web controller: initialize audio element, wire listeners, and poll playback state.
{
    // One-time setup: create audio element and attach listeners.
    {
        let servers = servers.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let repeat_mode = repeat_mode.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        let app_settings = app_settings.clone();
        let playback_position = playback_position.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut audio_state = audio_state.clone();
        let preview_playback = preview_playback.clone();

        use_effect(move || {
            let Some(_audio) = get_or_create_audio_element() else {
                return;
            };
            ensure_web_media_session_shortcuts();

            if let Some(doc) = window().and_then(|w| w.document()) {
                let click_cb = Closure::wrap(
                    Box::new(move || USER_INTERACTED.with(|c| c.set(true))) as Box<dyn FnMut()>,
                );
                let key_cb = Closure::wrap(Box::new(move |event: KeyboardEvent| {
                    USER_INTERACTED.with(|c| c.set(true));
                    if let Some(action) = shortcut_action_from_key(&event) {
                        event.prevent_default();
                        match action {
                            "next" => click_player_control_button("next-btn"),
                            "previous" => click_player_control_button("prev-btn"),
                            "toggle_play" => click_player_control_button("play-pause-btn"),
                            _ => {}
                        }
                    }
                }) as Box<dyn FnMut(KeyboardEvent)>);
                let touch_cb = Closure::wrap(
                    Box::new(move || USER_INTERACTED.with(|c| c.set(true))) as Box<dyn FnMut()>,
                );
                let _ = doc
                    .add_event_listener_with_callback("click", click_cb.as_ref().unchecked_ref());
                let _ = doc
                    .add_event_listener_with_callback("keydown", key_cb.as_ref().unchecked_ref());
                let _ = doc.add_event_listener_with_callback(
                    "touchstart",
                    touch_cb.as_ref().unchecked_ref(),
                );
                click_cb.forget();
                key_cb.forget();
                touch_cb.forget();
            }

            let mut current_time_signal = audio_state.peek().current_time;
            let mut duration_signal = audio_state.peek().duration;
            let mut playback_error_signal = audio_state.peek().playback_error;
            let mut playback_pos = playback_position.clone();
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let repeat_mode = repeat_mode.clone();
            let shuffle_enabled = shuffle_enabled.clone();
            let servers = servers.clone();

            audio_state.write().is_initialized.set(true);

            spawn(async move {
                let mut last_emit = 0.0f64;
                let mut last_duration = -1.0f64;
                let mut ended_for_song: Option<String> = None;
                let mut repeat_one_replayed_song: Option<String> = None;
                let mut paused_streak: u8 = 0;
                let mut playing_streak: u8 = 0;

                loop {
                    gloo_timers::future::TimeoutFuture::new(200).await;

                    let Some(audio) = get_or_create_audio_element() else {
                        continue;
                    };

                    let time = audio.current_time();
                    if (time - last_emit).abs() >= 0.2 {
                        last_emit = time;
                        current_time_signal.set(time);
                        playback_pos.set(time);
                    }

                    let dur = audio.duration();
                    if !dur.is_nan() && (dur - last_duration).abs() > 0.5 {
                        last_duration = dur;
                        duration_signal.set(dur);
                    }
                    let paused = audio.paused();

                    if !paused
                        && app_settings.peek().bookmark_auto_save
                        && !*preview_playback.peek()
                    {
                        if let Some(song) = now_playing.peek().clone() {
                            if can_save_server_bookmark(&song) {
                                let position_ms = (time * 1000.0).round().max(0.0) as u64;
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

                    let current_song = { now_playing.read().clone() };
                    if current_song.is_some() {
                        // Keep UI play/pause signals synced when playback is controlled
                        // outside app buttons (browser media controls, hardware keys, etc.).
                        if paused {
                            paused_streak = paused_streak.saturating_add(1);
                            playing_streak = 0;
                        } else {
                            playing_streak = playing_streak.saturating_add(1);
                            paused_streak = 0;
                        }

                        if *is_playing.peek() && paused_streak >= 2 && !audio.ended() {
                            is_playing.set(false);
                        } else if !*is_playing.peek() && playing_streak >= 2 {
                            is_playing.set(true);
                        }

                        if let Some(message) =
                            web_playback_error_message(&audio, current_song.as_ref())
                        {
                            if playback_error_signal.peek().as_ref() != Some(&message) {
                                playback_error_signal.set(Some(message));
                            }
                        } else if playback_error_signal.peek().is_some() {
                            let has_started = time > 0.0 || (!dur.is_nan() && dur > 0.0) || !paused;
                            if has_started {
                                playback_error_signal.set(None);
                            }
                        }
                    } else {
                        paused_streak = 0;
                        playing_streak = 0;
                        if *is_playing.peek() {
                            is_playing.set(false);
                        }
                        if playback_error_signal.peek().is_some() {
                            playback_error_signal.set(None);
                        }
                    }

                    if audio.ended() {
                        let current_id = current_song.as_ref().map(|s| s.id.clone());
                        if ended_for_song == current_id {
                            continue;
                        }
                        ended_for_song = current_id.clone();

                        let queue_snapshot = { queue.read().clone() };
                        let idx = { *queue_index.read() };
                        let repeat = { *repeat_mode.read() };
                        let shuffle = { *shuffle_enabled.read() };
                        let servers_snapshot = { servers.read().clone() };

                        if repeat != RepeatMode::One {
                            repeat_one_replayed_song = None;
                        }

                        if let Some(song) = current_song.clone() {
                            if *preview_playback.peek() {
                                continue;
                            }
                            scrobble_song(&servers_snapshot, &song, true);
                        }

                        if repeat == RepeatMode::One {
                            if let Some(song_id) = current_id.clone() {
                                if repeat_one_replayed_song.as_ref() != Some(&song_id) {
                                    repeat_one_replayed_song = Some(song_id);
                                    audio.set_current_time(0.0);
                                    if *is_playing.read() {
                                        web_try_play(&audio);
                                    }
                                } else {
                                    repeat_one_replayed_song = None;
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
                            }
                        } else if repeat == RepeatMode::All {
                            if let Some(song) = queue_snapshot.get(0).cloned() {
                                queue_index.set(0);
                                now_playing.set(Some(song));
                            }
                        } else {
                            is_playing.set(false);
                        }
                    } else {
                        ended_for_song = None;
                    }
                }
            });
        });
    }

}
