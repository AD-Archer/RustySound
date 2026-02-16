use crate::api::*;
use crate::components::{
    AddIntent, AddMenuController, AppView, Icon, Navigation, SongDetailsController,
};
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[component]
fn PlaylistSongRow(
    song: Song,
    display_index: usize,
    songs: Vec<Song>,
    queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    is_playing: Signal<bool>,
    servers: Signal<Vec<ServerConfig>>,
    add_menu: AddMenuController,
) -> Element {
    let song_details = use_context::<SongDetailsController>();
    let rating = song.user_rating.unwrap_or(0).min(5);
    let is_favorited = use_signal(|| song.starred.is_some());
    let is_current = now_playing()
        .as_ref()
        .map(|current| current.id == song.id)
        .unwrap_or(false);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 80))
        });

    let on_open_menu = {
        let mut add_menu = add_menu.clone();
        let song = song.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            add_menu.open(AddIntent::from_song(song.clone()));
        }
    };

    let on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let mut is_favorited = is_favorited.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let should_star = !is_favorited();
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&song_id, "song").await
                    } else {
                        client.unstar(&song_id, "song").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                }
            });
        }
    };

    let on_click_row = {
        let song = song.clone();
        let songs_for_queue = songs.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            queue.set(songs_for_queue.clone());
            queue_index.set(display_index - 1);
            now_playing.set(Some(song.clone()));
            is_playing.set(true);
        }
    };

    rsx! {
        div {
            class: if is_current {
                "w-full flex items-center gap-4 p-3 rounded-xl bg-emerald-500/5 transition-colors group cursor-pointer"
            } else {
                "w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer"
            },
            onclick: on_click_row,
            if is_current {
                span { class: "w-6 text-sm text-emerald-400",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            } else {
                span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{display_index}" }
                span { class: "w-6 text-sm text-white hidden group-hover:block",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            }
            button {
                class: "w-12 h-12 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                aria_label: "Open song menu",
                onclick: {
                    let song = song.clone();
                    let mut song_details = song_details.clone();
                    move |evt: MouseEvent| {
                        evt.stop_propagation();
                        song_details.open(song.clone());
                    }
                },
                match cover_url {
                    Some(url) => rsx! {
                        img { class: "w-full h-full object-cover", src: "{url}" }
                    },
                    None => rsx! {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                            Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                        }
                    },
                }
            }
            div { class: "flex-1 min-w-0 text-center md:text-left",
                p { class: if is_current { "text-sm font-medium text-emerald-400 truncate transition-colors" } else { "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors" },
                    "{song.title}"
                }
                p { class: "text-xs text-zinc-400 truncate",
                    "{song.album.clone().unwrap_or_default()}"
                }
            }
            div { class: "hidden sm:flex items-center gap-1 text-amber-400",
                for i in 1..=5 {
                    Icon {
                        name: if i <= rating { "star-filled".to_string() } else { "star".to_string() },
                        class: "w-3.5 h-3.5".to_string(),
                    }
                }
            }
            button {
                class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                aria_label: "Favorite",
                onclick: on_toggle_favorite,
                Icon {
                    name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                    class: "w-4 h-4".to_string(),
                }
            }
            button {
                class: "p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                aria_label: "Add to queue",
                onclick: on_open_menu,
                Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
            }
            span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
        }
    }
}

