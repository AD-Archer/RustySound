use crate::api::*;
use crate::components::views::home::SongRow;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn PlaylistDetailView(playlist_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let server = servers().into_iter().find(|s| s.id == server_id);

    let playlist_data = use_resource(move || {
        let server = server.clone();
        let playlist_id = playlist_id.clone();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_playlist(&playlist_id).await.ok()
            } else {
                None
            }
        }
    });

    let on_play_all = {
        let playlist_data_ref = playlist_data.clone();
        move |_| {
            if let Some(Some((_, songs))) = playlist_data_ref() {
                if !songs.is_empty() {
                    queue.set(songs.clone());
                    queue_index.set(0);
                    now_playing.set(Some(songs[0].clone()));
                    is_playing.set(true);
                }
            }
        }
    };

    rsx! {
        div { class: "space-y-8",
            // Back button
            button {
                class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
                onclick: move |_| {
                    if navigation.go_back().is_none() {
                        navigation.navigate_to(AppView::Playlists);
                    }
                },
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Playlists"
            }

            {match playlist_data() {
                Some(Some((playlist, songs))) => {
                    let cover_url = servers().iter()
                        .find(|s| s.id == playlist.server_id)
                        .and_then(|server| {
                            let client = NavidromeClient::new(server.clone());
                            playlist.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 500))
                        });

                    rsx! {
                        // Playlist header
                        div { class: "flex flex-col md:flex-row gap-8 mb-8",
                            // Cover art
                            div { class: "w-64 h-64 rounded-2xl bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0",
                                {match cover_url {
                                    Some(url) => rsx! {
                                        img { class: "w-full h-full object-cover", src: "{url}" }
                                    },
                                    None => rsx! {
                                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                                            Icon { name: "playlist".to_string(), class: "w-20 h-20 text-white/70".to_string() }
                                        }
                                    }
                                }}
                            }
                            // Playlist info
                            div { class: "flex flex-col justify-end",
                                p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2", "Playlist" }
                                h1 { class: "text-4xl font-bold text-white mb-4", "{playlist.name}" }
                                if let Some(comment) = &playlist.comment {
                                    p { class: "text-zinc-400 mb-4", "{comment}" }
                                }
                                div { class: "flex items-center gap-4 text-sm text-zinc-400",
                                    if let Some(owner) = &playlist.owner {
                                        span { "by {owner}" }
                                    }
                                    span { "{playlist.song_count} songs" }
                                    span { "{format_duration(playlist.duration)}" }
                                }
                                // Actions
                                div { class: "flex gap-3 mt-6",
                                    button {
                                        class: "px-8 py-3 rounded-full bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                        onclick: on_play_all,
                                        Icon { name: "play".to_string(), class: "w-5 h-5".to_string() }
                                        "Play"
                                    }
                                    button {
                                        class: "px-6 py-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-white hover:border-zinc-600 transition-colors flex items-center gap-2",
                                        Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
                                        "Shuffle"
                                    }
                                }
                            }
                        }

                        // Song list
                        div { class: "space-y-1",
                            for (index, song) in songs.iter().enumerate() {
                                SongRow {
                                    song: song.clone(),
                                    index: index + 1,
                                    onclick: {
                                        let song = song.clone();
                                        move |_| {
                                            now_playing.set(Some(song.clone()));
                                            is_playing.set(true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(None) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon { name: "playlist".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                        p { class: "text-zinc-400", "Playlist not found" }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        Icon { name: "loader".to_string(), class: "w-8 h-8 text-zinc-500".to_string() }
                    }
                }
            }}
        }
    }
}
