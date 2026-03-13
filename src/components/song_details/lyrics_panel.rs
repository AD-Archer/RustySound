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

fn plain_lyrics_lines(lyrics: &LyricsResult) -> Vec<String> {
    lyrics
        .plain_lyrics
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

#[derive(Clone, PartialEq)]
struct ScreenshotLyricBar {
    text: String,
    timestamp_seconds: Option<f64>,
}

fn screenshot_lyrics_bars(lyrics: &LyricsResult, sync_lyrics: bool) -> Vec<ScreenshotLyricBar> {
    if sync_lyrics && !lyrics.synced_lines.is_empty() {
        lyrics
            .synced_lines
            .iter()
            .map(|line| ScreenshotLyricBar {
                text: line.text.trim().to_string(),
                timestamp_seconds: Some(line.timestamp_seconds),
            })
            .filter(|line| !line.text.is_empty())
            .collect()
    } else {
        plain_lyrics_lines(lyrics)
            .into_iter()
            .map(|line| ScreenshotLyricBar {
                text: line,
                timestamp_seconds: None,
            })
            .collect()
    }
}

fn screenshot_bar_label(bar: &ScreenshotLyricBar, include_timestamp: bool) -> String {
    if include_timestamp {
        if let Some(timestamp_seconds) = bar.timestamp_seconds {
            return format!("{} {}", format_timestamp(timestamp_seconds), bar.text);
        }
    }

    bar.text.clone()
}

const RUSTYSOUND_MARK: Asset = asset!("/assets/favicon-96x96.png");

#[component]
fn LyricsPanel(props: LyricsPanelProps) -> Element {
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let audio_state = use_context::<Signal<AudioState>>();
    let search_panel_open = use_signal(|| false);
    let screenshot_view_open = use_signal(|| false);
    let screenshot_selection_start = use_signal(|| 0_usize);
    let screenshot_selection_count = use_signal(|| 1_usize);
    let screenshot_manual_selection = use_signal(|| false);
    let screenshot_shot_mode = use_signal(|| false);
    let programmatic_scroll_until_ms = use_signal(|| 0.0_f64);
    let manual_scroll_hold_until_ms = use_signal(|| 0.0_f64);
    let last_centered_index = use_signal(|| None::<usize>);

    let screenshot_settings = app_settings();
    let screenshot_mode_enabled = screenshot_settings.lyrics_screenshot_mode;
    let screenshot_show_timestamps = screenshot_settings.lyrics_screenshot_timestamps;
    let screenshot_song = controller.current().song;
    let screenshot_cover_url = screenshot_song
        .as_ref()
        .and_then(|song| song_cover_url(song, &servers(), 900))
        .filter(|url| !url.trim().is_empty());
    let screenshot_song_title = screenshot_song
        .as_ref()
        .map(|song| song.title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Unknown Song".to_string());
    let screenshot_song_artist = screenshot_song
        .as_ref()
        .and_then(|song| song.artist.as_ref())
        .map(|artist| artist.trim().to_string())
        .filter(|artist| !artist.is_empty());

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
    let screenshot_bars = display_lyrics
        .as_ref()
        .map(|lyrics| screenshot_lyrics_bars(lyrics, props.sync_lyrics))
        .unwrap_or_default();
    let screenshot_available = !screenshot_bars.is_empty();
    let screenshot_selected_start = if screenshot_available {
        screenshot_selection_start().min(screenshot_bars.len() - 1)
    } else {
        0
    };
    let screenshot_selected_count = if screenshot_available {
        screenshot_selection_count()
            .clamp(1, 5)
            .min(screenshot_bars.len() - screenshot_selected_start)
    } else {
        0
    };
    let screenshot_selected_end = if screenshot_selected_count > 0 {
        screenshot_selected_start + screenshot_selected_count - 1
    } else {
        0
    };
    let screenshot_scroll_container_id = format!(
        "lyrics-screenshot-scroll-{}",
        sanitize_dom_id(&props.panel_dom_key)
    );
    let screenshot_shot_mode_enabled = screenshot_shot_mode();
    let screenshot_selected_line_class =
        "block w-full rounded-2xl px-1 py-1.5 text-left text-[1.85rem] md:text-[3.05rem] font-semibold leading-[1.08] text-white whitespace-pre-wrap break-words transition-colors";
    let screenshot_unselected_line_class =
        "block w-full rounded-2xl px-1 py-1.5 text-left text-[1.85rem] md:text-[3.05rem] font-semibold leading-[1.08] text-white/36 hover:bg-white/6 hover:text-white/58 whitespace-pre-wrap break-words transition-colors";
    let screenshot_browser_width_class = "max-w-5xl";
    let screenshot_selected_bars = if screenshot_selected_count > 0 {
        screenshot_bars
            .iter()
            .skip(screenshot_selected_start)
            .take(screenshot_selected_count)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let screenshot_share_lyrics_class = match screenshot_selected_count {
        0 | 1 => "text-[2rem] md:text-[2.9rem] font-semibold leading-[1.02] text-zinc-950",
        2 => "text-[1.7rem] md:text-[2.35rem] font-semibold leading-[1.05] text-zinc-950",
        3 => "text-[1.45rem] md:text-[1.95rem] font-semibold leading-[1.08] text-zinc-950",
        4 => "text-[1.2rem] md:text-[1.6rem] font-semibold leading-[1.1] text-zinc-950",
        _ => "text-[1.02rem] md:text-[1.35rem] font-semibold leading-[1.1] text-zinc-950",
    };
    let screenshot_share_spacing_class = match screenshot_selected_count {
        0 | 1 => "space-y-6",
        2 => "space-y-5",
        3 => "space-y-4",
        4 => "space-y-3.5",
        _ => "space-y-3",
    };
    let toolbar_button_base_class =
        "h-10 w-10 rounded-full border flex items-center justify-center transition-colors";
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

    {
        let screenshot_view_open = screenshot_view_open.clone();
        let screenshot_selection_start = screenshot_selection_start.clone();
        let screenshot_scroll_container_id = screenshot_scroll_container_id.clone();
        let screenshot_bar_total = screenshot_bars.len();
        use_effect(move || {
            if !screenshot_view_open() || screenshot_bar_total == 0 {
                return;
            }

            let selected_start = screenshot_selection_start().min(screenshot_bar_total - 1);
            let line_id = format!("{screenshot_scroll_container_id}-line-{selected_start}");
            let script = format!(
                r#"(function() {{
                    const container = document.getElementById("{screenshot_scroll_container_id}");
                    const line = document.getElementById("{line_id}");
                    if (!container || !line) return;
                    const cRect = container.getBoundingClientRect();
                    const lRect = line.getBoundingClientRect();
                    const target = container.scrollTop + (lRect.top - cRect.top) - (cRect.height / 2) + (lRect.height / 2);
                    container.scrollTo({{ top: target, behavior: "auto" }});
                }})();"#
            );
            let _ = document::eval(&script);
        });
    }

    {
        let screenshot_view_open = screenshot_view_open.clone();
        let screenshot_manual_selection = screenshot_manual_selection.clone();
        let mut screenshot_selection_start = screenshot_selection_start.clone();
        let active_synced_index = active_synced_index;
        let sync_lyrics = props.sync_lyrics;
        let is_live_stream = props.is_live_stream;
        use_effect(move || {
            if !screenshot_view_open()
                || screenshot_manual_selection()
                || !sync_lyrics
                || is_live_stream
            {
                return;
            }

            if let Some(index) = active_synced_index {
                if screenshot_selection_start() != index {
                    screenshot_selection_start.set(index);
                }
            }
        });
    }

    let on_open_screenshot_view = {
        let mut screenshot_view_open = screenshot_view_open.clone();
        let mut screenshot_selection_start = screenshot_selection_start.clone();
        let mut screenshot_selection_count = screenshot_selection_count.clone();
        let mut screenshot_manual_selection = screenshot_manual_selection.clone();
        let mut screenshot_shot_mode = screenshot_shot_mode.clone();
        let active_synced_index = active_synced_index;
        let screenshot_bars = screenshot_bars.clone();
        move |_| {
            let focus_index = active_synced_index
                .unwrap_or(0)
                .min(screenshot_bars.len().saturating_sub(1));
            screenshot_selection_start.set(focus_index);
            screenshot_selection_count.set(1);
            screenshot_manual_selection.set(false);
            screenshot_shot_mode.set(false);
            screenshot_view_open.set(true);
        }
    };

    let on_close_screenshot_view = {
        let mut screenshot_view_open = screenshot_view_open.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            screenshot_view_open.set(false);
        }
    };

    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between gap-2",
                button {
                    class: if search_panel_open() { "{toolbar_button_base_class} border-emerald-500/50 text-emerald-300 hover:text-emerald-200" } else { "{toolbar_button_base_class} border-zinc-700/70 text-zinc-300 hover:text-white" },
                    title: if search_panel_open() { "Close lyrics search" } else { "Open lyrics search" },
                    onclick: on_toggle_search_panel,
                    Icon {
                        name: "search".to_string(),
                        class: "w-4.5 h-4.5".to_string(),
                    }
                }
                div { class: "flex items-center gap-2",
                    if screenshot_mode_enabled {
                        button {
                            class: if screenshot_available { "{toolbar_button_base_class} border-cyan-500/40 text-cyan-300 hover:text-white hover:border-cyan-300" } else { "{toolbar_button_base_class} border-zinc-700/70 text-zinc-500 cursor-not-allowed" },
                            title: "Open lyrics screenshot view",
                            disabled: !screenshot_available,
                            onclick: on_open_screenshot_view,
                            Icon {
                                name: "eye".to_string(),
                                class: "w-4.5 h-4.5".to_string(),
                            }
                        }
                    }
                    button {
                        class: "{toolbar_button_base_class} border-zinc-700/70 text-zinc-300 hover:text-white",
                        title: "Refresh lyrics",
                        onclick: move |evt| props.on_refresh.call(evt),
                        Icon {
                            name: "refresh-cw".to_string(),
                            class: "w-4.5 h-4.5".to_string(),
                        }
                    }
                    button {
                        class: "{toolbar_button_base_class} border-emerald-500/40 bg-emerald-500/20 text-emerald-300 hover:text-emerald-200",
                        title: "Open lyrics settings",
                        onclick: on_open_settings,
                        Icon {
                            name: "settings".to_string(),
                            class: "w-4.5 h-4.5".to_string(),
                        }
                    }
                }
            }

            if search_panel_open() {
                div { class: "rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-3 space-y-3",
                    p { class: "text-xs uppercase tracking-wider text-zinc-500",
                        "Search And Pick Lyrics"
                    }
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
                                                                p { class: "text-xs text-zinc-500 truncate", "{candidate.artist}" }
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
                match display_lyrics.clone() {
                    None => {
                        if let Some(error) = fetch_error {
                            rsx! {
                                div { class: "p-6 space-y-2",
                                    p { class: "text-sm text-zinc-400", "No lyrics found for this song." }
                                    p { class: "text-xs text-zinc-500", "Try a manual search and pick the exact match." }
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
                            let lines = plain_lyrics_lines(&lyrics);

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
                                            p { class: "text-base text-zinc-300 leading-relaxed whitespace-pre-wrap break-words",
                                                "{line}"
                                            }
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
                                    for (index , line) in lyrics.synced_lines.iter().enumerate() {
                                        button {
                                            id: format!("{scroll_container_id}-line-{index}"),
                                            class: if Some(index) == active_synced_index { "w-full text-left px-3 py-2.5 rounded-lg bg-emerald-500/15 text-emerald-300 overflow-hidden" } else { "w-full text-left px-3 py-2 rounded-lg text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/60 transition-colors overflow-hidden" },
                                            onclick: {
                                                let line = line.clone();
                                                move |_| on_seek_line(line.clone())
                                            },
                                            span { class: "text-xs text-zinc-500 mr-2 font-mono",
                                                "{format_timestamp(line.timestamp_seconds)}"
                                            }
                                            span { class: if Some(index) == active_synced_index { "text-lg md:text-xl font-semibold leading-relaxed whitespace-pre-wrap break-words align-top" } else { "text-base leading-relaxed whitespace-pre-wrap break-words align-top" },
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

            if screenshot_view_open() && screenshot_mode_enabled {
                div {
                    class: "fixed inset-0 z-[120] bg-black/88 backdrop-blur-md",
                    onclick: {
                        let mut screenshot_view_open = screenshot_view_open.clone();
                        let mut screenshot_shot_mode = screenshot_shot_mode.clone();
                        move |_| {
                            if screenshot_shot_mode() {
                                screenshot_shot_mode.set(false);
                            } else {
                                screenshot_view_open.set(false);
                            }
                        }
                    },
                    button {
                        class: "absolute top-12 right-4 z-20 rounded-full border border-white/15 bg-black/35 p-2 text-white/80 hover:text-white hover:border-white/30 transition-colors md:top-14 md:right-6",
                        onclick: on_close_screenshot_view,
                        Icon {
                            name: "x".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                    }
                    if !screenshot_shot_mode_enabled {
                        button {
                            class: "absolute top-12 left-4 z-20 rounded-full border border-white/15 bg-black/35 px-4 py-2 text-sm font-medium text-white/80 hover:text-white hover:border-white/30 transition-colors md:top-14 md:left-6",
                            onclick: {
                                let mut screenshot_shot_mode = screenshot_shot_mode.clone();
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    screenshot_shot_mode.set(true);
                                }
                            },
                            "Shot"
                        }
                    }
                    div {
                        class: "flex h-full w-full flex-col",
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        div { class: "relative flex-1 overflow-hidden bg-zinc-950 shadow-[0_40px_120px_rgba(0,0,0,0.65)]",
                            if let Some(url) = screenshot_cover_url.clone() {
                                img {
                                    class: "absolute inset-0 h-full w-full object-cover scale-110 blur-3xl opacity-35",
                                    src: "{url}",
                                    alt: "{screenshot_song_title}",
                                }
                            }
                            div { class: "absolute inset-0 bg-[linear-gradient(180deg,rgba(74,145,173,0.72)_0%,rgba(26,57,73,0.84)_42%,rgba(8,11,16,0.98)_100%)]" }
                            if screenshot_shot_mode_enabled {
                                div { class: "relative z-10 flex h-full min-h-0 w-full items-center justify-center px-4 pb-6 pt-16 md:px-8 md:pb-10 md:pt-20",
                                    div {
                                        class: "relative aspect-square overflow-hidden rounded-[2rem] border border-white/14 bg-[#62bac9] shadow-[0_28px_90px_rgba(0,0,0,0.35)]",
                                        style: "width:min(33rem, calc(100vw - 2.5rem), calc(100vh - 7rem));",
                                        div { class: "absolute inset-0 bg-[linear-gradient(180deg,rgba(255,255,255,0.14)_0%,rgba(255,255,255,0.03)_100%)]" }
                                        div { class: "relative flex h-full flex-col p-5 md:p-6",
                                            div { class: "flex items-start gap-3",
                                                if let Some(url) = screenshot_cover_url.clone() {
                                                    img {
                                                        class: "h-14 w-14 rounded-2xl object-cover shadow-lg md:h-16 md:w-16",
                                                        src: "{url}",
                                                        alt: "{screenshot_song_title}",
                                                    }
                                                } else {
                                                    div { class: "flex h-14 w-14 items-center justify-center rounded-2xl bg-zinc-900/10 text-zinc-950/75 md:h-16 md:w-16",
                                                        Icon {
                                                            name: "music".to_string(),
                                                            class: "h-7 w-7".to_string(),
                                                        }
                                                    }
                                                }
                                                div { class: "min-w-0 flex-1",
                                                    p { class: "truncate text-2xl font-semibold leading-tight text-zinc-950 md:text-[2rem]",
                                                        "{screenshot_song_title}"
                                                    }
                                                    if let Some(artist) = screenshot_song_artist.clone() {
                                                        p { class: "truncate text-lg font-medium text-zinc-950/65 md:text-[1.35rem]",
                                                            "{artist}"
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex flex-1 items-center py-5 md:py-6",
                                                if screenshot_selected_bars.is_empty() {
                                                    p { class: "text-xl font-semibold text-zinc-950/80",
                                                        "Lyrics unavailable."
                                                    }
                                                } else {
                                                    div { class: "w-full {screenshot_share_spacing_class}",
                                                        for bar in screenshot_selected_bars.iter() {
                                                            p { class: "{screenshot_share_lyrics_class}",
                                                                "{bar.text}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "flex items-center gap-3 pt-3",
                                                div { class: "flex items-center gap-3",
                                                    img {
                                                        class: "h-8 w-8 rounded-lg",
                                                        src: RUSTYSOUND_MARK,
                                                        alt: "RustySound",
                                                    }
                                                    div {
                                                        p { class: "text-sm font-semibold uppercase tracking-[0.22em] text-zinc-950/80",
                                                            "RustySound"
                                                        }
                                                        p { class: "text-xs text-zinc-950/50",
                                                            "Shared lyrics"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "relative z-10 mx-auto flex h-full min-h-0 w-full {screenshot_browser_width_class} flex-col px-6 pb-8 pt-24 md:px-12 md:pb-10 md:pt-28",
                                    div { class: "space-y-1 md:max-w-[70%]",
                                        h3 { class: "text-2xl md:text-4xl font-semibold leading-tight text-white",
                                            "{screenshot_song_title}"
                                        }
                                        if let Some(artist) = screenshot_song_artist.clone() {
                                            p { class: "text-sm md:text-base text-white/70",
                                                "{artist}"
                                            }
                                        }
                                    }
                                    div {
                                        id: "{screenshot_scroll_container_id}",
                                        class: "mt-8 flex-1 overflow-y-auto pr-2 md:mt-10",
                                        if screenshot_bars.is_empty() {
                                            p { class: "text-lg text-white/70", "Lyrics unavailable." }
                                        } else {
                                            div { class: "max-w-4xl space-y-4 pb-24 md:space-y-5 md:pb-28",
                                                for (index , bar) in screenshot_bars.iter().enumerate() {
                                                    button {
                                                        id: format!("{screenshot_scroll_container_id}-line-{index}"),
                                                        class: if index >= screenshot_selected_start && index <= screenshot_selected_end { screenshot_selected_line_class } else { screenshot_unselected_line_class },
                                                        onclick: {
                                                            let mut screenshot_selection_start = screenshot_selection_start.clone();
                                                            let mut screenshot_selection_count = screenshot_selection_count.clone();
                                                            let mut screenshot_manual_selection = screenshot_manual_selection.clone();
                                                            move |_| {
                                                                screenshot_manual_selection.set(true);
                                                                if index >= screenshot_selected_start
                                                                    && index - screenshot_selected_start < 5
                                                                {
                                                                    screenshot_selection_count.set(index - screenshot_selected_start + 1);
                                                                } else {
                                                                    screenshot_selection_start.set(index);
                                                                    screenshot_selection_count.set(1);
                                                                }
                                                            }
                                                        },
                                                        "{screenshot_bar_label(bar, screenshot_show_timestamps)}"
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