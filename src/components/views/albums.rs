use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};
use crate::components::views::home::AlbumCard;

#[component]
pub fn AlbumsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    
    let mut album_type = use_signal(|| "alphabeticalByName".to_string());
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    let selected_type = album_type();
    
    let albums = use_resource(move || {
        let servers = active_servers.clone();
        let album_type = selected_type.clone();
        async move {
            let mut albums = Vec::new();
            for server in servers {
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
            header { class: "mb-8",
                h1 { class: "text-3xl font-bold text-white mb-4", "Albums" }
                
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
            }
            
            {match albums() {
                Some(albums) => rsx! {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4",
                        for album in albums {
                            AlbumCard { 
                                album: album.clone(),
                                onclick: move |_| current_view.set(AppView::AlbumDetail(album.id.clone(), album.server_id.clone()))
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
