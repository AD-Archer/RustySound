//! Legacy (Tailwind) album card and song row, moved out of the old Home view.
//! Still used by Albums, Artist detail, Favorites, Search and Random until
//! their phase-2 rebuilds drop them; delete this file once nothing imports it.

use super::artist_links::{parse_artist_names, resolve_artist_id_for_name, ArtistNameLinks};
use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta,
};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{
    download_songs_batch, is_album_downloaded, is_song_downloaded, mark_collection_downloaded,
    prefetch_song_audio, sync_downloaded_collection_members,
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
pub fn AlbumCard(album: Album, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let is_favorited = use_signal(|| album.starred.is_some());
    let album_rating = use_signal(|| album.user_rating.unwrap_or(0).min(5));
    let mut show_context_menu = use_signal(|| false);
    let download_busy = use_signal(|| false);
    let downloaded = use_signal(|| is_album_downloaded(&album.server_id, &album.id));
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    let on_play_album = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            let servers_snapshot = servers();
            let Some(server) = servers_snapshot
                .iter()
                .find(|server| server.id == album.server_id)
                .cloned()
            else {
                return;
            };
            let settings_snapshot = app_settings();
            let album_meta = album.clone();
            let source_id = format!("{}::{}", album_meta.server_id, album_meta.id);
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let shuffle_enabled = shuffle_enabled.clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                if let Ok((_, songs)) = client.get_album(&album_meta.id).await {
                    let playable = if settings_snapshot.offline_mode {
                        songs
                            .into_iter()
                            .filter(|song| is_song_downloaded(song))
                            .collect::<Vec<_>>()
                    } else {
                        songs
                    };
                    if playable.is_empty() {
                        return;
                    }
                    let playable =
                        assign_collection_queue_meta(playable, QueueSourceKind::Album, source_id);
                    queue.set(playable.clone());
                    queue_index.set(0);
                    now_playing.set(Some(playable[0].clone()));
                    is_playing.set(true);
                    if shuffle_enabled() {
                        let _ = apply_collection_shuffle_mode(
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            true,
                        );
                    }
                }
            });
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

    let on_open_menu_for_context = {
        let mut add_menu = add_menu.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            add_menu.open(AddIntent::from_album(&album));
        }
    };

    let on_toggle_favorite = {
        let servers = servers.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let mut is_favorited = is_favorited.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            let should_star = !is_favorited();
            let servers = servers.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                let servers_snapshot = servers();
                if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&album_id, "album").await
                    } else {
                        client.unstar(&album_id, "album").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                }
            });
        }
    };
    let make_on_toggle_favorite = {
        let on_toggle_favorite = on_toggle_favorite.clone();
        move || {
            let mut on_toggle_favorite = on_toggle_favorite.clone();
            move |evt: MouseEvent| on_toggle_favorite(evt)
        }
    };

    let make_on_set_album_rating = {
        let servers = servers.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let album_rating = album_rating.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            let mut album_rating = album_rating.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                let normalized = new_rating.min(5);
                album_rating.set(normalized);
                let servers = servers.clone();
                let album_id = album_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&album_id, normalized).await;
                    }
                });
            }
        }
    };

    let album_artist_names = parse_artist_names(&album.artist);
    let direct_album_artist_id = if album_artist_names.len() == 1 {
        album.artist_id.clone()
    } else {
        None
    };
    let make_on_view_artist_from_menu_named = {
        let servers = servers.clone();
        let navigation = navigation.clone();
        let server_id = album.server_id.clone();
        let show_context_menu = show_context_menu.clone();
        let direct_album_artist_id = direct_album_artist_id.clone();
        move |artist_name: String| {
            let servers = servers.clone();
            let navigation = navigation.clone();
            let server_id = server_id.clone();
            let mut show_context_menu = show_context_menu.clone();
            let direct_album_artist_id = direct_album_artist_id.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_context_menu.set(false);
                if let Some(artist_id) = direct_album_artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id: server_id.clone(),
                    });
                    return;
                }
                let server = servers().iter().find(|s| s.id == server_id).cloned();
                let Some(server) = server else {
                    return;
                };
                let navigation = navigation.clone();
                let server_id = server_id.clone();
                let artist_name = artist_name.clone();
                spawn(async move {
                    if let Some(artist_id) = resolve_artist_id_for_name(server, artist_name).await {
                        navigation.navigate_to(AppView::ArtistDetailView {
                            artist_id,
                            server_id,
                        });
                    }
                });
            }
        }
    };

    let on_view_album_from_menu = {
        let navigation = navigation.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            navigation.navigate_to(AppView::AlbumDetailView {
                album_id: album_id.clone(),
                server_id: server_id.clone(),
            });
        }
    };

    let on_download_album = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        let mut download_busy = download_busy.clone();
        let mut downloaded = downloaded.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            if download_busy() || downloaded() {
                return;
            }
            let servers_snapshot = servers();
            let Some(server) = servers_snapshot
                .iter()
                .find(|server| server.id == album.server_id)
                .cloned()
            else {
                return;
            };
            let settings_snapshot = app_settings();
            let album_meta = album.clone();
            download_busy.set(true);
            spawn(async move {
                let client = NavidromeClient::new(server);
                if let Ok((_, songs)) = client.get_album(&album_meta.id).await {
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
                        downloaded.set(true);
                    }
                }
                download_busy.set(false);
            });
        }
    };
    let make_on_download_album = {
        let on_download_album = on_download_album.clone();
        move || {
            let mut on_download_album = on_download_album.clone();
            move |evt: MouseEvent| on_download_album(evt)
        }
    };

    rsx! {
        div {
            class: "rs-album-card relative group text-left cursor-pointer w-full",
            onclick: move |e| {
                show_context_menu.set(false);
                onclick.call(e);
            },
            // Album cover
            div { class: "rs-album-art aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-12 h-12 text-zinc-500".to_string(),
                                }
                            }
                        },
                    }
                }
                div { class: "absolute inset-0 bg-black/20 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" }
                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    button {
                        class: "p-3 rounded-full bg-emerald-500/95 text-white hover:bg-emerald-400 hover:scale-105 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300/70 transition-all opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10 shadow-xl pointer-events-auto",
                        aria_label: "Play album",
                        title: "Play album",
                        onclick: on_play_album,
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 ml-0.5".to_string(),
                        }
                    }
                }
                button {
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/85 text-zinc-200 hover:text-white hover:bg-zinc-800 hover:scale-105 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-zinc-300/60 transition-all opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10",
                    aria_label: "Album options",
                    title: "More options",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        menu_x.set(coords.x);
                        menu_y.set(coords.y);
                        show_context_menu.set(!show_context_menu());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            }
            // Album info
            div { class: "flex items-center gap-2 w-full min-w-0",
                p {
                    class: "font-medium text-white text-sm group-hover:text-emerald-400 transition-colors truncate max-w-full min-w-0",
                    title: "{album.name}",
                    "{album.name}"
                }
                button {
                    class: if is_favorited() { "flex-shrink-0 p-1 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 transition-colors" } else { "flex-shrink-0 p-1 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors" },
                    aria_label: if is_favorited() { "Unfavorite album" } else { "Favorite album" },
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-3.5 h-3.5".to_string(),
                    }
                }
            }
            div {
                class: "max-w-full inline-flex items-center gap-1 text-xs text-zinc-400 truncate",
                title: "{album.artist}",
                if downloaded() {
                    Icon {
                        name: "download".to_string(),
                        class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                    }
                }
                ArtistNameLinks {
                    artist_text: album.artist.clone(),
                    server_id: album.server_id.clone(),
                    fallback_artist_id: album.artist_id.clone(),
                    container_class: "inline-flex max-w-full min-w-0 items-center gap-1".to_string(),
                    button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors"
                        .to_string(),
                    separator_class: "text-zinc-500".to_string(),
                }
            }
            // Context menu
            if show_context_menu() {
                div {
                    class: "fixed inset-0 z-[9998]",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_context_menu.set(false);
                    },
                }
                div {
                    class: "fixed z-[9999] w-52 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                    style: anchored_menu_style(menu_x(), menu_y(), 208.0, 360.0),
                    onclick: move |evt: MouseEvent| evt.stop_propagation(),
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: on_view_album_from_menu,
                        Icon {
                            name: "album".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "View album"
                    }
                    button {
                        class: if shuffle_enabled() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10 hover:bg-emerald-500/20 transition-colors" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                        onclick: on_toggle_shuffle,
                        Icon {
                            name: "shuffle".to_string(),
                            class: if shuffle_enabled() { "w-4 h-4 text-emerald-300".to_string() } else { "w-4 h-4".to_string() },
                        }
                        if shuffle_enabled() {
                            "Shuffle: On"
                        } else {
                            "Shuffle: Off"
                        }
                    }
                    if downloaded() {
                        div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                            Icon {
                                name: "check".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Downloaded"
                        }
                    } else {
                        button {
                            class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                            disabled: download_busy(),
                            onclick: make_on_download_album(),
                            Icon {
                                name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if download_busy() {
                                "Downloading..."
                            } else {
                                "Download"
                            }
                        }
                    }
                    div { class: "border-t border-zinc-700/60 my-1" }
                    if !album_artist_names.is_empty() {
                        for artist_name in album_artist_names.iter() {
                            button {
                                key: "album-menu-artist-{artist_name}",
                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                onclick: make_on_view_artist_from_menu_named(artist_name.clone()),
                                Icon {
                                    name: "artist".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                if album_artist_names.len() > 1 {
                                    "View {artist_name}"
                                } else {
                                    "View artist"
                                }
                            }
                        }
                    }
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: on_open_menu_for_context,
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
        }
    }
}

