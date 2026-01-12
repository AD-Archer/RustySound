use dioxus::prelude::*;
use crate::api::*;
use crate::components::Icon;

#[component]
pub fn Player() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<Signal<f64>>();
    
    let current_song = now_playing();
    let playing = is_playing();
    let vol = volume();
    
    // Get cover art URL if available
    let cover_url = current_song.as_ref().and_then(|song| {
        let server = servers().iter().find(|s| s.id == song.server_id)?.clone();
        let client = NavidromeClient::new(server);
        song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 100))
    });
    
    // Get stream URL if available
    let stream_url = current_song.as_ref().and_then(|song| {
        let server = servers().iter().find(|s| s.id == song.server_id)?.clone();
        let client = NavidromeClient::new(server);
        Some(client.get_stream_url(&song.id))
    });
    
    let on_prev = move |_| {
        let idx = queue_index();
        let queue_list = queue();
        if idx > 0 && !queue_list.is_empty() {
            queue_index.set(idx - 1);
        }
    };
    
    let on_next = move |_| {
        let idx = queue_index();
        let queue_list = queue();
        if idx < queue_list.len().saturating_sub(1) {
            queue_index.set(idx + 1);
        }
    };
    
    let on_toggle = move |_| {
        is_playing.set(!playing);
    };
    
    rsx! {
        div { class: "fixed bottom-0 left-0 right-0 h-24 bg-zinc-950/95 backdrop-blur-xl border-t border-zinc-800/50 z-50",
            div { class: "h-full flex items-center justify-between px-6 gap-8",
                // Now playing info
                div { class: "flex items-center gap-4 min-w-0 w-1/4",
                    {
                        // Album art
                        // Track info
                        // Favorite button
                        match &current_song {
                            Some(song) => rsx! {
                                div { class: "w-14 h-14 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden shadow-lg",
                                    {
                                        match &cover_url {
                                            Some(url) => rsx! {
                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                            },
                                            None => rsx! {
                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-emerald-600 to-teal-700",
                                                    Icon { name: "music".to_string(), class: "w-6 h-6 text-white/70".to_string() }
                                                }
                                            },
                                        }
                                    }
                                }
                                div { class: "min-w-0",
                                    p { class: "text-sm font-medium text-white truncate", "{song.title}" }
                                    p { class: "text-xs text-zinc-400 truncate", "{song.artist.clone().unwrap_or_default()}" }
                                }
                                button { class: "p-2 text-zinc-400 hover:text-emerald-400 transition-colors",
                                    Icon { name: "heart".to_string(), class: "w-5 h-5".to_string() }
                                }
                            },
                            None => rsx! {
                                div { class: "w-14 h-14 rounded-lg bg-zinc-800/50 flex items-center justify-center",
                                    Icon { name: "music".to_string(), class: "w-6 h-6 text-zinc-600".to_string() }
                                }
                                div {
                                    p { class: "text-sm text-zinc-500", "No track playing" }
                                    p { class: "text-xs text-zinc-600", "Select a song to start" }
                                }
                            },
                        }
                    }
                }

                // Player controls
                div { class: "flex flex-col items-center gap-2 flex-1 max-w-2xl",
                    // Control buttons
                    div { class: "flex items-center gap-4",
                        // Shuffle
                        button { class: "p-2 text-zinc-400 hover:text-white transition-colors",
                            Icon {
                                name: "shuffle".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Previous
                        button {
                            class: "p-2 text-zinc-300 hover:text-white transition-colors",
                            onclick: on_prev,
                            Icon {
                                name: "prev".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Play/Pause
                        button {
                            class: "w-10 h-10 rounded-full bg-white flex items-center justify-center hover:scale-105 transition-transform shadow-lg",
                            onclick: on_toggle,
                            if playing {
                                Icon {
                                    name: "pause".to_string(),
                                    class: "w-5 h-5 text-black".to_string(),
                                }
                            } else {
                                Icon {
                                    name: "play".to_string(),
                                    class: "w-5 h-5 text-black ml-0.5".to_string(),
                                }
                            }
                        }
                        // Next
                        button {
                            class: "p-2 text-zinc-300 hover:text-white transition-colors",
                            onclick: on_next,
                            Icon {
                                name: "next".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Repeat
                        button { class: "p-2 text-zinc-400 hover:text-white transition-colors",
                            Icon {
                                name: "repeat".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                    // Progress bar
                    div { class: "flex items-center gap-3 w-full",
                        span { class: "text-xs text-zinc-500 w-10 text-right", "0:00" }
                        div { class: "flex-1 h-1.5 bg-zinc-800 rounded-full overflow-hidden cursor-pointer group",
                            div { class: "h-full bg-gradient-to-r from-emerald-500 to-teal-500 rounded-full w-0 group-hover:bg-emerald-400 transition-colors" }
                        }
                        span { class: "text-xs text-zinc-500 w-10",
                            {
                                current_song
                                    .as_ref()
                                    .map(|s| format_duration(s.duration))
                                    .unwrap_or_else(|| "--:--".to_string())
                            }
                        }
                    }
                }

                // Volume and other controls
                div { class: "flex items-center gap-4 w-1/4 justify-end",
                    // Queue button
                    button { class: "p-2 text-zinc-400 hover:text-white transition-colors",
                        Icon {
                            name: "queue".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                    }
                    // Volume
                    div { class: "flex items-center gap-2",
                        button { class: "p-2 text-zinc-400 hover:text-white transition-colors",
                            Icon {
                                name: "volume".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        div { class: "w-24 h-1.5 bg-zinc-800 rounded-full overflow-hidden cursor-pointer",
                            div {
                                class: "h-full bg-zinc-400 rounded-full transition-all",
                                style: "width: {vol * 100.0}%",
                            }
                        }
                    }
                }
            }

            // Hidden audio element for actual playback
            {
                // volume: vol,
                match stream_url {
                    Some(url) if playing => rsx! {
                        audio { src: "{url}", autoplay: true }
                    },
                    _ => rsx! {},
                }
            }
        }
    }
}
