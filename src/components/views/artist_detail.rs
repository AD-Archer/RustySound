//! Artist page: stage band with the round portrait, Play / Shuffle, top
//! songs, albums (and singles & EPs) as cover grids, similar artists.

use crate::api::*;
use crate::components::ui::{
    Artwork, CollectionActions, EmptyState, ErrorState, GridSkeleton, InsetList, Menu, MenuItem,
    RowGestureList, RowSwipe, SectionHeader, Skeleton, TrackRow,
};
use crate::components::views::library::hero::{DetailHero, DetailHeroSkeleton};
use crate::components::views::library::shared::{
    toggle_star, use_downloaded_keys, use_now_playing, use_song_patches,
};
use crate::components::views::library::tiles::{AlbumGrid, ArtistBubble};
use crate::components::{use_player, use_servers, AppView, QueuePosition};
use dioxus::prelude::*;
use futures_util::future::join_all;

const TOP_SHOWN: usize = 5;
const TOP_MAX: u32 = 20;

/// Singles and EPs by track count and length (iTunes' rule of thumb): up
/// to 3 tracks is a single, up to 6 tracks under 30 minutes is an EP.
fn is_single_or_ep(album: &Album) -> bool {
    let tracks = album.song_count;
    tracks > 0 && (tracks <= 3 || (tracks <= 6 && album.duration > 0 && album.duration < 30 * 60))
}

/// Every song of `albums` on source `server_id`, in album order (for Play / Shuffle).
async fn all_songs(servers: Vec<ServerConfig>, server_id: String, albums: Vec<Album>) -> Vec<Song> {
    let tasks = albums.iter().map(|album| {
        let client = MusicClient::for_source(&servers, &server_id);
        let id = album.id.clone();
        async move {
            let Some(client) = client else { return Vec::new() };
            client.get_album(&id).await.map(|(_, songs)| songs).unwrap_or_default()
        }
    });
    let mut seen = std::collections::HashSet::new();
    join_all(tasks)
        .await
        .into_iter()
        .flatten()
        .filter(|song| seen.insert(song.key()))
        .collect()
}

