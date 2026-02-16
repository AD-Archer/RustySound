use crate::api::*;
use crate::components::views::home::SongRow;
use crate::components::{AddIntent, AddMenuController, AppView, Icon, Navigation};
use dioxus::prelude::*;

fn render_album_item(
    album: Album,
    servers: Signal<Vec<ServerConfig>>,
    navigation: Navigation,
    add_menu: AddMenuController,
) -> Element {
    let album_id = album.id.clone();
    let album_server_id = album.server_id.clone();
    let album_id_for_nav = album_id.clone();
    let album_server_id_for_nav = album_server_id.clone();
    let album_clone_for_add = album.clone();
    let album_cover = servers()
        .iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    let album_cover_element = match &album_cover {
        Some(url) => rsx! {
            img {
                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-300",
                src: "{url}",
            }
        },
        None => rsx! {
            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                Icon {
                    name: "album".to_string(),
                    class: "w-12 h-12 text-zinc-500".to_string(),
                }
            }
        },
    };

    rsx! {
        div {
            key: "{album_id}",
            class: "group text-left cursor-pointer",
            onclick: move |_| {
                let navigation = navigation.clone();
                navigation
                    .navigate_to(AppView::AlbumDetailView {
                        album_id: album_id_for_nav.clone(),
                        server_id: album_server_id_for_nav.clone(),
                    });
            },
            div { class: "aspect-square rounded-xl bg-zinc-800 overflow-hidden mb-3 shadow-lg group-hover:shadow-emerald-500/20 transition-shadow relative",
                {album_cover_element}
                button {
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/70 text-zinc-200 hover:text-white hover:bg-emerald-500 transition-colors opacity-100 md:opacity-0 md:group-hover:opacity-100",
                    aria_label: "Add album",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let mut add_menu = add_menu.clone();
                        add_menu.open(AddIntent::from_album(&album_clone_for_add));
                    },
                    Icon {
                        name: "plus".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center",
                    div { class: "w-12 h-12 rounded-full bg-emerald-500 flex items-center justify-center shadow-lg transform translate-y-2 group-hover:translate-y-0 transition-transform",
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 text-white ml-0.5".to_string(),
                        }
                    }
                }
            }
            h3 { class: "font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                "{album.name}"
            }
            div { class: "flex items-center gap-2 text-sm text-zinc-400",
                if let Some(year) = album.year {
                    span { "{year}" }
                    span { "•" }
                }
                span { "{album.song_count} songs" }
            }
        }
    }
}

