//! Playlist page: stage band with the cover, Play / Shuffle (Download is in "…"), a
//! windowed track list (playlists run to thousands of songs), and an edit
//! mode with Move Up / Down, Remove, Rename, Delete and an Add Songs sheet.

mod add_songs;
mod edit_row;

use crate::api::*;
use crate::components::ui::{
    use_layout, Menu, MenuGap, MenuItem, Artwork, EmptyState,
    Button, ButtonVariant, TrackRow, VirtualList,
};
use crate::components::views::album_detail::download_with_progress;
use crate::components::views::library::hero::{DetailHero, DetailHeroSkeleton};
use crate::components::views::library::playlist_sheets::{DeletePlaylistSheet, RenamePlaylistSheet};
use crate::components::views::library::shared::{
    use_servers,
    is_auto_imported, song_key, songs_summary, use_downloaded_keys, use_now_playing, use_player,
    use_song_patches, ErrorState, ListSkeleton,
};
use crate::components::{AddIntent, AddMenuController, AppView, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{
    is_playlist_auto_download_tracked, mark_collection_downloaded,
    mark_playlist_auto_download_tracked, prefetch_song_audio_with_origin,
    sync_downloaded_collection_members, DownloadOrigin,
};
use add_songs::AddSongsSheet;
use dioxus::prelude::*;
use edit_row::EditRow;

#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    Rename,
    Delete,
    AddSongs,
}

