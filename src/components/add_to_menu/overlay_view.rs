// The add-to sheet: kit Sheet (bottom sheet on compact, centred panel on
// wide). Queue actions first, in the track menu's order (Play Next, Add to
// Queue), then the playlist picker inline, then suggested additions.
{
    let loading_playlists = playlists().is_none();
    let available = playlists().unwrap_or_default();
    let raw_filter = playlist_filter();
    let trimmed_filter = raw_filter.trim().to_string();
    let exact_match = !trimmed_filter.is_empty()
        && available
            .iter()
            .any(|p| p.name.trim().eq_ignore_ascii_case(&trimmed_filter));
    let needle = trimmed_filter.to_lowercase();
    let filtered: Vec<Playlist> = available
        .into_iter()
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .collect();
    let total_filtered = filtered.len();
    let limit = 40usize;
    let shown: Vec<Playlist> = filtered.into_iter().take(limit).collect();
    let can_create = playlist_guard.is_none() && !loading_playlists && !trimmed_filter.is_empty() && !exact_match;
    let busy = is_processing();
    let is_single_song = matches!(intent_for_display.target, AddTarget::Song(_));
    let (head_cover, head_server) = match &intent_for_display.target {
        AddTarget::Song(song) => (song.cover_art.clone(), song.server_id.clone()),
        AddTarget::Songs(songs) => (
            songs.first().and_then(|s| s.cover_art.clone()),
            songs.first().map(|s| s.server_id.clone()).unwrap_or_default(),
        ),
        AddTarget::Album { server_id, .. } | AddTarget::Playlist { server_id, .. } => {
            (None, server_id.clone())
        }
    };
    let head_src = preview_cover().flatten();
    let is_album = matches!(intent_for_display.target, AddTarget::Album { .. });
    let kind = match &intent_for_display.target {
        AddTarget::Song(_) => "Song".to_string(),
        AddTarget::Songs(songs) => format!("{} songs", songs.len()),
        AddTarget::Album { .. } => "Album".to_string(),
        AddTarget::Playlist { .. } => "Playlist".to_string(),
    };

    rsx! {
        Sheet {
            label: format!("Add {}", intent.label),
            testid: "add-menu".to_string(),
            on_close: {
                let mut controller = controller.clone();
                move |_| controller.close()
            },
            div { class: "rs-player-addhead",
                if is_album {
                    button {
                        r#type: "button",
                        class: "rs-player-addhead__art",
                        aria_label: "Open album",
                        onclick: on_cover_click,
                        Artwork { cover_art: head_cover.clone(), server_id: head_server.clone(), src: head_src.clone(), size_px: 52, placeholder: "album".to_string() }
                    }
                } else {
                    Artwork { cover_art: head_cover.clone(), server_id: head_server.clone(), src: head_src.clone(), size_px: 52, placeholder: "playlist".to_string() }
                }
                div { class: "rs-player-addhead__meta",
                    p { class: "rs-player-addhead__kind", "{kind}" }
                    h2 { class: "rs-player-addhead__title", "{intent.label}" }
                }
                IconButton {
                    icon: "x".to_string(),
                    label: "Close".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    onclick: {
                        let mut controller = controller.clone();
                        move |_| controller.close()
                    },
                }
            }

            if busy {
                div { class: "rs-player-addnote", role: "status",
                    span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                    "{processing_label().unwrap_or_else(|| \"Working…\".to_string())}"
                }
            } else if let Some((ok, text)) = message() {
                div {
                    class: "rs-player-addnote",
                    class: if ok { "rs-player-addnote--ok" } else { "rs-player-addnote--error" },
                    role: "status",
                    Icon { name: if ok { "check".to_string() } else { "info".to_string() }, class: "rs-ic rs-ic--sm".to_string() }
                    "{text}"
                }
            }

            ul { class: "rs-list rs-list--icons", aria_label: "Queue actions",
                li {
                    button {
                        r#type: "button",
                        class: "rs-row rs-row--compact",
                        disabled: busy,
                        "data-testid": "add-menu-play-next",
                        onclick: make_add_to_queue("next"),
                        span { class: "rs-row__lead", Icon { name: "play-next".to_string(), class: "rs-ic rs-ic--md".to_string() } }
                        span { class: "rs-row__meta", span { class: "rs-row__title", span { "Play Next" } } }
                    }
                }
                li {
                    button {
                        r#type: "button",
                        class: "rs-row rs-row--compact",
                        disabled: busy,
                        "data-testid": "add-menu-add-to-queue",
                        onclick: make_add_to_queue("end"),
                        span { class: "rs-row__lead", Icon { name: "add-to-queue".to_string(), class: "rs-ic rs-ic--md".to_string() } }
                        span { class: "rs-row__meta", span { class: "rs-row__title", span { "Add to Queue" } } }
                    }
                }
                if is_single_song {
                    li {
                        button {
                            r#type: "button",
                            class: "rs-row rs-row--compact",
                            disabled: busy,
                            "data-testid": "add-menu-similar-mix",
                            onclick: on_create_similar,
                            span { class: "rs-row__lead", Icon { name: "shuffle".to_string(), class: "rs-ic rs-ic--md".to_string() } }
                            span { class: "rs-row__meta",
                                span { class: "rs-row__title", span { "Create Similar Mix" } }
                                span { class: "rs-row__sub", style: "display:block", "Replaces the queue with ~50 similar songs" }
                            }
                        }
                    }
                }
            }

            SectionHeader { title: "Add to Playlist".to_string() }
            if let Some(reason) = playlist_guard.clone() {
                p { class: "rs-player-addnote rs-player-addnote--warn", "{reason}" }
            } else {
                label { class: "rs-field rs-player-search__field",
                    Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    input {
                        r#type: "search",
                        placeholder: "Find or name a playlist",
                        aria_label: "Find or name a playlist",
                        value: "{playlist_filter}",
                        "data-testid": "add-menu-playlist-search",
                        oninput: move |evt| {
                            let value = evt.value();
                            playlist_filter.set(value.clone());
                            new_playlist_name.set(value);
                        },
                    }
                }
                ul { class: "rs-list", aria_label: "Playlists", "data-testid": "add-menu-playlists",
                    if can_create {
                        li {
                            button {
                                r#type: "button",
                                class: "rs-row rs-row--compact",
                                disabled: busy,
                                "data-testid": "add-menu-create-playlist",
                                onclick: create_playlist,
                                span { class: "rs-row__lead", Icon { name: "plus".to_string(), class: "rs-ic rs-ic--md".to_string() } }
                                span { class: "rs-row__meta",
                                    span { class: "rs-row__title", span { "Create \u{201C}{trimmed_filter}\u{201D}" } }
                                }
                            }
                        }
                    }
                    if loading_playlists {
                        li { div { class: "rs-row rs-row--compact rs-player-addrow__muted", "Loading playlists…" } }
                    } else if shown.is_empty() && !can_create {
                        li { div { class: "rs-row rs-row--compact rs-player-addrow__muted",
                            if trimmed_filter.is_empty() { "No playlists of yours on this server yet." } else { "No matching playlists." }
                        } }
                    }
                    for playlist in shown {
                        li { key: "{playlist.id}",
                            button {
                                r#type: "button",
                                class: "rs-row",
                                disabled: busy,
                                onclick: make_add_to_playlist(playlist.id.clone()),
                                Artwork { cover_art: playlist.cover_art.clone(), server_id: playlist.server_id.clone(), size_px: 44, placeholder: "playlist".to_string() }
                                span { class: "rs-row__meta",
                                    span { class: "rs-row__title", span { "{playlist.name}" } }
                                    span { class: "rs-row__sub", style: "display:block", "{playlist.song_count} songs" }
                                }
                            }
                        }
                    }
                }
                if total_filtered > limit {
                    p { class: "rs-list-foot", "Showing the first {limit} playlists. Type to narrow." }
                }
            }

            if suggestion_destination().is_some() {
                SectionHeader { title: "Suggested Additions".to_string() }
                if suggestions_loading() {
                    Skeleton { rows: 3 }
                } else if suggestion_candidates().is_empty() {
                    p { class: "rs-list-foot", "No similar songs found yet." }
                } else {
                    ul { class: "rs-list", aria_label: "Suggested additions",
                        for song in suggestion_candidates() {
                            li { key: "{song.server_id}:{song.id}",
                                div { class: "rs-trow rs-trow--art rs-player-addrow",
                                    Artwork { cover_art: song.cover_art.clone(), server_id: song.server_id.clone(), size_px: 44 }
                                    div { class: "rs-trow__meta",
                                        div { class: "rs-trow__title", "{song.title}" }
                                        div { class: "rs-trow__sub", "{song.artist.clone().unwrap_or_else(|| \"Unknown Artist\".to_string())}" }
                                    }
                                    button {
                                        r#type: "button",
                                        class: "rs-btn rs-btn--plain",
                                        title: "Play 12 seconds, then return to your song",
                                        disabled: preview_song_key() == Some(song_key(&song)),
                                        onclick: {
                                            let song = song.clone();
                                            let on_preview_song = on_preview_song.clone();
                                            move |_| on_preview_song(song.clone())
                                        },
                                        if preview_song_key() == Some(song_key(&song)) { "Previewing…" } else { "Preview" }
                                    }
                                    button {
                                        r#type: "button",
                                        class: "rs-btn rs-btn--plain",
                                        disabled: busy,
                                        onclick: {
                                            let song = song.clone();
                                            let mut on_quick_add_suggestion = on_quick_add_suggestion.clone();
                                            move |_| on_quick_add_suggestion(song.clone())
                                        },
                                        "Add"
                                    }
                                }
                            }
                        }
                    }
                    p { class: "rs-list-foot", "Add refreshes the list with more picks like it." }
                }
            }
        }
    }
}
