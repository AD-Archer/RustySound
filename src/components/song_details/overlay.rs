// Full-screen song-details overlay split into state setup and RSX layout chunks.
#[component]
pub fn SongDetailsOverlay(controller: SongDetailsController) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let sidebar_open = use_context::<SidebarOpenSignal>().0;
    let app_settings = use_context::<Signal<AppSettings>>();
    let audio_state = use_context::<Signal<AudioState>>();
    let create_queue_busy = use_signal(|| false);
    let lyrics_search_title = use_signal(|| None::<String>);
    let lyrics_query_override = use_signal(|| None::<LyricsQuery>);
    let lyrics_candidate_search_term = use_signal(|| None::<String>);
    let lyrics_candidate_refresh_nonce = use_signal(|| 0u64);
    let lyrics_refresh_nonce = use_signal(|| 0u64);
    let lyrics_auto_retry_for_song = use_signal(|| None::<String>);
    let lrclib_upgrade_auto_retry_for_song = use_signal(|| None::<String>);
    let last_synced_lyrics_for_song = use_signal(|| None::<(String, LyricsResult)>);
    let last_song_key = use_signal(|| None::<String>);

    let state = controller.current();
    let selected_song = state.song.clone();
    let selected_song_key = selected_song
        .as_ref()
        .map(|song| format!("{}:{}", song.server_id, song.id));

    {
        let mut controller = controller.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let state = controller.current();
            if !state.is_open {
                return;
            }

            let Some(now_song) = now_playing() else {
                return;
            };

            let should_follow = state.song.as_ref() != Some(&now_song);

            if should_follow {
                controller.open(now_song);
            }
        });
    }

    {
        let mut lyrics_search_title = lyrics_search_title.clone();
        let mut lyrics_query_override = lyrics_query_override.clone();
        let mut lyrics_candidate_search_term = lyrics_candidate_search_term.clone();
        let mut lyrics_auto_retry_for_song = lyrics_auto_retry_for_song.clone();
        let mut lrclib_upgrade_auto_retry_for_song = lrclib_upgrade_auto_retry_for_song.clone();
        let mut last_synced_lyrics_for_song = last_synced_lyrics_for_song.clone();
        let selected_song_key = selected_song_key.clone();
        let mut last_song_key = last_song_key.clone();
        use_effect(move || {
            if last_song_key() != selected_song_key {
                last_song_key.set(selected_song_key.clone());
                lyrics_auto_retry_for_song.set(None);
                lrclib_upgrade_auto_retry_for_song.set(None);
                last_synced_lyrics_for_song.set(None);
                lyrics_search_title.set(None);
                lyrics_query_override.set(None);
                lyrics_candidate_search_term.set(None);
            }
        });
    }

    let related_resource = {
        let controller = controller.clone();
        use_resource(move || {
            let song = controller.current().song;
            let servers_snapshot = servers();
            async move { load_related_songs(song, servers_snapshot).await }
        })
    };

    let lyrics_resource = {
        let controller = controller.clone();
        let app_settings = app_settings.clone();
        let lyrics_query_override = lyrics_query_override.clone();
        let lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_resource(move || {
            let song = controller.current().song;
            let settings = app_settings();
            let query_override = lyrics_query_override();
            let _refresh_nonce = lyrics_refresh_nonce();
            async move {
                let Some(song) = song else {
                    return Err("No song selected.".to_string());
                };
                let query = query_override.unwrap_or_else(|| LyricsQuery::from_song(&song));
                fetch_first_available_lyrics(
                    query,
                    settings.lyrics_provider_order.clone(),
                    settings.lyrics_request_timeout_secs,
                )
                .await
            }
        })
    };

    {
        let selected_song_key = selected_song_key.clone();
        let mut lyrics_auto_retry_for_song = lyrics_auto_retry_for_song.clone();
        let mut lyrics_resource = lyrics_resource.clone();
        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_effect(move || {
            let latest_lyrics_result = lyrics_resource();
            let Some(song_key) = selected_song_key.clone() else {
                return;
            };

            let Some(Err(_)) = latest_lyrics_result else {
                return;
            };

            if lyrics_auto_retry_for_song() == Some(song_key.clone()) {
                return;
            }

            lyrics_auto_retry_for_song.set(Some(song_key));
            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
            lyrics_resource.restart();
        });
    }

    let lyrics_candidates_resource = {
        let controller = controller.clone();
        let app_settings = app_settings.clone();
        let lyrics_candidate_search_term = lyrics_candidate_search_term.clone();
        let lyrics_candidate_refresh_nonce = lyrics_candidate_refresh_nonce.clone();
        use_resource(move || {
            let song = controller.current().song;
            let settings = app_settings();
            let search_term = lyrics_candidate_search_term();
            let _refresh_nonce = lyrics_candidate_refresh_nonce();
            async move {
                let Some(song) = song else {
                    return Ok(Vec::<LyricsSearchCandidate>::new());
                };
                let Some(search_term) = search_term
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                else {
                    return Ok(Vec::<LyricsSearchCandidate>::new());
                };
                let mut query = LyricsQuery::from_song(&song);
                query.title = search_term;
                search_lyrics_candidates(
                    &query,
                    &settings.lyrics_provider_order,
                    settings.lyrics_request_timeout_secs,
                )
                .await
            }
        })
    };
    let lrclib_upgrade_resource = {
        let controller = controller.clone();
        let app_settings = app_settings.clone();
        let lyrics_query_override = lyrics_query_override.clone();
        let lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_resource(move || {
            let song = controller.current().song;
            let settings = app_settings();
            let query_override = lyrics_query_override();
            let _refresh_nonce = lyrics_refresh_nonce();
            async move {
                if settings.lyrics_unsynced_mode {
                    return Ok(None);
                }
                let Some(song) = song else {
                    return Ok(None);
                };
                let query = query_override.unwrap_or_else(|| LyricsQuery::from_song(&song));
                match fetch_lyrics_with_fallback(
                    &query,
                    &["lrclib".to_string()],
                    settings.lyrics_request_timeout_secs,
                )
                .await
                {
                    Ok(result) => Ok(Some(result)),
                    Err(err) => Err(err),
                }
            }
        })
    };
    {
        let selected_song_key = selected_song_key.clone();
        let mut last_synced_lyrics_for_song = last_synced_lyrics_for_song.clone();
        let lrclib_upgrade_resource = lrclib_upgrade_resource.clone();
        use_effect(move || {
            let Some(song_key) = selected_song_key.clone() else {
                return;
            };
            let Some(Ok(Some(upgrade))) = lrclib_upgrade_resource() else {
                return;
            };
            if upgrade.synced_lines.is_empty() {
                return;
            }
            let should_update = last_synced_lyrics_for_song()
                .as_ref()
                .map(|(cached_key, cached)| cached_key != &song_key || cached != &upgrade)
                .unwrap_or(true);
            if should_update {
                last_synced_lyrics_for_song.set(Some((song_key, upgrade)));
            }
        });
    }
    {
        let selected_song_key = selected_song_key.clone();
        let app_settings = app_settings.clone();
        let mut lrclib_upgrade_resource = lrclib_upgrade_resource.clone();
        let mut lrclib_upgrade_auto_retry_for_song = lrclib_upgrade_auto_retry_for_song.clone();
        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
        use_effect(move || {
            if app_settings().lyrics_unsynced_mode {
                return;
            }

            let latest_upgrade_result = lrclib_upgrade_resource();
            let Some(song_key) = selected_song_key.clone() else {
                return;
            };

            let Some(Err(_)) = latest_upgrade_result else {
                return;
            };

            if lrclib_upgrade_auto_retry_for_song() == Some(song_key.clone()) {
                return;
            }

            lrclib_upgrade_auto_retry_for_song.set(Some(song_key));
            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
            lrclib_upgrade_resource.restart();
        });
    }

    if !state.is_open {
        return rsx! {};
    }

    let Some(song) = selected_song else {
        return rsx! {};
    };
    let is_live_stream = is_live_song(&song);

    let settings = app_settings();
    let sync_lyrics = !settings.lyrics_unsynced_mode;
    let cached_synced_lyrics = last_synced_lyrics_for_song().and_then(|(song_key, lyrics)| {
        if selected_song_key.as_ref() == Some(&song_key) {
            Some(lyrics)
        } else {
            None
        }
    });
    let selected_lyrics = pick_display_lyrics(
        sync_lyrics,
        lyrics_resource(),
        lrclib_upgrade_resource(),
        cached_synced_lyrics,
    );

    let desktop_tab = match state.active_tab {
        SongDetailsTab::Details => SongDetailsTab::Lyrics,
        other => other,
    };

    let cover_url = song_cover_url(&song, &servers(), 700);

    let queue_snapshot = queue();
    let current_queue_index = queue_index();
    let up_next = queue_snapshot
        .into_iter()
        .enumerate()
        .filter(|(index, _)| *index > current_queue_index)
        .take(60)
        .collect::<Vec<_>>();

    let current_time = (audio_state().current_time)();
    let offset_seconds = settings.lyrics_offset_ms as f64 / 1000.0;
    let mini_lyrics_preview = build_mini_lyrics_preview(
        selected_lyrics.clone(),
        sync_lyrics,
        current_time,
        offset_seconds,
    );

    let song_title = if song.title.trim().is_empty() {
        "Unknown Song".to_string()
    } else {
        song.title.clone()
    };
    let on_open_song_actions = {
        let mut add_menu = add_menu.clone();
        let song = song.clone();
        move |_| {
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };

    {
        let mut controller = controller.clone();
        use_effect(move || {
            if !is_live_stream {
                return;
            }
            if controller.current().active_tab == SongDetailsTab::Queue {
                controller.set_tab(SongDetailsTab::Lyrics);
            }
        });
    }


    include!("overlay_view.rs")
}
