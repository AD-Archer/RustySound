use crate::api::*;
use crate::components::views::search::ArtistCard;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn ArtistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut search_query = use_signal(String::new);
    let limit = use_signal(|| 30usize);

    let artists = use_resource(move || {
        let servers = servers();
        let limit = limit();
        let query = search_query();
        async move {
            let mut artists = Vec::new();
            let mut more_available = false;
            if query.trim().is_empty() {
                for server in servers.into_iter().filter(|s| s.active) {
                    let client = NavidromeClient::new(server);
                    if let Ok(server_artists) = client.get_artists().await {
                        artists.extend(server_artists);
                    }
                }
                artists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                if artists.len() > limit {
                    more_available = true;
                }
                artists.truncate(limit);
            } else {
                for server in servers.into_iter().filter(|s| s.active) {
                    let client = NavidromeClient::new(server);
                    if let Ok(results) = client.search(&query, limit as u32 + 1, 0, 0).await {
                        if results.artists.len() > limit {
                            more_available = true;
                        }
                        let mut subset = results.artists;
                        subset.truncate(limit);
                        artists.extend(subset);
                    }
                }
                artists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            (artists, more_available)
        }
    });

    rsx! {
        div { class: "space-y-8",
            header { class: "page-header page-header--split",
                div {
                    h1 { class: "page-title", "Artists" }
                    p { class: "page-subtitle", "All artists from your connected servers" }
                }
                div { class: "relative w-full md:max-w-xs",
                    Icon {
                        name: "search".to_string(),
                        class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-500".to_string(),
                    }
                    input {
                        class: "w-full pl-10 pr-4 py-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-xl text-sm text-white placeholder:text-zinc-500 focus:outline-none focus:border-emerald-500/50 focus:ring-2 focus:ring-emerald-500/20",
                        placeholder: "Search artists",
                        value: search_query,
                        oninput: move |e| {
                            let value = e.value();
                            if value.is_empty() || value.len() >= 2 {
                                search_query.set(value);
                            }
                        },
                    }
                }
            }

            {
                match artists() {
                    Some((artists, more_available)) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let has_query = !query.is_empty();
                        let display: Vec<Artist> = artists.into_iter().take(limit()).collect();

                        rsx! {
                            if display.is_empty() {
                                div { class: "flex flex-col items-center justify-center py-20",
                                    Icon {
                                        name: "artist".to_string(),
                                        class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                                    }
                                    if has_query {
                                        p { class: "text-zinc-300", "No artists match \"{raw_query}\"" }
                                    } else {
                                        p { class: "text-zinc-400", "No artists found" }
                                    }
                                }
                            } else {
                                div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6",
                                    for artist in display {
                                        ArtistCard {
                                            artist: artist.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let artist_id = artist.id.clone();
                                                let artist_server_id = artist.server_id.clone();
                                                move |_| {
                                                    navigation
                                                        .navigate_to(
                                                            AppView::ArtistDetailView {
                                                                artist_id: artist_id.clone(),
                                                                server_id: artist_server_id.clone(),
                                                            },
                                                        )
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
