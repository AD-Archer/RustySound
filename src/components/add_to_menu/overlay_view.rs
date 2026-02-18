// Render the add overlay, playlist picker, and suggestion UI.
{
    let render_playlist_picker = || {
        let loading = playlists().is_none();
        let available = playlists().unwrap_or_default();
        let filter = playlist_filter().to_lowercase();
        let mut filtered: Vec<Playlist> = if filter.is_empty() {
            available
        } else {
            available
                .into_iter()
                .filter(|p| p.name.to_lowercase().contains(&filter))
                .collect()
        };
        let total_filtered = filtered.len();
        let limit = 40usize;
        let limited: Vec<Playlist> = filtered.drain(..).take(limit).collect();
        let truncated = total_filtered > limited.len();
        let servers_list = servers();
        rsx! {
            div { class: "space-y-4",
                h3 { class: "text-lg font-semibold text-white", "Add to playlist" }
                input {
                    class: "w-full px-3 py-2 rounded-lg bg-zinc-900/50 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                    placeholder: "Search playlists",
                    value: playlist_filter,
                    oninput: move |e| playlist_filter.set(e.value()),
                }
                if let Some(reason) = playlist_guard.clone() {
                    div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm",
                        "{reason}"
                    }
                } else if loading {
                    div { class: "flex items-center gap-2 text-sm text-zinc-400",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-4 h-4 animate-spin".to_string(),
                        }
                        "Loading playlists..."
                    }
                } else if limited.is_empty() {
                    p { class: "text-sm text-zinc-400", "No user-created playlists found on the active server." }
                } else {
                    div { class: "max-h-56 overflow-y-auto space-y-2 pr-1",
                        for playlist in limited {
                            button {
                                class: "w-full px-3 py-2 rounded-xl bg-zinc-900/50 border border-zinc-800 hover:border-emerald-500/60 hover:text-white text-left text-sm text-zinc-300 transition-colors flex items-center gap-3",
                                onclick: make_add_to_playlist(playlist.id.clone()),
                                if let Some(url) = playlist
                                    .cover_art
                                    .as_ref()
                                    .and_then(|cover| {
                                        servers_list
                                            .iter()
                                            .find(|s| s.id == playlist.server_id)
                                            .map(|srv| {
                                                NavidromeClient::new(srv.clone()).get_cover_art_url(cover, 80)
                                            })
                                    })
                                {
                                    img {
                                        class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80",
                                        src: "{url}",
                                        alt: "Playlist art",
                                    }
                                } else {
                                    div { class: "w-10 h-10 rounded-md bg-zinc-800/70 border border-zinc-800/80 flex items-center justify-center",
                                        Icon {
                                            name: "playlist".to_string(),
                                            class: "w-4 h-4 text-zinc-500".to_string(),
                                        }
                                    }
                                }
                                div { class: "min-w-0",
                                    div { class: "font-medium truncate", "{playlist.name}" }
                                    p { class: "text-xs text-zinc-500", "{playlist.song_count} songs" }
                                }
                            }
                        }
                        if truncated {
                            p { class: "text-xs text-zinc-500 pt-1",
                                "Showing first {limit} playlists"
                            }
                        }
                    }
                }
                div { class: "space-y-2 pt-2 border-t border-zinc-800",
                    label { class: "text-xs uppercase tracking-wide text-zinc-500", "Create new" }
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            class: "flex-1 px-3 py-2 rounded-lg bg-zinc-900/50 border border-zinc-800 text-white placeholder:text-zinc-600 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "Playlist name",
                            value: new_playlist_name,
                            oninput: move |e| new_playlist_name.set(e.value()),
                        }
                        button {
                            class: if is_processing() { "px-4 py-2 rounded-lg bg-emerald-500/60 text-white cursor-not-allowed flex items-center gap-2" } else { "px-4 py-2 rounded-lg bg-emerald-500 text-white hover:bg-emerald-400 transition-colors flex items-center gap-2" },
                            onclick: create_playlist,
                            disabled: is_processing(),
                            if is_processing() {
                                Icon {
                                    name: "loader".to_string(),
                                    class: "w-4 h-4 animate-spin".to_string(),
                                }
                                "Working..."
                            } else {
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "Create"
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[95] flex items-end md:items-center justify-center bg-black/60 backdrop-blur-sm px-3 pb-20 md:pb-0 pt-3 md:pt-0",
            onclick: on_backdrop_close,
            div {
                class: "w-full md:max-w-xl max-h-[82vh] overflow-y-auto bg-zinc-900/95 border border-zinc-800 rounded-2xl shadow-2xl p-5 space-y-5",
                onclick: move |evt: MouseEvent| evt.stop_propagation(),
                div { class: "flex items-center justify-between gap-3",
                    div { class: "flex items-center gap-3 min-w-0",
                        if let Some(Some(cover)) = preview_cover() {
                            img {
                                class: "w-12 h-12 rounded-lg object-cover border border-zinc-800/80 cursor-pointer",
                                src: "{cover}",
                                alt: "Cover",
                                onclick: on_cover_click,
                            }
                        } else {
                            div { class: "w-12 h-12 rounded-lg bg-zinc-800/70 border border-zinc-800/80 flex items-center justify-center",
                                Icon {
                                    name: "playlist".to_string(),
                                    class: "w-5 h-5 text-zinc-500".to_string(),
                                }
                            }
                        }
                        div { class: "min-w-0",
                            p { class: "text-xs uppercase tracking-wide text-zinc-500",
                                "Add options"
                            }
                            h2 { class: "text-lg font-semibold text-white truncate",
                                "{intent.label}"
                            }
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 transition-colors",
                        onclick: on_close,
                        Icon {
                            name: "x".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                    }
                }

                if let Some((is_success, text)) = message() {
                    div { class: if is_success { "p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/40 text-emerald-200 text-sm" } else { "p-3 rounded-lg bg-red-500/10 border border-red-500/40 text-red-200 text-sm" },
                        "{text}"
                    }
                }

                if is_processing() {
                    div { class: "min-h-44 flex flex-col items-center justify-center gap-4 text-center",
                        Icon {
                            name: "loader".to_string(),
                            class: "w-8 h-8 text-amber-300 animate-spin".to_string(),
                        }
                        p { class: "text-sm text-zinc-300",
                            "{processing_label().unwrap_or_else(|| \"Working...\".to_string())}"
                        }
                        p { class: "text-xs text-zinc-500",
                            "Please wait while RustySound builds your queue."
                        }
                    }
                } else if show_playlist_picker() {
                    {render_playlist_picker()}
                } else {
                    div { class: "space-y-3",
                        div { class: "w-full grid grid-cols-1 sm:grid-cols-2 gap-2",
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white font-medium transition-colors",
                                onclick: make_add_to_queue("end"),
                                disabled: is_processing(),
                                span { "Add to queue (end)" }
                                Icon {
                                    name: "plus".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                                onclick: make_add_to_queue("next"),
                                disabled: is_processing(),
                                span { "Play next" }
                                Icon {
                                    name: "chevron-right".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                            onclick: on_open_playlist_picker,
                            disabled: is_processing(),
                            span { "Add to playlist" }
                            Icon {
                                name: "playlist".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        if matches!(intent_for_display.target, AddTarget::Song(_)) {
                            button {
                                class: "w-full flex items-center justify-between px-4 py-3 rounded-xl bg-zinc-800 text-white hover:bg-zinc-700 transition-colors",
                                onclick: on_create_similar,
                                disabled: is_processing(),
                                span { "Create similar mix" }
                                Icon {
                                    name: "shuffle".to_string(),
                                    class: "w-5 h-5".to_string(),
                                }
                            }
                        }
                    }
                    if let Some(reason) = playlist_guard {
                        div { class: "p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-200 text-sm",
                            "{reason}"
                        }
                    }
                    if suggestion_destination().is_some() {
                        div { class: "pt-3 border-t border-zinc-800 space-y-3",
                            div { class: "flex items-center justify-between",
                                h3 { class: "text-sm font-semibold text-zinc-200", "Suggested additions" }
                                span { class: "text-xs text-zinc-500", "4 + 4 seed suggestions" }
                            }
                            p { class: "text-xs text-zinc-500",
                                "Quick Add adds the song and refreshes this list with more similar picks."
                            }
                            if suggestions_loading() {
                                div { class: "flex items-center gap-2 text-xs text-zinc-400",
                                    Icon {
                                        name: "loader".to_string(),
                                        class: "w-4 h-4 animate-spin".to_string(),
                                    }
                                    "Loading suggestions..."
                                }
                            } else if suggestion_candidates().is_empty() {
                                p { class: "text-xs text-zinc-500", "No similar songs found yet." }
                            } else {
                                div { class: "max-h-64 overflow-y-auto space-y-2 pr-1",
                                    for song in suggestion_candidates() {
                                        div {
                                            class: "w-full p-3 rounded-xl bg-zinc-900/60 border border-zinc-800 hover:border-emerald-500/50 transition-colors",
                                            div { class: "flex items-center justify-between gap-3",
                                                div { class: "min-w-0",
                                                    p { class: "text-sm text-white truncate", "{song.title}" }
                                                    p { class: "text-xs text-zinc-500 truncate",
                                                        "{song.artist.clone().unwrap_or_else(|| \"Unknown Artist\".to_string())}"
                                                    }
                                                }
                                                div { class: "flex items-center gap-2",
                                                    button {
                                                        class: if preview_song_key()
                                                            == Some(song_key(&song))
                                                        {
                                                            "px-2 py-1 rounded-lg border border-zinc-700 text-zinc-500 text-xs cursor-not-allowed"
                                                        } else {
                                                            "px-2 py-1 rounded-lg border border-zinc-700 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors text-xs"
                                                        },
                                                        title: "Play a short preview, then return to your current song",
                                                        disabled: preview_song_key()
                                                            == Some(song_key(&song)),
                                                        onclick: {
                                                            let song = song.clone();
                                                            let on_preview_song = on_preview_song.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                on_preview_song(song.clone());
                                                            }
                                                        },
                                                        if preview_song_key()
                                                            == Some(song_key(&song))
                                                        {
                                                            "Previewing..."
                                                        } else {
                                                            "Preview"
                                                        }
                                                    }
                                                    button {
                                                        class: "px-2 py-1 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-xs hover:text-white hover:bg-emerald-500/30 transition-colors",
                                                        onclick: {
                                                            let song = song.clone();
                                                            let mut on_quick_add_suggestion =
                                                                on_quick_add_suggestion.clone();
                                                            move |evt: MouseEvent| {
                                                                evt.stop_propagation();
                                                                on_quick_add_suggestion(song.clone())
                                                            }
                                                        },
                                                        "Quick add"
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
    }
}