#[component]
pub fn ArtistDetailView(artist_id: String, server_id: String) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();

    let artist_server = servers().into_iter().find(|s| s.id == server_id);
    let artist_server_for_artist = artist_server.clone();
    let artist_server_for_top = artist_server.clone();

    let artist_data = use_resource(move || {
        let server = artist_server_for_artist.clone();
        let artist_id = artist_id.clone();
        async move {
            if let Some(server) = server {
                let client = NavidromeClient::new(server);
                client.get_artist(&artist_id).await.ok()
            } else {
                None
            }
        }
    });

    let top_songs_data = use_resource({
        let server = artist_server_for_top.clone();
        let artist_data = artist_data.clone();
        move || {
            let server = server.clone();
            let artist_name = artist_data()
                .and_then(|value| value.map(|(artist, _)| artist.name.clone()))
                .filter(|name| !name.is_empty());
            async move {
                match (server, artist_name) {
                    (Some(server), Some(artist_name)) => {
                        let client = NavidromeClient::new(server);
                        client.get_top_songs(&artist_name, 20).await.ok()
                    }
                    _ => None,
                }
            }
        }
    });

    let mut is_favorited = use_signal(|| false);

    let _on_add_album = {
        let artist_data_ref = artist_data.clone();
        let servers = servers.clone();
        let mut queue = queue.clone();
        move |_: MouseEvent| {
            if let Some(Some((_, albums))) = artist_data_ref() {
                spawn(async move {
                    let mut all_songs = Vec::new();
                    for album in albums {
                        if let Some(server) =
                            servers().iter().find(|s| s.id == album.server_id).cloned()
                        {
                            let client = NavidromeClient::new(server);
                            if let Ok((_, songs)) = client.get_album(&album.id).await {
                                all_songs.extend(songs);
                            }
                        }
                    }
                    queue.with_mut(|q| q.extend(all_songs));
                });
            }
        }
    };

    use_effect(move || {
        if let Some(Some((artist, _))) = artist_data() {
            is_favorited.set(artist.starred.is_some());
        }
    });

    let on_favorite_toggle = move |_| {
        if let Some(Some((artist, _))) = artist_data() {
            let server_list = servers();
            if let Some(server) = server_list
                .iter()
                .find(|s| s.id == artist.server_id)
                .cloned()
            {
                let artist_id = artist.id.clone();
                let should_star = !is_favorited();
                let mut is_favorited = is_favorited;
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = if should_star {
                        client.star(&artist_id, "artist").await
                    } else {
                        client.unstar(&artist_id, "artist").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                });
            }
        }
    };

    rsx! {
        button {
            class: "flex items-center gap-2 text-zinc-400 hover:text-white transition-colors mb-4",
            onclick: move |_| {
                if navigation.go_back().is_none() {
                    navigation.navigate_to(AppView::ArtistsView {});
                }
            },
            Icon { name: "prev".to_string(), class: "w-4 h-4".to_string() }
            "Back to Artists"
        }

        {
            match artist_data() {
                Some(Some((artist, albums))) => {
                    let top_songs = top_songs_data().flatten().unwrap_or_default();
                    let cover_url = servers()
                        .iter()
                        .find(|s| s.id == artist.server_id)
                        .and_then(|server| {
                            let client = NavidromeClient::new(server.clone());
                            artist
                                .cover_art
                                .as_ref()
                                .map(|ca| client.get_cover_art_url(ca, 500))
                        });

                    let total_albums = albums.len();
                    let total_songs: u32 = albums.iter().map(|a| a.song_count).sum();
                    let cover_element = match cover_url {
                        Some(url) => rsx! {
                            img {
                                src: "{url}",
                                alt: "{artist.name}",
                                class: "w-full h-full object-cover",
                                loading: "lazy",
                            }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-emerald-600 to-teal-700",
                                Icon {
                                    name: "artist".to_string(),
                                    class: "w-24 h-24 text-white/70".to_string(),
                                }
                            }
                        },
                    };

                    rsx! {
                        div { class: "flex flex-col md:flex-row gap-8 mb-12",
                            div { class: "w-48 h-48 md:w-64 md:h-64 rounded-full bg-zinc-800 overflow-hidden shadow-2xl flex-shrink-0 mx-auto md:mx-0",
                                {cover_element}
                            }
                            div { class: "flex flex-col justify-end text-center md:text-left",
                                p { class: "text-sm text-zinc-400 uppercase tracking-wide mb-2 font-medium",
                                    "Artist"
                                }
                                h1 { class: "text-5xl md:text-6xl font-bold text-white mb-4", "{artist.name}" }
                                div { class: "flex items-center gap-4 text-sm text-zinc-400 justify-center md:justify-start",
                                    span { class: "flex items-center gap-1",
                                        Icon { name: "album".to_string(), class: "w-4 h-4".to_string() }
                                        "{total_albums} albums"
                                    }
                                    span { "•" }
                                    span { class: "flex items-center gap-1",
                                        Icon { name: "music".to_string(), class: "w-4 h-4".to_string() }
                                        "{total_songs} songs"
                                    }
                                }
                                div { class: "flex gap-3 mt-6 justify-center md:justify-start",
                                    button {
                                        class: if is_favorited() { "p-3 rounded-full border border-zinc-700 text-emerald-400 hover:text-emerald-300 hover:border-emerald-500/50 transition-colors" } else { "p-3 rounded-full border border-zinc-700 text-zinc-400 hover:text-emerald-400 hover:border-emerald-500/50 transition-colors" },
                                        onclick: on_favorite_toggle,
                                        Icon {
                                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                            class: "w-5 h-5".to_string(),
                                        }
                                    }
                                }
                            }
                        }
                        section { class: "space-y-6",
                            h2 { class: "text-2xl font-bold text-white", "Albums" }
                            div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-6",
                                {
                                    albums
                                        .iter()
                                        .map(|album| {
                                            render_album_item(
                                                album.clone(),
                                                servers.clone(),
                                                navigation.clone(),
                                                add_menu.clone(),
                                            )
                                        })
                                }
                            }
                        }
                        if !top_songs.is_empty() {
                            section { class: "space-y-4 mt-10",
                                h2 { class: "text-2xl font-bold text-white", "Popular Songs" }
                                div { class: "rounded-2xl border border-zinc-800/80 bg-zinc-900/30 p-2",
                                    for (index, song) in top_songs.iter().enumerate() {
                                        {
                                            let song_for_queue = song.clone();
                                            let songs_for_queue = top_songs.clone();
                                            rsx! {
                                                SongRow {
                                                    song: song.clone(),
                                                    index: index + 1,
                                                    onclick: move |_| {
                                                        queue.set(songs_for_queue.clone());
                                                        queue_index.set(index);
                                                        now_playing.set(Some(song_for_queue.clone()));
                                                        is_playing.set(true);
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some(None) => rsx! {
                    div { class: "flex flex-col items-center justify-center py-20",
                        Icon {
                            name: "artist".to_string(),
                            class: "w-16 h-16 text-zinc-600 mb-4".to_string(),
                        }
                        p { class: "text-zinc-400", "Artist not found" }
                    }
                },
                None => rsx! {
                    div { class: "flex items-center justify-center py-20",
                        div { class: "animate-pulse flex flex-col items-center",
                            div { class: "w-48 h-48 rounded-full bg-zinc-800 mb-6" }
                            div { class: "h-8 w-48 bg-zinc-800 rounded mb-4" }
                            div { class: "h-4 w-32 bg-zinc-800 rounded" }
                        }
                    }
                },
            }
        }
    }
}
