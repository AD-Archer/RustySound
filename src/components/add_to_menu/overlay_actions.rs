// Build action handlers that enqueue songs, modify playlists, and quick-add suggestions.
{
    let make_add_to_queue = |mode: &'static str| {
        let servers = servers.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let mut is_processing = is_processing.clone();
        let mut processing_label = processing_label.clone();
        let mut message = message.clone();
        let mut suggestion_destination = suggestion_destination.clone();
        let mut suggestion_candidates = suggestion_candidates.clone();
        let mut suggestions_loading = suggestions_loading.clone();
        let intent = intent_for_queue.clone();

        move |_| {
            if is_processing() {
                return;
            }
            is_processing.set(true);
            processing_label.set(Some("Adding to queue...".to_string()));
            let servers_snapshot = servers();
            let target = intent.target.clone();
            let queue = queue.clone();
            let queue_index = queue_index.clone();
            spawn(async move {
                let songs_to_add = match resolve_target_songs(&servers_snapshot, &target).await {
                    Ok(songs) => songs,
                    Err(err) => {
                        message.set(Some((false, err)));
                        processing_label.set(None);
                        is_processing.set(false);
                        return;
                    }
                };

                if songs_to_add.is_empty() {
                    message.set(Some((false, "No songs available to add.".to_string())));
                    processing_label.set(None);
                    is_processing.set(false);
                    return;
                }

                let first_seed = songs_to_add.first().cloned();
                let recent_seed = songs_to_add.last().cloned();
                enqueue_items(queue.clone(), queue_index, songs_to_add.clone(), mode);

                suggestion_destination.set(Some(SuggestionDestination::Queue));
                suggestions_loading.set(true);
                suggestion_candidates.set(Vec::new());
                message.set(Some((
                    true,
                    format!("Added {} song(s) to queue.", songs_to_add.len()),
                )));

                let suggestions =
                    build_dual_seed_suggestions(&servers_snapshot, first_seed, recent_seed).await;
                suggestion_candidates.set(suggestions);
                suggestions_loading.set(false);

                processing_label.set(None);
                is_processing.set(false);
            });
        }
    };

    let make_add_to_playlist = {
        let servers = servers.clone();
        let is_processing = is_processing.clone();
        let message = message.clone();
        let show_playlist_picker = show_playlist_picker.clone();
        let intent = intent_for_playlist.clone();
        let active_server = active_server_for_playlist.clone();
        let controller = controller.clone();
        let suggestion_destination = suggestion_destination.clone();
        let suggestion_candidates = suggestion_candidates.clone();
        let suggestions_loading = suggestions_loading.clone();
        let processing_label = processing_label.clone();

        move |playlist_id: String| {
            let servers = servers.clone();
            let mut is_processing = is_processing.clone();
            let mut message = message.clone();
            let mut show_playlist_picker = show_playlist_picker.clone();
            let intent = intent.clone();
            let active_server = active_server.clone();
            let mut suggestion_destination = suggestion_destination.clone();
            let mut suggestion_candidates = suggestion_candidates.clone();
            let mut suggestions_loading = suggestions_loading.clone();
            let mut processing_label = processing_label.clone();
            let _controller = controller.clone();

            move |_| {
                if is_processing() {
                    return;
                }

                if let Some(reason) = requires_single_server(&intent.target, &active_server) {
                    message.set(Some((false, reason)));
                    return;
                }

                let Some(active) = servers().into_iter().find(|s| s.active) else {
                    message.set(Some((false, "No active server found.".to_string())));
                    return;
                };

                let target = intent.target.clone();
                let playlist_id_for_fetch = playlist_id.clone();
                let servers_snapshot = servers();
                is_processing.set(true);
                processing_label.set(Some("Adding to playlist...".to_string()));
                spawn(async move {
                    let songs_to_add = match resolve_target_songs(&servers_snapshot, &target).await
                    {
                        Ok(songs) => songs,
                        Err(err) => {
                            message.set(Some((false, err)));
                            show_playlist_picker.set(true);
                            processing_label.set(None);
                            is_processing.set(false);
                            return;
                        }
                    };

                    if songs_to_add.is_empty() {
                        message.set(Some((false, "No songs available to add.".to_string())));
                        show_playlist_picker.set(true);
                        processing_label.set(None);
                        is_processing.set(false);
                        return;
                    }

                    let first_seed = songs_to_add.first().cloned();
                    let client = NavidromeClient::new(active.clone());
                    let ids: Vec<String> =
                        songs_to_add.iter().map(|song| song.id.clone()).collect();
                    let result = client
                        .add_songs_to_playlist(&playlist_id_for_fetch, &ids)
                        .await;

                    match result {
                        Ok(_) => {
                            show_playlist_picker.set(false);
                            message.set(Some((
                                true,
                                format!("Added {} song(s) to playlist.", ids.len()),
                            )));
                            suggestion_destination.set(Some(SuggestionDestination::Playlist {
                                playlist_id: playlist_id_for_fetch.clone(),
                                server_id: active.id.clone(),
                            }));
                            suggestions_loading.set(true);
                            suggestion_candidates.set(Vec::new());

                            let recent_seed = client
                                .get_playlist(&playlist_id_for_fetch)
                                .await
                                .ok()
                                .and_then(|(_, songs)| songs.last().cloned())
                                .or_else(|| songs_to_add.last().cloned());

                            let mut suggestions = build_dual_seed_suggestions(
                                &servers_snapshot,
                                first_seed,
                                recent_seed,
                            )
                            .await;
                            suggestions.retain(|song| song.server_id == active.id);
                            suggestions.truncate(8);
                            suggestion_candidates.set(suggestions);
                            suggestions_loading.set(false);
                        }
                        Err(err) => {
                            message.set(Some((false, format!("Unable to add: {err}"))));
                            show_playlist_picker.set(true);
                            suggestions_loading.set(false);
                        }
                    }
                    processing_label.set(None);
                    is_processing.set(false);
                });
            }
        }
    };

    let create_playlist = {
        let _controller = controller.clone();
        let servers = servers.clone();
        let mut is_processing = is_processing.clone();
        let mut message = message.clone();
        let new_playlist_name = new_playlist_name.clone();
        let intent = intent_for_create.clone();
        let active_server = active_server_for_create.clone();
        let playlists = playlists.clone();

        move |_| {
            if is_processing() {
                return;
            }

            let name = new_playlist_name().trim().to_string();
            if name.is_empty() {
                message.set(Some((false, "Please enter a playlist name.".to_string())));
                return;
            }

            if let Some(reason) = requires_single_server(&intent.target, &active_server) {
                message.set(Some((false, reason)));
                return;
            }

            let Some(active) = servers().into_iter().find(|s| s.active) else {
                message.set(Some((false, "No active server found.".to_string())));
                return;
            };

            let target = intent.target.clone();
            is_processing.set(true);
            let mut message = message.clone();
            let mut new_playlist_name = new_playlist_name.clone();
            let playlists = playlists.clone();

            spawn(async move {
                let client = NavidromeClient::new(active);
                let mut playlists = playlists;
                // Collect song ids up front so we can add them exactly once after creation.
                let song_ids: Result<Vec<String>, String> = match target {
                    AddTarget::Song(song) => Ok(vec![song.id.clone()]),
                    AddTarget::Songs(songs) => Ok(songs.iter().map(|s| s.id.clone()).collect()),
                    AddTarget::Album { album_id, .. } => client
                        .get_album(&album_id)
                        .await
                        .map(|(_, songs)| songs.into_iter().map(|s| s.id).collect())
                        .map_err(|e| format!("Failed to fetch album tracks: {e}")),
                    AddTarget::Playlist { playlist_id, .. } => client
                        .get_playlist(&playlist_id)
                        .await
                        .map(|(_, songs)| songs.into_iter().map(|s| s.id).collect())
                        .map_err(|e| format!("Failed to fetch playlist tracks: {e}")),
                };

                match song_ids {
                    Err(err) => message.set(Some((false, err))),
                    Ok(ids) => match client.create_playlist(&name, None, &[]).await {
                        Ok(created_id) => {
                            if !ids.is_empty() {
                                let Some(pid) = created_id else {
                                    message.set(Some((
                                        false,
                                        "Playlist was created but the server did not return an id, so songs could not be added."
                                            .to_string(),
                                    )));
                                    is_processing.set(false);
                                    return;
                                };
                                if let Err(err) = client.add_songs_to_playlist(&pid, &ids).await {
                                    message.set(Some((
                                        false,
                                        format!("Playlist created but could not add songs: {err}"),
                                    )));
                                    is_processing.set(false);
                                    return;
                                }
                            }
                            message.set(Some((true, format!("Playlist \"{}\" created.", name))));
                            new_playlist_name.set(String::new());
                            // Hint to reload playlist list next time
                            playlists.restart();
                        }
                        Err(err) => message.set(Some((false, err))),
                    },
                }
                is_processing.set(false);
            });
        }
    };

    let on_open_playlist_picker = {
        let mut show_playlist_picker = show_playlist_picker.clone();
        move |_| show_playlist_picker.set(true)
    };

    let on_create_similar = {
        let controller = controller.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut is_processing = is_processing.clone();
        let mut processing_label = processing_label.clone();
        let mut message = message.clone();
        let intent = intent_for_similar.clone();

        move |_| {
            if *is_processing.peek() {
                return;
            }

            let AddTarget::Song(song) = &intent.target else {
                return;
            };

            let Some(server) = servers().into_iter().find(|s| s.id == song.server_id) else {
                message.set(Some((false, "Song server is not available.".to_string())));
                return;
            };

            let seed_song = song.clone();
            let seed_id = song.id.clone();
            let seed_genre = song.genre.clone();
            let seed_artist_id = song.artist_id.clone();
            let mut message = message.clone();
            let mut controller = controller.clone();
            is_processing.set(true);
            processing_label.set(Some("Building similar mix...".to_string()));
            spawn(async move {
                let client = NavidromeClient::new(server);
                let mut similar = client
                    .get_similar_songs(&seed_id, 50)
                    .await
                    .unwrap_or_default();

                // Fallbacks for servers without Last.fm similar-song support.
                if similar.is_empty() {
                    let random_pool = client.get_random_songs(80).await.unwrap_or_default();
                    if let Some(genre) = seed_genre.as_deref() {
                        let genre_lower = genre.to_lowercase();
                        similar = random_pool
                            .iter()
                            .filter(|song| {
                                song.genre
                                    .as_ref()
                                    .map(|value| value.to_lowercase() == genre_lower)
                                    .unwrap_or(false)
                            })
                            .cloned()
                            .collect();
                    }
                    if similar.is_empty() {
                        similar = random_pool;
                    }
                }

                if similar.is_empty() {
                    if let Some(artist_id) = seed_artist_id.as_deref() {
                        if let Ok((_, albums)) = client.get_artist(artist_id).await {
                            for album in albums.into_iter().take(6) {
                                if let Ok((_, mut album_songs)) = client.get_album(&album.id).await
                                {
                                    similar.append(&mut album_songs);
                                }
                                if similar.len() >= 50 {
                                    break;
                                }
                            }
                        }
                    }
                }

                if similar.is_empty() {
                    similar = client.get_random_songs(50).await.unwrap_or_default();
                }

                let seed_key = format!("{}::{}", seed_song.server_id, seed_song.id);
                let mut seen = std::collections::HashSet::new();
                let mut mix = Vec::new();
                seen.insert(seed_key.clone());
                mix.push(seed_song.clone());

                for track in similar {
                    let track_key = format!("{}::{}", track.server_id, track.id);
                    if track_key == seed_key {
                        continue;
                    }
                    if seen.insert(track_key) {
                        mix.push(track);
                    }
                    if mix.len() >= 50 {
                        break;
                    }
                }

                if mix.len() <= 1 {
                    message.set(Some((
                        false,
                        "Could not find enough similar songs for this track.".to_string(),
                    )));
                } else {
                    queue.set(mix.clone());
                    queue_index.set(0);
                    now_playing.set(Some(mix[0].clone()));
                    is_playing.set(true);
                    controller.close();
                }
                processing_label.set(None);
                is_processing.set(false);
            });
        }
    };

    let on_quick_add_suggestion = {
        let servers = servers.clone();
        let mut queue = queue.clone();
        let mut is_processing = is_processing.clone();
        let mut processing_label = processing_label.clone();
        let mut message = message.clone();
        let suggestion_destination = suggestion_destination.clone();
        let mut suggestion_candidates = suggestion_candidates.clone();
        let mut suggestions_loading = suggestions_loading.clone();
        move |song: Song| {
            if is_processing() || suggestions_loading() {
                return;
            }
            let Some(destination) = suggestion_destination() else {
                return;
            };

            is_processing.set(true);
            processing_label.set(Some("Quick adding suggestion...".to_string()));
            let servers_snapshot = servers();
            let song_to_add = song.clone();
            spawn(async move {
                let quick_add_result: Result<(), String> = match destination.clone() {
                    SuggestionDestination::Queue => {
                        queue.with_mut(|items| items.push(song_to_add.clone()));
                        Ok(())
                    }
                    SuggestionDestination::Playlist {
                        playlist_id,
                        server_id,
                    } => match servers_snapshot.iter().find(|s| s.id == server_id).cloned() {
                        Some(server) => {
                            let client = NavidromeClient::new(server);
                            client
                                .add_songs_to_playlist(&playlist_id, &[song_to_add.id.clone()])
                                .await
                        }
                        None => Err("Playlist server is not available.".to_string()),
                    },
                };

                match quick_add_result {
                    Ok(_) => {
                        message.set(Some((
                            true,
                            format!("Quick added \"{}\".", song_to_add.title),
                        )));
                        suggestions_loading.set(true);
                        let mut follow_up =
                            fetch_similar_songs_for_seed(&servers_snapshot, &song_to_add, 8).await;
                        if let SuggestionDestination::Playlist { server_id, .. } = destination {
                            follow_up.retain(|candidate| candidate.server_id == server_id);
                        }
                        suggestion_candidates.set(follow_up);
                        suggestions_loading.set(false);
                    }
                    Err(err) => {
                        message.set(Some((false, format!("Quick add failed: {err}"))));
                    }
                }
                processing_label.set(None);
                is_processing.set(false);
            });
        }
    };

    (
        make_add_to_queue,
        make_add_to_playlist,
        create_playlist,
        on_open_playlist_picker,
        on_create_similar,
        on_quick_add_suggestion,
    )
}
