use dioxus::prelude::*;
use crate::api::*;
use crate::components::{AppView, Icon};
use crate::components::views::home::{AlbumCard, SongRow};
use crate::components::views::search::ArtistCard;

#[component]
pub fn FavoritesView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    
    let mut active_tab = use_signal(|| "songs".to_string());
    
    let active_servers: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
    
    let favorites = use_resource(move || {
        let servers = active_servers.clone();
        async move {
            let mut artists = Vec::new();
            let mut albums = Vec::new();
            let mut songs = Vec::new();
            
            for server in servers {
                let client = NavidromeClient::new(server);
                if let Ok((a, al, s)) = client.get_starred().await {
                    artists.extend(a);
                    albums.extend(al);
                    songs.extend(s);
                }
            }
            
            (artists, albums, songs)
        }
    });
    
    let tab = active_tab();
    
    rsx! {
        div { class: "space-y-8",
            header { class: "page-header gap-4",
                h1 { class: "page-title", "Favorites" }

                // Tabs
                div { class: "flex flex-wrap gap-2",
                    button {
                        class: if tab == "songs" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| active_tab.set("songs".to_string()),
                        "Songs"
                    }
                    button {
                        class: if tab == "albums" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| active_tab.set("albums".to_string()),
                        "Albums"
                    }
                    button {
                        class: if tab == "artists" { "px-4 py-2 rounded-full bg-emerald-500/20 text-emerald-400 text-sm font-medium" } else { "px-4 py-2 rounded-full bg-zinc-800/50 text-zinc-400 hover:text-white text-sm font-medium transition-colors" },
                        onclick: move |_| active_tab.set("artists".to_string()),
                        "Artists"
                    }
                }
            }

            {
                match favorites() {
                    Some((artists, albums, songs)) => rsx! {
                        {
                            match tab.as_str() {
                                "songs" => rsx! {
                                    if songs.is_empty() {
                                        EmptyFavorites { item_type: "songs".to_string() }
                                    } else {
                                        div { class: "space-y-1",
                                            for (index , song) in songs.iter().enumerate() {
                                                SongRow {
                                                    song: song.clone(),
                                                    index: index + 1,
                                                    onclick: {
                                                        let song = song.clone();
                                                        move |_| {
                                                            now_playing.set(Some(song.clone()));
                                                            is_playing.set(true);
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                },
                                "albums" => rsx! {
                                    if albums.is_empty() {
                                        EmptyFavorites { item_type: "albums".to_string() }
                                    } else {
                                        div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4",
                                            for album in albums {
                                                AlbumCard {
                                                    album: album.clone(),
                                                    onclick: move |_| {
                                                        current_view.set(AppView::AlbumDetail(album.id.clone(), album.server_id.clone()))
                                                    },
                                                }
                                            }
                                        }
                                    }
                                },
                                "artists" => rsx! {
                                    if artists.is_empty() {
                                        EmptyFavorites { item_type: "artists".to_string() }
                                    } else {
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
                                    }
                                },
                                _ => rsx! {},
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

#[component]
fn EmptyFavorites(item_type: String) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-20",
            Icon {
                name: "heart".to_string(),
                class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
            }
            h2 { class: "text-xl font-semibold text-white mb-2", "No favorite {item_type}" }
            p { class: "text-zinc-400", "Star {item_type} in your Navidrome server to see them here" }
        }
    }
}
