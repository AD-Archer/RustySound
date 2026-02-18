use crate::api::*;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use crate::db::AppSettings;
use crate::offline_audio::{is_song_downloaded, prefetch_song_audio};
use dioxus::prelude::*;

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
    let artist_id = song.artist_id.clone();
    let server_id = song.server_id.clone();
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
            class: if is_current {
                "relative w-full flex items-center gap-4 p-3 rounded-xl bg-emerald-500/5 transition-colors group cursor-pointer"
            } else {
                "relative w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer"
            },
            onclick: move |e| {
                show_mobile_actions.set(false);
                onclick.call(e);
            },
            // Index
            if is_current {
                span { class: "w-6 text-sm text-emerald-400",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            } else {
                span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{index}" }
                span { class: "w-6 text-sm text-white hidden group-hover:block",
                    Icon { name: "play".to_string(), class: "w-4 h-4".to_string() }
                }
            }
            // Cover
            if album_id.is_some() {
                button {
                    class: "w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
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
            }
            // Song info
            div { class: "flex-1 min-w-0 text-left",
                p { class: if is_current { "text-sm font-medium text-emerald-400 truncate transition-colors max-w-full" } else { "text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors max-w-full" },
                    "{song.title}"
                }
                if artist_id.is_some() {
                    p { class: "text-xs text-zinc-400 truncate max-w-full",
                        "{song.artist.clone().unwrap_or_default()}"
                    }
                } else {
                    p { class: "text-xs text-zinc-400 truncate max-w-full",
                        "{song.artist.clone().unwrap_or_default()}"
                    }
                }
            }
            // Actions
            div { class: "flex items-center gap-2 md:gap-3 relative",
                if downloaded() {
                    span {
                        class: "hidden md:inline-flex text-emerald-400",
                        title: "Downloaded",
                        Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
                    }
                } else {
                    button {
                        class: if download_busy() {
                            "hidden md:inline-flex p-2 rounded-lg text-zinc-500 cursor-not-allowed"
                        } else {
                            "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors"
                        },
                        aria_label: "Download song",
                        disabled: download_busy(),
                        onclick: make_on_download_song(),
                        Icon {
                            name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                    }
                }
                if current_rating() > 0 {
                    div { class: "hidden md:flex items-center gap-1 text-amber-400",
                        for i in 1..=5 {
                            Icon {
                                name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
                button {
                    class: if is_favorited() { "hidden md:inline-flex p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "hidden md:inline-flex p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                button {
                    class: "hidden md:inline-flex p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: make_on_open_menu(),
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
                button {
                    class: "md:hidden p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors",
                    aria_label: "Song actions",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_mobile_actions.set(!show_mobile_actions());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                if show_mobile_actions() {
                    div {
                        class: "md:hidden absolute right-0 top-10 z-20 w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_open_menu(),
                            Icon { name: "plus".to_string(), class: "w-4 h-4".to_string() }
                            "Add To..."
                        }
                        if downloaded() {
                            div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                                Icon { name: "check".to_string(), class: "w-4 h-4".to_string() }
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
                        div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500", "Rating" }
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
                    }
                }
            }
        }
    }
}
