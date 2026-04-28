use crate::api::*;
use crate::components::views::artist_links::{
    parse_artist_names, resolve_artist_id_for_name, ArtistNameLinks,
};
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{is_song_downloaded, prefetch_song_audio};
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

/// Song row tailored for album detail pages: adds per-song favorite toggle.
#[component]
pub fn AlbumSongRow(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(|| song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let download_busy = use_signal(|| false);
    let mut show_mobile_actions = use_signal(|| false);
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);
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

    let album_id = song.album_id.clone();
    let server_id = song.server_id.clone();
    let song_artist_names = parse_artist_names(song.artist.as_deref().unwrap_or_default());
    let direct_song_artist_id = if song_artist_names.len() == 1 {
        song.artist_id.clone()
    } else {
        None
    };
    let on_album_cover = {
        let navigation = navigation.clone();
        let album_id = album_id.clone();
        let server_id = server_id.clone();
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
            class: if is_current { "relative grid w-full grid-cols-[1.75rem_2.5rem_minmax(0,1fr)_4.5rem] items-center gap-3 p-3 rounded-xl bg-emerald-500/5 transition-colors group cursor-pointer" } else { "relative grid w-full grid-cols-[1.75rem_2.5rem_minmax(0,1fr)_4.5rem] items-center gap-3 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer" },
            onclick: move |e| {
                show_mobile_actions.set(false);
                onclick.call(e);
            },
            // Index
            if is_current {
                span { class: "flex w-7 items-center justify-center text-sm text-emerald-400 justify-self-center",
                    Icon {
                        name: "play".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            } else {
                span { class: "flex w-7 items-center justify-center text-sm text-zinc-500 group-hover:hidden justify-self-center", "{index}" }
                span { class: "hidden w-7 items-center justify-center text-sm text-white group-hover:flex justify-self-center",
                    Icon {
                        name: "play".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            }
            // Cover
            if album_id.is_some() {
                button {
                    class: "rs-song-art w-10 h-10 rounded bg-zinc-800 overflow-hidden justify-self-center pointer-events-none md:pointer-events-auto",
                    aria_label: "Open album",
                    onclick: on_album_cover,
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
                div { class: "rs-song-art w-10 h-10 rounded bg-zinc-800 overflow-hidden justify-self-center",
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
            div { class: "min-w-0 flex flex-col items-center text-center md:items-start md:text-left",
                p { class: if is_current { "max-w-full truncate text-sm font-medium text-emerald-400 transition-colors" } else { "max-w-full truncate text-sm font-medium text-white group-hover:text-emerald-400 transition-colors" },
                    "{song.title}"
                }
                div { class: "mt-1 inline-flex max-w-full items-center justify-center gap-1 text-xs text-zinc-400 md:justify-start",
                    ArtistNameLinks {
                        artist_text: song.artist.clone().unwrap_or_default(),
                        server_id: song.server_id.clone(),
                        fallback_artist_id: song.artist_id.clone(),
                        container_class: "inline-flex max-w-full min-w-0 items-center gap-1 justify-center md:justify-start".to_string(),
                        button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors".to_string(),
                        separator_class: "text-zinc-500".to_string(),
                    }
                    if downloaded() {
                        Icon {
                            name: "download".to_string(),
                            class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                        }
                    }
                }
            }
            div { class: "relative flex items-center justify-center gap-1 justify-self-center",
                button {
                    class: if is_favorited() { "p-1.5 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 transition-colors" } else { "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors" },
                    aria_label: if is_favorited() { "Unfavorite" } else { "Favorite" },
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                button {
                    class: "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors",
                    aria_label: "Song actions",
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
                        style: anchored_menu_style(menu_x(), menu_y(), 176.0, 360.0),
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
                        if album_id.is_some() {
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
                                    key: "album-row-menu-artist-{song.id}-{artist_name}",
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
