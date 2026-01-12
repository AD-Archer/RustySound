use dioxus::prelude::*;
use crate::api::*;
use crate::components::Icon;
use crate::api::models::format_duration;

#[component]
pub fn QueueView() -> Element {
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
            header { class: "mb-8 flex items-center justify-between",
                div {
                    h1 { class: "text-3xl font-bold text-white mb-2", "Play Queue" }
                    p { class: "text-zinc-400",
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
                        div { class: "p-4 bg-emerald-500/10 border-b border-zinc-700/50",
                            p { class: "text-xs font-semibold text-emerald-400 uppercase tracking-wider mb-2",
                                "Now Playing"
                            }
                            div { class: "flex items-center justify-between group",
                                div { class: "flex items-center gap-4",
                                    // Cover art
                                    div { class: "w-12 h-12 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden",
                                        div { class: "w-full h-full bg-zinc-700 flex items-center justify-center",
                                            Icon {
                                                name: "music".to_string(),
                                                class: "w-5 h-5 text-zinc-500".to_string(),
                                            }
                                        }
                                    }

                                    div {
                                        p { class: "font-medium text-white", "{current.title}" }
                                        p { class: "text-sm text-zinc-400",
                                            "{current.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                        }
                                    }
                                }

                                div { class: "text-sm text-zinc-500 font-mono",
                                    "{format_duration(current.duration)}"
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
        
                                            div { class: "min-w-0",
                                                p { class: if is_current { "text-emerald-400 font-medium truncate" } else { "text-zinc-300 truncate group-hover:text-white" },
                                                    "{song.title}"
                                                }
                                                p { class: "text-xs text-zinc-500 truncate",
                                                    "{song.artist.as_ref().map(|s| s.as_str()).unwrap_or(\"\")}"
                                                }
                                            }
                                        }
        
                                        div { class: "flex items-center gap-4",
                                            span { class: "text-sm text-zinc-600 font-mono group-hover:hidden",
                                                "{format_duration(song.duration)}"
                                            }
        
                                            button {
                                                class: "hidden group-hover:block p-2 text-zinc-500 hover:text-red-400 transition-colors",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    queue
                                                        .with_mut(|q| {
                                                            if idx < q.len() {
                                                                q.remove(idx);
                                                            }
                                                        });
                                                    if idx < current_index {
                                                        queue_index.set(current_index - 1);
                                                    } else if idx == current_index && !queue().is_empty() {
                                                        let new_idx = if idx >= queue().len() {
                                                            queue().len().saturating_sub(1)
                                                        } else {
                                                            idx
                                                        };
                                                        queue_index.set(new_idx);
                                                        if let Some(new_song) = queue().get(new_idx) {
                                                            now_playing.set(Some(new_song.clone()));
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
