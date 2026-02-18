// Related songs panel and add/play actions.

#[derive(Props, Clone, PartialEq)]
struct RelatedPanelProps {
    related: Option<Vec<Song>>,
}

#[component]
fn RelatedPanel(props: RelatedPanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let controller = use_context::<SongDetailsController>();
    let add_menu = use_context::<AddMenuController>();

    let Some(related) = props.related.clone() else {
        return rsx! {
            div { class: "h-full flex items-center justify-center text-zinc-500 text-sm gap-2",
                Icon { name: "loader".to_string(), class: "w-4 h-4".to_string() }
                "Loading related songs..."
            }
        };
    };

    if related.is_empty() {
        return rsx! {
            div { class: "h-full flex items-center justify-center text-zinc-500 text-sm",
                "No related songs found for this track."
            }
        };
    }
    let servers_snapshot = servers();

    rsx! {
        div { class: "h-full overflow-y-visible md:overflow-y-auto pr-1 space-y-2",
            for related_song in related {
                button {
                    class: "w-full text-left flex items-center gap-3 p-3 rounded-xl border border-zinc-800/80 hover:border-emerald-500/40 hover:bg-emerald-500/5 transition-colors",
                    onclick: {
                        let related_song = related_song.clone();
                        let mut queue = queue.clone();
                        let mut queue_index = queue_index.clone();
                        let mut now_playing = now_playing.clone();
                        let mut is_playing = is_playing.clone();
                        let mut controller = controller.clone();
                        move |_| {
                            let song_for_queue = related_song.clone();
                            let mut found_index = None;
                            queue.with_mut(|items| {
                                if let Some(existing_index) = items.iter().position(|entry| {
                                    entry.id == song_for_queue.id
                                        && entry.server_id == song_for_queue.server_id
                                }) {
                                    found_index = Some(existing_index);
                                } else {
                                    items.push(song_for_queue.clone());
                                    found_index = Some(items.len().saturating_sub(1));
                                }
                            });
                            if let Some(index) = found_index {
                                queue_index.set(index);
                                now_playing.set(Some(related_song.clone()));
                                is_playing.set(true);
                                controller.open(related_song.clone());
                            }
                        }
                    },
                    {
                        match song_cover_url(&related_song, &servers_snapshot, 96) {
                            Some(url) => rsx! {
                                img {
                                    src: "{url}",
                                    alt: "{related_song.title}",
                                    class: "w-10 h-10 rounded-md object-cover border border-zinc-800/80 flex-shrink-0",
                                    loading: "lazy",
                                }
                            },
                            None => rsx! {
                                div { class: "w-10 h-10 rounded-md bg-zinc-800 flex items-center justify-center text-zinc-500 border border-zinc-800/80 flex-shrink-0",
                                    Icon { name: "music".to_string(), class: "w-4 h-4".to_string() }
                                }
                            },
                        }
                    }
                    div { class: "min-w-0 flex-1",
                        p { class: "text-sm text-white truncate", "{related_song.title}" }
                        p { class: "text-xs text-zinc-500 truncate",
                            "{related_song.artist.clone().unwrap_or_default()}"
                        }
                    }
                    div { class: "flex items-center gap-2 flex-shrink-0",
                        span { class: "text-xs text-zinc-500 font-mono", "{format_duration(related_song.duration)}" }
                        button {
                            class: "p-1.5 rounded-md border border-zinc-700 text-zinc-400 hover:text-white hover:border-zinc-500 transition-colors",
                            title: "Add to queue or playlist",
                            onclick: {
                                let mut add_menu = add_menu.clone();
                                let related_song = related_song.clone();
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    add_menu.open(AddIntent::from_song(related_song.clone()));
                                }
                            },
                            Icon {
                                name: "playlist".to_string(),
                                class: "w-3.5 h-3.5".to_string(),
                            }
                        }
                    }
                }
            }
        }
    }
}

