//! Album page: stage band header, Play / Shuffle, the "…" menu and the
//! numbered track list (phone-album mock).

use crate::api::*;
use crate::components::ui::{
    Menu, MenuGap, MenuItem, Artwork, EmptyState, Button, ButtonVariant,
};
use crate::components::views::album_song_row::AlbumTracks;
use crate::components::views::artist_links::ArtistNameLinks;
use crate::components::views::library::hero::{DetailHero, DetailHeroSkeleton};
use crate::components::views::library::shared::{
    use_servers,
    songs_summary, use_player, use_song_patches, ErrorState, ListSkeleton,
};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Downloads `songs` a few at a time so the menu can show progress.
pub(super) async fn download_with_progress(
    songs: Vec<Song>,
    servers: Vec<ServerConfig>,
    settings: AppSettings,
    mut progress: Signal<Option<(usize, usize)>>,
) -> crate::offline_audio::DownloadBatchReport {
    let total = songs.len();
    let mut report = crate::offline_audio::DownloadBatchReport::default();
    progress.set(Some((0, total)));
    for (index, chunk) in songs.chunks(4).enumerate() {
        let part = crate::offline_audio::download_songs_batch(chunk, &servers, &settings).await;
        report.downloaded += part.downloaded;
        report.skipped += part.skipped;
        report.failed += part.failed;
        report.purged += part.purged;
        progress.set(Some(((index * 4 + chunk.len()).min(total), total)));
    }
    report
}

