use crate::api::*;
use crate::components::views::home::AlbumCard;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn AlbumsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();

    let mut album_type = use_signal(|| "alphabeticalByName".to_string());
    let mut search_query = use_signal(String::new);

    let albums = use_resource(move || {
        let servers = servers();
        let album_type = album_type();
        async move {
            let mut albums = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                if let Ok(server_albums) = client.get_albums(&album_type, 50, 0).await {
                    albums.extend(server_albums);
                }
            }
            albums
        }
    });

    let album_types = vec![
        ("alphabeticalByName", "A-Z"),
        ("newest", "Newest"),
        ("frequent", "Most Played"),
        ("recent", "Recently Played"),
        ("random", "Random"),
    ];

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header gap-4",
                h1 { class: "page-title", "Albums" }

                div { class: "flex flex-col gap-3 md:flex-row md:items-center md:justify-between",
                    // Filter tabs
                    div { class: "flex gap-2 flex-wrap",
                        for (value, label) in album_types {
                            button {
                                class: if album_type() == value {
                                    "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium"
                                } else {
                                    "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors"
                                },
                                onclick: move |_| album_type.set(value.to_string()),
                                "{label}"
                            }
                        }
                    }
                    // Search
                    div { class: "relative w-full md:max-w-xs",
                        Icon {
                            name: "search".to_string(),
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-sm text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                            placeholder: "Search albums",
                            value: search_query,
                            oninput: move |e| search_query.set(e.value()),
                        }
                    }
                }
            }

            {match albums() {
                Some(albums) => {
                    let raw_query = search_query().trim().to_string();
                    let query = raw_query.to_lowercase();
                    let mut filtered = Vec::new();
                    if query.is_empty() {
                        filtered = albums.clone();
                    } else {
                        for album in &albums {
                            let name = album.name.to_lowercase();
                            let artist = album.artist.to_lowercase();
                            if name.contains(&query) || artist.contains(&query) {
                                filtered.push(album.clone());
                            }
                        }
                    }
                    let has_query = !query.is_empty();

                    rsx! {
                        if filtered.is_empty() {
                            div { class: "flex flex-col items-center justify-center py-20",
                                Icon { name: "album".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                                if has_query {
                                    p { class: "text-zinc-300", "No albums match \"{raw_query}\"" }
                                } else {
                                    p { class: "text-zinc-400", "No albums found" }
                                }
                            }
                        } else {
                            div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4",
                                for album in filtered {
                                    AlbumCard {
                                        album: album.clone(),
                                        onclick: {
                                            let navigation = navigation.clone();
                                            let album_id = album.id.clone();
                                            let album_server_id = album.server_id.clone();
                                            move |_| {
                                                navigation.navigate_to(AppView::AlbumDetail(
                                                    album_id.clone(),
                                                    album_server_id.clone(),
                                                ))
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        Icon { name: "loader".to_string(), class: "w-8 h-8 text-zinc-500".to_string() }
                    }
                }
            }}
        }
    }
}
