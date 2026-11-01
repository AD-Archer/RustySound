// Main add menu component split into setup, actions, and rendering chunks.
#[component]
pub fn AddToMenuOverlay(controller: AddMenuController) -> Element {
    let servers = use_servers();
    let player = use_player();
    let preview = use_context::<Preview>();
    let toast = use_context::<StatusToast>();
    let offline = use_setting(|s| s.offline_mode);

    let show_playlist_picker = use_signal(|| false);
    let mut playlist_filter = use_signal(String::new);
    let mut new_playlist_name = use_signal(String::new);
    let is_processing = use_signal(|| false);
    let processing_label = use_signal(|| None::<String>);
    let suggestion_destination = use_signal(|| None::<SuggestionDestination>);
    let suggestion_candidates = use_signal(Vec::<Song>::new);
    let suggestions_loading = use_signal(|| false);
    let was_open = use_signal(|| false);

    let playlists = {
        let controller = controller.clone();
        let servers = servers.clone();
        use_resource(move || {
            let intent_is_open = controller.current().is_some();
            let servers = servers();
            async move {
                if !intent_is_open {
                    return Vec::new();
                }

                let active: Vec<_> = servers.into_iter().filter(|s| s.active).collect();
                if active.len() != 1 {
                    return Vec::new();
                }

                let active_server = active[0].clone();
                let username = active_server.username.trim().to_lowercase();
                let client = NavidromeClient::new(active_server);
                client
                    .get_playlists()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|playlist| {
                        let owned_by_user = playlist
                            .owner
                            .as_ref()
                            .map(|owner| owner.trim().eq_ignore_ascii_case(&username))
                            .unwrap_or(false);
                        let is_auto_imported = playlist
                            .comment
                            .as_ref()
                            .map(|comment| comment.to_lowercase().contains("auto-imported"))
                            .unwrap_or(false);
                        owned_by_user && !is_auto_imported
                    })
                    .collect()
            }
        })
    };

    // Reset picker state whenever a new intent opens
    {
        let mut show_playlist_picker = show_playlist_picker.clone();
        let mut new_playlist_name = new_playlist_name.clone();
        let mut is_processing = is_processing.clone();
        let mut processing_label = processing_label.clone();
        let mut suggestion_destination = suggestion_destination.clone();
        let mut suggestion_candidates = suggestion_candidates.clone();
        let mut suggestions_loading = suggestions_loading.clone();
        let mut was_open = was_open.clone();
        let controller = controller.clone();
        use_effect(move || {
            let is_open = controller.current().is_some();
            let previously_open = was_open();
            if is_open && !previously_open {
                show_playlist_picker.set(false);
                new_playlist_name.set(String::new());
                is_processing.set(false);
                processing_label.set(None);
                suggestion_destination.set(None);
                suggestion_candidates.set(Vec::new());
                suggestions_loading.set(false);
            }
            if previously_open != is_open {
                was_open.set(is_open);
            }
        });
    }

    let active_server = {
        let servers_snapshot = servers();
        let active: Vec<_> = servers_snapshot.into_iter().filter(|s| s.active).collect();
        if active.len() == 1 {
            Some(active[0].clone())
        } else {
            None
        }
    };
    let Some(intent) = controller.current() else {
        return rsx! {};
    };
    let intent_for_queue = intent.clone();
    let intent_for_playlist = intent.clone();
    let intent_for_create = intent.clone();
    let intent_for_similar = intent.clone();
    let intent_for_display = intent.clone();
    let active_server_for_playlist = active_server.clone();
    let active_server_for_create = active_server.clone();

    let requires_single_server =
        |target: &AddTarget, active: &Option<ServerConfig>| -> Option<String> {
            match (target, active) {
                (_, None) => Some("Playlist actions need exactly one active server.".to_string()),
                (AddTarget::Song(song), Some(server)) => {
                    if server.id != song.server_id {
                        Some("Activate the song's server to add it to a playlist.".to_string())
                    } else {
                        None
                    }
                }
                (AddTarget::Songs(songs), Some(server)) => {
                    let mismatched = songs.iter().any(|s| s.server_id != server.id);
                    if mismatched {
                        Some(
                            "All songs must come from the active server to add to a playlist."
                                .to_string(),
                        )
                    } else {
                        None
                    }
                }
                (AddTarget::Album { server_id, .. }, Some(server)) => {
                    if server.id != *server_id {
                        Some("Activate this album's server to add it to a playlist.".to_string())
                    } else {
                        None
                    }
                }
                (AddTarget::Playlist { server_id, .. }, Some(server)) => {
                    if server.id != *server_id {
                        Some("Activate this playlist's server to merge it.".to_string())
                    } else {
                        None
                    }
                }
            }
        };

    let playlist_guard = requires_single_server(&intent_for_display.target, &active_server);

    // Preview cover for album/playlist targets using the first song's art when available
    let preview_cover = {
        let intent = intent_for_display.clone();
        let servers = servers.clone();
        use_resource(move || {
            let intent = intent.clone();
            let servers = servers();
            async move {
                match intent.target {
                    AddTarget::Album {
                        album_id,
                        cover_art,
                        ref server_id,
                    } => {
                        let server = servers.iter().find(|s| s.id == *server_id).cloned();
                        let Some(server) = server else { return None };
                        let client = NavidromeClient::new(server);
                        if let Some(ca) = cover_art {
                            return Some(client.get_cover_art_url(&ca, 200));
                        }
                        if let Ok((_, songs)) = client.get_album(&album_id).await {
                            if let Some(song) = songs.first() {
                                if let Some(cover) = &song.cover_art {
                                    return Some(client.get_cover_art_url(cover, 180));
                                }
                            }
                        }
                        None
                    }
                    AddTarget::Playlist {
                        playlist_id,
                        cover_art,
                        ref server_id,
                    } => {
                        let server = servers.iter().find(|s| s.id == *server_id).cloned();
                        let Some(server) = server else { return None };
                        let client = NavidromeClient::new(server);
                        if let Some(ca) = cover_art {
                            return Some(client.get_cover_art_url(&ca, 200));
                        }
                        if let Ok((_, songs)) = client.get_playlist(&playlist_id).await {
                            if let Some(song) = songs.first() {
                                if let Some(cover) = &song.cover_art {
                                    return Some(client.get_cover_art_url(cover, 180));
                                }
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
        })
    };


    // Quick Listen: 30 seconds from the server, without touching the queue
    // (the same preview the track menu plays).
    let can_quick_listen = move |song: &Song| {
        !offline() && Capabilities::for_source(&servers.read(), &song.server_id).quick_listen
    };
    let quick_listening = move |song: &Song| {
        (preview.current)().as_deref() == Some(quick_listen_id(song).as_str())
    };
    let on_quick_listen = move |song: Song| preview.quick_listen(song, &servers.peek());

    let navigation = use_context::<Navigation>();
    let on_cover_click = {
        let navigation = navigation.clone();
        let mut controller = controller.clone();
        let intent = intent_for_display.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let AddTarget::Album {
                album_id,
                server_id,
                ..
            } = &intent.target
            {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id.clone(),
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let (
        make_add_to_queue,
        make_add_to_playlist,
        create_playlist,
        on_create_similar,
        on_quick_add_suggestion,
    ) = include!("overlay_actions.rs");

    include!("overlay_view.rs")
}
