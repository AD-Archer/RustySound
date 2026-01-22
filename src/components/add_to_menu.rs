use crate::api::*;
use crate::components::{Icon, Navigation, AppView};
use dioxus::prelude::*;

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

#[component]
pub fn AddToMenuOverlay(controller: AddMenuController) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();

    let show_playlist_picker = use_signal(|| false);
    let mut playlist_filter = use_signal(String::new);
    let mut new_playlist_name = use_signal(String::new);
    let is_processing = use_signal(|| false);
    let message = use_signal(|| None::<(bool, String)>);

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

                let client = NavidromeClient::new(active[0].clone());
                client.get_playlists().await.unwrap_or_default()
            }
        })
    };

    // Reset picker state whenever a new intent opens
    {
        let mut show_playlist_picker = show_playlist_picker.clone();
        let mut new_playlist_name = new_playlist_name.clone();
        let mut message = message.clone();
        let mut is_processing = is_processing.clone();
        let controller = controller.clone();
        use_effect(move || {
            if controller.current().is_some() {
                show_playlist_picker.set(false);
                new_playlist_name.set(String::new());
                message.set(None);
                is_processing.set(false);
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
    let active_server_for_similar = active_server.clone();

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
        move |_| controller.close()
    };

    let navigation = use_context::<Navigation>();
    let on_cover_click = {
        let navigation = navigation.clone();
        let mut controller = controller.clone();
        let intent = intent_for_display.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let AddTarget::Album { album_id, server_id, .. } = &intent.target {
                controller.close();
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id.clone(),
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let enqueue_items = |mut queue: Signal<Vec<Song>>, items: Vec<Song>, mode: &str| {
        queue.with_mut(|q| match mode {
            "next" => {
                let insert_at = 1.min(q.len());
                for (idx, song) in items.into_iter().enumerate() {
                    q.insert(insert_at + idx, song);
                }
            }
            _ => q.extend(items),
        });
    };

    let make_add_to_queue = |mode: &'static str| {
        let mut controller = controller.clone();
        let servers = servers.clone();
        let queue = queue.clone();
        let mut is_processing = is_processing.clone();
        let mut message = message.clone();
        let intent = intent_for_queue.clone();

        move |_| {
            if is_processing() {
                return;
            }

            match intent.target.clone() {
                AddTarget::Song(song) => {
                    enqueue_items(queue.clone(), vec![song], mode);
                    controller.close();
                }
                AddTarget::Songs(songs) => {
                    enqueue_items(queue.clone(), songs, mode);
                    controller.close();
                }
                AddTarget::Album { album_id, server_id, .. } => {
                    let server = servers().into_iter().find(|s| s.id == server_id);
                    if let Some(server) = server {
                        let album_id = album_id.clone();
                        is_processing.set(true);
                        let queue = queue.clone();
                        let mut controller = controller.clone();
                        let mut message = message.clone();
                        spawn(async move {
                            let client = NavidromeClient::new(server);
                            match client.get_album(&album_id).await {
                                Ok((_, songs)) => {
                                    enqueue_items(queue.clone(), songs, mode);
                                    controller.close();
                                }
                                Err(err) => {
                                    message.set(Some((
                                        false,
                                        format!("Failed to load album: {err}"),
                                    )));
                                }
                            }
                            is_processing.set(false);
                        });
                    } else {
                        message.set(Some((
                            false,
                            "Server is not available for this album.".to_string(),
                        )));
                    }
                }
                AddTarget::Playlist { playlist_id, server_id, .. } => {
                    let server = servers().into_iter().find(|s| s.id == server_id);
                    if let Some(server) = server {
                        let playlist_id = playlist_id.clone();
                        is_processing.set(true);
                        let queue = queue.clone();
                        let mut controller = controller.clone();
                        let mut message = message.clone();
                        spawn(async move {
                            let client = NavidromeClient::new(server);
                            match client.get_playlist(&playlist_id).await {
                                Ok((_, songs)) => {
                                    enqueue_items(queue.clone(), songs, mode);
                                    controller.close();
                                }
                                Err(err) => {
                                    message.set(Some((
                                        false,
                                        format!("Failed to load playlist: {err}"),
                                    )));
                                }
                            }
                            is_processing.set(false);
                        });
                    } else {
                        message.set(Some((
                            false,
                            "Server is not available for this playlist.".to_string(),
                        )));
                    }
                }
            }
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

        move |playlist_id: String| {
            let servers = servers.clone();
            let mut is_processing = is_processing.clone();
            let mut message = message.clone();
            let mut show_playlist_picker = show_playlist_picker.clone();
            let intent = intent.clone();
            let active_server = active_server.clone();
            let controller = controller.clone();

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
                is_processing.set(true);
                let mut controller = controller.clone();
                spawn(async move {
                    let client = NavidromeClient::new(active);
                    let result = match target {
                        AddTarget::Song(song) => {
                            client
                                .add_songs_to_playlist(&playlist_id_for_fetch, &[song.id])
                                .await
                        }
                        AddTarget::Songs(songs) => {
                            let ids: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
                            client
                                .add_songs_to_playlist(&playlist_id_for_fetch, &ids)
                                .await
                        }
                        AddTarget::Album { album_id, .. } => {
                            client
                                .add_album_to_playlist(&playlist_id_for_fetch, &album_id)
                                .await
                        }
                        AddTarget::Playlist {
                            playlist_id: from, ..
                        } => {
                            client
                                .add_playlist_to_playlist(&from, &playlist_id_for_fetch)
                                .await
                        }
                    };

                    match result {
                        Ok(_) => {
                            controller.close();
                        }
                        Err(err) => {
                            message.set(Some((false, format!("Unable to add: {err}"))));
                            show_playlist_picker.set(true);
                        }
                    }
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
                // Collect song ids first so we can force-add after creation.
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
                    Ok(ids) => match client.create_playlist(&name, None, &ids).await {
                        Ok(created_id) => {
                            // Some servers ignore songId on create; ensure songs are added if we got an id.
                            if let (Some(pid), true) = (created_id, !ids.is_empty()) {
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
        let mut is_processing = is_processing.clone();
        let mut message = message.clone();
        let intent = intent_for_similar.clone();
        let active_server = active_server_for_similar.clone();

        move |_| {
            if is_processing() {
                return;
            }

            let AddTarget::Song(song) = &intent.target else {
                return;
            };

            if let Some(reason) = requires_single_server(&intent.target, &active_server) {
                message.set(Some((false, reason)));
                return;
            }

            let Some(active) = servers().into_iter().find(|s| s.active) else {
                message.set(Some((false, "No active server found.".to_string())));
                return;
            };

            let seed_id = song.id.clone();
            let mut message = message.clone();
            let mut controller = controller.clone();
            is_processing.set(true);
            spawn(async move {
                let client = NavidromeClient::new(active);
                match client.create_similar_playlist(&seed_id, None, 40).await {
                    Ok(_) => controller.close(),
                    Err(err) => message.set(Some((false, format!("Could not create mix: {err}")))),
                }
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
                    p { class: "text-sm text-zinc-400", "No playlists found on the active server." }
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
        div { class: "fixed inset-0 z-50 flex items-end md:items-center justify-center bg-black/60 backdrop-blur-sm px-3",
            div { class: "w-full md:max-w-lg bg-zinc-900/95 border border-zinc-800 rounded-t-2xl md:rounded-2xl shadow-2xl p-5 space-y-5",
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
                        class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors",
                        onclick: on_close,
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

                if show_playlist_picker() {
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
                }
            }
        }
    }
}
