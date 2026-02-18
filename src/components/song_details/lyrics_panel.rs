// Full lyrics panel including manual search and candidate results.

#[derive(Props, Clone, PartialEq)]
struct LyricsPanelProps {
    panel_dom_key: String,
    lyrics: Option<Result<LyricsResult, String>>,
    lyrics_candidates: Option<Result<Vec<LyricsSearchCandidate>, String>>,
    lyrics_candidates_search_term: Option<String>,
    selected_query_override: Option<LyricsQuery>,
    current_time: f64,
    offset_seconds: f64,
    sync_lyrics: bool,
    is_live_stream: bool,
    on_refresh: EventHandler<MouseEvent>,
    default_search_title: String,
    manual_search_title: Option<String>,
    on_manual_search: EventHandler<String>,
    on_select_lyrics_candidate: EventHandler<LyricsQuery>,
    on_clear_manual_search: EventHandler<MouseEvent>,
}

#[component]
fn LyricsPanel(props: LyricsPanelProps) -> Element {
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let search_panel_open = use_signal(|| false);
    let programmatic_scroll_until_ms = use_signal(|| 0.0_f64);
    let manual_scroll_hold_until_ms = use_signal(|| 0.0_f64);
    let last_centered_index = use_signal(|| None::<usize>);

    let on_open_settings = {
        let navigation = navigation.clone();
        let mut controller = controller.clone();
        move |_| {
            controller.close();
            navigation.navigate_to(AppView::SettingsView {});
        }
    };

    let mut search_input = use_signal(|| {
        props
            .manual_search_title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| props.default_search_title.clone())
    });

    let on_toggle_search_panel = {
        let mut search_panel_open = search_panel_open.clone();
        move |_| {
            search_panel_open.set(!search_panel_open());
        }
    };

    let on_search_submit = {
        let search_input = search_input.clone();
        let on_manual_search = props.on_manual_search.clone();
        move |_| {
            on_manual_search.call(search_input().trim().to_string());
        }
    };

    let on_use_current_song = {
        let mut search_input = search_input.clone();
        let default_search_title = props.default_search_title.clone();
        let on_clear_manual_search = props.on_clear_manual_search.clone();
        move |evt: MouseEvent| {
            search_input.set(default_search_title.clone());
            on_clear_manual_search.call(evt);
        }
    };

    let mut on_pick_candidate = {
        let on_select_lyrics_candidate = props.on_select_lyrics_candidate.clone();
        let mut search_panel_open = search_panel_open.clone();
        move |query: LyricsQuery| {
            on_select_lyrics_candidate.call(query);
            search_panel_open.set(false);
        }
    };

    {
        let mut search_input = search_input.clone();
        let manual_search_title = props.manual_search_title.clone();
        let default_search_title = props.default_search_title.clone();
        use_effect(move || {
            let next_value = manual_search_title
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default_search_title.clone());
            if search_input() != next_value {
                search_input.set(next_value);
            }
        });
    }

    let mut on_seek_line = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let offset_seconds = props.offset_seconds;
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        move |line: LyricLine| {
            if !sync_lyrics || is_live_stream {
                return;
            }
            let target = (line.timestamp_seconds - offset_seconds).max(0.0);
            playback_position.set(target);
            audio_state.write().current_time.set(target);
            seek_to(target);
        }
    };

    let last_successful_lyrics = use_signal(|| None::<LyricsResult>);
    {
        let mut last_successful_lyrics = last_successful_lyrics.clone();
        let latest_lyrics = props.lyrics.clone();
        use_effect(move || {
            if let Some(Ok(lyrics)) = latest_lyrics.as_ref() {
                if last_successful_lyrics().as_ref() != Some(lyrics) {
                    last_successful_lyrics.set(Some(lyrics.clone()));
                }
            }
        });
    }

    let fetch_error = match props.lyrics.clone() {
        Some(Err(error)) => Some(error),
        _ => None,
    };
    let display_lyrics = match props.lyrics.clone() {
        Some(Ok(lyrics)) => Some(lyrics),
        Some(Err(_)) | None => last_successful_lyrics(),
    };
    let playback_seconds_signal = playback_position();
    let playback_seconds = if (props.current_time - playback_seconds_signal).abs() > 1.0 {
        props.current_time
    } else {
        playback_seconds_signal
    };
    let active_synced_index = if !props.sync_lyrics || props.is_live_stream {
        None
    } else {
        display_lyrics.as_ref().and_then(|lyrics| {
            active_lyric_index(
                &lyrics.synced_lines,
                playback_seconds + props.offset_seconds,
            )
        })
    };

    let scroll_container_id = format!("lyrics-scroll-{}", sanitize_dom_id(&props.panel_dom_key));

    let on_lyrics_scrolled = {
        let programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let mut manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        move |_| {
            let now = now_millis();
            if now < programmatic_scroll_until_ms() {
                return;
            }
            manual_scroll_hold_until_ms.set(now + 1800.0);
            last_centered_index.set(None);
        }
    };

    {
        let active_synced_index = active_synced_index;
        let scroll_container_id = scroll_container_id.clone();
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        let audio_state = audio_state.clone();
        let mut programmatic_scroll_until_ms = programmatic_scroll_until_ms.clone();
        let manual_scroll_hold_until_ms = manual_scroll_hold_until_ms.clone();
        let mut last_centered_index = last_centered_index.clone();
        use_effect(move || {
            let _playback_tick = (audio_state().current_time)();
            let Some(index) = active_synced_index else {
                return;
            };
            if !sync_lyrics || is_live_stream {
                return;
            }
            if now_millis() < manual_scroll_hold_until_ms() {
                return;
            }

            let should_recenter = last_centered_index() != Some(index);
            if !should_recenter {
                return;
            }

            let line_id = format!("{scroll_container_id}-line-{index}");
            let script = format!(
                r#"(function() {{
                    const container = document.getElementById("{scroll_container_id}");
                    const line = document.getElementById("{line_id}");
                    if (!container || !line) return;
                    const cRect = container.getBoundingClientRect();
                    const lRect = line.getBoundingClientRect();
                    const target = container.scrollTop + (lRect.top - cRect.top) - (cRect.height / 2) + (lRect.height / 2);
                    container.scrollTo({{ top: target, behavior: "auto" }});
                }})();"#
            );
            let _ = document::eval(&script);
            programmatic_scroll_until_ms.set(now_millis() + 250.0);
            last_centered_index.set(Some(index));
        });
    }

    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between gap-2",
                button {
                    class: if search_panel_open() {
                        "px-3 py-1.5 rounded-lg border border-emerald-500/50 text-emerald-300 hover:text-emerald-200 text-xs transition-colors flex items-center gap-2"
                    } else {
                        "px-3 py-1.5 rounded-lg border border-zinc-700/70 text-zinc-300 hover:text-white text-xs transition-colors flex items-center gap-2"
                    },
                    onclick: on_toggle_search_panel,
                    Icon { name: "search".to_string(), class: "w-3.5 h-3.5".to_string() }
                    if search_panel_open() {
                        "Close Search"
                    } else {
                        "Find Lyrics"
                    }
                }
                div { class: "flex items-center gap-2",
                    button {
                        class: "px-3 py-1.5 rounded-lg border border-zinc-700/70 text-zinc-300 hover:text-white text-xs transition-colors",
                        onclick: move |evt| props.on_refresh.call(evt),
                        "Refresh"
                    }
                    button {
                        class: "px-3 py-1.5 rounded-lg bg-emerald-500/20 border border-emerald-500/40 text-emerald-300 hover:text-emerald-200 text-xs transition-colors",
                        onclick: on_open_settings,
                        "Lyrics Settings"
                    }
                }
            }

            if search_panel_open() {
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 space-y-3",
                    p { class: "text-xs uppercase tracking-wider text-zinc-500", "Search And Pick Lyrics" }
                    div { class: "flex flex-col sm:flex-row gap-2",
                        input {
                            r#type: "text",
                            value: "{search_input}",
                            placeholder: "Enter a song title",
                            class: "flex-1 px-3 py-2 rounded-lg border border-zinc-700 bg-zinc-950 text-zinc-100 text-sm focus:outline-none focus:border-emerald-500/50",
                            oninput: move |evt| search_input.set(evt.value()),
                        }
                        button {
                            class: "px-3 py-2 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-white text-sm transition-colors",
                            onclick: on_search_submit,
                            "Search"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg border border-zinc-700 text-zinc-400 hover:text-white transition-colors text-sm",
                            onclick: on_use_current_song,
                            "Use Current Song"
                        }
                    }

                    if let Some(search_term) = props
                        .lyrics_candidates_search_term
                        .clone()
                        .filter(|value| !value.trim().is_empty())
                    {
                        div { class: "rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2 space-y-2",
                            p { class: "text-[11px] text-zinc-500 px-1",
                                "Pick the best match for \"{search_term}\""
                            }
                            match props.lyrics_candidates.clone() {
                                None => rsx! {
                                    div { class: "px-2 py-3 flex items-center gap-2 text-zinc-500 text-sm",
                                        Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                        "Searching..."
                                    }
                                },
                                Some(Err(error)) => rsx! {
                                    p { class: "px-2 py-2 text-xs text-zinc-500 break-words", "{error}" }
                                },
                                Some(Ok(candidates)) => {
                                    if candidates.is_empty() {
                                        rsx! {
                                            p { class: "px-2 py-2 text-xs text-zinc-500", "No lyric matches found." }
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "max-h-56 overflow-y-auto pr-1 space-y-1",
                                                for candidate in candidates {
                                                    button {
                                                        class: if props
                                                            .selected_query_override
                                                            .as_ref()
                                                            == Some(&candidate.query)
                                                        {
                                                            "w-full text-left p-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10"
                                                        } else {
                                                            "w-full text-left p-2 rounded-lg border border-zinc-800/70 hover:border-zinc-600 hover:bg-zinc-900/70 transition-colors"
                                                        },
                                                        onclick: {
                                                            let query = candidate.query.clone();
                                                            move |_| on_pick_candidate(query.clone())
                                                        },
                                                        div { class: "flex items-center justify-between gap-3",
                                                            div { class: "min-w-0",
                                                                p { class: "text-sm text-white truncate", "{candidate.title}" }
                                                                p { class: "text-xs text-zinc-500 truncate",
                                                                    "{candidate.artist}"
                                                                }
                                                            }
                                                            div { class: "text-right flex-shrink-0",
                                                                p { class: "text-[10px] uppercase tracking-wider text-zinc-500",
                                                                    "{candidate.provider.label()}"
                                                                }
                                                                if let Some(duration) = candidate.duration_seconds {
                                                                    p { class: "text-[11px] text-zinc-500 font-mono",
                                                                        "{format_duration(duration)}"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if !candidate.album.trim().is_empty() {
                                                            p { class: "text-[11px] text-zinc-600 truncate mt-1", "{candidate.album}" }
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

            div {
                id: "{scroll_container_id}",
                onscroll: on_lyrics_scrolled,
                class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 min-h-[52vh] md:min-h-[64vh] max-h-[76vh] overflow-y-auto overflow-x-hidden",
                if props.is_live_stream {
                    p { class: "px-5 pt-4 text-xs text-zinc-500",
                        "Live stream detected: synced lyric scrolling and seek controls are disabled."
                    }
                }
                match display_lyrics {
                    None => {
                        if let Some(error) = fetch_error {
                            rsx! {
                                div { class: "p-6 space-y-2",
                                    p { class: "text-sm text-zinc-400", "No lyrics found for this song." }
                                    p { class: "text-xs text-zinc-500",
                                        "Try a manual search and pick the exact match."
                                    }
                                    p { class: "text-xs text-zinc-600 break-words", "{error}" }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-6 flex items-center justify-center text-zinc-500 gap-2",
                                    Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                                    "Loading lyrics..."
                                }
                            }
                        }
                    }
                    Some(lyrics) => {
                        if !props.sync_lyrics || lyrics.synced_lines.is_empty() {
                            let lines = lyrics
                                .plain_lyrics
                                .lines()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .collect::<Vec<_>>();

                            rsx! {
                                div { class: "p-5 space-y-2",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    if props.sync_lyrics && lyrics.synced_lines.is_empty() {
                                        p { class: "text-xs text-zinc-500",
                                            "Synced timestamps are not available from this result. Showing plain lyrics."
                                        }
                                    }
                                    if lines.is_empty() {
                                        p { class: "text-base text-zinc-500", "Lyrics unavailable." }
                                    } else {
                                        for line in lines {
                                            p { class: "text-base text-zinc-300 leading-relaxed whitespace-pre-wrap break-words", "{line}" }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {
                                div { class: "p-4 space-y-1",
                                    if fetch_error.is_some() {
                                        p { class: "text-xs text-amber-300/90 pb-1",
                                            "Using last loaded lyrics because the latest fetch failed."
                                        }
                                    }
                                    div { class: "text-xs uppercase tracking-wider text-zinc-500 pb-1",
                                        "Source: {lyrics.provider.label()}"
                                    }
                                    for (index, line) in lyrics.synced_lines.iter().enumerate() {
                                        button {
                                            id: format!("{scroll_container_id}-line-{index}"),
                                            class: if Some(index) == active_synced_index {
                                                "w-full text-left px-3 py-2.5 rounded-lg bg-emerald-500/15 text-emerald-300 overflow-hidden"
                                            } else {
                                                "w-full text-left px-3 py-2 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/60 transition-colors overflow-hidden"
                                            },
                                            onclick: {
                                                let line = line.clone();
                                                move |_| on_seek_line(line.clone())
                                            },
                                            span { class: "text-xs text-zinc-500 mr-2 font-mono",
                                                "{format_timestamp(line.timestamp_seconds)}"
                                            }
                                            span {
                                                class: if Some(index) == active_synced_index {
                                                    "text-lg md:text-xl font-semibold leading-relaxed whitespace-pre-wrap break-words align-top"
                                                } else {
                                                    "text-base leading-relaxed whitespace-pre-wrap break-words align-top"
                                                },
                                                "{line.text}"
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