#[component]
pub fn PlaylistDetailView(playlist_id: String, server_id: String) -> Element {
    let servers = use_servers();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let layout = use_layout();
    let player = use_player();
    let mut retry = use_signal(|| 0u32);
    let mut songs = use_signal(Vec::<Song>::new);
    let mut meta_override = use_signal(|| None::<Playlist>);
    let mut status = use_signal(|| None::<String>);
    let mut editing = use_signal(|| false);
    let mut sheet = use_signal(|| None::<Sheet>);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut starred = use_signal(|| false);
    let mut dl_busy = use_signal(|| false);
    let dl_progress = use_signal(|| None::<(usize, usize)>);
    use_song_patches(songs);
    let downloaded = use_downloaded_keys(songs);
    let (playing_key, live) = use_now_playing();

    let data = use_resource(use_reactive!(|(playlist_id, server_id)| {
        let _ = retry();
        let server = servers().into_iter().find(|s| s.id == server_id);
        async move {
            let Some(server) = server else {
                return Err("This playlist's server isn't configured.".to_string());
            };
            NavidromeClient::new(server).get_playlist(&playlist_id).await
        }
    }));

    use_effect(move || {
        if let Some(Ok((playlist, list))) = &*data.read() {
            songs.set(list.clone());
            starred.set(playlist.starred.is_some());
            meta_override.set(None);
        }
    });

    let base = match &*data.read() {
        None => {
            return rsx! {
                div { class: "rs-view rs-view--flush rs-library-detail",
                    DetailHeroSkeleton { back_label: "Playlists".to_string(), fallback: AppView::PlaylistsView {} }
                    div { class: "rs-library-page", ListSkeleton { rows: 10 } }
                }
            };
        }
        Some(Err(err)) => {
            let err = err.clone();
            let not_found = err.to_lowercase().contains("not found");
            return rsx! {
                div { class: "rs-view rs-library-detail",
                    if not_found {
                        EmptyState {
                            icon: "playlist".to_string(),
                            title: "Playlist not found".to_string(),
                            message: "It may have been deleted on the server.".to_string(),
                            Button {
                                label: "All Playlists".to_string(),
                                variant: ButtonVariant::Tinted,
                                onclick: move |_| navigation.navigate_to(AppView::PlaylistsView {}),
                            }
                        }
                    } else {
                        ErrorState {
                            title: "Couldn't open this playlist".to_string(),
                            error: err,
                            on_retry: move |_| retry += 1,
                        }
                    }
                }
            };
        }
        Some(Ok((playlist, _))) => playlist.clone(),
    };
    let playlist = meta_override().unwrap_or(base);

    let server = servers.read().iter().find(|s| s.id == playlist.server_id).cloned();
    let auto_imported = is_auto_imported(&playlist);
    let read_only = auto_imported;
    let count = songs.read().len();
    // Subsonic playlist durations are seconds (the old header divided by 1000).
    let total_secs: u64 = {
        let summed: u64 = songs.read().iter().map(|s| s.duration as u64).sum();
        if summed > 0 { summed } else { playlist.duration as u64 }
    };
    let meta = songs_summary(count.max(playlist.song_count as usize), total_secs);
    let all_downloaded = count > 0 && downloaded.read().len() >= count;
    let owner_line = playlist
        .owner
        .clone()
        .filter(|o| server.as_ref().map(|s| !s.username.eq_ignore_ascii_case(o)).unwrap_or(true));
    let note = if auto_imported {
        Some("Auto-imported from a playlist file on the server, so it's read-only here.".to_string())
    } else {
        playlist.comment.clone().filter(|c| !c.trim().is_empty())
    };
    let source = Some((
        QueueSourceKind::Playlist,
        format!("{}::{}", playlist.server_id, playlist.id),
    ));
    let can_download = !cfg!(target_arch = "wasm32");
    let is_editing = editing() && !read_only;
    let row_h = 58.0;
    let _ = layout;

    // ---- Actions --------------------------------------------------------------
    let play_from = {
        let source = source.clone();
        move |index: usize| {
            let list = songs.peek().clone();
            status.set(player.play(list, index, source.clone()).err());
        }
    };
    let shuffle_all = {
        let source = source.clone();
        move |_| {
            let list = songs.peek().clone();
            status.set(player.shuffle_play(list, source.clone()).err());
        }
    };

    let download = {
        let playlist = playlist.clone();
        move |_| {
            menu_at.set(None);
            if dl_busy() {
                return;
            }
            let list = songs.peek().clone();
            if list.is_empty() {
                return;
            }
            let servers_now = servers.peek().clone();
            let settings = app_settings.peek().clone();
            let playlist = playlist.clone();
            dl_busy.set(true);
            status.set(None);
            spawn(async move {
                let report = download_with_progress(list.clone(), servers_now, settings, dl_progress).await;
                mark_collection_downloaded("playlist", &playlist.server_id, &playlist.id, &playlist.name, list.len());
                mark_playlist_auto_download_tracked(&playlist.server_id, &playlist.id, &playlist.name, list.len());
                sync_downloaded_collection_members("playlist", &playlist.server_id, &playlist.id, &list);
                songs.with_mut(|_| {});
                status.set(Some(if report.failed > 0 {
                    format!("Downloaded {} songs; {} failed.", report.downloaded + report.skipped, report.failed)
                } else {
                    "Playlist downloaded.".to_string()
                }));
                let mut dl_progress = dl_progress;
                dl_progress.set(None);
                dl_busy.set(false);
            });
        }
    };

    let toggle_star = {
        let playlist = playlist.clone();
        let server = server.clone();
        move |_| {
            menu_at.set(None);
            let Some(server) = server.clone() else { return };
            let should = !starred();
            let id = playlist.id.clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                let result = if should {
                    client.star(&id, "playlist").await
                } else {
                    client.unstar(&id, "playlist").await
                };
                if result.is_ok() {
                    starred.set(should);
                }
            });
        }
    };

    // Moves a song locally, then saves the order; reloads on failure.
    let move_song = {
        let playlist = playlist.clone();
        let server = server.clone();
        move |from: usize, to: usize| {
            let mut ids = Vec::new();
            songs.with_mut(|list| {
                if from >= list.len() || to >= list.len() || from == to {
                    return;
                }
                let song = list.remove(from);
                list.insert(to, song);
                ids = list.iter().map(|s| s.id.clone()).collect();
            });
            if ids.is_empty() {
                return;
            }
            let Some(server) = server.clone() else { return };
            let id = playlist.id.clone();
            spawn(async move {
                let total = ids.len();
                let client = NavidromeClient::new(server);
                if let Err(err) = client.reorder_playlist(&id, &ids, total).await {
                    status.set(Some(format!("Couldn't save the new order: {err}")));
                    retry += 1;
                } else {
                    client.refresh_playlist_cache();
                }
            });
        }
    };

    let remove_song = {
        let playlist = playlist.clone();
        let server = server.clone();
        move |index: usize| {
            let Some(server) = server.clone() else { return };
            if index >= songs.peek().len() {
                return;
            }
            let removed = songs.write().remove(index);
            let id = playlist.id.clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                match client.remove_songs_from_playlist(&id, &[index]).await {
                    Ok(_) => client.refresh_playlist_cache(),
                    Err(err) => {
                        songs.with_mut(|list| list.insert(index.min(list.len()), removed));
                        status.set(Some(format!("Couldn't remove the song: {err}")));
                    }
                }
            });
        }
    };

    let add_song = {
        let playlist = playlist.clone();
        let server = server.clone();
        move |song: Song| {
            let Some(server) = server.clone() else { return };
            let playlist = playlist.clone();
            let settings = app_settings.peek().clone();
            let servers_now = servers.peek().clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                if let Err(err) = client.add_songs_to_playlist(&playlist.id, &[song.id.clone()]).await {
                    status.set(Some(format!("Couldn't add “{}”: {err}", song.title)));
                    return;
                }
                client.refresh_playlist_cache();
                songs.write().push(song.clone());
                // Keep a downloaded playlist complete.
                if is_playlist_auto_download_tracked(&playlist.server_id, &playlist.id) {
                    let total = songs.peek().len();
                    mark_collection_downloaded("playlist", &playlist.server_id, &playlist.id, &playlist.name, total);
                    mark_playlist_auto_download_tracked(&playlist.server_id, &playlist.id, &playlist.name, total);
                    if settings.downloads_enabled && settings.auto_downloads_enabled {
                        let mut settings = settings.clone();
                        settings.downloads_enabled = true;
                        let _ = prefetch_song_audio_with_origin(&song, &servers_now, &settings, DownloadOrigin::Auto).await;
                    }
                    let list = songs.peek().clone();
                    sync_downloaded_collection_members("playlist", &playlist.server_id, &playlist.id, &list);
                }
            });
        }
    };

    let dl_detail = dl_progress().map(|(done, total)| format!("{done} of {total}"));
    let playing_now = playing_key();
    let live_now = live();

    rsx! {
        div { class: "rs-view rs-view--flush rs-library-detail", "data-testid": "playlist-detail",
            DetailHero {
                title: playlist.name.clone(),
                back_label: "Playlists".to_string(),
                fallback: AppView::PlaylistsView {},
                art: rsx! {
                    Artwork {
                        cover_art: playlist.cover_art.clone(),
                        server_id: playlist.server_id.clone(),
                        size_px: 220,
                        fill: true,
                        hero: true,
                        eager: true,
                        alt: format!("{} cover", playlist.name),
                        placeholder: "playlist".to_string(),
                    }
                },
                subtitle: owner_line.map(|o| rsx! { span { class: "rs-library-hero__owner", "by {o}" } }),
                meta,
                badge: if all_downloaded { Some("Downloaded".to_string()) } else { None },
                note,
                more_testid: "playlist-more".to_string(),
                on_more: move |evt: MouseEvent| {
                    let p = evt.client_coordinates();
                    menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                },
            }

            div { class: "rs-library-page",
                if is_editing {
                    div { class: "rs-library-actions",
                        Button {
                            label: "Add Songs".to_string(),
                            icon: "plus".to_string(),
                            variant: ButtonVariant::Tinted,
                            block: true,
                            testid: "playlist-add-songs".to_string(),
                            onclick: move |_| sheet.set(Some(Sheet::AddSongs)),
                        }
                        Button {
                            label: "Done".to_string(),
                            icon: "check".to_string(),
                            block: true,
                            testid: "playlist-edit-done".to_string(),
                            onclick: move |_| editing.set(false),
                        }
                    }
                } else {
                    div { class: "rs-library-actions",
                        Button {
                            label: "Play".to_string(),
                            icon: "play".to_string(),
                            block: true,
                            disabled: count == 0,
                            testid: "playlist-play".to_string(),
                            onclick: {
                                let mut play_from = play_from.clone();
                                move |_| play_from(0)
                            },
                        }
                        Button {
                            label: "Shuffle".to_string(),
                            icon: "shuffle".to_string(),
                            variant: ButtonVariant::Tinted,
                            block: true,
                            disabled: count == 0,
                            testid: "playlist-shuffle".to_string(),
                            onclick: shuffle_all,
                        }
                    }
                }
                if let Some(detail) = dl_detail.clone() {
                    p { class: "rs-library-status", role: "status", "Downloading {detail}…" }
                }
                if let Some(text) = status() {
                    p { class: "rs-library-status", role: "status", "{text}" }
                }

                if count == 0 {
                    EmptyState {
                        icon: "playlist".to_string(),
                        title: "This playlist is empty".to_string(),
                        message: if read_only { "There's nothing in it yet.".to_string() } else { "Add songs from any track menu, or here with Add Songs.".to_string() },
                        if !read_only {
                            Button {
                                label: "Add Songs".to_string(),
                                icon: "plus".to_string(),
                                onclick: move |_| {
                                    editing.set(true);
                                    sheet.set(Some(Sheet::AddSongs));
                                },
                            }
                        }
                    }
                } else {
                    VirtualList {
                        item_count: count,
                        row_height: row_h,
                        class: "rs-list rs-library-songlist".to_string(),
                        testid: "playlist-tracks".to_string(),
                        render_row: move |index: usize| {
                            let Some(song) = songs.read().get(index).cloned() else {
                                return rsx! { li {} };
                            };
                            let key = song_key(&song);
                            if is_editing {
                                let last = count.saturating_sub(1);
                                return rsx! {
                                    EditRow {
                                        key: "{key}:{index}",
                                        song,
                                        index,
                                        last,
                                        on_move: {
                                            let mut move_song = move_song.clone();
                                            move |(from, to): (usize, usize)| move_song(from, to)
                                        },
                                        on_remove: {
                                            let mut remove_song = remove_song.clone();
                                            move |i: usize| remove_song(i)
                                        },
                                    }
                                };
                            }
                            let playing = playing_now.as_deref() == Some(key.as_str());
                            rsx! {
                                TrackRow {
                                    key: "{key}:{index}",
                                    playing,
                                    live: playing && live_now,
                                    downloaded: downloaded.read().contains(&key),
                                    index,
                                    song,
                                    on_play: {
                                        let mut play_from = play_from.clone();
                                        move |_| play_from(index)
                                    },
                                }
                            }
                        },
                    }
                    p { class: "rs-list-foot", "{songs_summary(count, total_secs)}" }
                }
            }

            if let Some(anchor) = menu_at() {
                Menu {
                    label: format!("{} actions", playlist.name),
                    anchor,
                    on_close: move |_| menu_at.set(None),
                    testid: "playlist-menu".to_string(),
                    MenuItem {
                        label: "Play Next".to_string(),
                        icon: "play-next".to_string(),
                        disabled: count == 0,
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), true);
                        },
                    }
                    MenuItem {
                        label: "Add to Queue".to_string(),
                        icon: "add-to-queue".to_string(),
                        disabled: count == 0,
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), false);
                        },
                    }
                    MenuItem {
                        label: "Add to Playlist…".to_string(),
                        icon: "playlist".to_string(),
                        onclick: {
                            let playlist = playlist.clone();
                            let add_menu = add_menu.clone();
                            move |_| {
                                menu_at.set(None);
                                let mut add_menu = add_menu.clone();
                                add_menu.open(AddIntent::from_playlist(&playlist));
                            }
                        },
                    }
                    MenuItem {
                        label: if starred() { "Unfavorite".to_string() } else { "Favorite".to_string() },
                        icon: if starred() { "star-filled".to_string() } else { "star".to_string() },
                        onclick: toggle_star,
                    }
                    if can_download {
                        MenuItem {
                            label: if dl_busy() { "Downloading…".to_string() } else if all_downloaded { "Download Again".to_string() } else { "Download".to_string() },
                            icon: if all_downloaded { "downloaded".to_string() } else { "download".to_string() },
                            detail: dl_detail.clone(),
                            disabled: dl_busy() || count == 0,
                            testid: "playlist-download".to_string(),
                            onclick: download.clone(),
                        }
                    }
                    MenuGap {}
                    if read_only {
                        div { class: "rs-menu__note",
                            "Auto-imported playlists are read-only. Edit the playlist file on the server instead."
                        }
                    } else {
                        MenuItem {
                            label: if editing() { "Done Editing".to_string() } else { "Edit".to_string() },
                            icon: "edit".to_string(),
                            testid: "playlist-edit".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                editing.toggle();
                            },
                        }
                        MenuItem {
                            label: "Rename…".to_string(),
                            icon: "edit".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                sheet.set(Some(Sheet::Rename));
                            },
                        }
                        MenuItem {
                            label: "Delete Playlist…".to_string(),
                            icon: "trash".to_string(),
                            danger: true,
                            onclick: move |_| {
                                menu_at.set(None);
                                sheet.set(Some(Sheet::Delete));
                            },
                        }
                    }
                }
            }

            match sheet() {
                Some(Sheet::Rename) => rsx! {
                    RenamePlaylistSheet {
                        playlist: playlist.clone(),
                        on_close: move |_| sheet.set(None),
                        on_renamed: {
                            let playlist = playlist.clone();
                            move |name: String| {
                                sheet.set(None);
                                let mut renamed = playlist.clone();
                                renamed.name = name;
                                meta_override.set(Some(renamed));
                            }
                        },
                    }
                },
                Some(Sheet::Delete) => rsx! {
                    DeletePlaylistSheet {
                        playlist: playlist.clone(),
                        on_close: move |_| sheet.set(None),
                        on_deleted: move |_| {
                            sheet.set(None);
                            navigation.navigate_to(AppView::PlaylistsView {});
                        },
                    }
                },
                Some(Sheet::AddSongs) => rsx! {
                    AddSongsSheet {
                        server: server.clone(),
                        songs,
                        on_add: {
                            let add_song = add_song.clone();
                            move |song: Song| add_song(song)
                        },
                        on_close: move |_| sheet.set(None),
                    }
                },
                None => rsx! {},
            }
        }
    }
}
