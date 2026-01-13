use crate::api::*;
use crate::components::views::search::ArtistCard;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub fn ArtistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut search_query = use_signal(String::new);

    let artists = use_resource(move || {
        let servers = servers();
        async move {
            let mut artists = Vec::new();
            for server in servers.into_iter().filter(|s| s.active) {
                let client = NavidromeClient::new(server);
                if let Ok(server_artists) = client.get_artists().await {
                    artists.extend(server_artists);
                }
            }
            // Sort by name
            artists.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            artists
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
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
            }

            {
                match artists() {
                    Some(artists) => {
                        let raw_query = search_query().trim().to_string();
                        let query = raw_query.to_lowercase();
                        let mut filtered = Vec::new();
                        if query.is_empty() {
                            filtered = artists.clone();
                        } else {
                            for artist in &artists {
                                if artist.name.to_lowercase().contains(&query) {
                                    filtered.push(artist.clone());
                                }
                            }
                        }
                        let has_query = !query.is_empty();

                        rsx! {
                            if filtered.is_empty() {
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
                                    for artist in filtered {
                                        ArtistCard {
                                            artist: artist.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let artist_id = artist.id.clone();
                                                let artist_server_id = artist.server_id.clone();
                                                move |_| {
                                                    navigation.navigate_to(AppView::ArtistDetail(
                                                        artist_id.clone(),
                                                        artist_server_id.clone(),
                                                    ))
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    },
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
