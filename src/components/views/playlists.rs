use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};

#[component]
pub fn PlaylistsView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    
    let playlists = use_resource(move || {
        let servers = active_servers.clone();
        async move {
            let mut playlists = Vec::new();
            for server in servers {
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
            header { class: "mb-8",
                h1 { class: "text-3xl font-bold text-white mb-2", "Playlists" }
                p { class: "text-zinc-400", "Your playlists from all servers" }
            }
            
            {match playlists() {
                Some(playlists) if !playlists.is_empty() => rsx! {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4",
                        for playlist in playlists {
                            PlaylistCard { 
                                playlist: playlist.clone(),
                                onclick: move |_| current_view.set(AppView::PlaylistDetail(playlist.id.clone(), playlist.server_id.clone()))
                            }
                        }
                    }
                },
                Some(_) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon { name: "playlist".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                        h2 { class: "text-xl font-semibold text-white mb-2", "No playlists yet" }
                        p { class: "text-zinc-400", "Create playlists in your Navidrome server" }
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

#[component]
fn PlaylistCard(playlist: Playlist, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    
    let cover_url = servers().iter()
        .find(|s| s.id == playlist.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            playlist.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 300))
        });
    
    rsx! {
        button {
            class: "group text-left",
            onclick: move |e| onclick.call(e),
            // Playlist cover
            div { class: "aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {match cover_url {
                    Some(url) => rsx! {
                        img { class: "w-full h-full object-cover", src: "{url}" }
                    },
                    None => rsx! {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-indigo-600 to-purple-700",
                            Icon { name: "playlist".to_string(), class: "w-12 h-12 text-white/70".to_string() }
                        }
                    }
                }}
                // Play overlay
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                    div { class: "w-12 h-12 rounded-full bg-emerald-500 flex items-center justify-center shadow-xl transform scale-90 group-hover:scale-100 transition-transform",
                        Icon { name: "play".to_string(), class: "w-5 h-5 text-white ml-0.5".to_string() }
                    }
                }
            }
            // Playlist info
            p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors", "{playlist.name}" }
            p { class: "text-xs text-zinc-400", "{playlist.song_count} songs • {format_duration(playlist.duration)}" }
        }
    }
}
