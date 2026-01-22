use crate::api::models::format_duration;
use crate::api::*;
use crate::components::{
    seek_to, AppView, Icon, Navigation, PlaybackPositionSignal, SeekRequestSignal,
};
use dioxus::prelude::*;

#[component]
pub fn BookmarksView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut refresh_key = use_signal(|| 0u32);

    let bookmarks = use_resource(move || {
        let _refresh = refresh_key();
        let servers = servers();
        async move {
            let active_servers: Vec<ServerConfig> =
                servers.into_iter().filter(|s| s.active).collect();
            let mut items = Vec::new();

            for server in active_servers {
                let client = NavidromeClient::new(server.clone());
                if let Ok(mut found) = client.get_bookmarks().await {
                    items.append(&mut found);
                }
            }

            items.sort_by(|a, b| {
                let changed_a = a.changed.as_deref().unwrap_or("");
                let changed_b = b.changed.as_deref().unwrap_or("");
                changed_b.cmp(changed_a)
            });
            items
        }
    });

    let has_active_server = servers().iter().any(|s| s.active);

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Bookmarks" }
                    p { class: "page-subtitle", "Resume where you left off across your library." }
                }
                button {
                    class: "px-3 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors flex items-center gap-2",
                    onclick: move |_| refresh_key.set(refresh_key() + 1),
                    Icon { name: "repeat".to_string(), class: "w-4 h-4".to_string() }
                    "Refresh"
                }
            }

            if !has_active_server {
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon { name: "bookmark".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                    h2 { class: "text-xl font-semibold text-white mb-2", "No servers connected" }
                    p { class: "text-zinc-400 text-center max-w-md", "Add a Navidrome server to fetch your bookmarks." }
                    button {
                        class: "mt-6 px-6 py-3 bg-emerald-500 hover:bg-emerald-400 text-white font-medium rounded-xl transition-colors",
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::Settings)
                        },
                        "Add server"
                    }
                }
            } else {
                match bookmarks() {
                    Some(list) => rsx! {
                        if list.is_empty() {
                            div { class: "flex flex-col items-center justify-center py-20",
                                Icon { name: "bookmark".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                                h2 { class: "text-xl font-semibold text-white mb-2", "No bookmarks yet" }
                                p { class: "text-zinc-400 text-center max-w-lg", "Create a bookmark while listening to jump back to that spot later." }
                            }
                        } else {
                            div { class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4",
                                for bookmark in list {
                                    BookmarkCard {
                                        bookmark: bookmark.clone(),
                                        on_deleted: {
                                            let mut refresh_key = refresh_key.clone();
                                            move || refresh_key.set(refresh_key() + 1)
                                        },
                                    }
                                }
                            }
                        }
                    },
                    None => rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon { name: "loader".to_string(), class: "w-8 h-8 text-zinc-500".to_string() }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn BookmarkCard(bookmark: Bookmark, on_deleted: EventHandler<()>) -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let mut seek_request = use_context::<SeekRequestSignal>().0;
    let cover_url = servers()
        .iter()
        .find(|s| s.id == bookmark.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            bookmark
                .entry
                .cover_art
                .as_ref()
                .map(|id| client.get_cover_art_url(id, 200))
        });

    let position = format_duration((bookmark.position / 1000) as u32);
    let song = bookmark.entry.clone();

    let on_resume = {
        let song = song.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        move |_| {
            let start_at = bookmark.position as f64 / 1000.0;
            seek_request.set(Some((song.id.clone(), start_at)));
            queue.set(vec![song.clone()]);
            queue_index.set(0);
            now_playing.set(Some(song.clone()));
            playback_position.set(start_at);
            seek_to(start_at);
            is_playing.set(true);
        }
    };

    let on_delete = {
        let song_id = song.id.clone();
        let server_id = bookmark.server_id.clone();
        let servers = servers.clone();
        move |_| {
            let servers = servers();
            if let Some(server) = servers.iter().find(|s| s.id == server_id).cloned() {
                let on_deleted = on_deleted.clone();
                let song_id = song_id.clone();
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    if client.delete_bookmark(&song_id).await.is_ok() {
                        on_deleted.call(());
                    }
                });
            }
        }
    };

    let on_album_cover = {
        let navigation = navigation.clone();
        let album_id = song.album_id.clone();
        let server_id = song.server_id.clone();
        move |_| {
            if let Some(album) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetail(album, server_id.clone()));
            }
        }
    };

    let on_album_text = {
        let navigation = navigation.clone();
        let album_id = song.album_id.clone();
        let server_id = song.server_id.clone();
        move |_| {
            if let Some(album) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetail(album, server_id.clone()));
            }
        }
    };

    let on_artist = {
        let navigation = navigation.clone();
        let artist_id = song.artist_id.clone();
        let server_id = song.server_id.clone();
        move |_| {
            if let Some(artist) = artist_id.clone() {
                navigation.navigate_to(AppView::ArtistDetail(artist, server_id.clone()));
            }
        }
    };

    rsx! {
        div { class: "p-4 rounded-2xl border border-zinc-800/70 bg-zinc-900/50 backdrop-blur",
            div { class: "flex gap-4",
                if song.album_id.is_some() {
                    button {
                        class: "w-20 h-20 rounded-xl bg-zinc-800 overflow-hidden flex-shrink-0",
                        onclick: on_album_cover,
                        {
                            match cover_url {
                                Some(url) => rsx! { img { src: "{url}", alt: "{song.title}", class: "w-full h-full object-cover" } },
                                None => rsx! {
                                    div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                        Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                                    }
                                },
                            }
                        }
                    }
                } else {
                    div { class: "w-20 h-20 rounded-xl bg-zinc-800 overflow-hidden flex-shrink-0",
                        {
                            match cover_url {
                                Some(url) => rsx! { img { src: "{url}", alt: "{song.title}", class: "w-full h-full object-cover" } },
                                None => rsx! {
                                    div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                        Icon { name: "music".to_string(), class: "w-5 h-5 text-zinc-500".to_string() }
                                    }
                                },
                            }
                        }
                    }
                }
                div { class: "flex-1 min-w-0 space-y-2",
                    div { class: "flex items-start justify-between gap-3",
                        div { class: "min-w-0",
                            p { class: "font-semibold text-white truncate", "{song.title}" }
                            if song.artist_id.is_some() {
                                button {
                                    class: "text-sm text-emerald-400 hover:text-emerald-300 transition-colors truncate",
                                    onclick: on_artist,
                                    "{song.artist.clone().unwrap_or_default()}"
                                }
                            } else {
                                p { class: "text-sm text-zinc-400 truncate", "{song.artist.clone().unwrap_or_default()}" }
                            }
                            if song.album_id.is_some() {
                                button {
                                    class: "text-xs text-zinc-500 hover:text-emerald-400 transition-colors truncate",
                                    onclick: on_album_text,
                                    "{song.album.clone().unwrap_or_default()}"
                                }
                            } else {
                                p { class: "text-xs text-zinc-500 truncate", "{song.album.clone().unwrap_or_default()}" }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 text-right",
                            span { class: "text-xs px-2 py-1 rounded-full bg-zinc-800 text-zinc-300", "{position}" }
                            if let Some(ref comment) = bookmark.comment {
                                span { class: "text-xs text-zinc-500 truncate max-w-[180px]", "{comment}" }
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        span { class: "text-xs text-zinc-500 px-2 py-1 rounded-full bg-zinc-800/80", "{song.server_name}" }
                        if let Some(changed) = bookmark.changed.clone() {
                            span { class: "text-xs text-zinc-500 px-2 py-1 rounded-full bg-zinc-800/80", "{changed}" }
                        }
                    }
                    div { class: "flex items-center gap-3 pt-2",
                        button {
                            class: "px-3 py-2 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-white text-sm font-medium transition-colors",
                            onclick: on_resume,
                            "Resume"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white text-sm transition-colors flex items-center gap-2",
                            onclick: on_delete,
                            Icon { name: "trash".to_string(), class: "w-4 h-4".to_string() }
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
