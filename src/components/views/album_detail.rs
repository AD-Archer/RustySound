use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta,
};
use crate::components::views::album_song_row::AlbumSongRow;
use crate::components::views::artist_links::ArtistNameLinks;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{
    download_songs_batch, is_album_downloaded, is_song_downloaded, mark_collection_downloaded,
    sync_downloaded_collection_members,
};
use dioxus::prelude::*;

fn anchored_menu_style(
    anchor_x: f64,
    anchor_y: f64,
    menu_width: f64,
    menu_max_height: f64,
) -> String {
    let preferred_top = (anchor_y + 8.0).max(8.0);
    let preferred_left = (anchor_x - menu_width).max(4.0);
    format!(
        "top: clamp(8px, {:.1}px, calc(100vh - {:.1}px - 8px)); left: clamp(4px, {:.1}px, calc(100vw - {:.1}px - 4px)); max-height: min({:.1}px, calc(100vh - 16px)); overflow-y: auto;",
        preferred_top, menu_max_height, preferred_left, menu_width, menu_max_height
    )
}

#[component]
pub fn AlbumDetailView(album_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let download_busy = use_signal(|| false);
    let download_status = use_signal(|| None::<String>);
    let mut album_rating = use_signal(|| 0u32);
    let mut show_album_menu = use_signal(|| false);
    let mut album_menu_x = use_signal(|| 0f64);
    let mut album_menu_y = use_signal(|| 0f64);

    let server = servers().into_iter().find(|s| s.id == server_id);

    let album_id_for_resource = album_id.clone();
    let album_data = use_resource(move || {
        let server = server.clone();
        let album_id = album_id_for_resource.clone();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_album(&album_id).await.ok()
            } else {
                None
            }
        }
    });

    let on_play_all = {
        let source_server_id = server_id.clone();
        let source_album_id = album_id.clone();
        let album_data_ref = album_data.clone();
        let app_settings = app_settings.clone();
        let mut download_status = download_status.clone();
        move |_| {
            if let Some(Some((_, songs))) = album_data_ref() {
                if !songs.is_empty() {
                    let settings = app_settings();
                    let playable = if settings.offline_mode {
                        songs
                            .iter()
                            .filter(|song| is_song_downloaded(song))
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        songs.clone()
                    };
                    if playable.is_empty() {
                        download_status.set(Some(
                            "No downloaded songs in this album are available for offline playback."
                                .to_string(),
                        ));
                        return;
                    }
                    let playable = assign_collection_queue_meta(
                        playable,
                        QueueSourceKind::Album,
                        format!("{}::{}", source_server_id, source_album_id),
                    );
                    queue.set(playable.clone());
                    queue_index.set(0);
                    now_playing.set(Some(playable[0].clone()));
                    is_playing.set(true);
                    let shuffle = shuffle_enabled();
                    if shuffle {
                        let _ = apply_collection_shuffle_mode(
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            true,
                        );
                    }
                }
            }
        }
    };

    let on_open_album_menu = {
        let album_data_ref = album_data.clone();
        let mut add_menu = add_menu.clone();
        let mut show_album_menu = show_album_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_album_menu.set(false);
            if let Some(Some((album, _))) = album_data_ref() {
                add_menu.open(AddIntent::from_album(&album));
            }
        }
    };

    let on_toggle_shuffle = {
        let mut shuffle_enabled = shuffle_enabled.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let next = !shuffle_enabled();
            shuffle_enabled.set(next);
            let _ = apply_collection_shuffle_mode(
                queue.clone(),
                queue_index.clone(),
                now_playing.clone(),
                next,
            );
        }
    };

    let on_download_album = {
        let album_data_ref = album_data.clone();
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let mut download_busy = download_busy.clone();
        let mut download_status = download_status.clone();
        move |_| {
            if download_busy() {
                return;
            }

            let Some(Some((album, songs))) = album_data_ref() else {
                download_status.set(Some("No songs available to download.".to_string()));
                return;
            };
            if songs.is_empty() {
                download_status.set(Some("No songs available to download.".to_string()));
                return;
            }

            let servers_snapshot = servers();
            if servers_snapshot.is_empty() {
                download_status.set(Some("No servers configured.".to_string()));
                return;
            }

            let settings_snapshot = app_settings();
            let album_meta = album.clone();
            download_busy.set(true);
            download_status.set(Some("Downloading album songs...".to_string()));
            spawn(async move {
                let report =
                    download_songs_batch(&songs, &servers_snapshot, &settings_snapshot).await;
                if report.downloaded > 0 || report.skipped > 0 {
                    mark_collection_downloaded(
                        "album",
                        &album_meta.server_id,
                        &album_meta.id,
                        &album_meta.name,
                        songs.len(),
                    );
                    sync_downloaded_collection_members(
                        "album",
                        &album_meta.server_id,
                        &album_meta.id,
                        &songs,
                    );
                }
                download_status.set(Some(format!(
                    "Album download complete: {} new, {} skipped, {} failed, {} purged.",
                    report.downloaded, report.skipped, report.failed, report.purged
                )));
                download_busy.set(false);
            });
        }
    };

    use_effect(move || {
        if let Some(Some((album, _))) = album_data() {
            album_rating.set(album.user_rating.unwrap_or(0).min(5));
        }
    });

    let make_on_set_album_rating = {
        let servers = servers.clone();
        let album_data_ref = album_data.clone();
        let album_rating = album_rating.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let album_data_ref = album_data_ref.clone();
            let mut album_rating = album_rating.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                let normalized = new_rating.min(5);
                album_rating.set(normalized);
                if let Some(Some((album, _))) = album_data_ref() {
                    let album_id = album.id.clone();
                    let server_id = album.server_id.clone();
                    let servers = servers.clone();
                    spawn(async move {
                        if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                            let client = NavidromeClient::new(server.clone());
                            let _ = client.set_rating(&album_id, normalized).await;
                        }
                    });
                }
            }
        }
    };

    let on_view_artist_from_menu = {
        let navigation = navigation.clone();
        let mut show_album_menu = show_album_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_album_menu.set(false);
            if let Some(Some((album, _))) = album_data() {
                if let Some(artist_id) = album.artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id: album.server_id.clone(),
                    });
                }
            }
        }
    };

    rsx! {
        div { class: "space-y-8 overflow-x-hidden",
            // Back button
            button {
                class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
                onclick: move |_| {
                    if navigation.go_back().is_none() {
                        navigation.navigate_to(AppView::Albums {});
                    }
                },
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Albums"
            }

            {

                // Album header
                // Cover art
                // Album info
                // Actions

                // Song list
                match album_data() {
                    Some(Some((album, songs))) => {
                        let cover_art_id = album
                            .cover_art
                            .as_ref()
                            .filter(|value| !value.trim().is_empty())
                            .cloned()
                            .or_else(|| {
                                songs.iter().find_map(|song| {
                                    song.cover_art
                                        .as_ref()
                                        .filter(|value| !value.trim().is_empty())
                                        .cloned()
                                })
                            })
                            .or_else(|| {
                                if album.id.trim().is_empty() {
                                    None
                                } else {
                                    Some(album.id.clone())
                                }
                            });

                        let cover_url = servers()
                            .iter()
                            .find(|s| s.id == album.server_id)
                            .and_then(|server| cover_art_id.as_ref().map(|cover_art_id| {
                                let client = NavidromeClient::new(server.clone());
                                client.get_cover_art_url(cover_art_id, 500)
                            }));
                        let downloaded_song_count =
                            songs.iter().filter(|song| is_song_downloaded(song)).count();
                        let album_downloaded = is_album_downloaded(&album.server_id, &album.id);
                        let album_fully_downloaded =
                            !songs.is_empty() && downloaded_song_count >= songs.len();
                        rsx! {
                            div { class: "flex flex-col md:flex-row gap-8 mb-8 overflow-x-hidden items-center md:items-end",
                                div { class: "w-64 h-64 rounded-2xl bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0 mx-auto md:mx-0",
                                    {
                                        match cover_url {
                                            Some(url) => rsx! {
                                                img {
                                                    src: "{url}",
                                                    alt: "{album.name} cover",
                                                    class: "w-full h-full object-cover",
                                                    loading: "lazy",
                                                }
                                            },
                                            None => rsx! {
                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                    Icon {
                                                        name: "album".to_string(),
                                                        class: "w-20 h-20 text-zinc-500".to_string(),
                                                    }
                                                }
                                            },
                                        }
                                    }
                                }
                                div { class: "flex flex-col justify-end max-w-full text-center md:text-left",
                                    p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2", "Album" }
                                    div { class: "flex flex-wrap items-baseline gap-x-2 gap-y-1 justify-center md:justify-start mb-2 max-w-full",
                                        h1 {
                                            class: "text-3xl md:text-4xl font-bold text-white max-w-full",
                                            style: "word-break: break-word; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;",
                                            "{album.name}"
                                        }
                                        span { class: "text-zinc-500", "•" }
                                        div { class: "flex items-center gap-2 max-w-full min-w-0",
                                            ArtistNameLinks {
                                                artist_text: album.artist.clone(),
                                                server_id: album.server_id.clone(),
                                                fallback_artist_id: album.artist_id.clone(),
                                                container_class: "inline-flex max-w-full min-w-0 items-center gap-1 text-lg text-zinc-300".to_string(),
                                                button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors".to_string(),
                                                separator_class: "text-zinc-500".to_string(),
                                            }
                                            if album_downloaded {
                                                Icon {
                                                    name: "download".to_string(),
                                                    class: "w-4 h-4 text-emerald-400 flex-shrink-0".to_string(),
                                                }
                                            }
                                        }
                                    }
                                    div { class: "flex items-center gap-4 text-sm text-zinc-400 justify-center md:justify-start",
                                        if let Some(year) = album.year {
                                            span { "{year}" }
                                        }
                                        span { "{album.song_count} songs" }
                                        span { "{format_duration(album.duration / 1000)}" }
                                        span { "{downloaded_song_count} downloaded" }
                                    }
                                    div { class: "mt-6 w-full max-w-sm grid grid-cols-4 gap-2 md:max-w-none md:flex md:flex-wrap md:gap-3 justify-center md:justify-start",
                                        button {
                                            class: "col-span-1 p-3 rounded-full bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center justify-center gap-2 md:px-8",
                                            onclick: on_play_all,
                                            title: "Play album",
                                            Icon { name: "play".to_string(), class: "w-5 h-5".to_string() }
                                            span { class: "hidden md:inline", "Play" }
                                        }
                                        button {
                                            class: if download_busy() {
                                                "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-500 cursor-not-allowed flex items-center justify-center"
                                            } else if album_fully_downloaded {
                                                "col-span-1 p-3 rounded-full bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center justify-center"
                                            } else {
                                                "col-span-1 p-3 rounded-full border border-emerald-500/60 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors flex items-center justify-center"
                                            },
                                            disabled: download_busy(),
                                            onclick: on_download_album,
                                            title: if download_busy() {
                                                "Downloading album"
                                            } else if album_fully_downloaded {
                                                "Album fully downloaded"
                                            } else {
                                                "Download album"
                                            },
                                            Icon {
                                                name: if download_busy() {
                                                    "loader".to_string()
                                                } else if album_fully_downloaded {
                                                    "check".to_string()
                                                } else {
                                                    "download".to_string()
                                                },
                                                class: "w-5 h-5".to_string(),
                                            }
                                        }
                                        button {
                                            class: if shuffle_enabled() {
                                                "col-span-1 p-3 rounded-full bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center justify-center"
                                            } else {
                                                "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                            },
                                            onclick: on_toggle_shuffle,
                                            title: if shuffle_enabled() {
                                                "Shuffle is on"
                                            } else {
                                                "Shuffle is off"
                                            },
                                            Icon {
                                                name: "shuffle".to_string(),
                                                class: "w-5 h-5".to_string(),
                                            }
                                        }
                                        button {
                                            class: "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center",
                                            onclick: move |evt: MouseEvent| {
                                                evt.stop_propagation();
                                                let coords = evt.client_coordinates();
                                                album_menu_x.set(coords.x);
                                                album_menu_y.set(coords.y);
                                                show_album_menu.set(!show_album_menu());
                                            },
                                            title: "More album actions",
                                            Icon { name: "more-horizontal".to_string(), class: "w-5 h-5".to_string() }
                                        }
                                    }
                                    if show_album_menu() {
                                        div {
                                            class: "fixed inset-0 z-[9998]",
                                            onclick: move |evt: MouseEvent| {
                                                evt.stop_propagation();
                                                show_album_menu.set(false);
                                            },
                                        }
                                        div {
                                            class: "fixed z-[9999] w-52 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                                            style: anchored_menu_style(
                                                album_menu_x(),
                                                album_menu_y(),
                                                208.0,
                                                320.0,
                                            ),
                                            onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                            if album.artist_id.is_some() {
                                                button {
                                                    class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                                    onclick: on_view_artist_from_menu,
                                                    Icon {
                                                        name: "artist".to_string(),
                                                        class: "w-4 h-4".to_string(),
                                                    }
                                                    "View artist"
                                                }
                                            }
                                            button {
                                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                                onclick: on_open_album_menu,
                                                Icon {
                                                    name: "plus".to_string(),
                                                    class: "w-4 h-4".to_string(),
                                                }
                                                "Add to..."
                                            }
                                            div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                                                "Rating"
                                            }
                                            div { class: "flex items-center gap-1 px-2 pb-1",
                                                for i in 1u32..=5u32 {
                                                    button {
                                                        class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                                        onclick: make_on_set_album_rating(i),
                                                        Icon {
                                                            name: if i <= album_rating() { "star-filled".to_string() } else { "star".to_string() },
                                                            class: "w-3.5 h-3.5".to_string(),
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(status) = download_status() {
                                        p { class: "text-xs text-zinc-500 mt-2", "{status}" }
                                    }
                                }
                            }

                            div { class: "space-y-1",
                                for (index , song) in songs.iter().enumerate() {
                                    {
                                        let song_clone = song.clone();
                                        let album_source_id = format!(
                                            "{}::{}",
                                            album.server_id.clone(),
                                            album.id.clone()
                                        );
                                        let songs_for_queue = songs.clone();
                                        let app_settings = app_settings.clone();
                                        let mut download_status = download_status.clone();
                                        rsx! {
                                            AlbumSongRow {
                                                song: song.clone(),
                                                index: index + 1,
                                                onclick: move |_| {
                                                    let settings = app_settings();
                                                    let playable = if settings.offline_mode {
                                                        songs_for_queue
                                                            .iter()
                                                            .filter(|song| is_song_downloaded(song))
                                                            .cloned()
                                                            .collect::<Vec<_>>()
                                                    } else {
                                                        songs_for_queue.clone()
                                                    };
                                                    if playable.is_empty() {
                                                        download_status.set(Some(
                                                            "No downloaded songs in this album are available for offline playback."
                                                                .to_string(),
                                                        ));
                                                        return;
                                                    }
                                                    let playable = assign_collection_queue_meta(
                                                        playable,
                                                        QueueSourceKind::Album,
                                                        album_source_id.clone(),
                                                    );
                                                    let target_index = playable
                                                        .iter()
                                                        .position(|entry| entry.id == song_clone.id)
                                                        .unwrap_or(0);
                                                    queue.set(playable.clone());
                                                    queue_index.set(target_index);
                                                    now_playing.set(Some(playable[target_index].clone()));
                                                    is_playing.set(true);
                                                    let shuffle = shuffle_enabled();
                                                    if shuffle {
                                                        let _ = apply_collection_shuffle_mode(
                                                            queue.clone(),
                                                            queue_index.clone(),
                                                            now_playing.clone(),
                                                            true,
                                                        );
                                                    }
                                                },
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
                                name: "album".to_string(),
                                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                            }
                            p { class: "text-zinc-400", "Album not found" }
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
            }
        }
    }
}
