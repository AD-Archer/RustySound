//! Playlist page: stage band with the cover, Play / Shuffle (Download is in "…"), and a
//! windowed track list (playlists run to thousands of songs).
//!
//! On the user's own playlists (see `edits::playlist_editable`) rows always
//! drag by their handle and swipe for Remove / Add to…, with Undo; there is
//! no Edit mode. "Add Songs" sits above the list, and the "…" menu also
//! holds Add Songs, Rename and Delete. Other playlists' rows swipe for
//! Play Next / Add to….

mod add_songs;
mod edits;

use crate::api::*;
use crate::components::ui::{
    use_layout, Menu, MenuAction, MenuGap, MenuItem, Artwork, EmptyState,
    Button, ButtonVariant, RowGestureList, RowSwipe, SectionHeader, StatusToast, TrackRow, UndoToast,
    VirtualList,
};
use crate::components::views::album_detail::download_with_progress;
use crate::components::views::library::hero::{DetailHero, DetailHeroSkeleton};
use crate::components::views::library::playlist_sheets::{DeletePlaylistSheet, RenamePlaylistSheet};
use crate::components::views::library::song_flow::{use_song_runs, wants_flow, SongFlow};
use crate::components::views::library::shared::{
    use_servers,
    is_auto_imported, song_key, songs_summary, use_downloaded_keys, use_now_playing, use_player,
    use_song_patches, ErrorState, ListSkeleton,
};
use crate::components::{
    can_share, share_link, AddIntent, AddMenuController, AppView, Navigation, PlaylistSync,
    ShareTarget, SyncStatus,
};
use crate::db::AppSettings;
use crate::offline_audio::{
    mark_collection_downloaded, mark_playlist_auto_download_tracked,
    sync_downloaded_collection_members,
};
use crate::components::views::discover::saved::is_saved_mix_copy;
use add_songs::AddSongsSheet;
use dioxus::prelude::*;
use edits::{playlist_editable, use_playlist_edits, PLAYLIST_LIST, REMOVE_FROM_PLAYLIST};

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
    let toast = use_context::<StatusToast>();
    // The data resource below takes `server_id`.
    let share_server = server_id.clone();
    let layout = use_layout();
    let player = use_player();
    let mut retry = use_signal(|| 0u32);
    let mut songs = use_signal(Vec::<Song>::new);
    let mut meta_override = use_signal(|| None::<Playlist>);
    let mut status = use_signal(|| None::<String>);
    let mut sheet = use_signal(|| None::<Sheet>);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut dl_busy = use_signal(|| false);
    let dl_progress = use_signal(|| None::<(usize, usize)>);
    // "Remove Songs from Favorites" asks first (it can undo many hearts).
    let mut confirm_unstar = use_signal(|| false);
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    use_song_patches(songs);
    let downloaded = use_downloaded_keys(songs);
    let (playing_key, live) = use_now_playing();
    let runs = use_song_runs(songs);
    // Re-fetched (quietly: the page keeps showing the old list meanwhile)
    // after each playlist sync pass.
    let sync = use_context::<PlaylistSync>();

    let data = use_resource(use_reactive!(|(playlist_id, server_id)| {
        let _ = retry();
        let _ = (sync.revision)();
        let server = servers().into_iter().find(|s| s.id == server_id);
        async move {
            let Some(server) = server else {
                return Err("This playlist's server isn't configured.".to_string());
            };
            NavidromeClient::new(server).get_playlist(&playlist_id).await
        }
    }));

    use_effect(move || {
        if let Some(Ok((_, list))) = &*data.read() {
            songs.set(list.clone());
            meta_override.set(None);
        }
    });

    let edits = use_playlist_edits(songs, data, meta_override, servers, status, retry, player);
    let mut undo = edits.undo;

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
    let editable = playlist_editable(&playlist, server.as_ref());
    let read_only = !editable;
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
    // The saved copy of a generated mix is the user's playlist (no RustySound
    // mark), but saving that mix again replaces it, so say so.
    let mix_copy = is_saved_mix_copy(
        &app_settings.read().discover_saved_mix_playlists,
        &playlist.server_id,
        &playlist.id,
    );
    let note = if auto_imported {
        Some("Auto-imported from a playlist file on the server, so it's read-only here.".to_string())
    } else if mix_copy {
        Some("Saved from a RustySound mix. Saving that mix again replaces this playlist.".to_string())
    } else {
        playlist.comment.clone().filter(|c| !c.trim().is_empty())
    };
    let source = Some((
        QueueSourceKind::Playlist,
        format!("{}::{}", playlist.server_id, playlist.id),
    ));
    let can_download = !cfg!(target_arch = "wasm32");
    let row_h = 58.0;
    // Cover Flow of the songs in the stage band.
    let show_flow = wants_flow(&runs.read(), count > 0, playlist.song_count as usize);
    let wide = !layout.is_compact();

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

    // Subsonic has no readable "favorite playlist" (Navidrome 0.64 accepts
    // star on a playlist id but never reports it back), so the playlist menu
    // offers what favorites really are: starring every song in it, labelled
    // as such. Optimistic, rolled back for the songs a failed call missed.
    let all_starred = count > 0 && songs.read().iter().all(|s| s.starred.is_some());
    let star_all = {
        let server = server.clone();
        move || {
            menu_at.set(None);
            confirm_unstar.set(false);
            let Some(server) = server.clone() else { return };
            let should = !songs.peek().iter().all(|s| s.starred.is_some());
            let before: Vec<Option<String>> = songs.peek().iter().map(|s| s.starred.clone()).collect();
            let ids: Vec<String> = songs
                .peek()
                .iter()
                .filter(|s| s.starred.is_some() != should)
                .map(|s| s.id.clone())
                .collect();
            if ids.is_empty() {
                return;
            }
            let marker = should.then(|| "local".to_string());
            songs.with_mut(|list| list.iter_mut().for_each(|s| s.starred = marker.clone()));
            spawn(async move {
                let total = ids.len();
                let client = NavidromeClient::new(server.clone());
                let done = match client.star_songs(&ids, should).await {
                    Ok(done) => done,
                    Err((done, err)) => {
                        status.set(Some(format!("Couldn't update Favorites for {} songs: {err}", total - done)));
                        done
                    }
                };
                let updated: std::collections::HashSet<&String> = ids.iter().take(done).collect();
                songs.with_mut(|list| {
                    for (song, prior) in list.iter_mut().zip(before.iter()) {
                        if !updated.contains(&song.id) && prior.is_some() != should {
                            song.starred = prior.clone();
                        }
                    }
                });
                // Keep the queue and the player's heart in step.
                let in_set = |s: &Song| s.server_id == server.id && updated.contains(&s.id);
                queue.with_mut(|items| items.iter_mut().filter(|s| in_set(s)).for_each(|s| s.starred = marker.clone()));
                if now_playing.peek().as_ref().map(|s| in_set(s)).unwrap_or(false) {
                    now_playing.with_mut(|s| {
                        if let Some(s) = s.as_mut() {
                            s.starred = marker.clone();
                        }
                    });
                }
                if done == total {
                    status.set(Some(if should {
                        format!("Added {done} songs to Favorites.")
                    } else {
                        format!("Removed {done} songs from Favorites.")
                    }));
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
                flow: show_flow.then(|| rsx! {
                    SongFlow {
                        songs,
                        runs,
                        list_testid: "playlist-tracks".to_string(),
                        wide,
                        label: format!("Songs in {}", playlist.name),
                        on_play: {
                            let mut play_from = play_from.clone();
                            move |index: usize| play_from(index)
                        },
                    }
                }),
            }

            div { class: "rs-library-page",
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
                if let Some(detail) = dl_detail.clone() {
                    p { class: "rs-library-status", role: "status", "Downloading {detail}…" }
                }
                if let Some(text) = status() {
                    p { class: "rs-library-status", role: "status", "{text}" }
                }
                div { class: "rs-library-sync", SyncStatus { testid: "playlist-sync".to_string() } }

                if count == 0 {
                    EmptyState {
                        icon: "playlist".to_string(),
                        title: "This playlist is empty".to_string(),
                        message: if read_only { "There's nothing in it yet.".to_string() } else { "Add songs from any track menu, or here with Add Songs.".to_string() },
                        if !read_only {
                            Button {
                                label: "Add Songs".to_string(),
                                icon: "plus".to_string(),
                                testid: "playlist-add-songs".to_string(),
                                onclick: move |_| sheet.set(Some(Sheet::AddSongs)),
                            }
                        }
                    }
                } else {
                    if editable {
                        // The one place to add songs without opening a menu:
                        // a quiet text action above the list.
                        SectionHeader {
                            title: "Songs".to_string(),
                            action: "Add Songs".to_string(),
                            on_action: move |_| sheet.set(Some(Sheet::AddSongs)),
                            testid: "playlist-add-songs".to_string(),
                        }
                    }
                    RowGestureList {
                        id: PLAYLIST_LIST,
                        reorder: (editable && count > 1).then(|| (0, count - 1)),
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
                                let playing = playing_now.as_deref() == Some(key.as_str());
                                rsx! {
                                    TrackRow {
                                        key: "{key}:{index}",
                                        playing,
                                        live: playing && live_now,
                                        downloaded: downloaded.read().contains(&key),
                                        index,
                                        song,
                                        handle: editable && count > 1,
                                        swipe: if editable { RowSwipe::Edit } else { RowSwipe::Browse },
                                        remove_label: editable.then(|| REMOVE_FROM_PLAYLIST.to_string()),
                                        on_play: {
                                            let mut play_from = play_from.clone();
                                            move |_| play_from(index)
                                        },
                                    }
                                }
                            },
                        }
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
                        label: if all_starred { "Remove Songs from Favorites".to_string() } else { "Add Songs to Favorites".to_string() },
                        icon: if all_starred { "heart-filled".to_string() } else { "heart".to_string() },
                        detail: Some(count.to_string()),
                        disabled: count == 0,
                        testid: "playlist-menu-favorite-songs".to_string(),
                        onclick: {
                            let mut star_all = star_all.clone();
                            move |_| {
                                if all_starred {
                                    menu_at.set(None);
                                    confirm_unstar.set(true);
                                } else {
                                    star_all();
                                }
                            }
                        },
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
                    if can_share(&servers.read(), &share_server, app_settings.read().offline_mode) {
                        MenuItem {
                            label: "Share Link…".to_string(),
                            icon: "share".to_string(),
                            testid: "playlist-menu-share".to_string(),
                            onclick: {
                                let playlist = playlist.clone();
                                let server_id = share_server.clone();
                                move |_| {
                                    menu_at.set(None);
                                    share_link(
                                        &servers.peek(),
                                        toast,
                                        ShareTarget {
                                            server_id: server_id.clone(),
                                            id: playlist.id.clone(),
                                            title: playlist.name.clone(),
                                        },
                                    );
                                }
                            },
                        }
                    }
                    MenuGap {}
                    if read_only {
                        div { class: "rs-menu__note",
                            if auto_imported {
                                "Auto-imported playlists are read-only. Edit the playlist file on the server instead."
                            } else {
                                "This playlist belongs to someone else, so it's read-only here."
                            }
                        }
                    } else {
                        MenuItem {
                            label: "Add Songs…".to_string(),
                            icon: "plus".to_string(),
                            testid: "playlist-menu-add-songs".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                sheet.set(Some(Sheet::AddSongs));
                            },
                        }
                        MenuItem {
                            label: "Rename…".to_string(),
                            icon: "edit".to_string(),
                            testid: "playlist-menu-rename".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                sheet.set(Some(Sheet::Rename));
                            },
                        }
                        MenuItem {
                            label: "Delete Playlist…".to_string(),
                            icon: "trash".to_string(),
                            danger: true,
                            testid: "playlist-menu-delete".to_string(),
                            onclick: move |_| {
                                menu_at.set(None);
                                sheet.set(Some(Sheet::Delete));
                            },
                        }
                    }
                }
            }

            if let Some(entry) = undo() {
                UndoToast {
                    message: format!("Removed “{}” from the playlist", entry.song.title),
                    generation: entry.generation,
                    on_undo: move |_| edits.undo_remove(),
                    on_close: move |_| undo.set(None),
                }
            }

            if confirm_unstar() {
                Menu {
                    message: format!("Remove all {count} songs in “{}” from Favorites?", playlist.name),
                    on_close: move |_| confirm_unstar.set(false),
                    testid: "playlist-unstar-confirm".to_string(),
                    actions: vec![MenuAction::new("remove", "Remove from Favorites", "heart").danger()],
                    on_action: {
                        let mut star_all = star_all.clone();
                        move |_: String| star_all()
                    },
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
                        on_add: move |song: Song| edits.add(song),
                        on_close: move |_| sheet.set(None),
                    }
                },
                None => rsx! {},
            }
        }
    }
}
