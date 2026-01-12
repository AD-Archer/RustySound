use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};
use crate::components::views::home::SongRow;

#[component]
pub fn AlbumDetailView(album_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    
    let server = servers().into_iter().find(|s| s.id == server_id);
    
    let album_data = use_resource(move || {
        let server = server.clone();
        let album_id = album_id.clone();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_album(&album_id).await.ok()
            } else {
                None
            }
        }
    });
    
    let on_play_all = {
        let album_data_ref = album_data.clone();
        move |_| {
            if let Some(Some((_, songs))) = album_data_ref() {
                if !songs.is_empty() {
                    queue.set(songs.clone());
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
                onclick: move |_| current_view.set(AppView::Albums),
                Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
                "Back to Albums"
            }

            {

                // Album header
                // Cover art
                // Album info
                // Actions

                // Song list
                match album_data() {
                    Some(Some((album, songs))) => {
                        let cover_url = servers()
                            .iter()
                            .find(|s| s.id == album.server_id)
                            .and_then(|server| {
                                let client = NavidromeClient::new(server.clone());
                                album
                                    .cover_art
                                    .as_ref()
                                    .map(|ca| client.get_cover_art_url(ca, 500))
                            });
                        rsx! {
                            div { class: "flex flex-col md:flex-row gap-8 mb-8",
                                div { class: "w-64 h-64 rounded-2xl bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0",
                                    {
                                        match cover_url {
                                            Some(url) => rsx! {
                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                            },
                                            None => rsx! {
                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                                    Icon {
                                                        name: "album".to_string(),
                                                        class: "w-20 h-20 text-zinc-500".to_string(),
                                                    }
                                                }
                                            },
                                        }
                                    }
                                }
                                div { class: "flex flex-col justify-end",
                                    p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2", "Album" }
                                    h1 { class: "text-4xl font-bold text-white mb-4", "{album.name}" }
                                    p { class: "text-lg text-zinc-300 mb-2", "{album.artist}" }
                                    div { class: "flex items-center gap-4 text-sm text-zinc-400",
                                        if let Some(year) = album.year {
                                            span { "{year}" }
                                        }
                                        span { "{album.song_count} songs" }
                                        span { "{format_duration(album.duration)}" }
                                    }
                                    div { class: "flex gap-3 mt-6",
                                        button {
                                            class: "px-8 py-3 rounded-full bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors flex items-center gap-2",
                                            onclick: on_play_all,
                                            Icon { name: "play".to_string(), class: "w-5 h-5".to_string() }
                                            "Play"
                                        }
                                        button { class: "p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors",
                                            Icon { name: "heart".to_string(), class: "w-5 h-5".to_string() }
                                        }
                                    }
                                }
                            }
                
                            div { class: "space-y-1",
                                for (index , song) in songs.iter().enumerate() {
                                    SongRow {
                                        song: song.clone(),
                                        index: index + 1,
                                        onclick: {
                                            let song = song.clone();
                                            move |_| {
                                                now_playing.set(Some(song.clone()));
                                                is_playing.set(true);
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                    Some(None) => rsx! {
                        div { class: "flex flex-col items-center justify-center py-20",
                            Icon {
                                name: "album".to_string(),
                                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                            }
                            p { class: "text-zinc-400", "Album not found" }
                        }
                    },
                    None => rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-8 h-8 text-zinc-500".to_string(),
                            }
                        }
                    },
                }
            }
        }
    }
}
