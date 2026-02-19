use crate::api::*;
use crate::components::views::album_song_row::AlbumSongRow;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{download_songs_batch, is_song_downloaded, mark_collection_downloaded};
use dioxus::prelude::*;

#[component]
pub fn AlbumDetailView(album_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let download_busy = use_signal(|| false);
    let download_status = use_signal(|| None::<String>);

    let server = servers().into_iter().find(|s| s.id == server_id);

    let album_data = use_resource(move || {
        let server = server.clone();
        let album_id = album_id.clone();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_album(&album_id).await.ok()
            } else {
                None
            }
        }
    });

    let mut is_favorited = use_signal(|| false);

    let on_play_all = {
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
                    queue.set(playable.clone());
                    queue_index.set(0);
                    now_playing.set(Some(playable[0].clone()));
                    is_playing.set(true);
                }
            }
        }
    };

    let on_open_album_menu = {
        let album_data_ref = album_data.clone();
        let mut add_menu = add_menu.clone();
        move |_: MouseEvent| {
            if let Some(Some((album, _))) = album_data_ref() {
                add_menu.open(AddIntent::from_album(&album));
            }
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
            is_favorited.set(album.starred.is_some());
        }
    });

    let on_favorite_toggle = move |_| {
        if let Some(Some((album, _))) = album_data() {
            let server_list = servers();
            if let Some(server) = server_list
                .iter()
                .find(|s| s.id == album.server_id)
                .cloned()
            {
                let album_id = album.id.clone();
                let should_star = !is_favorited();
                let mut is_favorited = is_favorited;
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = if should_star {
                        client.star(&album_id, "album").await
                    } else {
                        client.unstar(&album_id, "album").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                });
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
                                        if let Some(artist_id) = &album.artist_id {
                                            button {
                                                class: "text-lg text-zinc-300 hover:text-emerald-400 transition-colors max-w-full min-w-0",
                                                style: "word-break: break-word; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical;",
                                                onclick: {
                                                    let artist_id = artist_id.clone();
                                                    let server_id = album.server_id.clone();
                                                    let navigation = navigation.clone();
                                                    move |evt| {
                                                        evt.stop_propagation();
                                                        navigation.navigate_to(AppView::ArtistDetailView {
                                                            artist_id: artist_id.clone(),
                                                            server_id: server_id.clone(),
                                                        });
                                                    }
                                                },
                                                "{album.artist}"
                                            }
                                        } else {
                                            p { class: "text-lg text-zinc-300 max-w-full",
                                                style: "word-break: break-word; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical;",
                                                "{album.artist}"
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
                                    div { class: "mt-6 w-full max-w-sm grid grid-cols-5 gap-2 md:max-w-none md:flex md:flex-wrap md:gap-3 justify-center md:justify-start",
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
                                            } else {
                                                "col-span-1 p-3 rounded-full border border-emerald-500/60 text-emerald-300 hover:text-white hover:border-emerald-400 transition-colors flex items-center justify-center"
                                            },
                                            disabled: download_busy(),
                                            onclick: on_download_album,
                                            title: if download_busy() { "Downloading album" } else { "Download album" },
                                            Icon {
                                                name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                                class: "w-5 h-5".to_string(),
                                            }
                                        }
                                        button {
                                            class: "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center",
                                            onclick: {
                                                let album_data_ref = album_data.clone();
                                                let app_settings = app_settings.clone();
                                                let mut download_status = download_status.clone();
                                                move |_: MouseEvent| {
                                                    if let Some(Some((_, songs))) = album_data_ref() {
                                                        if !songs.is_empty() {
                                                            let settings = app_settings();
                                                            let mut playable = if settings.offline_mode {
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
                                                            use rand::seq::SliceRandom;
                                                            playable.shuffle(&mut rand::thread_rng());
                                                            queue.set(playable.clone());
                                                            queue_index.set(0);
                                                            now_playing.set(Some(playable[0].clone()));
                                                            is_playing.set(true);
                                                        }
                                                    }
                                                }
                                            },
                                            Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
                                        }
                                        button {
                                            class: "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors flex items-center justify-center",
                                            onclick: on_favorite_toggle,
                                            Icon {
                                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                                class: "w-5 h-5".to_string(),
                                            }
                                        }
                                        button {
                                            class: "col-span-1 p-3 rounded-full border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center",
                                            onclick: on_open_album_menu,
                                            Icon { name: "plus".to_string(), class: "w-5 h-5".to_string() }
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
                                                    let target_index = playable
                                                        .iter()
                                                        .position(|entry| entry.id == song_clone.id)
                                                        .unwrap_or(0);
                                                    queue.set(playable.clone());
                                                    queue_index.set(target_index);
                                                    now_playing.set(Some(playable[target_index].clone()));
                                                    is_playing.set(true);
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
