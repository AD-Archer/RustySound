use dioxus::prelude::*;
use crate::api::*;
use crate::components::Icon;

#[component]
pub fn RadioView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    
    let stations = use_resource(move || {
        let servers = active_servers.clone();
        async move {
            let mut stations = Vec::new();
            for server in servers {
                let client = NavidromeClient::new(server);
                if let Ok(server_stations) = client.get_internet_radio_stations().await {
                    stations.extend(server_stations);
                }
            }
            stations
        }
    });
    
    rsx! {
        div { class: "space-y-8",
            header { class: "page-header",
                h1 { class: "page-title", "Radio Stations" }
                p { class: "page-subtitle", "Internet radio from your servers" }
            }
            
            {match stations() {
                Some(stations) if !stations.is_empty() => rsx! {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                        for station in stations {
                            RadioStationCard { 
                                station: station.clone(),
                                onclick: {
                                    let station = station.clone();
                                    move |_| {
                                        // Create a fake song for radio playback
                                        let radio_song = Song {
                                            id: station.id.clone(),
                                            title: station.name.clone(),
                                            artist: Some("Internet Radio".to_string()),
                                            album: None,
                                            album_id: None,
                                            artist_id: None,
                                            duration: 0,
                                            track: None,
                                            cover_art: None,
                                            content_type: Some("audio/mpeg".to_string()),
                                            suffix: None,
                                            bitrate: None,
                                            starred: None,
                                            year: None,
                                            genre: None,
                                            server_id: station.server_id.clone(),
                                            server_name: "Radio".to_string(),
                                        };
                                        now_playing.set(Some(radio_song));
                                        is_playing.set(true);
                                    }
                                }
                            }
                        }
                    }
                },
                Some(_) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon { name: "radio".to_string(), class: "w-16 h-16 text-zinc-600 mb-4".to_string() }
                        h2 { class: "text-xl font-semibold text-white mb-2", "No radio stations" }
                        p { class: "text-zinc-400", "Add radio stations in your Navidrome server" }
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
fn RadioStationCard(station: RadioStation, onclick: EventHandler<MouseEvent>) -> Element {
    let initials: String = station.name.chars()
        .filter(|c| c.is_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    
    rsx! {
        button {
            class: "flex items-center gap-4 p-4 rounded-xl bg-zinc-800/30 border border-zinc-700/30 hover:bg-zinc-800/50 hover:border-emerald-500/30 transition-all group",
            onclick: move |e| onclick.call(e),
            // Station icon
            div { class: "w-14 h-14 rounded-xl bg-gradient-to-br from-amber-500 to-orange-600 flex items-center justify-center flex-shrink-0 shadow-lg",
                span { class: "text-white font-bold text-lg", "{initials}" }
            }
            // Station info
            div { class: "flex-1 min-w-0 text-left",
                p { class: "font-medium text-white truncate group-hover:text-emerald-400 transition-colors", "{station.name}" }
                p { class: "text-xs text-zinc-400 truncate", "{station.stream_url}" }
            }
            // Play icon
            div { class: "w-10 h-10 rounded-full bg-zinc-700/50 group-hover:bg-emerald-500 flex items-center justify-center transition-colors",
                Icon { name: "play".to_string(), class: "w-4 h-4 text-zinc-400 group-hover:text-white ml-0.5".to_string() }
            }
        }
    }
}
