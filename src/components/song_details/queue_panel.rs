// Queue panel for building and navigating the upcoming tracks list.

#[derive(Props, Clone, PartialEq)]
struct QueuePanelProps {
    up_next: Vec<(usize, Song)>,
    seed_song: Song,
    create_queue_busy: Signal<bool>,
    disabled_for_live: bool,
}

#[component]
fn QueuePanel(props: QueuePanelProps) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let is_playing = use_context::<Signal<bool>>();
    let controller = use_context::<SongDetailsController>();

    let on_create_queue = {
        let seed_song = props.seed_song.clone();
        let mut create_queue_busy = props.create_queue_busy.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        let mut queue_index = queue_index.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        move |_| {
            if create_queue_busy() {
                return;
            }
            create_queue_busy.set(true);
            let seed_song = seed_song.clone();
            let servers_snapshot = servers();
            spawn(async move {
                let generated = build_queue_from_seed(seed_song.clone(), servers_snapshot).await;
                if generated.is_empty() {
                    queue.set(vec![seed_song.clone()]);
                    queue_index.set(0);
                    now_playing.set(Some(seed_song));
                } else {
                    queue.set(generated.clone());
                    queue_index.set(0);
                    now_playing.set(generated.first().cloned());
                }
                is_playing.set(true);
                create_queue_busy.set(false);
            });
        }
    };
    let servers_snapshot = servers();

    if props.disabled_for_live {
        return rsx! {
            div { class: "h-full flex flex-col items-center justify-center text-center px-4 gap-3",
                p { class: "text-zinc-400 text-sm", "Up Next is unavailable during live radio playback." }
                p { class: "text-zinc-500 text-xs", "Queue editing and queue generation are disabled until you play a regular track." }
            }
        };
    }

    if props.up_next.is_empty() {
        return rsx! {
            div { class: "h-full flex flex-col items-center justify-center text-center px-4 gap-3",
                p { class: "text-zinc-400 text-sm", "Queue is empty for this track." }
                p { class: "text-zinc-500 text-xs", "Generate a queue from similar songs and keep this song as the seed." }
                button {
                    class: if (props.create_queue_busy)() {
                        "px-4 py-2 rounded-xl bg-zinc-700 text-zinc-300 text-sm cursor-not-allowed"
                    } else {
                        "px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-white text-sm transition-colors"
                    },
                    disabled: (props.create_queue_busy)(),
                    onclick: on_create_queue,
                    if (props.create_queue_busy)() {
                        "Creating queue..."
                    } else {
                        "Create Queue From This Song"
                    }
                }
            }
        };
    }

    rsx! {
        div { class: "h-full overflow-y-visible md:overflow-y-auto pr-1 space-y-2",
            for (index, entry) in props.up_next.iter() {
                div {
                    key: "{entry.server_id}:{entry.id}:{index}",
                    class: "w-full rounded-xl border border-zinc-800/80 transition-all",
                    div { class: "flex items-center gap-2 p-3",
                        button {
                            class: "flex-1 text-left flex items-center gap-3 hover:bg-emerald-500/5 rounded-lg px-1 py-1 transition-colors min-w-0",
                            onclick: {
                                let selected_song = entry.clone();
                                let mut queue_index = queue_index.clone();
                                let mut now_playing = now_playing.clone();
                                let mut is_playing = is_playing.clone();
                                let mut controller = controller.clone();
                                let index = *index;
                                move |_| {
                                    queue_index.set(index);
                                    now_playing.set(Some(selected_song.clone()));
                                    is_playing.set(true);
                                    controller.open(selected_song.clone());
                                }
                            },
                            span { class: "w-6 text-xs text-zinc-500 text-right font-mono flex-shrink-0",
                                "{index + 1}"
                            }
                            {
                                match song_cover_url(entry, &servers_snapshot, 96) {
                                    Some(url) => rsx! {
                                        img {
                                            src: "{url}",
                                            alt: "{entry.title}",
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
                                p { class: "text-sm text-white truncate", "{entry.title}" }
                                p { class: "text-xs text-zinc-500 truncate",
                                    "{entry.artist.clone().unwrap_or_default()}"
                                }
                            }
                            span { class: "text-xs text-zinc-500 font-mono flex-shrink-0",
                                "{format_duration(entry.duration)}"
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            button {
                                r#type: "button",
                                class: if *index > queue_index().saturating_add(1) {
                                    "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                } else {
                                    "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                },
                                title: "Move up",
                                disabled: *index <= queue_index().saturating_add(1),
                                onclick: {
                                    let queue = queue.clone();
                                    let queue_index_signal = queue_index.clone();
                                    let now_playing = now_playing.clone();
                                    let source_index = *index;
                                    move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        if source_index <= queue_index_signal().saturating_add(1) {
                                            return;
                                        }
                                        reorder_queue_entry(
                                            queue.clone(),
                                            queue_index_signal.clone(),
                                            now_playing.clone(),
                                            source_index,
                                            source_index.saturating_sub(1),
                                        );
                                    }
                                },
                                Icon { name: "chevron-up".to_string(), class: "w-3.5 h-3.5".to_string() }
                            }
                            button {
                                r#type: "button",
                                class: if *index + 1 < queue().len() {
                                    "w-7 h-7 rounded-md border border-zinc-700/80 text-zinc-300 hover:text-white hover:border-emerald-500/60 transition-colors flex items-center justify-center"
                                } else {
                                    "w-7 h-7 rounded-md border border-zinc-800 text-zinc-600 cursor-not-allowed flex items-center justify-center"
                                },
                                title: "Move down",
                                disabled: *index + 1 >= queue().len(),
                                onclick: {
                                    let queue = queue.clone();
                                    let queue_index_signal = queue_index.clone();
                                    let now_playing = now_playing.clone();
                                    let source_index = *index;
                                    move |evt: MouseEvent| {
                                        evt.stop_propagation();
                                        if source_index + 1 >= queue().len() {
                                            return;
                                        }
                                        reorder_queue_entry(
                                            queue.clone(),
                                            queue_index_signal.clone(),
                                            now_playing.clone(),
                                            source_index,
                                            source_index.saturating_add(1),
                                        );
                                    }
                                },
                                Icon { name: "chevron-down".to_string(), class: "w-3.5 h-3.5".to_string() }
                            }
                        }
                        button {
                            class: "p-2 rounded-lg border border-zinc-800/80 text-zinc-500 hover:text-red-400 hover:border-red-500/40 transition-colors",
                            title: "Remove from queue",
                            onclick: {
                                let queue = queue.clone();
                                let queue_index = queue_index.clone();
                                let now_playing = now_playing.clone();
                                let is_playing = is_playing.clone();
                                let remove_index = *index;
                                move |evt: MouseEvent| {
                                    evt.stop_propagation();
                                    remove_queue_entry(
                                        queue.clone(),
                                        queue_index.clone(),
                                        now_playing.clone(),
                                        is_playing.clone(),
                                        remove_index,
                                    );
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

