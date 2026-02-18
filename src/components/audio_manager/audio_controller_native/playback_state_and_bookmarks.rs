// Native controller: apply play/pause/repeat/volume and pause-time bookmark persistence.
{
    // Handle play/pause state changes.
    {
        let is_playing = is_playing.clone();
        use_effect(move || {
            if is_playing() {
                native_audio_command(serde_json::json!({ "type": "play" }));
            } else {
                native_audio_command(serde_json::json!({ "type": "pause" }));
            }
        });
    }

    // Handle repeat mode changes.
    {
        let repeat_mode = repeat_mode.clone();
        use_effect(move || {
            let _ = repeat_mode();
            native_audio_command(serde_json::json!({
                "type": "loop",
                // Repeat behavior is handled in ended-event logic.
                "enabled": false,
            }));
        });
    }

    // Handle volume changes.
    {
        let volume = volume.clone();
        use_effect(move || {
            native_audio_command(serde_json::json!({
                "type": "volume",
                "value": volume().clamp(0.0, 1.0),
            }));
        });
    }

    // Persist a bookmark when playback pauses.
    {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut last_bookmark = last_bookmark.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let playback_position = playback_position.clone();
        let preview_playback = preview_playback.clone();
        use_effect(move || {
            if is_playing() {
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

            let position_ms = (playback_position.peek().max(0.0) * 1000.0).round() as u64;
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
