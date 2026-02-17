use crate::api::*;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;
use std::collections::HashSet;

#[derive(Clone)]
#[allow(dead_code)]
pub enum AddTarget {
    Song(Song),
    Songs(Vec<Song>),
    Album {
        album_id: String,
        server_id: String,
        cover_art: Option<String>,
    },
    Playlist {
        playlist_id: String,
        server_id: String,
        cover_art: Option<String>,
    },
}

#[derive(Clone)]
pub struct AddIntent {
    pub target: AddTarget,
    pub label: String,
}

impl AddIntent {
    pub fn from_song(song: Song) -> Self {
        Self {
            label: song.title.clone(),
            target: AddTarget::Song(song),
        }
    }

    #[allow(dead_code)]
    pub fn from_songs(label: String, songs: Vec<Song>) -> Self {
        Self {
            label,
            target: AddTarget::Songs(songs),
        }
    }

    pub fn from_album(album: &Album) -> Self {
        Self {
            label: album.name.clone(),
            target: AddTarget::Album {
                album_id: album.id.clone(),
                server_id: album.server_id.clone(),
                cover_art: album.cover_art.clone(),
            },
        }
    }

    pub fn from_playlist(playlist: &Playlist) -> Self {
        Self {
            label: playlist.name.clone(),
            target: AddTarget::Playlist {
                playlist_id: playlist.id.clone(),
                server_id: playlist.server_id.clone(),
                cover_art: playlist.cover_art.clone(),
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct AddMenuController {
    pub intent: Signal<Option<AddIntent>>,
}

impl AddMenuController {
    pub fn new(intent: Signal<Option<AddIntent>>) -> Self {
        Self { intent }
    }

    pub fn open(&mut self, intent: AddIntent) {
        self.intent.set(Some(intent));
    }

    pub fn close(&mut self) {
        self.intent.set(None);
    }

    pub fn current(&self) -> Option<AddIntent> {
        (self.intent)()
    }
}

#[derive(Clone, PartialEq)]
enum SuggestionDestination {
    Queue,
    Playlist {
        playlist_id: String,
        server_id: String,
    },
}

fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

async fn resolve_target_songs(
    servers: &[ServerConfig],
    target: &AddTarget,
) -> Result<Vec<Song>, String> {
    match target {
        AddTarget::Song(song) => Ok(vec![song.clone()]),
        AddTarget::Songs(songs) => Ok(songs.clone()),
        AddTarget::Album {
            album_id,
            server_id,
            ..
        } => {
            let Some(server) = servers.iter().find(|s| s.id == *server_id).cloned() else {
                return Err("Server is not available for this album.".to_string());
            };
            let client = NavidromeClient::new(server);
            client
                .get_album(album_id)
                .await
                .map(|(_, songs)| songs)
                .map_err(|err| format!("Failed to load album: {err}"))
        }
        AddTarget::Playlist {
            playlist_id,
            server_id,
            ..
        } => {
            let Some(server) = servers.iter().find(|s| s.id == *server_id).cloned() else {
                return Err("Server is not available for this playlist.".to_string());
            };
            let client = NavidromeClient::new(server);
            client
                .get_playlist(playlist_id)
                .await
                .map(|(_, songs)| songs)
                .map_err(|err| format!("Failed to load playlist: {err}"))
        }
    }
}

async fn fetch_similar_songs_for_seed(
    servers: &[ServerConfig],
    seed: &Song,
    count: usize,
) -> Vec<Song> {
    if count == 0 {
        return Vec::new();
    }

    let Some(server) = servers.iter().find(|s| s.id == seed.server_id).cloned() else {
        return Vec::new();
    };

    let client = NavidromeClient::new(server);
    let lookup_count = (count as u32).saturating_mul(4).max(count as u32);
    let mut similar = client
        .get_similar_songs(&seed.id, lookup_count)
        .await
        .unwrap_or_default();

    if similar.is_empty() {
        similar = client
            .get_similar_songs2(&seed.id, lookup_count)
            .await
            .unwrap_or_default();
    }

    if similar.is_empty() {
        similar = client
            .get_random_songs((count as u32).saturating_mul(6).max(20))
            .await
            .unwrap_or_default();
    }

    let seed_key = song_key(seed);
    let mut seen = HashSet::<String>::new();
    let mut output = Vec::<Song>::new();
    for song in similar {
        let key = song_key(&song);
        if key == seed_key {
            continue;
        }
        if seen.insert(key) {
            output.push(song);
        }
        if output.len() >= count {
            break;
        }
    }

    output
}

async fn build_dual_seed_suggestions(
    servers: &[ServerConfig],
    first_seed: Option<Song>,
    recent_seed: Option<Song>,
) -> Vec<Song> {
    let mut suggestions = Vec::<Song>::new();
    let mut seen = HashSet::<String>::new();

    if let Some(seed) = first_seed {
        for song in fetch_similar_songs_for_seed(servers, &seed, 4).await {
            let key = song_key(&song);
            if seen.insert(key) {
                suggestions.push(song);
            }
        }
    }

    if let Some(seed) = recent_seed {
        for song in fetch_similar_songs_for_seed(servers, &seed, 4).await {
            let key = song_key(&song);
            if seen.insert(key) {
                suggestions.push(song);
            }
        }
    }

    suggestions.truncate(8);
    suggestions
}

#[component]
pub fn AddToMenuOverlay(controller: AddMenuController) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();

    let show_playlist_picker = use_signal(|| false);
    let mut playlist_filter = use_signal(String::new);
    let mut new_playlist_name = use_signal(String::new);
    let is_processing = use_signal(|| false);
    let processing_label = use_signal(|| None::<String>);
    let message = use_signal(|| None::<(bool, String)>);
    let suggestion_destination = use_signal(|| None::<SuggestionDestination>);
    let suggestion_candidates = use_signal(Vec::<Song>::new);
    let suggestions_loading = use_signal(|| false);

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
        let mut message = message.clone();
        let mut is_processing = is_processing.clone();
        let mut processing_label = processing_label.clone();
        let mut suggestion_destination = suggestion_destination.clone();
        let mut suggestion_candidates = suggestion_candidates.clone();
        let mut suggestions_loading = suggestions_loading.clone();
        let controller = controller.clone();
        use_effect(move || {
            if controller.current().is_some() {
                show_playlist_picker.set(false);
                new_playlist_name.set(String::new());
                message.set(None);
                is_processing.set(false);
                processing_label.set(None);
                suggestion_destination.set(None);
                suggestion_candidates.set(Vec::new());
                suggestions_loading.set(false);
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

    let on_close = {
        let mut controller = controller.clone();
        let is_processing = is_processing.clone();
        move |_| {
            if *is_processing.peek() {
                return;
            }
            controller.close()
        }
    };

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

    let enqueue_items =
        |mut queue: Signal<Vec<Song>>, queue_index: Signal<usize>, items: Vec<Song>, mode: &str| {
            queue.with_mut(|q| match mode {
                "next" => {
                    let insert_at = queue_index().saturating_add(1).min(q.len());
                    for (idx, song) in items.into_iter().enumerate() {
                        q.insert(insert_at + idx, song);
                    }
                }
                _ => q.extend(items),
            });
        };

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

    let render_playlist_picker = || {
        let loading = playlists().is_none();
        let available = playlists().unwrap_or_default();
        let filter = playlist_filter().to_lowercase();
        let mut filtered: Vec<Playlist> = if filter.is_empty() {
            available
        } else {
            available
                .into_iter()
                .filter(|p| p.name.to_lowercase().contains(&filter))
                .collect()
        };
        let total_filtered = filtered.len();
        let limit = 40usize;
        let limited: Vec<Playlist> = filtered.drain(..).take(limit).collect();
        let truncated = total_filtered > limited.len();
        let servers_list = servers();
        rsx! {
            div { class: "space-y-4",
                h3 { class: "text-lg font-semibold text-white", "Add to playlist" }
                input {
                    class: "w-full px-3 py-2 rounded-lg bg-zinc-900/50 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                    placeholder: "Search playlists",
                    value: playlist_filter,
                    oninput: move |e| playlist_filter.set(e.value()),
                }
                if let Some(reason) = playlist_guard.clone() {
                    div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm",
                        "{reason}"
                    }
                } else if loading {
                    div { class: "flex items-center gap-2 text-sm text-zinc-400",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-4 h-4 animate-spin".to_string(),
                        }
                        "Loading playlists..."
                    }
                } else if limited.is_empty() {
                    p { class: "text-sm text-zinc-400", "No user-created playlists found on the active server." }
                } else {
                    div { class: "max-h-56 overflow-y-auto space-y-2 pr-1",
                        for playlist in limited {
                            button {
                                class: "w-full px-3 py-2 rounded-xl bg-zinc-900/50 border border-zinc-800 hover:border-emerald-500/60 hover:text-white text-left text-sm text-zinc-300 transition-colors flex items-center gap-3",
                                onclick: make_add_to_playlist(playlist.id.clone()),
                                if let Some(url) = playlist
                                    .cover_art
                                    .as_ref()
                                    .and_then(|cover| {
                                        servers_list
                                            .iter()
                                            .find(|s| s.id == playlist.server_id)
                                            .map(|srv| {
                                                NavidromeClient::new(srv.clone()).get_cover_art_url(cover, 80)
                                            })
                                    })
                                {
                                    img {
                                        class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80",
                                        src: "{url}",
                                        alt: "Playlist art",
                                    }
                                } else {
                                    div { class: "w-10 h-10 rounded-md bg-zinc-800/70 border border-zinc-800/80 flex items-center justify-center",
                                        Icon {
                                            name: "playlist".to_string(),
                                            class: "w-4 h-4 text-zinc-500".to_string(),
                                        }
                                    }
                                }
                                div { class: "min-w-0",
                                    div { class: "font-medium truncate", "{playlist.name}" }
                                    p { class: "text-xs text-zinc-500", "{playlist.song_count} songs" }
                                }
                            }
                        }
                        if truncated {
                            p { class: "text-xs text-zinc-500 pt-1",
                                "Showing first {limit} playlists"
                            }
                        }
                    }
                }
                div { class: "space-y-2 pt-2 border-t border-zinc-800",
                    label { class: "text-xs uppercase tracking-wide text-zinc-500", "Create new" }
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            class: "flex-1 px-3 py-2 rounded-lg bg-zinc-900/50 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "Playlist name",
                            value: new_playlist_name,
                            oninput: move |e| new_playlist_name.set(e.value()),
                        }
                        button {
                            class: if is_processing() { "px-4 py-2 rounded-lg bg-emerald-500/60 text-white cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-lg bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center gap-2" },
                            onclick: create_playlist,
                            disabled: is_processing(),
                            if is_processing() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 animate-spin".to_string(),
                                }
                                "Working..."
                            } else {
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Create"
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "fixed inset-0 z-[95] flex items-end md:items-center justify-center bg-black/60 backdrop-blur-sm px-3 pb-20 md:pb-0 pt-3 md:pt-0",
            div { class: "w-full md:max-w-xl max-h-[82vh] overflow-y-auto bg-zinc-900/95 border border-zinc-800 rounded-2xl shadow-2xl p-5 space-y-5",
                div { class: "flex items-center justify-between gap-3",
                    div { class: "flex items-center gap-3 min-w-0",
                        if let Some(Some(cover)) = preview_cover() {
                            img {
                                class: "w-12 h-12 rounded-lg object-cover border border-zinc-800/80 cursor-pointer",
                                src: "{cover}",
                                alt: "Cover",
                                onclick: on_cover_click,
                            }
                        } else {
                            div { class: "w-12 h-12 rounded-lg bg-zinc-800/70 border border-zinc-800/80 flex items-center justify-center",
                                Icon {
                                    name: "playlist".to_string(),
                                    class: "w-5 h-5 text-zinc-500".to_string(),
                                }
                            }
                        }
                        div { class: "min-w-0",
                            p { class: "text-xs uppercase tracking-wide text-zinc-500",
                                "Add options"
                            }
                            h2 { class: "text-lg font-semibold text-white truncate",
                                "{intent.label}"
                            }
                        }
                    }
                    button {
                        class: if is_processing() { "p-2 rounded-lg text-zinc-600 cursor-not-allowed" } else { "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors" },
                        onclick: on_close,
                        disabled: is_processing(),
                        Icon {
                            name: "x".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                    }
                }

                if let Some((is_success, text)) = message() {
                    div { class: if is_success { "p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/40 text-emerald-200 text-sm" } else { "p-3 rounded-lg bg-red-500/10 border border-red-500/40 text-red-200 text-sm" },
                        "{text}"
                    }
                }

                if is_processing() {
                    div { class: "min-h-44 flex flex-col items-center justify-center gap-4 text-center",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-amber-300 animate-spin".to_string(),
                        }
                        p { class: "text-sm text-zinc-300",
                            "{processing_label().unwrap_or_else(|| \"Working...\".to_string())}"
                        }
                        p { class: "text-xs text-zinc-500",
                            "Please wait while RustySound builds your queue."
                        }
                    }
                } else if show_playlist_picker() {
                    {render_playlist_picker()}
                } else {
                    div { class: "space-y-3",
                        div { class: "w-full grid grid-cols-1 sm:grid-cols-2 gap-2",
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors",
                                onclick: make_add_to_queue("end"),
                                disabled: is_processing(),
                                span { "Add to queue (end)" }
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                                onclick: make_add_to_queue("next"),
                                disabled: is_processing(),
                                span { "Play next" }
                                Icon {
                                    name: "chevron-right".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                            onclick: on_open_playlist_picker,
                            disabled: is_processing(),
                            span { "Add to playlist" }
                            Icon {
                                name: "playlist".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        if matches!(intent_for_display.target, AddTarget::Song(_)) {
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                                onclick: on_create_similar,
                                disabled: is_processing(),
                                span { "Create similar mix" }
                                Icon {
                                    name: "shuffle".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                        }
                    }
                    if let Some(reason) = playlist_guard {
                        div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm",
                            "{reason}"
                        }
                    }
                    if suggestion_destination().is_some() {
                        div { class: "pt-3 border-t border-zinc-800 space-y-3",
                            div { class: "flex items-center justify-between",
                                h3 { class: "text-sm font-semibold text-zinc-200", "Suggested additions" }
                                span { class: "text-xs text-zinc-500", "4 + 4 seed suggestions" }
                            }
                            p { class: "text-xs text-zinc-500",
                                "Quick Add adds the song and refreshes this list with more similar picks."
                            }
                            if suggestions_loading() {
                                div { class: "flex items-center gap-2 text-xs text-zinc-400",
                                    Icon {
                                        name: "loader".to_string(),
                                        class: "w-4 h-4 animate-spin".to_string(),
                                    }
                                    "Loading suggestions..."
                                }
                            } else if suggestion_candidates().is_empty() {
                                p { class: "text-xs text-zinc-500", "No similar songs found yet." }
                            } else {
                                div { class: "max-h-64 overflow-y-auto space-y-2 pr-1",
                                    for song in suggestion_candidates() {
                                        button {
                                            class: "w-full p-3 rounded-xl bg-zinc-900/60 border border-zinc-800 hover:border-emerald-500/50 text-left transition-colors",
                                            onclick: {
                                                let song = song.clone();
                                                let mut on_quick_add_suggestion =
                                                    on_quick_add_suggestion.clone();
                                                move |_| on_quick_add_suggestion(song.clone())
                                            },
                                            div { class: "flex items-center justify-between gap-3",
                                                div { class: "min-w-0",
                                                    p { class: "text-sm text-white truncate", "{song.title}" }
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.clone().unwrap_or_else(|| \"Unknown Artist\".to_string())}"
                                                    }
                                                }
                                                span { class: "px-2 py-1 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-xs",
                                                    "Quick add"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