#[component]
pub fn AlbumDetailView(album_id: String, server_id: String) -> Element {
    let servers = use_servers();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let player = use_player();
    let mut retry = use_signal(|| 0u32);
    let mut songs = use_signal(Vec::<Song>::new);
    let mut status = use_signal(|| None::<String>);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut rating = use_signal(|| 0u32);
    let mut dl_progress = use_signal(|| None::<(usize, usize)>);
    let mut dl_busy = use_signal(|| false);
    let mut album_downloaded = use_signal(|| false);
    use_song_patches(songs);

    let data = use_resource(use_reactive!(|(album_id, server_id)| {
        let _ = retry();
        let server = servers().into_iter().find(|s| s.id == server_id);
        async move {
            let Some(server) = server else {
                return Err("This album's server isn't configured.".to_string());
            };
            NavidromeClient::new(server).get_album(&album_id).await
        }
    }));

    use_effect(move || {
        if let Some(Ok((album, list))) = &*data.read() {
            songs.set(list.clone());
            rating.set(album.user_rating.unwrap_or(0).min(5));
            let (sid, aid) = (album.server_id.clone(), album.id.clone());
            spawn(async move {
                let done = crate::offline_audio::is_album_downloaded(&sid, &aid);
                if *album_downloaded.peek() != done {
                    album_downloaded.set(done);
                }
            });
        }
    });

    let album = match &*data.read() {
        None => {
            return rsx! {
                div { class: "rs-view rs-view--flush rs-library-detail",
                    DetailHeroSkeleton { back_label: "Albums".to_string(), fallback: AppView::Albums {} }
                    div { class: "rs-library-page", ListSkeleton { rows: 8 } }
                }
            };
        }
        Some(Err(err)) => {
            let not_found = err.to_lowercase().contains("not found");
            let err = err.clone();
            return rsx! {
                div { class: "rs-view rs-library-detail",
                    if not_found {
                        EmptyState {
                            icon: "album".to_string(),
                            title: "Album not found".to_string(),
                            message: "It may have been removed from the server, or the link is out of date.".to_string(),
                            Button {
                                label: "Browse Albums".to_string(),
                                variant: ButtonVariant::Tinted,
                                onclick: move |_| navigation.navigate_to(AppView::Albums {}),
                            }
                        }
                    } else {
                        ErrorState {
                            title: "Couldn't open this album".to_string(),
                            error: err,
                            on_retry: move |_| retry += 1,
                        }
                    }
                }
            };
        }
        Some(Ok((album, _))) => album.clone(),
    };

    let source = Some((
        QueueSourceKind::Album,
        format!("{}::{}", album.server_id, album.id),
    ));
    let total_secs: u64 = {
        let from_songs: u64 = songs.read().iter().map(|s| s.duration as u64).sum();
        if from_songs > 0 {
            from_songs
        } else {
            album.duration as u64
        }
    };
    let count = songs.read().len().max(album.song_count as usize);
    let mut meta_parts: Vec<String> = Vec::new();
    if let Some(genre) = album.genre.clone().filter(|g| !g.trim().is_empty()) {
        meta_parts.push(genre);
    }
    if let Some(year) = album.year.filter(|y| *y > 0) {
        meta_parts.push(year.to_string());
    }
    meta_parts.push(songs_summary(count, total_secs));
    let meta = meta_parts.join(" · ");
    let can_download = !cfg!(target_arch = "wasm32");

    let play_from = {
        let source = source.clone();
        move |index: usize| {
            let list = songs.peek().clone();
            status.set(player.play(list, index, source.clone()).err());
        }
    };
    let on_shuffle = {
        let source = source.clone();
        move |_| {
            let list = songs.peek().clone();
            status.set(player.shuffle_play(list, source.clone()).err());
        }
    };
    let mut close_menu = move || menu_at.set(None);

    let download_album = {
        let album = album.clone();
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
            let album = album.clone();
            dl_busy.set(true);
            status.set(None);
            spawn(async move {
                let report = download_with_progress(list.clone(), servers_now, settings, dl_progress).await;
                if report.downloaded > 0 || report.skipped > 0 {
                    crate::offline_audio::mark_collection_downloaded(
                        "album", &album.server_id, &album.id, &album.name, list.len(),
                    );
                    crate::offline_audio::sync_downloaded_collection_members(
                        "album", &album.server_id, &album.id, &list,
                    );
                    album_downloaded.set(true);
                    // Re-check the per-song marks.
                    songs.with_mut(|_| {});
                }
                status.set(Some(if report.failed > 0 {
                    format!("Downloaded {} songs; {} failed.", report.downloaded + report.skipped, report.failed)
                } else {
                    "Album downloaded.".to_string()
                }));
                dl_progress.set(None);
                dl_busy.set(false);
            });
        }
    };

    let set_rating = {
        let album = album.clone();
        move |value: u32| {
            rating.set(value);
            let server = servers.peek().iter().find(|s| s.id == album.server_id).cloned();
            let album_id = album.id.clone();
            if let Some(server) = server {
                spawn(async move {
                    let _ = NavidromeClient::new(server).set_rating(&album_id, value).await;
                });
            }
        }
    };

    let go_artist = {
        let album = album.clone();
        move |_| {
            menu_at.set(None);
            if let Some(artist_id) = album.artist_id.clone() {
                navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id,
                    server_id: album.server_id.clone(),
                });
            }
        }
    };

    let dl_detail = dl_progress().map(|(done, total)| format!("{done} of {total}"));
    let rating_now = rating();

    rsx! {
        div { class: "rs-view rs-view--flush rs-library-detail", "data-testid": "album-detail",
            DetailHero {
                title: album.name.clone(),
                back_label: "Albums".to_string(),
                fallback: AppView::Albums {},
                art: rsx! {
                    Artwork {
                        cover_art: album.cover_art.clone().or(Some(album.id.clone())),
                        server_id: album.server_id.clone(),
                        size_px: 220,
                        fill: true,
                        hero: true,
                        eager: true,
                        alt: format!("{} cover", album.name),
                        placeholder: "album".to_string(),
                    }
                },
                subtitle: if album.artist.trim().is_empty() { None } else { Some(rsx! {
                    ArtistNameLinks {
                        artist_text: album.artist.clone(),
                        server_id: album.server_id.clone(),
                        fallback_artist_id: album.artist_id.clone(),
                    }
                }) },
                meta,
                badge: if album_downloaded() { Some("Downloaded".to_string()) } else { None },
                more_testid: "album-more".to_string(),
                on_more: move |evt: MouseEvent| {
                    let p = evt.client_coordinates();
                    menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                },
            }

            div { class: "rs-library-page",
                div { class: "rs-library-actions",
                    Button {
                        label: "Play".to_string(),
                        icon: "play".to_string(),
                        block: true,
                        testid: "album-play".to_string(),
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
                        testid: "album-shuffle".to_string(),
                        onclick: on_shuffle,
                    }
                }
                // Download progress stays visible after the menu closes.
                if let Some(detail) = dl_detail.clone() {
                    p { class: "rs-library-status", role: "status", "Downloading {detail}…" }
                }
                if let Some(text) = status() {
                    p { class: "rs-library-status", role: "status", "{text}" }
                }
                if songs.read().is_empty() {
                    EmptyState {
                        icon: "album".to_string(),
                        title: "No songs".to_string(),
                        message: "This album has no songs on the server.".to_string(),
                    }
                } else {
                    AlbumTracks {
                        songs,
                        on_play: {
                            let mut play_from = play_from.clone();
                            move |index| play_from(index)
                        },
                    }
                    p { class: "rs-list-foot", "{songs_summary(songs.read().len(), total_secs)}" }
                }
            }

            if let Some(anchor) = menu_at() {
                Menu {
                    label: format!("{} actions", album.name),
                    anchor,
                    on_close: move |_| close_menu(),
                    testid: "album-menu".to_string(),
                    MenuItem {
                        label: "Play Next".to_string(),
                        icon: "play-next".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), true);
                        },
                    }
                    MenuItem {
                        label: "Add to Queue".to_string(),
                        icon: "add-to-queue".to_string(),
                        testid: "album-menu-queue".to_string(),
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(songs.peek().clone(), false);
                        },
                    }
                    MenuItem {
                        label: "Add to Playlist…".to_string(),
                        icon: "playlist".to_string(),
                        onclick: {
                            let album = album.clone();
                            let add_menu = add_menu.clone();
                            move |_| {
                                menu_at.set(None);
                                let mut add_menu = add_menu.clone();
                                add_menu.open(AddIntent::from_album(&album));
                            }
                        },
                    }
                    MenuGap {}
                    if can_download {
                        MenuItem {
                            label: if dl_busy() { "Downloading…".to_string() } else if album_downloaded() { "Download Again".to_string() } else { "Download Album".to_string() },
                            icon: "download".to_string(),
                            detail: dl_detail.clone(),
                            disabled: dl_busy(),
                            onclick: download_album,
                        }
                    }
                    MenuItem {
                        label: "Go to Artist".to_string(),
                        icon: "artist".to_string(),
                        disabled: album.artist_id.is_none(),
                        onclick: go_artist,
                    }
                    MenuGap {}
                    div { class: "rs-menu__note", "Rate album" }
                    div { class: "rs-menu__stars", role: "group", aria_label: "Album rating",
                        for value in 1..=5u32 {
                            button {
                                key: "{value}",
                                r#type: "button",
                                class: "rs-menu__star",
                                class: if value <= rating_now { "rs-menu__star--on" },
                                aria_label: "{value} star",
                                aria_pressed: if value == rating_now { "true" } else { "false" },
                                onclick: {
                                    let mut set_rating = set_rating.clone();
                                    move |_| set_rating(value)
                                },
                                Icon {
                                    name: if value <= rating_now { "star-filled".to_string() } else { "star".to_string() },
                                    class: "rs-ic rs-ic--md".to_string(),
                                }
                            }
                        }
                        button {
                            r#type: "button",
                            class: "rs-menu__star",
                            aria_label: "Clear rating",
                            onclick: {
                                let mut set_rating = set_rating.clone();
                                move |_| set_rating(0)
                            },
                            Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        }
                    }
                }
            }
        }
    }
}
