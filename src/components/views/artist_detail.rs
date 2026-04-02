use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta, normalize_manual_queue_songs,
};
use crate::components::views::home::{AlbumCard, SongRow};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

const ARTIST_ALBUM_BATCH_SIZE: usize = 24;

fn render_album_item(album: Album, navigation: Navigation) -> Element {
    let album_id = album.id.clone();
    let album_server_id = album.server_id.clone();
    let album_id_for_nav = album_id.clone();
    let album_server_id_for_nav = album_server_id.clone();

    rsx! {
        AlbumCard {
            key: "{album_id}",
            album: album.clone(),
            onclick: move |_| {
                let navigation = navigation.clone();
                navigation
                    .navigate_to(AppView::AlbumDetailView {
                        album_id: album_id_for_nav.clone(),
                        server_id: album_server_id_for_nav.clone(),
                    });
            },
        }
    }
}

#[component]
pub fn ArtistDetailView(artist_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let mut visible_album_count = use_signal(|| ARTIST_ALBUM_BATCH_SIZE);
    let mut current_artist_id = use_signal(|| artist_id.clone());
    let mut current_server_id = use_signal(|| server_id.clone());

    use_effect({
        let artist_id = artist_id.clone();
        let server_id = server_id.clone();
        move || {
            if current_artist_id() != artist_id {
                eprintln!(
                    "[artist-detail.route] artist_id change {} -> {}",
                    current_artist_id(),
                    artist_id
                );
                current_artist_id.set(artist_id.clone());
                visible_album_count.set(ARTIST_ALBUM_BATCH_SIZE);
            }
            if current_server_id() != server_id {
                eprintln!(
                    "[artist-detail.route] server_id change {} -> {}",
                    current_server_id(),
                    server_id
                );
                current_server_id.set(server_id.clone());
                visible_album_count.set(ARTIST_ALBUM_BATCH_SIZE);
            }
        }
    });

    let artist_server = servers().into_iter().find(|s| s.id == current_server_id());

    let artist_data = use_resource(move || {
        let server_id = current_server_id();
        let artist_id = current_artist_id();
        let server = servers().into_iter().find(|s| s.id == server_id);
        async move {
            if let Some(server) = server {
                eprintln!(
                    "[artist-detail.fetch.start] artist_id={} server_id={}",
                    artist_id, server_id
                );
                let client = NavidromeClient::new(server);
                match client.get_artist(&artist_id).await {
                    Ok((artist, albums)) => {
                        eprintln!(
                            "[artist-detail.fetch.ok] requested_artist_id={} returned_artist_id={} server_id={} albums={}",
                            artist_id,
                            artist.id,
                            server_id,
                            albums.len()
                        );
                        Some((artist, albums))
                    }
                    Err(err) => {
                        eprintln!(
                            "[artist-detail.fetch.err] artist_id={} server_id={} err={}",
                            artist_id, server_id, err
                        );
                        None
                    }
                }
            } else {
                eprintln!(
                    "[artist-detail.fetch.skip] missing server artist_id={} server_id={}",
                    artist_id, server_id
                );
                None
            }
        }
    });

    let top_songs_data = use_resource({
        let artist_data = artist_data.clone();
        move || {
            let server_id = current_server_id();
            let server = servers().into_iter().find(|s| s.id == server_id);
            let artist_name = artist_data()
                .and_then(|value| value.map(|(artist, _)| artist.name.clone()))
                .filter(|name| !name.is_empty());
            async move {
                match (server, artist_name) {
                    (Some(server), Some(artist_name)) => {
                        eprintln!(
                            "[artist-detail.top.start] artist_name='{}' server_id={}",
                            artist_name, server_id
                        );
                        let client = NavidromeClient::new(server);
                        match client.get_top_songs(&artist_name, 20).await {
                            Ok(songs) => {
                                eprintln!(
                                    "[artist-detail.top.ok] artist_name='{}' songs={}",
                                    artist_name,
                                    songs.len()
                                );
                                Some(songs)
                            }
                            Err(err) => {
                                eprintln!(
                                    "[artist-detail.top.err] artist_name='{}' err={}",
                                    artist_name, err
                                );
                                None
                            }
                        }
                    }
                    _ => None,
                }
            }
        }
    });

    let mut is_favorited = use_signal(|| false);

    let _on_add_album = {
        let artist_data_ref = artist_data.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        move |_: MouseEvent| {
            if let Some(Some((_, albums))) = artist_data_ref() {
                spawn(async move {
                    let mut all_songs = Vec::new();
                    for album in albums {
                        if let Some(server) =
                            servers().iter().find(|s| s.id == album.server_id).cloned()
                        {
                            let client = NavidromeClient::new(server);
                            if let Ok((_, songs)) = client.get_album(&album.id).await {
                                all_songs.extend(songs);
                            }
                        }
                    }
                    queue.with_mut(|q| q.extend(all_songs));
                });
            }
        }
    };

    use_effect(move || {
        if let Some(Some((artist, _))) = artist_data() {
            is_favorited.set(artist.starred.is_some());
        }
    });

    let on_favorite_toggle = move |_| {
        if let Some(Some((artist, _))) = artist_data() {
            let server_list = servers();
            if let Some(server) = server_list
                .iter()
                .find(|s| s.id == artist.server_id)
                .cloned()
            {
                let artist_id = artist.id.clone();
                let should_star = !is_favorited();
                let mut is_favorited = is_favorited;
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = if should_star {
                        client.star(&artist_id, "artist").await
                    } else {
                        client.unstar(&artist_id, "artist").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                });
            }
        }
    };

    rsx! {
        button {
            class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
            onclick: move |_| {
                if navigation.go_back().is_none() {
                    navigation.navigate_to(AppView::ArtistsView {});
                }
            },
            Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
            "Back to Artists"
        }

        {
            match artist_data() {
                Some(Some((artist, albums))) => {
                    let requested_artist_id = current_artist_id();
                    let requested_server_id = current_server_id();
                    let server_matches = artist.server_id.is_empty()
                        || artist.server_id == requested_server_id;
                    if artist.id != requested_artist_id || !server_matches {
                        eprintln!(
                            "[artist-detail.stale] requested_artist_id={} requested_server_id={} returned_artist_id={} returned_server_id={}",
                            requested_artist_id,
                            requested_server_id,
                            artist.id,
                            artist.server_id
                        );
                        rsx! {
                            div { class: "flex flex-col items-center justify-center py-20",
                                div { class: "w-16 h-16 rounded-full border-2 border-zinc-700 border-t-emerald-500 animate-spin mb-4" }
                                p { class: "text-zinc-400", "Loading artist..." }
                            }
                        }
                    } else {
                        let top_songs = top_songs_data().flatten().unwrap_or_default();
                        let cover_url = artist_server.as_ref().and_then(|server| {
                            let client = NavidromeClient::new(server.clone());
                            artist
                                .cover_art
                                .as_ref()
                                .map(|ca| client.get_cover_art_url(ca, 500))
                        });

                        let total_albums = albums.len();
                        let total_songs: u32 = albums.iter().map(|a| a.song_count).sum();
                        let current_album_limit = visible_album_count().min(total_albums);
                        let remaining_albums = total_albums.saturating_sub(current_album_limit);
                        let cover_element = match cover_url {
                            Some(url) => rsx! {
                                img {
                                    src: "{url}",
                                    alt: "{artist.name}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy",
                                }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-emerald-600 to-teal-700",
                                    Icon {
                                        name: "artist".to_string(),
                                        class: "w-24 h-24 text-white/70".to_string(),
                                    }
                                }
                            },
                        };

                        rsx! {
                            div { class: "flex flex-col md:flex-row gap-8 mb-12",
                                div { class: "w-48 h-48 md:w-64 md:h-64 rounded-full bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0 mx-auto md:mx-0",
                                    {cover_element}
                            }
                            div { class: "flex flex-col justify-end text-center md:text-left",
                                p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2 font-medium",
                                    "Artist"
                                }
                                h1 { class: "text-5xl md:text-6xl font-bold text-white mb-4", "{artist.name}" }
                                div { class: "flex items-center gap-4 text-sm text-zinc-400 justify-center md:justify-start",
                                    span { class: "flex items-center gap-1",
                                        Icon { name: "album".to_string(), class: "w-4 h-4".to_string() }
                                        "{total_albums} albums"
                                    }
                                    span { "•" }
                                    span { class: "flex items-center gap-1",
                                        Icon { name: "music".to_string(), class: "w-4 h-4".to_string() }
                                        "{total_songs} songs"
                                    }
                                }
                                div { class: "flex gap-3 mt-6 justify-center md:justify-start",
                                    button {
                                        class: if is_favorited() { "p-3 rounded-full border border-zinc-700 text-emerald-400 hover:text-emerald-300 hover:border-emerald-500/50 transition-colors" } else { "p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors" },
                                        onclick: on_favorite_toggle,
                                        Icon {
                                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                            class: "w-5 h-5".to_string(),
                                        }
                                    }
                                }
                            }
                        }
                        section { class: "space-y-6",
                            h2 { class: "text-2xl font-bold text-white", "Albums" }
                            div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6",
                                {
                                    albums
                                        .iter()
                                        .take(current_album_limit)
                                        .map(|album| {
                                            render_album_item(album.clone(), navigation.clone())
                                        })
                                }
                            }
                            if remaining_albums > 0 {
                                div { class: "flex justify-center pt-2",
                                    button {
                                        class: "px-4 py-2 rounded-xl border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 hover:bg-zinc-800/40 transition-colors text-sm",
                                        onclick: {
                                            let mut visible_album_count = visible_album_count.clone();
                                            move |_| {
                                                let next = visible_album_count()
                                                    .saturating_add(ARTIST_ALBUM_BATCH_SIZE)
                                                    .min(total_albums);
                                                visible_album_count.set(next);
                                            }
                                        },
                                        "Show {remaining_albums} More Albums"
                                    }
                                }
                            }
                        }
                        if !top_songs.is_empty() {
                            section { class: "space-y-4 mt-10",
                                h2 { class: "text-2xl font-bold text-white", "Popular Songs" }
                                div { class: "rounded-2xl border border-zinc-800/80 bg-zinc-900/30 p-2",
                                    for (index, song) in top_songs.iter().enumerate() {
                                        {
                                            let song_for_queue = song.clone();
                                            let albums_for_queue = albums.clone();
                                            let servers = servers.clone();
                                            let shuffle_enabled = shuffle_enabled.clone();
                                            rsx! {
                                                SongRow {
                                                    song: song.clone(),
                                                    index: index + 1,
                                                    show_download: true,
                                                    show_duration: false,
                                                    show_duration_in_menu: true,
                                                    onclick: move |_| {
                                                        if shuffle_enabled() {
                                                            let seed_song = song_for_queue.clone();
                                                            let servers_snapshot = servers();
                                                            let albums_for_queue = albums_for_queue.clone();
                                                            let mut queue = queue.clone();
                                                            let mut queue_index = queue_index.clone();
                                                            let mut now_playing = now_playing.clone();
                                                            let mut is_playing = is_playing.clone();
                                                            spawn(async move {
                                                                let mut all_songs = Vec::<Song>::new();
                                                                for album in albums_for_queue.iter() {
                                                                    let Some(server) = servers_snapshot
                                                                        .iter()
                                                                        .find(|entry| entry.id == album.server_id)
                                                                        .cloned()
                                                                    else {
                                                                        continue;
                                                                    };
                                                                    let client = NavidromeClient::new(server);
                                                                    if let Ok((_, mut album_songs)) =
                                                                        client.get_album(&album.id).await
                                                                    {
                                                                        all_songs.append(&mut album_songs);
                                                                    }
                                                                }

                                                                if all_songs.is_empty() {
                                                                    all_songs.push(seed_song.clone());
                                                                }

                                                                let mut seen = std::collections::HashSet::<String>::new();
                                                                all_songs.retain(|entry| {
                                                                    seen.insert(format!(
                                                                        "{}::{}",
                                                                        entry.server_id, entry.id
                                                                    ))
                                                                });

                                                                let source_id = format!(
                                                                    "{}::{}",
                                                                    seed_song.server_id,
                                                                    seed_song
                                                                        .artist_id
                                                                        .clone()
                                                                        .unwrap_or_else(|| "artist".to_string())
                                                                );
                                                                let all_songs = assign_collection_queue_meta(
                                                                    all_songs,
                                                                    QueueSourceKind::Artist,
                                                                    source_id,
                                                                );
                                                                let target_index = all_songs
                                                                    .iter()
                                                                    .position(|entry| {
                                                                        entry.id == seed_song.id
                                                                            && entry.server_id == seed_song.server_id
                                                                    })
                                                                    .unwrap_or(0);
                                                                queue.set(all_songs.clone());
                                                                queue_index.set(target_index);
                                                                now_playing.set(
                                                                    all_songs.get(target_index).cloned(),
                                                                );
                                                                is_playing.set(true);
                                                                let _ = apply_collection_shuffle_mode(
                                                                    queue.clone(),
                                                                    queue_index.clone(),
                                                                    now_playing.clone(),
                                                                    true,
                                                                );
                                                            });
                                                        } else {
                                                            let single_queue = normalize_manual_queue_songs(vec![
                                                                song_for_queue.clone(),
                                                            ]);
                                                            queue.set(single_queue.clone());
                                                            queue_index.set(0);
                                                            now_playing.set(single_queue.first().cloned());
                                                            is_playing.set(true);
                                                        }
                                                    },
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
                            name: "artist".to_string(),
                            class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "Artist not found" }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        div { class: "animate-pulse flex flex-col items-center",
                            div { class: "w-48 h-48 rounded-full bg-zinc-800 mb-6" }
                            div { class: "h-8 w-48 bg-zinc-800 rounded mb-4" }
                            div { class: "h-4 w-32 bg-zinc-800 rounded" }
                        }
                    }
                },
            }
        }
    }
}