#[component]
pub fn PlaylistDetailView(playlist_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let add_menu = use_context::<AddMenuController>();
    let mut is_favorited = use_signal(|| false);
    let reload = use_signal(|| 0usize);
    let mut song_search = use_signal(String::new);
    let edit_mode = use_signal(|| false);
    let deleting_playlist = use_signal(|| false);
    let delete_error = use_signal(|| None::<String>);
    let reorder_error = use_signal(|| None::<String>);
    let mut song_list = use_signal(|| Vec::<Song>::new());
    let mut show_delete_confirm = use_signal(|| false);
    let drag_source_index = use_signal(|| None::<usize>);

    let server = servers().into_iter().find(|s| s.id == server_id);
    let server_for_playlist = server.clone();

    let playlist_data = use_resource(move || {
        let server = server_for_playlist.clone();
        let playlist_id = playlist_id.clone();
        let _reload = reload();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_playlist(&playlist_id).await.ok()
            } else {
                None
            }
        }
    });

    let search_results = {
        let server = server.clone();
        use_resource(move || {
            let server = server.clone();
            let query = song_search();
            async move {
                if query.trim().len() < 2 {
                    return Vec::new();
                }
                if let Some(server) = server.clone() {
                    let client = NavidromeClient::new(server);
                    if let Ok(res) = client.search(query.trim(), 0, 0, 25).await {
                        return res.songs;
                    }
                }
                Vec::new()
            }
        })
    };

    let on_play_all = {
        let playlist_data_ref = playlist_data.clone();
        move |_| {
            if let Some(Some((_, songs))) = playlist_data_ref() {
                if !songs.is_empty() {
                    queue.set(songs.clone());
                    queue_index.set(0);
                    now_playing.set(Some(songs[0].clone()));
                    is_playing.set(true);
                }
            }
        }
    };

    let on_open_playlist_menu = {
        let playlist_data_ref = playlist_data.clone();
        let mut add_menu = add_menu.clone();
        move |_: MouseEvent| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                add_menu.open(AddIntent::from_playlist(&playlist));
            }
        }
    };

    use_effect(move || {
        if let Some(Some((playlist, _))) = playlist_data() {
            is_favorited.set(playlist.starred.is_some());
        }
    });

    let is_auto_imported = {
        if let Some(Some((playlist, _))) = playlist_data() {
            playlist
                .comment
                .as_ref()
                .map(|c| c.to_lowercase().contains("auto-imported"))
                .unwrap_or(false)
        } else {
            false
        }
    };

    let on_favorite_toggle = {
        let playlist_data_ref = playlist_data.clone();
        let mut is_favorited = is_favorited.clone();
        let servers = servers.clone();
        move |_| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let should_star = !is_favorited();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let result = if should_star {
                            client.star(&playlist_id, "playlist").await
                        } else {
                            client.unstar(&playlist_id, "playlist").await
                        };
                        if result.is_ok() {
                            is_favorited.set(should_star);
                        }
                    });
                }
            }
        }
    };

    let on_remove_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let song_list_signal = song_list.clone();
        move |song_index: usize| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let mut song_list = song_list_signal.clone();
                    if song_index >= song_list().len() {
                        return;
                    }
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let result = client
                            .remove_songs_from_playlist(&playlist_id, &[song_index])
                            .await;
                        if result.is_ok() {
                            song_list.with_mut(|list| {
                                if song_index < list.len() {
                                    list.remove(song_index);
                                }
                            });
                        }
                    });
                }
            }
        }
    };

    let on_add_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let mut reload = reload.clone();
        move |song_id: String| {
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let _ = client.add_songs_to_playlist(&playlist_id, &[song_id]).await;
                        reload.set(reload() + 1);
                    });
                }
            }
        }
    };

    let on_reorder_song = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let mut song_list = song_list.clone();
        let mut reorder_error = reorder_error.clone();
        let reload = reload.clone();
        Rc::new(RefCell::new(move |source_index: usize, target_index: usize| {
            let mut ordered_song_ids = Vec::<String>::new();
            let mut reordered = false;
            song_list.with_mut(|list| {
                if list.len() < 2
                    || source_index >= list.len()
                    || target_index >= list.len()
                    || source_index == target_index
                {
                    return;
                }

                let moved_song = list.remove(source_index);
                let insert_index = if source_index < target_index {
                    target_index.saturating_sub(1)
                } else {
                    target_index
                };
                list.insert(insert_index, moved_song);
                ordered_song_ids = list.iter().map(|song| song.id.clone()).collect();
                reordered = true;
            });

            if !reordered {
                return;
            }

            reorder_error.set(None);

            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    let playlist_id = playlist.id.clone();
                    let total_songs = ordered_song_ids.len();
                    let mut reorder_error = reorder_error.clone();
                    let mut reload = reload.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        if let Err(err) = client
                            .reorder_playlist(&playlist_id, &ordered_song_ids, total_songs)
                            .await
                        {
                            reorder_error
                                .set(Some(format!("Failed to save playlist order: {err}")));
                            reload.set(reload().saturating_add(1));
                        }
                    });
                }
            }
        }))
    };

    let on_toggle_edit_mode = {
        let mut edit_mode = edit_mode.clone();
        let mut song_search = song_search.clone();
        let mut drag_source_index = drag_source_index.clone();
        let mut reorder_error = reorder_error.clone();
        move |_| {
            let next_edit_state = !edit_mode();
            edit_mode.set(next_edit_state);
            song_search.set(String::new());
            drag_source_index.set(None);
            reorder_error.set(None);
        }
    };

    let delete_playlist_action = {
        let playlist_data_ref = playlist_data.clone();
        let servers = servers.clone();
        let deleting_playlist = deleting_playlist.clone();
        let delete_error = delete_error.clone();
        let navigation = navigation.clone();
        Rc::new(RefCell::new(move || {
            let mut deleting_playlist_flag = deleting_playlist.clone();
            if deleting_playlist_flag() {
                return;
            }
            if let Some(Some((playlist, _))) = playlist_data_ref() {
                if let Some(server) = servers()
                    .iter()
                    .find(|s| s.id == playlist.server_id)
                    .cloned()
                {
                    deleting_playlist_flag.set(true);
                    let playlist_id = playlist.id.clone();
                    let navigation = navigation.clone();
                    let mut deleting_playlist_clone = deleting_playlist_flag.clone();
                    let mut delete_error_clone = delete_error.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        match client.delete_playlist(&playlist_id).await {
                            Ok(_) => navigation.navigate_to(AppView::PlaylistsView {}),
                            Err(err) => delete_error_clone.set(Some(err)),
                        }
                        deleting_playlist_clone.set(false);
                    });
                }
            }
        }))
    };
    let on_delete_playlist = {
        let mut show_delete_confirm = show_delete_confirm.clone();
        let mut delete_error = delete_error.clone();
        move |_: MouseEvent| {
            delete_error.set(None);
            show_delete_confirm.set(true);
        }
    };
    let on_confirm_delete = {
        let delete_playlist_action = delete_playlist_action.clone();
        let mut show_delete_confirm = show_delete_confirm.clone();
        move |_: MouseEvent| {
            show_delete_confirm.set(false);
            delete_playlist_action.borrow_mut()();
        }
    };

    use_effect(move || {
        if let Some(Some((_, songs))) = playlist_data() {
            song_list.set(songs.clone());
        }
    });

    rsx! {
        div { class: "space-y-8",
            button {
                class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
                onclick: move |_| {
                    if navigation.go_back().is_none() {
                        navigation.navigate_to(AppView::PlaylistsView {});
                    }
                },
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Playlists"
            }

            match playlist_data() {
                Some(Some((playlist, songs))) => {
                    let cover_url = servers()
                        .iter()
                        .find(|s| s.id == playlist.server_id)
                        .and_then(|server| {
                            let client = NavidromeClient::new(server.clone());
                            playlist
                                .cover_art
                                .as_ref()
                                .map(|ca| client.get_cover_art_url(ca, 500))
                        });
                    let hide_comment = playlist
                        .comment
                        .as_ref()
                        .map(|c| c.to_lowercase().contains("auto-imported"))
                        .unwrap_or(false);
                    let editing_allowed = !is_auto_imported;

                    rsx! {
                        div { class: "flex flex-col md:flex-row gap-8 mb-8 items-center md:items-end",
                            div { class: "w-64 h-64 rounded-2xl bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0",
                                match cover_url {
                                    Some(url) => rsx! {





                                        img { class: "w-full h-full object-cover", src: "{url}" }
                                    },
                                    None => rsx! {


                                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                                            Icon {
                                                name: "playlist".to_string(),
                                                class: "w-20 h-20 text-white/70".to_string(),
                                            }
                                        }
                                    },
                                }
                            }
                            div { class: "flex flex-col justify-end text-center md:text-left",
                                p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2", "Playlist" }
                                h1 { class: "text-4xl font-bold text-white mb-4", "{playlist.name}" }
                                if let Some(comment) = &playlist.comment {
                                    if !hide_comment {
                                        p { class: "text-zinc-400 mb-4", "{comment}" }
                                    }
                                }
                                if hide_comment {
                                    p { class: "text-xs text-amber-300 bg-amber-500/10 border border-amber-500/40 rounded-lg px-3 py-2",
                                        "Auto-imported playlists cannot be edited."
                                    }
                                }
                                if let Some(err) = delete_error() {
                                    div { class: "p-3 rounded-lg bg-red-500/10 border border-red-500/40 text-red-200 text-sm mb-3",
                                        "{err}"
                                    }
                                }
                                if let Some(err) = reorder_error() {
                                    div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm mb-3",
                                        "{err}"
                                    }
                                }
                                div { class: "flex items-center gap-4 text-sm text-zinc-400 justify-center md:justify-start",
                                    if let Some(owner) = &playlist.owner {
                                        span { "by {owner}" }
                                    }
                                    span { "{playlist.song_count} songs" }
                                    span { "{format_duration(playlist.duration / 1000)}" }
                                }
                                div { class: "flex gap-3 mt-6 flex-wrap justify-center md:justify-start",
                                    button {
                                        class: "px-8 py-3 rounded-full bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                        onclick: on_play_all,
                                        Icon { name: "play".to_string(), class: "w-5 h-5".to_string() }
                                        "Play"
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors",
                                        onclick: {
                                            let playlist_data_ref = playlist_data.clone();
                                            move |_: MouseEvent| {
                                                if let Some(Some((_, songs))) = playlist_data_ref() {
                                                    if !songs.is_empty() {
                                                        let mut shuffled = songs.clone();
                                                        use rand::seq::SliceRandom;
                                                        shuffled.shuffle(&mut rand::thread_rng());
                                                        queue.set(shuffled.clone());
                                                        queue_index.set(0);
                                                        now_playing.set(Some(shuffled[0].clone()));
                                                        is_playing.set(true);
                                                    }
                                                }
                                            }
                                        },
                                        Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors",
                                        onclick: on_favorite_toggle,
                                        Icon {
                                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                            class: "w-5 h-5".to_string(),
                                        }
                                    }
                                    button {
                                        class: "p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors",
                                        onclick: on_open_playlist_menu,
                                        Icon { name: "plus".to_string(), class: "w-5 h-5".to_string() }
                                    }
                                    if editing_allowed {
                                        button {
                                            class: "px-4 py-2 rounded-full border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-sm",
                                            onclick: on_toggle_edit_mode,
                                            if edit_mode() {
                                                "Done editing"
                                            } else {
                                                "Edit playlist"
                                            }
                                        }
                                    }
                                    if editing_allowed {
                                        button {
                                            class: "px-4 py-2 rounded-full border border-red-500/60 text-red-300 hover:text-white hover:bg-red-500/10 transition-colors text-sm",
                                            onclick: on_delete_playlist,
                                            disabled: deleting_playlist(),
                                            if deleting_playlist() {
                                                "Deleting..."
                                            } else {
                                                "Delete playlist"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "space-y-1",
                            for (index , song) in song_list().iter().enumerate() {
                                if edit_mode() {
                                    {
                                        let cover_url = servers()
                                            .iter()
                                            .find(|s| s.id == song.server_id)
                                            .and_then(|server| {
                                                let client = NavidromeClient::new(server.clone());
                                                song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                            });
                                        rsx! {
                                            div {
                                                key: "{song.server_id}:{song.id}:{index}",
                                                draggable: editing_allowed,
                                                class: if drag_source_index() == Some(index) {
                                                    "flex items-center gap-3 p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/40 opacity-75"
                                                } else {
                                                    "flex items-center gap-3 p-3 rounded-lg bg-zinc-900/60 border border-zinc-800"
                                                },
                                                ondragstart: {
                                                    let mut drag_source_index = drag_source_index.clone();
                                                    let source_index = index;
                                                    move |_| {
                                                        if editing_allowed {
                                                            drag_source_index.set(Some(source_index));
                                                        }
                                                    }
                                                },
                                                ondragend: {
                                                    let mut drag_source_index = drag_source_index.clone();
                                                    move |_| {
                                                        drag_source_index.set(None);
                                                    }
                                                },
                                                ondragover: move |evt| {
                                                    if editing_allowed {
                                                        evt.prevent_default();
                                                    }
                                                },
                                                ondrop: {
                                                    let mut drag_source_index = drag_source_index.clone();
                                                    let on_reorder_song = on_reorder_song.clone();
                                                    let target_index = index;
                                                    move |evt| {
                                                        if !editing_allowed {
                                                            return;
                                                        }
                                                        evt.prevent_default();
                                                        let Some(source_index) = drag_source_index() else {
                                                            return;
                                                        };
                                                        drag_source_index.set(None);
                                                        on_reorder_song.borrow_mut()(source_index, target_index);
                                                    }
                                                },
                                                div {
                                                    class: "text-zinc-600 cursor-grab active:cursor-grabbing",
                                                    title: "Drag to reorder",
                                                    Icon { name: "bars".to_string(), class: "w-4 h-4".to_string() }
                                                }
                                                div { class: "w-12 h-12 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    match cover_url {
                                                        Some(url) => rsx! {
                                                            img { class: "w-full h-full object-cover", src: "{url}" }
                                                        },
                                                        None => rsx! {
                                                            div { class: "w-full h-full flex items-center justify-center bg-zinc-800",
                                                                Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                                            }
                                                        },
                                                    }
                                                }
                                                div { class: "min-w-0 flex-1",
                                                    p { class: "text-sm text-white truncate", "{song.title}" }
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.clone().unwrap_or_default()} • {song.album.clone().unwrap_or_default()}"
                                                    }
                                                }
                                                if editing_allowed {
                                                    button {
                                                        class: "p-2 rounded-full bg-zinc-950/70 text-zinc-300 hover:text-red-300 hover:bg-red-500/20 transition-colors",
                                                        onclick: {
                                                            let remove_index = index;
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                on_remove_song(remove_index);
                                                            }
                                                        },
                                                        Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    PlaylistSongRow {
                                        song: song.clone(),
                                        display_index: index + 1,
                                        songs: songs.clone(),
                                        queue: queue.clone(),
                                        queue_index: queue_index.clone(),
                                        now_playing: now_playing.clone(),
                                        is_playing: is_playing.clone(),
                                        servers: servers.clone(),
                                        add_menu: add_menu.clone(),
                                    }
                                }
                            }
                        }

                        if editing_allowed && edit_mode() {
                            div { class: "mt-6 space-y-3 p-4 rounded-xl bg-zinc-900/60 border border-zinc-800",
                                h3 { class: "text-sm font-semibold text-white", "Add songs to this playlist" }
                                input {
                                    class: "w-full px-3 py-2 rounded-lg bg-zinc-950/60 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                                    placeholder: "Search songs to add",
                                    value: song_search,
                                    oninput: move |e| song_search.set(e.value()),
                                }
                                if let Some(results) = search_results() {
                                    if results.is_empty() && song_search().trim().len() >= 2 {
                                        p { class: "text-sm text-zinc-500", "No songs found." }
                                    } else if !results.is_empty() {
                                        div { class: "space-y-2 max-h-64 overflow-y-auto pr-1",
                                            for res in results {
                                                div { class: "flex items-center justify-between gap-3 p-2 rounded-lg hover:bg-zinc-800/50 transition-colors",
                                                    {
                                                        let cover_url = servers()
                                                            .iter()
                                                            .find(|s| s.id == res.server_id)
                                                            .and_then(|server| {
                                                                let client = NavidromeClient::new(server.clone());
                                                                res.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                                            });
                                                        rsx! {
                                                            if let Some(url) = cover_url {
                                                                img {
                                                                    class: "w-10 h-10 rounded object-cover border border-zinc-800/80",
                                                                    src: "{url}",
                                                                }
                                                            } else {
                                                                div { class: "w-10 h-10 rounded bg-zinc-800 flex items-center justify-center border border-zinc-800/80",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-4 h-4 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { class: "min-w-0 flex-1",
                                                        p { class: "text-sm text-white truncate", "{res.title}" }
                                                        p { class: "text-xs text-zinc-500 truncate",
                                                            "{res.artist.clone().unwrap_or_default()} • {res.album.clone().unwrap_or_default()}"
                                                        }
                                                    }
                                                    button {
                                                        class: "px-3 py-1 rounded-lg border border-emerald-500/60 text-emerald-300 hover:text-white hover:bg-emerald-500/10 transition-colors text-xs",
                                                        onclick: {
                                                            let song_id = res.id.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                on_add_song(song_id.clone());
                                                            }
                                                        },
                                                        "Add"
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
                Some(None) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon {
                            name: "playlist".to_string(),
                            class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "Playlist not found" }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-zinc-500".to_string(),
                        }
                    }
                },
            }
            if show_delete_confirm() {
                div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    div { class: "bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4",
                        h2 { class: "text-xl font-bold text-white mb-4", "Delete Playlist" }
                        p { class: "text-zinc-300 mb-6",
                            "Are you sure you want to delete this playlist? This action cannot be undone."
                        }
                        div { class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors",
                                onclick: move |_| show_delete_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg bg-red-600 hover:bg-red-500 text-white transition-colors",
                                onclick: on_confirm_delete,
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}
