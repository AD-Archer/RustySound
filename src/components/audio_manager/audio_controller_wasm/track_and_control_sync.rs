// Web controller: sync track/queue state and apply transport, volume, and bookmark effects.
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
                        defer_signal_update(move || {
                            queue_index.set(pos);
                        });
                    }
                }
            }
        });
    }

    // Update audio source and track changes for bookmarks/scrobbles.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let volume = volume.clone();
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
            let previous_song = last_song_for_bookmark.peek().clone();

            if let Some(prev) = previous_song {
                if Some(prev.id.clone()) != song_id {
                    let position_ms = get_or_create_audio_element()
                        .map(|a| a.current_time())
                        .unwrap_or(0.0)
                        .mul_add(1000.0, 0.0)
                        .round()
                        .max(0.0) as u64;
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

            let last_id = last_song_id.peek().clone();
            if song_id != last_id {
                last_song_id.set(song_id.clone());
                last_song_for_bookmark.set(song.clone());
            }

            let Some(song) = song else {
                if let Some(audio) = get_or_create_audio_element() {
                    let _ = audio.pause();
                    audio.set_src("");
                    let _ = audio.remove_attribute("src");
                    audio.load();
                }
                last_src.set(None);
                is_playing.set(false);
                audio_state.write().playback_error.set(None);
                return;
            };

            let servers_snapshot = servers.peek().clone();
            if let Some(url) = resolve_stream_url(&song, &servers_snapshot) {
                if Some(url.clone()) != *last_src.peek() {
                    last_src.set(Some(url.clone()));
                    audio_state.write().playback_error.set(None);
                    if let Some(audio) = get_or_create_audio_element() {
                        audio.set_src(&url);
                        audio.set_volume(volume.peek().clamp(0.0, 1.0));

                        if let Some((target_id, target_pos)) = seek_request.peek().clone() {
                            if target_id == song.id {
                                audio.set_current_time(target_pos);
                                let mut playback_position = playback_position.clone();
                                let mut audio_state = audio_state.clone();
                                defer_signal_update(move || {
                                    playback_position.set(target_pos);
                                    audio_state.write().current_time.set(target_pos);
                                    seek_request.set(None);
                                });
                            }
                        }

                        let was_playing = *is_playing.peek();
                        if has_user_interacted() && was_playing {
                            web_try_play(&audio);
                        } else {
                            let _ = audio.pause();
                            is_playing.set(false);
                        }
                    }
                }

                if !*preview_playback.peek() {
                    scrobble_song(&servers_snapshot, &song, false);
                }
            } else if let Some(audio) = get_or_create_audio_element() {
                audio.set_src("");
                last_src.set(None);
                is_playing.set(false);
                let message = if song.server_name == "Radio" {
                    let station_name = song
                        .album
                        .clone()
                        .or_else(|| song.artist.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| "this station".to_string());
                    format!("No station found: \"{station_name}\" has no playable stream URL.")
                } else {
                    "Unable to load this audio source.".to_string()
                };
                audio_state.write().playback_error.set(Some(message));
            }
        });
    }

    // Handle play/pause state changes.
    {
        let mut is_playing = is_playing.clone();
        use_effect(move || {
            let playing = is_playing();
            if let Some(audio) = get_or_create_audio_element() {
                if playing {
                    if has_user_interacted() {
                        if audio.paused() {
                            web_try_play(&audio);
                        }
                    } else {
                        is_playing.set(false);
                    }
                } else if !audio.paused() {
                    let _ = audio.pause();
                }
            }
        });
    }

    // Handle repeat mode changes.
    {
        let repeat_mode = repeat_mode.clone();
        use_effect(move || {
            let _ = repeat_mode();
            if let Some(audio) = get_or_create_audio_element() {
                // Repeat behavior is handled in ended-event logic.
                audio.set_loop(false);
            }
        });
    }

    // Handle volume changes.
    {
        let volume = volume.clone();
        use_effect(move || {
            let vol = volume().clamp(0.0, 1.0);
            if let Some(audio) = get_or_create_audio_element() {
                audio.set_volume(vol);
            }
        });
    }

    // Persist a server bookmark when playback stops/pauses.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut last_bookmark = last_bookmark.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let preview_playback = preview_playback.clone();
        use_effect(move || {
            let playing = is_playing();
            if playing {
                return;
            }
            if *preview_playback.peek() {
                return;
            }

            let Some(song) = now_playing() else {
                return;
            };
            if !can_save_server_bookmark(&song) {
                return;
            }

            let position_ms = get_or_create_audio_element()
                .map(|a| a.current_time())
                .unwrap_or(0.0)
                .mul_add(1000.0, 0.0)
                .round()
                .max(0.0) as u64;

            if position_ms <= 1500 {
                return;
            }
            if !app_settings.peek().bookmark_auto_save {
                return;
            }

            let should_save = match last_bookmark.peek().clone() {
                Some((id, pos)) => id != song.id || position_ms.abs_diff(pos) >= 2000,
                None => true,
            };

            if should_save {
                let servers_snapshot = servers.peek().clone();
                let bookmark_limit = app_settings.peek().bookmark_limit.clamp(1, 5000) as usize;
                let song_id = song.id.clone();
                let server_id = song.server_id.clone();
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
        });
    }

}