#[component]
pub fn ArtistDetailView(artist_id: String, server_id: String) -> Element {
    let servers = use_servers();
    let player = use_player();
    let mut retry = use_signal(|| 0u32);
    let mut status = use_signal(|| None::<String>);
    let mut starred = use_signal(|| false);
    let mut show_all_top = use_signal(|| false);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut busy = use_signal(|| false);
    let mut top_songs = use_signal(Vec::<Song>::new);
    use_song_patches(top_songs);
    let downloaded = use_downloaded_keys(top_songs);
    let (playing_key, live) = use_now_playing();

    let client_for = move |id: &str| MusicClient::for_source(&servers.peek(), id);

    let data = use_resource(use_reactive!(|(artist_id, server_id)| {
        let _ = retry();
        let client = MusicClient::for_source(&servers(), &server_id);
        async move {
            let Some(client) = client else {
                return Err("This artist's server isn't configured.".to_string());
            };
            client.get_artist(&artist_id).await
        }
    }));

    // Top songs and similar artists load after the artist, independently.
    let top = use_resource(move || {
        let found = match &*data.read() {
            Some(Ok((artist, _))) => Some((artist.name.clone(), artist.server_id.clone())),
            _ => None,
        };
        async move {
            let (name, sid) = found?;
            client_for(&sid)?.get_top_songs(&name, TOP_MAX).await.ok()
        }
    });
    let similar = use_resource(move || {
        let found = match &*data.read() {
            Some(Ok((artist, _))) => Some((artist.id.clone(), artist.server_id.clone())),
            _ => None,
        };
        async move {
            let (id, sid) = found?;
            // Similar artists come from the server (Last.fm behind it).
            let MusicClient::Subsonic(client) = client_for(&sid)? else {
                return None;
            };
            let info = client.get_artist_info2(&id, 16, false).await.ok()?;
            Some(info.similar.into_iter().filter(|a| a.in_library()).take(12).collect::<Vec<_>>())
        }
    });

    use_effect(move || {
        if let Some(Ok((artist, _))) = &*data.read() {
            starred.set(artist.starred.is_some());
        }
    });
    use_effect(move || {
        if let Some(Some(list)) = &*top.read() {
            top_songs.set(list.clone());
        }
    });

    let (artist, albums) = match &*data.read() {
        None => {
            return rsx! {
                div { class: "rs-view rs-view--flush rs-library-detail",
                    DetailHeroSkeleton { back_label: "Artists".to_string(), fallback: AppView::ArtistsView {}, round: true }
                    div { class: "rs-library-page", GridSkeleton { count: 6 } }
                }
            };
        }
        Some(Err(err)) => {
            let err = err.clone();
            return rsx! {
                div { class: "rs-view rs-library-detail",
                    ErrorState {
                        title: "Couldn't open this artist".to_string(),
                        error: err,
                        on_retry: move |_| retry += 1,
                    }
                }
            };
        }
        Some(Ok(pair)) => pair.clone(),
    };

    let mut albums_sorted = albums.clone();
    albums_sorted.sort_by(|a, b| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)).then(a.name.cmp(&b.name)));
    let (singles, full): (Vec<Album>, Vec<Album>) = albums_sorted.into_iter().partition(is_single_or_ep);
    let total_songs: u32 = albums.iter().map(|a| a.song_count).sum();
    let meta = format!(
        "{} · {}",
        if albums.len() == 1 { "1 album".to_string() } else { format!("{} albums", albums.len()) },
        if total_songs == 1 { "1 song".to_string() } else { format!("{total_songs} songs") }
    );
    let source = Some((QueueSourceKind::Artist, artist.key()));

    let start = {
        let albums = albums.clone();
        let sid = artist.server_id.clone();
        move |shuffle: bool| {
            if busy() {
                return;
            }
            if client_for(&sid).is_none() {
                return;
            }
            let (servers_now, sid) = (servers.peek().clone(), sid.clone());
            let albums = albums.clone();
            let source = source.clone();
            busy.set(true);
            status.set(None);
            spawn(async move {
                let songs = all_songs(servers_now, sid, albums).await;
                let result = if shuffle {
                    player.shuffle_play(songs, source)
                } else {
                    player.play(songs, 0, source)
                };
                status.set(result.err());
                busy.set(false);
            });
        }
    };

    let toggle_favorite = {
        let artist = artist.clone();
        move |_| {
            menu_at.set(None);
            toggle_star(&servers.peek(), &artist.server_id, "artist", artist.id.clone(), starred, status);
        }
    };

    let top_list = top_songs.read().clone();
    let top_count = top_list.len();
    let shown = if show_all_top() { top_count } else { top_count.min(TOP_SHOWN) };
    let playing_now = playing_key();
    let live_now = live();
    let similar_list = similar.read().clone().flatten().unwrap_or_default();

    rsx! {
        div { class: "rs-view rs-view--flush rs-library-detail", "data-testid": "artist-detail",
            DetailHero {
                title: artist.name.clone(),
                round: true,
                back_label: "Artists".to_string(),
                fallback: AppView::ArtistsView {},
                art: rsx! {
                    Artwork {
                        cover_art: artist.cover_art.clone(),
                        server_id: artist.server_id.clone(),
                        size_px: 220,
                        fill: true,
                        round: true,
                        eager: true,
                        alt: artist.name.clone(),
                        placeholder: "artist".to_string(),
                    }
                },
                meta,
                badge: crate::local_music::is_local(&artist.server_id)
                    .then(|| crate::local_music::LOCAL_SOURCE_NAME.to_string()),
                badge_icon: "hard-drive".to_string(),
                on_more: move |evt: MouseEvent| {
                    let p = evt.client_coordinates();
                    menu_at.set(Some(Some((p.x - 250.0, p.y + 14.0))));
                },
            }
            div { class: "rs-library-page",
                CollectionActions {
                    testid: "artist".to_string(),
                    disabled: busy() || albums.is_empty(),
                    on_play: {
                        let mut start = start.clone();
                        move |_| start(false)
                    },
                    on_shuffle: {
                        let mut start = start.clone();
                        move |_| start(true)
                    },
                }
                if busy() {
                    p { class: "rs-library-status", role: "status", "Loading songs…" }
                }
                if let Some(text) = status() {
                    p { class: "rs-library-status", role: "status", "{text}" }
                }

                match &*top.read() {
                    None => rsx! {
                        SectionHeader { title: "Top Songs".to_string() }
                        Skeleton { rows: 3 }
                    },
                    Some(_) if top_count == 0 => rsx! {},
                    Some(_) => rsx! {
                        SectionHeader {
                            title: "Top Songs".to_string(),
                            action: if top_count > TOP_SHOWN { Some(if show_all_top() { "Show Less".to_string() } else { "Show All".to_string() }) } else { None },
                            testid: "artist-top-all".to_string(),
                            on_action: move |_| show_all_top.toggle(),
                        }
                        // Swipe a song for Play Next / Add to….
                        RowGestureList { id: "artist-top-songs", swipe: RowSwipe::Browse,
                        InsetList { label: "Top songs".to_string(), class: "rs-library-tracks".to_string(),
                            for (index, song) in top_list.into_iter().take(shown).enumerate() {
                                {
                                    let key = song.key();
                                    let playing = playing_now.as_deref() == Some(key.as_str());
                                    let sub = song.album.clone().unwrap_or_default();
                                    rsx! {
                                        TrackRow {
                                            key: "{key}",
                                            subtitle: sub,
                                            playing,
                                            live: playing && live_now,
                                            downloaded: downloaded.read().contains(&key),
                                            index,
                                            song,
                                            on_play: move |_| {
                                                let list = top_songs.peek().clone();
                                                status.set(player.play(list, index, None).err());
                                            },
                                        }
                                    }
                                }
                            }
                        }
                        }
                    },
                }

                if albums.is_empty() {
                    EmptyState {
                        icon: "album".to_string(),
                        title: "No albums".to_string(),
                        message: "This artist has no albums on the server.".to_string(),
                    }
                }
                if !full.is_empty() {
                    SectionHeader { title: "Albums".to_string() }
                    AlbumGrid { albums: full, show_artist: false, label: "Albums".to_string() }
                }
                if !singles.is_empty() {
                    SectionHeader { title: "Singles & EPs".to_string() }
                    AlbumGrid { albums: singles, show_artist: false, label: "Singles and EPs".to_string() }
                }
                if !similar_list.is_empty() {
                    SectionHeader { title: "Similar Artists".to_string() }
                    div { class: "rs-library-bubbles", role: "list", aria_label: "Similar artists",
                        for info in similar_list {
                            div { key: "{info.server_id}:{info.name}", role: "listitem",
                                ArtistBubble {
                                    artist_id: info.id.clone().unwrap_or_default(),
                                    server_id: info.server_id.clone(),
                                    name: info.name.clone(),
                                    cover_art: info.cover_art.clone(),
                                }
                            }
                        }
                    }
                }
            }

            if let Some(anchor) = menu_at() {
                Menu {
                    label: format!("{} actions", artist.name),
                    anchor,
                    on_close: move |_| menu_at.set(None),
                    MenuItem {
                        label: if starred() { "Remove from Favorites".to_string() } else { "Favorite".to_string() },
                        icon: if starred() { "heart-filled".to_string() } else { "heart".to_string() },
                        testid: "artist-favorite".to_string(),
                        onclick: toggle_favorite,
                    }
                    MenuItem {
                        label: "Play Top Songs".to_string(),
                        icon: "play".to_string(),
                        disabled: top_count == 0,
                        onclick: move |_| {
                            menu_at.set(None);
                            let list = top_songs.peek().clone();
                            status.set(player.play(list, 0, None).err());
                        },
                    }
                    MenuItem {
                        label: "Add Top Songs to Queue".to_string(),
                        icon: "add-to-queue".to_string(),
                        disabled: top_count == 0,
                        onclick: move |_| {
                            menu_at.set(None);
                            player.enqueue(top_songs.peek().clone(), QueuePosition::End);
                        },
                    }
                }
            }
        }
    }
}