#[component]
pub fn SongRow(
    song: Song,
    index: usize,
    onclick: EventHandler<MouseEvent>,
    #[props(default = true)] show_download: bool,
    #[props(default = true)] show_duration: bool,
    #[props(default)] show_favorite_indicator: bool,
    #[props(default)] show_duration_in_menu: bool,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(|| song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let download_busy = use_signal(|| false);
    let mut show_mobile_actions = use_signal(|| false);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 80))
        });

    let album_id = song.album_id.clone();
    let server_id = song.server_id.clone();

    let on_album_click_cover = {
        let album_id = album_id.clone();
        let server_id = server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id_val,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let make_on_view_album = {
        let navigation = navigation.clone();
        let album_id = song.album_id.clone();
        let server_id = song.server_id.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let navigation = navigation.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if let Some(album_id_val) = album_id.clone() {
                    navigation.navigate_to(AppView::AlbumDetailView {
                        album_id: album_id_val,
                        server_id: server_id.clone(),
                    });
                }
            }
        }
    };

    let song_artist_names = parse_artist_names(song.artist.as_deref().unwrap_or_default());
    let direct_song_artist_id = if song_artist_names.len() == 1 {
        song.artist_id.clone()
    } else {
        None
    };
    let make_on_view_artist_named = {
        let servers = servers.clone();
        let navigation = navigation.clone();
        let server_id = song.server_id.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        let direct_song_artist_id = direct_song_artist_id.clone();
        move |artist_name: String| {
            let servers = servers.clone();
            let navigation = navigation.clone();
            let server_id = server_id.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            let direct_song_artist_id = direct_song_artist_id.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if let Some(artist_id_val) = direct_song_artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id: artist_id_val,
                        server_id: server_id.clone(),
                    });
                    return;
                }
                let server = servers().iter().find(|s| s.id == server_id).cloned();
                let Some(server) = server else {
                    return;
                };
                let navigation = navigation.clone();
                let server_id = server_id.clone();
                let artist_name = artist_name.clone();
                spawn(async move {
                    if let Some(artist_id) = resolve_artist_id_for_name(server, artist_name).await {
                        navigation.navigate_to(AppView::ArtistDetailView {
                            artist_id,
                            server_id,
                        });
                    }
                });
            }
        }
    };

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                add_menu.open(AddIntent::from_song(song.clone()));
            }
        }
    };

    let make_on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let current_rating = current_rating.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut current_rating = current_rating.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let normalized = new_rating.min(5);
                current_rating.set(normalized);
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&song_id, normalized).await;
                    }
                });
            }
        }
    };

    let make_on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let queue = queue.clone();
        let is_favorited = is_favorited.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut queue = queue.clone();
            let mut is_favorited = is_favorited.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let should_star = !is_favorited();
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    let servers_snapshot = servers();
                    if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let result = if should_star {
                            client.star(&song_id, "song").await
                        } else {
                            client.unstar(&song_id, "song").await
                        };
                        if result.is_ok() {
                            is_favorited.set(should_star);
                            queue.with_mut(|items| {
                                for s in items.iter_mut() {
                                    if s.id == song_id {
                                        s.starred = if should_star {
                                            Some("local".to_string())
                                        } else {
                                            None
                                        };
                                    }
                                }
                            });
                        }
                    }
                });
            }
        }
    };

    let make_on_download_song = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let song = song.clone();
        let download_busy = download_busy.clone();
        let downloaded = downloaded.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let app_settings = app_settings.clone();
            let song = song.clone();
            let mut download_busy = download_busy.clone();
            let mut downloaded = downloaded.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if download_busy() || downloaded() {
                    return;
                }
                let servers_snapshot = servers();
                if servers_snapshot.is_empty() {
                    return;
                }
                let mut settings_snapshot = app_settings();
                settings_snapshot.downloads_enabled = true;
                download_busy.set(true);
                let song = song.clone();
                spawn(async move {
                    if prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot)
                        .await
                        .is_ok()
                    {
                        downloaded.set(true);
                    }
                    download_busy.set(false);
                });
            }
        }
    };

    rsx! {
        div {
            class: "relative w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer",
            onclick: move |e| {
                show_mobile_actions.set(false);
                onclick.call(e);
            },
            // Index
            span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{index}" }
            span { class: "w-6 h-6 hidden group-hover:inline-flex items-center justify-center rounded-full bg-emerald-500/95 text-white shadow-lg transition-all group-hover:scale-105 group-hover:-translate-y-0.5",
                Icon {
                    name: "play".to_string(),
                    class: "w-3.5 h-3.5 ml-0.5".to_string(),
                }
            }
            // Cover
            if album_id.is_some() {
                button {
                    class: "rs-song-art w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0 pointer-events-none md:pointer-events-auto",
                    aria_label: "Open album",
                    onclick: on_album_click_cover,
                    {
                        match cover_url {
                            Some(url) => rsx! {
                                img { class: "w-full h-full object-cover", src: "{url}" }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                }
                            },
                        }
                    }
                }
            } else {
                div { class: "rs-song-art w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                    {
                        match cover_url {
                            Some(url) => rsx! {
                                img { class: "w-full h-full object-cover", src: "{url}" }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                }
                            },
                        }
                    }
                }
            }
            // Song info
            div { class: "flex-1 min-w-0 text-left",
                div { class: "flex items-center justify-between gap-2 min-w-0",
                    div { class: "flex flex-col min-w-0 flex-1",
                        p { class: "min-w-0 text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                            "{song.title}"
                        }
                        div { class: "mt-1 max-w-full inline-flex items-center gap-1 text-xs text-zinc-400",
                            ArtistNameLinks {
                                artist_text: song.artist.clone().unwrap_or_default(),
                                server_id: song.server_id.clone(),
                                fallback_artist_id: song.artist_id.clone(),
                                container_class: "inline-flex max-w-full min-w-0 items-center gap-1".to_string(),
                                button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors"
                                    .to_string(),
                                separator_class: "text-zinc-500".to_string(),
                            }
                            if show_download && downloaded() {
                                Icon {
                                    name: "download".to_string(),
                                    class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-1 flex-shrink-0 -mr-1",
                        button {
                            class: if is_favorited() { "p-1.5 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all" } else { "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all" },
                            aria_label: if is_favorited() { "Unfavorite" } else { "Favorite" },
                            title: if show_favorite_indicator { if is_favorited() { "Favorited" } else { "Not favorited" } } else if is_favorited() { "Unfavorite" } else { "Favorite" },
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                        }
                        button {
                            class: "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all",
                            aria_label: "Song actions",
                            title: "More options",
                            onclick: move |evt: MouseEvent| {
                                evt.stop_propagation();
                                let coords = evt.client_coordinates();
                                menu_x.set(coords.x);
                                menu_y.set(coords.y);
                                show_mobile_actions.set(!show_mobile_actions());
                            },
                            Icon {
                                name: "more-horizontal".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    }
                }
                if show_mobile_actions() {
                    div {
                        class: "fixed inset-0 z-[9998]",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            show_mobile_actions.set(false);
                        },
                    }
                    div {
                        class: "fixed z-[9999] w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                        style: anchored_menu_style(menu_x(), menu_y(), 176.0, 320.0),
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_open_menu(),
                            Icon {
                                name: "plus".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Add To..."
                        }
                        if song.album_id.is_some() {
                            button {
                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                onclick: make_on_view_album(),
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "View album"
                            }
                        }
                        if !song_artist_names.is_empty() {
                            for artist_name in song_artist_names.iter() {
                                button {
                                    key: "song-row-menu-artist-{song.id}-{artist_name}",
                                    class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                    onclick: make_on_view_artist_named(artist_name.clone()),
                                    Icon {
                                        name: "artist".to_string(),
                                        class: "w-4 h-4".to_string(),
                                    }
                                    if song_artist_names.len() > 1 {
                                        "View {artist_name}"
                                    } else {
                                        "View artist"
                                    }
                                }
                            }
                        }
                        if show_download {
                            if downloaded() {
                                div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                                    Icon {
                                        name: "check".to_string(),
                                        class: "w-4 h-4".to_string(),
                                    }
                                    "Downloaded"
                                }
                            } else {
                                button {
                                    class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                                    disabled: download_busy(),
                                    onclick: make_on_download_song(),
                                    Icon {
                                        name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                    if download_busy() {
                                        "Downloading..."
                                    } else {
                                        "Download"
                                    }
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if is_favorited() {
                                "Unfavorite"
                            } else {
                                "Favorite"
                            }
                        }
                        div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                            "Rating"
                        }
                        div { class: "flex items-center gap-1 px-2 pb-1",
                            for i in 1..=5 {
                                button {
                                    class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                    onclick: make_on_set_rating(i as u32),
                                    Icon {
                                        name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                        class: "w-3.5 h-3.5".to_string(),
                                    }
                                }
                            }
                        }
                        if show_duration || show_duration_in_menu {
                            div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                                "Length"
                            }
                            p { class: "px-2.5 pb-2 text-xs text-zinc-300",
                                "{format_duration(song.duration)}"
                            }
                        }
                    }
                }
            }
        }
    }
}
