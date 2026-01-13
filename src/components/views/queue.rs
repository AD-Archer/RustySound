use crate::api::models::format_duration;
use crate::api::*;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn QueueView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let current_index = queue_index();
    let songs: Vec<Song> = queue().into_iter().collect();
    let current_song = now_playing();

    let on_clear = move |_| {
        queue.set(Vec::new());
        queue_index.set(0);
        now_playing.set(None);
        is_playing.set(false);
    };

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Play Queue" }
                    p { class: "page-subtitle",
                        "{songs.len()} songs • {format_duration(songs.iter().map(|s| s.duration).sum())}"
                    }
                }

                if !songs.is_empty() {
                    button {
                        class: "px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-300 hover:text-white transition-colors flex items-center gap-2",
                        onclick: on_clear,
                        Icon {
                            name: "trash".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Clear Queue"
                    }
                }
            }

            if songs.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20",
                    Icon {
                        name: "queue".to_string(),
                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                    }
                    p { class: "text-zinc-400", "Your queue is empty" }
                    p { class: "text-zinc-500 text-sm mt-2",
                        "Add songs from albums or search to start listening"
                    }
                }
            } else {
                div { class: "bg-zinc-800/30 rounded-2xl border border-zinc-700/30 overflow-hidden",
                    // Current Song Section
                    if let Some(ref current) = current_song {
                        {
                            let current_cover = servers()
                                .iter()
                                .find(|s| s.id == current.server_id)
                                .and_then(|server| {
                                    let client = NavidromeClient::new(server.clone());
                                    current.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                });
                            rsx! {
                                div { class: "p-4 bg-emerald-500/10 border-b border-zinc-700/50",
                                    p { class: "text-xs font-semibold text-emerald-400 uppercase tracking-wider mb-2",
                                        "Now Playing"
                                    }
                                    div { class: "flex items-center justify-between group",
                                        div { class: "flex items-center gap-4",
                                            // Cover art
                                            if current.album_id.is_some() {
                                                button {
                                                    class: "w-12 h-12 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden",
                                                    aria_label: "Open album",
                                                    onclick: {
                                                        let album_id = current.album_id.clone();
                                                        let server_id = current.server_id.clone();
                                                        let navigation = navigation.clone();
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            if let Some(album_id) = album_id.clone() {
                                                                navigation.navigate_to(AppView::AlbumDetail(
                                                                    album_id,
                                                                    server_id.clone(),
                                                                ));
                                                            }
                                                        }
                                                    },
                                                    {
                                                        match current_cover {
                                                            Some(url) => rsx! {
                                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full bg-zinc-700 flex items-center justify-center",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-5 h-5 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            } else {
                                                div { class: "w-12 h-12 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden",
                                                    {
                                                        match current_cover {
                                                            Some(url) => rsx! {
                                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full bg-zinc-700 flex items-center justify-center",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-5 h-5 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }

                                            div {
                                                p { class: "font-medium text-white", "{current.title}" }
                                                if current.artist_id.is_some() {
                                                    button {
                                                        class: "text-sm text-zinc-400 hover:text-emerald-400 transition-colors text-left",
                                                        onclick: {
                                                            let artist_id = current.artist_id.clone();
                                                            let server_id = current.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(artist_id) = artist_id.clone() {
                                                                    navigation.navigate_to(AppView::ArtistDetail(
                                                                        artist_id,
                                                                        server_id.clone(),
                                                                    ));
                                                                }
                                                            }
                                                        },
                                                        "{current.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-sm text-zinc-400",
                                                        "{current.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "text-sm text-zinc-500 font-mono",
                                            "{format_duration(current.duration)}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Queue List
                    div { class: "divide-y divide-zinc-800/50",
                        for (idx , song) in songs.into_iter().enumerate() {
                            {
                                let is_current = idx == current_index;
                                let song_id = song.id.clone();
                                let cover_url = servers()
                                    .iter()
                                    .find(|s| s.id == song.server_id)
                                    .and_then(|server| {
                                        let client = NavidromeClient::new(server.clone());
                                        song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 80))
                                    });
                                rsx! {
                                    div {
                                        key: "{song_id}-{idx}",
                                        class: if is_current { "p-3 bg-emerald-500/5 flex items-center justify-between" } else { "p-3 hover:bg-zinc-700/30 transition-colors flex items-center justify-between group cursor-pointer" },
                                        onclick: move |_| {
                                            if !is_current {
                                                queue_index.set(idx);
                                                now_playing.set(Some(song.clone()));
                                                is_playing.set(true);
                                            }
                                        },

                                    // Index or playing indicator

                                    // Adjust current index if needed



                                        // Remove the song from queue

                                        // Adjust index and now_playing based on removal
                                        // Queue is now empty
                                        // Removed song before current, just adjust index
                                        // Removed the currently playing song
                                        // Set now_playing first to avoid the queue_index effect overriding
                                        // Preserve playback state
                                        // If idx > current_index, nothing needs to change
                                        div { class: "flex items-center gap-4 overflow-hidden",
                                            div { class: "w-8 text-center text-sm flex-shrink-0",
                                                if is_current {
                                                    Icon {
                                                        name: "play".to_string(),
                                                        class: "w-4 h-4 text-emerald-400 mx-auto".to_string(),
                                                    }
                                                } else {
                                                    span { class: "text-zinc-500", "{idx + 1}" }
                                                }
                                            }
                                            if song.album_id.is_some() {
                                                button {
                                                    class: "w-12 h-12 rounded-lg bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    aria_label: "Open album",
                                                    onclick: {
                                                        let album_id = song.album_id.clone();
                                                        let server_id = song.server_id.clone();
                                                        let navigation = navigation.clone();
                                                        move |evt: MouseEvent| {
                                                            evt.stop_propagation();
                                                            if let Some(album_id) = album_id.clone() {
                                                                navigation.navigate_to(AppView::AlbumDetail(
                                                                    album_id,
                                                                    server_id.clone(),
                                                                ));
                                                            }
                                                        }
                                                    },
                                                    {
                                                        match cover_url {
                                                            Some(url) => rsx! {
                                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-4 h-4 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            } else {
                                                div { class: "w-12 h-12 rounded-lg bg-zinc-800 overflow-hidden flex-shrink-0",
                                                    {
                                                        match cover_url {
                                                            Some(url) => rsx! {
                                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                                            },
                                                            None => rsx! {
                                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                                    Icon {
                                                                        name: "music".to_string(),
                                                                        class: "w-4 h-4 text-zinc-500".to_string(),
                                                                    }
                                                                }
                                                            },
                                                        }
                                                    }
                                                }
                                            }

                                            div { class: "min-w-0",
                                                p { class: if is_current { "text-emerald-400 font-medium truncate" } else { "text-zinc-300 truncate group-hover:text-white" },
                                                    "{song.title}"
                                                }
                                                if song.artist_id.is_some() {
                                                    button {
                                                        class: "text-xs text-zinc-500 truncate hover:text-emerald-400 transition-colors",
                                                        onclick: {
                                                            let artist_id = song.artist_id.clone();
                                                            let server_id = song.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(artist_id) = artist_id.clone() {
                                                                    navigation.navigate_to(AppView::ArtistDetail(
                                                                        artist_id,
                                                                        server_id.clone(),
                                                                    ));
                                                                }
                                                            }
                                                        },
                                                        "{song.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                                if song.album_id.is_some() {
                                                    button {
                                                        class: "text-xs text-zinc-600 truncate hover:text-emerald-400 transition-colors hidden sm:block",
                                                        onclick: {
                                                            let album_id = song.album_id.clone();
                                                            let server_id = song.server_id.clone();
                                                            let navigation = navigation.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                if let Some(album_id) = album_id.clone() {
                                                                    navigation.navigate_to(AppView::AlbumDetail(
                                                                        album_id,
                                                                        server_id.clone(),
                                                                    ));
                                                                }
                                                            }
                                                        },
                                                        "{song.album.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                } else {
                                                    p { class: "text-xs text-zinc-600 truncate hidden sm:block",
                                                        "{song.album.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                    }
                                                }
                                            }
                                        }

                                        div { class: "flex items-center gap-4",
                                            span { class: "text-sm text-zinc-600 font-mono group-hover:hidden",
                                                "{format_duration(song.duration)}"
                                            }

                                            button {
                                                class: "p-2 text-zinc-500 hover:text-red-400 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    let was_playing = is_playing();
                                                    let q_len = queue().len();

                                                    queue

                                                        .with_mut(|q| {
                                                            if idx < q.len() {
                                                                q.remove(idx);
                                                            }
                                                        });
                                                    if q_len <= 1 {
                                                        queue_index.set(0);
                                                        now_playing.set(None);
                                                        is_playing.set(false);
                                                    } else if idx < current_index {
                                                        queue_index.set(current_index - 1);
                                                    } else if idx == current_index {
                                                        let new_queue = queue();
                                                        let new_idx = idx.min(new_queue.len().saturating_sub(1));
                                                        if let Some(new_song) = new_queue.get(new_idx) {
                                                            now_playing.set(Some(new_song.clone()));
                                                            queue_index.set(new_idx);
                                                            is_playing.set(was_playing);
                                                        } else {
                                                            now_playing.set(None);
                                                            is_playing.set(false);
                                                        }
                                                    }
                                                },
                                                Icon { name: "x".to_string(), class: "w-4 h-4".to_string() }
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
    }
}
