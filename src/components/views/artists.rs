use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};
use crate::components::views::search::ArtistCard;

#[component]
pub fn ArtistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    
    let artists = use_resource(move || {
        let servers = active_servers.clone();
        async move {
            let mut artists = Vec::new();
            for server in servers {
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
            header { class: "mb-8",
                h1 { class: "text-3xl font-bold text-white mb-2", "Artists" }
                p { class: "text-zinc-400", "All artists from your connected servers" }
            }

            {
                match artists() {
                    Some(artists) => rsx! {
                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6",
                            for artist in artists {
                                ArtistCard {
                                    artist: artist.clone(),
                                    onclick: move |_| {
                                        current_view
                                            .set(AppView::ArtistDetail(artist.id.clone(), artist.server_id.clone()))
                                    },
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
