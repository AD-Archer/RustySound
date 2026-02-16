use crate::api::*;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use dioxus::prelude::*;

/// Song row tailored for album detail pages: adds per-song favorite toggle.
#[component]
pub fn AlbumSongRow(song: Song, index: usize, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
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

    let album_id = song.album_id.clone();
    let artist_id = song.artist_id.clone();
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
        let mut queue = queue.clone();
        let mut is_favorited = is_favorited.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
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
    };

    rsx! {
        div {
            class: if is_current {
                "w-full flex items-center gap-4 p-3 rounded-xl bg-emerald-500/5 transition-colors group cursor-pointer"
            } else {
                "w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer"
            },
            onclick: move |e| onclick.call(e),
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
            // Rating / favorite
            div { class: "flex items-center gap-2",
                button {
                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-500 hover:text-emerald-400 transition-colors" },
                    aria_label: "Favorite",
                    onclick: on_toggle_favorite,
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-4 h-4".to_string(),
                    }
                }
                if rating > 0 {
                    div { class: "hidden sm:flex items-center gap-1 text-amber-400",
                        for i in 1..=5 {
                            Icon {
                                name: if i <= rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
            }
            // Actions
            div { class: "flex items-center gap-3",
                button {
                    class: "p-2 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add to queue",
                    onclick: on_open_menu,
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                span { class: "text-sm text-zinc-500", "{format_duration(song.duration)}" }
            }
        }
    }
}
