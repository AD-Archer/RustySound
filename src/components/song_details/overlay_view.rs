// Overlay layout for desktop and mobile tabs.
{
    rsx! {
        div {
            class: "fixed inset-0 z-[80] bg-zinc-950",
            div {
                class: "w-full h-full border border-zinc-800/80 bg-zinc-950 overflow-hidden flex flex-col song-details-shell",
                div { class: "flex items-center justify-between px-4 md:px-6 py-4 border-b border-zinc-800/80",
                    div { class: "flex items-center gap-3 min-w-0",
                        button {
                            class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/80 transition-colors",
                            aria_label: "Open navigation menu",
                            onclick: {
                                let mut sidebar_open = sidebar_open.clone();
                                move |_| sidebar_open.set(true)
                            },
                            Icon { name: "menu".to_string(), class: "w-5 h-5".to_string() }
                        }
                        div { class: "min-w-0",
                            p { class: "text-xs uppercase tracking-[0.2em] text-zinc-500", "Song Menu" }
                            h2 { class: "text-lg md:text-2xl font-semibold text-white truncate", "{song_title}" }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/80 transition-colors",
                            aria_label: "Song actions",
                            onclick: on_open_song_actions,
                            Icon {
                                name: "more-horizontal".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        button {
                            class: "p-2 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800/80 transition-colors",
                            aria_label: "Close song details",
                            onclick: {
                                let mut controller = controller.clone();
                                move |_| controller.close()
                            },
                            Icon { name: "x".to_string(), class: "w-5 h-5".to_string() }
                        }
                    }
                }

                div { class: "hidden md:grid grid-cols-12 flex-1 overflow-hidden",
                    div { class: "col-span-4 border-r border-zinc-800/80 p-6 overflow-y-auto",
                        DetailsPanel {
                            song: song.clone(),
                            cover_url: cover_url.clone(),
                        }
                    }

                    div { class: "col-span-8 flex flex-col min-h-0",
                        div { class: "flex items-center gap-2 px-4 py-3 border-b border-zinc-800/70",
                            for tab in DESKTOP_TABS {
                                {
                                    let queue_tab_disabled =
                                        is_live_stream && tab == SongDetailsTab::Queue;
                                    rsx! {
                                        button {
                                            class: if queue_tab_disabled {
                                                "px-3 py-1.5 rounded-lg border border-zinc-800 text-zinc-600 cursor-not-allowed text-sm"
                                            } else if tab == desktop_tab {
                                                "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-sm"
                                            } else {
                                                "px-3 py-1.5 rounded-lg border border-zinc-700/60 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors text-sm"
                                            },
                                            disabled: queue_tab_disabled,
                                            onclick: {
                                                let mut controller = controller.clone();
                                                move |_| {
                                                    if queue_tab_disabled {
                                                        return;
                                                    }
                                                    controller.set_tab(tab);
                                                }
                                            },
                                            "{tab.label()}"
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "flex-1 min-h-0 p-4",
                            if desktop_tab == SongDetailsTab::Queue {
                                QueuePanel {
                                    up_next: up_next.clone(),
                                    seed_song: song.clone(),
                                    create_queue_busy,
                                    disabled_for_live: is_live_stream,
                                }
                            }
                            if desktop_tab == SongDetailsTab::Related {
                                RelatedPanel {
                                    related: related_resource(),
                                }
                            }
                            if desktop_tab == SongDetailsTab::Lyrics {
                                LyricsPanel {
                                    key: "{song.server_id}:{song.id}:desktop",
                                    panel_dom_key: format!("{}:{}:desktop", song.server_id, song.id),
                                    lyrics: selected_lyrics.clone(),
                                    lyrics_candidates: lyrics_candidates_resource(),
                                    lyrics_candidates_search_term: lyrics_candidate_search_term(),
                                    selected_query_override: lyrics_query_override(),
                                    current_time,
                                    offset_seconds,
                                    sync_lyrics,
                                    is_live_stream,
                                    on_refresh: {
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    default_search_title: song.title.clone(),
                                    manual_search_title: lyrics_search_title(),
                                    on_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |title: String| {
                                            let normalized = title.trim().to_string();
                                            if normalized.is_empty() {
                                                lyrics_search_title.set(None);
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(None);
                                            } else {
                                                lyrics_search_title.set(Some(normalized.clone()));
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(Some(normalized));
                                                lyrics_candidate_refresh_nonce.set(
                                                    lyrics_candidate_refresh_nonce()
                                                        .saturating_add(1),
                                                );
                                            }
                                        }
                                    },
                                    on_select_lyrics_candidate: {
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |query: LyricsQuery| {
                                            lyrics_search_title.set(Some(query.title.clone()));
                                            lyrics_query_override.set(Some(query));
                                            lyrics_refresh_nonce
                                                .set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    on_clear_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_search_title.set(None);
                                            lyrics_query_override.set(None);
                                            lyrics_candidate_search_term.set(None);
                                            lyrics_candidate_refresh_nonce.set(
                                                lyrics_candidate_refresh_nonce().saturating_add(1),
                                            );
                                        }
                                    },
                                }
                            }
                        }
                    }
                }

                div { class: "md:hidden flex-1 min-h-0 flex flex-col",
                    div { class: "px-3 py-3 border-b border-zinc-800/80 overflow-x-auto",
                        div { class: "flex items-center gap-2 min-w-max",
                            for tab in MOBILE_TABS {
                                {
                                    let queue_tab_disabled =
                                        is_live_stream && tab == SongDetailsTab::Queue;
                                    rsx! {
                                        button {
                                            class: if queue_tab_disabled {
                                                "px-3 py-1.5 rounded-lg border border-zinc-800 text-zinc-600 cursor-not-allowed text-sm"
                                            } else if tab == state.active_tab {
                                                "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 text-sm"
                                            } else {
                                                "px-3 py-1.5 rounded-lg border border-zinc-700/60 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors text-sm"
                                            },
                                            disabled: queue_tab_disabled,
                                            onclick: {
                                                let mut controller = controller.clone();
                                                move |_| {
                                                    if queue_tab_disabled {
                                                        return;
                                                    }
                                                    controller.set_tab(tab);
                                                }
                                            },
                                            "{tab.label()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if state.active_tab == SongDetailsTab::Details {
                        div { class: "p-3 song-details-mobile-content min-h-0 flex-1 flex flex-col gap-3 overflow-hidden",
                            div { class: "flex-1 min-h-0 overflow-y-auto pr-1",
                                DetailsPanel {
                                    song: song.clone(),
                                    cover_url: cover_url.clone(),
                                }
                            }
                            MiniLyricsStrip {
                                preview: mini_lyrics_preview,
                                is_live_stream,
                            }
                        }
                    } else {
                        div { class: "p-3 song-details-mobile-content min-h-0 flex-1 overflow-y-auto",
                            if state.active_tab == SongDetailsTab::Queue {
                                QueuePanel {
                                    up_next: up_next.clone(),
                                    seed_song: song.clone(),
                                    create_queue_busy,
                                    disabled_for_live: is_live_stream,
                                }
                            }
                            if state.active_tab == SongDetailsTab::Related {
                                RelatedPanel {
                                    related: related_resource(),
                                }
                            }
                            if state.active_tab == SongDetailsTab::Lyrics {
                                LyricsPanel {
                                    key: "{song.server_id}:{song.id}:mobile",
                                    panel_dom_key: format!("{}:{}:mobile", song.server_id, song.id),
                                    lyrics: selected_lyrics,
                                    lyrics_candidates: lyrics_candidates_resource(),
                                    lyrics_candidates_search_term: lyrics_candidate_search_term(),
                                    selected_query_override: lyrics_query_override(),
                                    current_time,
                                    offset_seconds,
                                    sync_lyrics,
                                    is_live_stream,
                                    on_refresh: {
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_refresh_nonce.set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    default_search_title: song.title.clone(),
                                    manual_search_title: lyrics_search_title(),
                                    on_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |title: String| {
                                            let normalized = title.trim().to_string();
                                            if normalized.is_empty() {
                                                lyrics_search_title.set(None);
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(None);
                                            } else {
                                                lyrics_search_title.set(Some(normalized.clone()));
                                                lyrics_query_override.set(None);
                                                lyrics_candidate_search_term.set(Some(normalized));
                                                lyrics_candidate_refresh_nonce.set(
                                                    lyrics_candidate_refresh_nonce().saturating_add(1),
                                                );
                                            }
                                        }
                                    },
                                    on_select_lyrics_candidate: {
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_resource = lyrics_resource.clone();
                                        let mut lyrics_refresh_nonce = lyrics_refresh_nonce.clone();
                                        move |query: LyricsQuery| {
                                            lyrics_search_title.set(Some(query.title.clone()));
                                            lyrics_query_override.set(Some(query));
                                            lyrics_refresh_nonce
                                                .set(lyrics_refresh_nonce().saturating_add(1));
                                            lyrics_resource.restart();
                                        }
                                    },
                                    on_clear_manual_search: {
                                        let mut lyrics_search_title = lyrics_search_title.clone();
                                        let mut lyrics_query_override = lyrics_query_override.clone();
                                        let mut lyrics_candidate_search_term =
                                            lyrics_candidate_search_term.clone();
                                        let mut lyrics_candidate_refresh_nonce =
                                            lyrics_candidate_refresh_nonce.clone();
                                        move |_| {
                                            lyrics_search_title.set(None);
                                            lyrics_query_override.set(None);
                                            lyrics_candidate_search_term.set(None);
                                            lyrics_candidate_refresh_nonce.set(
                                                lyrics_candidate_refresh_nonce().saturating_add(1),
                                            );
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
