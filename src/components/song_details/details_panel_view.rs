// Render details-panel cover, metadata, and controls.
{
    rsx! {
        div { class: "space-y-5",
            div { class: "flex justify-center",
                if props.song.album_id.is_some() {
                    button {
                        class: "w-full max-w-md aspect-square rounded-2xl border border-zinc-800/80 overflow-hidden bg-zinc-900/60 shadow-2xl hover:ring-2 hover:ring-emerald-500/50 transition-all",
                        onclick: on_open_album_cover,
                        title: "Open album",
                        {
                            match props.cover_url.clone() {
                                Some(url) => rsx! {
                                    img {
                                        src: "{url}",
                                        alt: "{props.song.title}",
                                        class: "w-full h-full object-cover",
                                        loading: "lazy",
                                    }
                                },
                                None => rsx! {
                                    div { class: "w-full h-full bg-gradient-to-br from-zinc-800 to-zinc-900 flex items-center justify-center",
                                        Icon { name: "music".to_string(), class: "w-20 h-20 text-zinc-600".to_string() }
                                    }
                                },
                            }
                        }
                    }
                } else {
                    div { class: "w-full max-w-md aspect-square rounded-2xl border border-zinc-800/80 overflow-hidden bg-zinc-900/60 shadow-2xl",
                        {
                            match props.cover_url.clone() {
                                Some(url) => rsx! {
                                    img {
                                        src: "{url}",
                                        alt: "{props.song.title}",
                                        class: "w-full h-full object-cover",
                                        loading: "lazy",
                                    }
                                },
                                None => rsx! {
                                    div { class: "w-full h-full bg-gradient-to-br from-zinc-800 to-zinc-900 flex items-center justify-center",
                                        Icon { name: "music".to_string(), class: "w-20 h-20 text-zinc-600".to_string() }
                                    }
                                },
                            }
                        }
                    }
                }
            }

            div { class: "space-y-3 text-center",
                h3 { class: "text-xl md:text-2xl font-semibold text-white leading-tight break-words", "{props.song.title}" }
                div { class: "space-y-1 pt-1",
                    p { class: "text-[10px] uppercase tracking-[0.18em] text-zinc-500", "Artist" }
                    if props.song.artist_id.is_some() {
                        button {
                            class: "text-sm text-emerald-300 hover:text-emerald-200 transition-colors whitespace-normal break-words leading-snug",
                            onclick: on_open_artist,
                            "{song_artist}"
                        }
                    } else {
                        p { class: "text-sm text-zinc-300 whitespace-normal break-words leading-snug", "{song_artist}" }
                    }
                }
                div { class: "space-y-1 pt-3 border-t border-zinc-800/70",
                    p { class: "text-[10px] uppercase tracking-[0.18em] text-zinc-500", "Album" }
                    if props.song.album_id.is_some() {
                        button {
                            class: "text-sm text-zinc-300 hover:text-white transition-colors whitespace-normal break-words leading-snug",
                            onclick: on_open_album,
                            "{song_album}"
                        }
                    } else {
                        p { class: "text-sm text-zinc-400 whitespace-normal break-words leading-snug", "{song_album}" }
                    }
                }
            }

            div { class: "grid grid-cols-3 gap-2 text-center",
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Duration" }
                    p { class: "text-sm text-zinc-200 mt-1", "{format_duration(props.song.duration)}" }
                }
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Server" }
                    p { class: "text-sm text-zinc-200 mt-1 truncate", "{props.song.server_name}" }
                }
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/50 p-3",
                    p { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Track" }
                    p { class: "text-sm text-zinc-200 mt-1", "{props.song.track.unwrap_or(0)}" }
                }
            }

            div { class: "rounded-2xl border border-zinc-800/80 bg-zinc-900/50 p-3 space-y-3",
                div { class: "flex items-center justify-between gap-2",
                    p { class: "text-sm font-medium text-white", "Now Playing Controls" }
                    if is_selected_song_now_playing {
                        span { class: "text-[10px] uppercase tracking-wider text-emerald-300", "This Song Is Playing" }
                    } else if now_playing_song.is_some() {
                        span { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Playing Another Song" }
                    } else {
                        span { class: "text-[10px] uppercase tracking-wider text-zinc-500", "Idle" }
                    }
                }

                if let Some(current_song) = now_playing_song.clone() {
                    div { class: "space-y-1",
                        p { class: "text-sm text-zinc-100 truncate", "{current_song.title}" }
                        p { class: "text-xs text-zinc-500 truncate",
                            "{current_song.artist.clone().unwrap_or_default()}"
                        }
                    }

                    div { class: "space-y-1",
                        div { class: "flex items-center justify-between text-xs text-zinc-500",
                            span { "{format_duration(current_time as u32)}" }
                            span { "{format_duration(display_duration.max(0.0) as u32)}" }
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: playback_percent.round() as i32,
                            disabled: display_duration <= 0.0,
                            class: "w-full h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-emerald-500 disabled:opacity-40 disabled:cursor-not-allowed",
                            oninput: on_seek_now_playing,
                            onchange: on_seek_now_playing,
                        }
                    }

                    div { class: "space-y-2",
                        div { class: "flex items-center gap-2",
                            Icon {
                                name: if volume() > 0.5 { "volume-2".to_string() } else if volume() > 0.0 { "volume-1".to_string() } else { "volume-x".to_string() },
                                class: "w-4 h-4 text-zinc-400".to_string(),
                            }
                            input {
                                r#type: "range",
                                min: "0",
                                max: "100",
                                value: (volume() * 100.0).round() as i32,
                                class: "flex-1 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-zinc-400",
                                oninput: on_volume_change,
                                onchange: on_volume_change,
                            }
                            span { class: "text-xs text-zinc-500 w-10 text-right",
                                "{(volume() * 100.0).round() as i32}%"
                            }
                        }

                        div { class: "flex items-center justify-between gap-3",
                            div { class: "flex items-center gap-2",
                                button {
                                    class: if is_selected_song_favorited {
                                        "p-2 rounded-full border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 transition-colors"
                                    } else {
                                        "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors"
                                    },
                                    onclick: on_toggle_song_favorite,
                                    title: if is_selected_song_favorited { "Unfavorite song" } else { "Favorite song" },
                                    Icon {
                                        name: if is_selected_song_favorited { "heart-filled".to_string() } else { "heart".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                }
                                button {
                                    class: if current_repeat_mode == RepeatMode::One {
                                        "p-2 rounded-full border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 transition-colors"
                                    } else {
                                        "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors"
                                    },
                                    onclick: on_cycle_loop,
                                    title: if current_repeat_mode == RepeatMode::One { "Loop one (on)" } else { "Loop one (off)" },
                                    Icon {
                                        name: if current_repeat_mode == RepeatMode::One { "repeat-1".to_string() } else { "repeat".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                }
                                button {
                                    class: "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors",
                                    onclick: on_add_to_playlist,
                                    title: "Add to queue or playlist",
                                    Icon { name: "playlist".to_string(), class: "w-4 h-4".to_string() }
                                }
                            }

                            div { class: "relative",
                                button {
                                    class: if now_playing_rating > 0 {
                                        "p-2 rounded-full border border-amber-500/50 text-amber-400 hover:text-amber-300 transition-colors"
                                    } else {
                                        "p-2 rounded-full border border-zinc-700 text-zinc-400 hover:text-white transition-colors"
                                    },
                                    onclick: move |_| rating_open.set(!rating_open()),
                                    title: "Rate now playing",
                                    Icon {
                                        name: if now_playing_rating > 0 { "star-filled".to_string() } else { "star".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                }
                                if rating_open() {
                                    div { class: "absolute right-0 bottom-11 z-20 bg-zinc-950/95 border border-zinc-800 rounded-xl px-3 py-2 shadow-xl flex items-center gap-2",
                                        for value in 1u32..=5u32 {
                                            button {
                                                class: if value <= now_playing_rating {
                                                    "text-amber-400 hover:text-amber-300 transition-colors"
                                                } else {
                                                    "text-zinc-500 hover:text-zinc-300 transition-colors"
                                                },
                                                onclick: {
                                                    let mut on_set_now_playing_rating = on_set_now_playing_rating.clone();
                                                    move |_| on_set_now_playing_rating(value)
                                                },
                                                Icon {
                                                    name: if value <= now_playing_rating { "star-filled".to_string() } else { "star".to_string() },
                                                    class: "w-4 h-4".to_string(),
                                                }
                                            }
                                        }
                                        button {
                                            class: "ml-1 text-[11px] text-zinc-500 hover:text-zinc-300 transition-colors",
                                            onclick: {
                                                let mut on_set_now_playing_rating = on_set_now_playing_rating.clone();
                                                move |_| on_set_now_playing_rating(0)
                                            },
                                            "Clear"
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    p { class: "text-sm text-zinc-500", "No song is currently playing. Start playback to unlock progress and rating controls." }
                }
            }

            div { class: "grid grid-cols-3 gap-3",
                button {
                    class: if can_prev {
                        "h-11 rounded-xl border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors flex items-center justify-center"
                    } else {
                        "h-11 rounded-xl border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                    },
                    disabled: !can_prev,
                    onclick: on_prev_song,
                    Icon { name: "prev".to_string(), class: "w-5 h-5".to_string() }
                }
                button {
                    class: "h-11 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white transition-colors flex items-center justify-center",
                    onclick: on_toggle_selected_playback,
                    Icon {
                        name: if is_selected_song_now_playing && currently_playing { "pause".to_string() } else { "play".to_string() },
                        class: "w-5 h-5".to_string(),
                    }
                }
                button {
                    class: if can_next {
                        "h-11 rounded-xl border border-zinc-700 text-zinc-300 hover:text-white hover:border-zinc-500 transition-colors flex items-center justify-center"
                    } else {
                        "h-11 rounded-xl border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                    },
                    disabled: !can_next,
                    onclick: on_next_song,
                    Icon { name: "next".to_string(), class: "w-5 h-5".to_string() }
                }
            }
        }
    }
}
