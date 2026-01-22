use crate::api::*;
use crate::components::{AppView, AddIntent, AddMenuController, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn PlaylistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut search_query = use_signal(String::new);
    let limit = use_signal(|| 30usize);
    let refresh = use_signal(|| 0usize);
    let single_active_server = servers().iter().filter(|s| s.active).count() == 1;

    let playlists = use_resource(move || {
        let servers = servers();
        let _refresh = refresh(); // dependency to force reload
        async move {
            let mut playlists = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                if let Ok(server_playlists) = client.get_playlists().await {
                    playlists.extend(server_playlists);
                }
            }
            playlists
        }
    });

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Playlists" }
                    p { class: "page-subtitle", "Your playlists from all servers" }
                    if !single_active_server {
                        p { class: "text-sm text-amber-200/80 bg-amber-500/10 border border-amber-500/40 rounded-lg px-3 py-2 mt-2",
                            "Playlist creation and merging require exactly one active server."
                        }
                    }
                }
                div { class: "relative w-full md:max-w-xs",
                    Icon {
                        name: "search".to_string(),
                        class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-sm text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                        placeholder: "Search playlists",
                        value: search_query,
                        oninput: move |e| {
                            let value = e.value();
                            if value.is_empty() || value.len() >= 2 {
                                search_query.set(value);
                            }
                        },
                    }
                }
                button {
                    class: "px-4 py-2 rounded-xl bg-zinc-800/60 hover:bg-zinc-800 text-zinc-200 text-sm font-medium transition-colors",
                    onclick: {
                        let mut refresh = refresh.clone();
                        move |_| refresh.set(refresh() + 1)
                    },
                    "Refresh"
                }
            }

            {

                match playlists() {
                    Some(playlists) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let mut filtered = playlists.clone();
                        // Newest first: sort descending by creation if available, fallback name
                        filtered.sort_by(|a, b| b.id.cmp(&a.id));
                        if !query.is_empty() {
                            filtered.retain(|p| p.name.to_lowercase().contains(&query));
                        }
                        let has_query = !query.is_empty();
                        let more_available = filtered.len() > limit();
                        let display: Vec<Playlist> = filtered
                            .into_iter()
                            .take(limit())
                            .collect();
                        rsx! {
                            if display.is_empty() {
                                div { class: "flex flex-col items-center justify-center py-20",
                                    Icon {
                                        name: "playlist".to_string(),
                                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                    }
                                    if has_query {
                                        p { class: "text-zinc-300", "No playlists match \"{raw_query}\"" }
                                    } else {
                                        h2 { class: "text-xl font-semibold text-white mb-2", "No playlists yet" }
                                        p { class: "text-zinc-400", "Create playlists in your Navidrome server" }
                                    }
                                }
                            } else {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4",
                                    for playlist in display {
                                        PlaylistCard {
                                            playlist: playlist.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let playlist_id = playlist.id.clone();
                                                let playlist_server_id = playlist.server_id.clone();
                                                move |_| {
                                                    navigation
                                                        .navigate_to(AppView::PlaylistDetailView {
                                                            playlist_id: playlist_id.clone(),
                                                            server_id: playlist_server_id.clone(),
                                                        })
                                                }
                                            },
                                        }
                                    }
                                }
                                if more_available {
                                    div { class: "flex justify-center mt-4",
                                        button {
                                            class: "px-4 py-2 rounded-xl bg-zinc-800/60 hover:bg-zinc-800 text-zinc-200 text-sm font-medium transition-colors",
                                            onclick: {
                                                let mut limit = limit.clone();
                                                move |_| limit.set(limit() + 30)
                                            },
                                            "View more"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    None => rsx! {
                        div { class: "flex items-center justify-center py-20",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-8 h-8 text-zinc-500".to_string(),
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn PlaylistCard(playlist: Playlist, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();

    let on_open_menu = {
        let mut add_menu = add_menu.clone();
        let playlist = playlist.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            add_menu.open(AddIntent::from_playlist(&playlist));
        }
    };

    let cover_url = servers()
        .iter()
        .find(|s| s.id == playlist.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            playlist
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    rsx! {
        button { class: "group text-left", onclick: move |e| onclick.call(e),
            // Playlist cover
            div { class: "aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                                Icon {
                                    name: "playlist".to_string(),
                                    class: "w-12 h-12 text-white/70".to_string(),
                                }
                            }
                        },
                    }
                }
                button {
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/80 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10",
                    aria_label: "Add playlist to queue",
                    onclick: on_open_menu,
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                // Play overlay
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                    div { class: "w-12 h-12 rounded-full bg-emerald-500 flex items-center justify-center shadow-xl transform scale-90 group-hover:scale-100 transition-transform",
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 text-white ml-0.5".to_string(),
                        }
                    }
                }
            }
            // Playlist info
            p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors",
                "{playlist.name}"
            }
            p { class: "text-xs text-zinc-400",
                "{playlist.song_count} songs • {format_duration(playlist.duration / 1000)}"
            }
        }
    }
}
