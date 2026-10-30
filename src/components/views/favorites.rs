//! Favorites: starred songs, albums and artists from every active server.
//!
//! Songs are TrackRows (windowed once the list is long); albums are a cover
//! grid and artists round thumbs, both rendered a page at a time as the user
//! scrolls. Unfavoriting (from the track menu or a tile menu) removes the
//! item in place.

use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta,
};
use crate::components::ui::{
    use_long_press, Menu, Artwork, EmptyState, InsetList, MenuAction, Button,
    ButtonVariant, Segmented, Skeleton, TrackMenuController, TrackRow, VirtualList,
};
use crate::components::{AddIntent, AddMenuController, AppView, IsPlayingSignal, Navigation};
use crate::diagnostics::{log_perf, PerfTimer};
use dioxus::prelude::*;
use std::collections::HashSet;

/// Grid tiles mounted per scroll page.
const GRID_PAGE: usize = 48;
/// Above this many songs the list is windowed.
const SONGS_WINDOW_AFTER: usize = 80;
/// Height of an art TrackRow (`--row-h`).
const TRACK_ROW_PX: f64 = 58.0;

#[derive(Clone, PartialEq, Default)]
struct FavoritesData {
    songs: Vec<Song>,
    albums: Vec<Album>,
    artists: Vec<Artist>,
}

#[derive(Clone, PartialEq)]
enum TileMenu {
    Album(Album, Option<(f64, f64)>),
    Artist(Artist, Option<(f64, f64)>),
}

async fn load_favorites(servers: Vec<ServerConfig>) -> FavoritesData {
    let started = PerfTimer::now();
    let mut data = FavoritesData::default();
    let mut seen = HashSet::<String>::new();
    for server in servers {
        let client = NavidromeClient::new(server);
        if let Ok((artists, albums, songs)) = client.get_starred().await {
            data.artists.extend(artists);
            data.albums.extend(albums);
            for song in songs {
                if seen.insert(format!("{}::{}", song.server_id, song.id)) {
                    data.songs.push(song);
                }
            }
        }
    }
    log_perf(
        "favorites.load",
        started,
        &format!(
            "songs={} albums={} artists={}",
            data.songs.len(),
            data.albums.len(),
            data.artists.len()
        ),
    );
    data
}

#[component]
pub fn FavoritesView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut add_menu = use_context::<AddMenuController>();
    let track_menu = use_context::<TrackMenuController>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;

    let mut tab = use_signal(|| 0usize);
    let mut data = use_signal(|| None::<FavoritesData>);
    let mut album_limit = use_signal(|| GRID_PAGE);
    let mut artist_limit = use_signal(|| GRID_PAGE);
    let mut tile_menu = use_signal(|| None::<TileMenu>);
    let mut notice = use_signal(|| None::<String>);

    // Load (and reload when the server list changes). The API layer caches
    // getStarred2 and drops that cache on every star/unstar.
    use_effect(move || {
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        spawn(async move {
            let loaded = load_favorites(active).await;
            data.set(Some(loaded));
        });
    });

    // A song unfavorited from the track menu leaves the list at once.
    use_effect(move || {
        let Some(change) = (track_menu.last_change)() else {
            return;
        };
        if change.starred != Some(false) {
            return;
        }
        let present = data
            .peek()
            .as_ref()
            .map(|d| d.songs.iter().any(|s| change.matches(s)))
            .unwrap_or(false);
        if present {
            data.with_mut(|d| {
                if let Some(d) = d.as_mut() {
                    d.songs.retain(|s| !change.matches(s));
                }
            });
        }
    });

    let current_key = use_memo(move || {
        now_playing
            .read()
            .as_ref()
            .map(|s| (s.server_id.clone(), s.id.clone()))
    });

    let mut play_songs = move |start: usize, shuffle: bool| {
        let songs = data
            .peek()
            .as_ref()
            .map(|d| d.songs.clone())
            .unwrap_or_default();
        if songs.is_empty() {
            return;
        }
        let songs = assign_collection_queue_meta(
            songs,
            QueueSourceKind::Favorites,
            "favorites::songs".to_string(),
        );
        let start = start.min(songs.len() - 1);
        now_playing.set(songs.get(start).cloned());
        queue.set(songs);
        queue_index.set(start);
        is_playing.set(true);
        if shuffle || *shuffle_enabled.peek() {
            let _ = apply_collection_shuffle_mode(queue, queue_index, now_playing, true);
        }
    };

    let on_tile_action = move |key: String| {
        let Some(target) = tile_menu.peek().clone() else {
            return;
        };
        tile_menu.set(None);
        match target {
            TileMenu::Album(album, _) => match key.as_str() {
                "open" => navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album.id.clone(),
                    server_id: album.server_id.clone(),
                }),
                "add" => add_menu.open(AddIntent::from_album(&album)),
                "play" | "shuffle" => {
                    let server = servers.peek().iter().find(|s| s.id == album.server_id).cloned();
                    let Some(server) = server else { return };
                    let shuffle = key == "shuffle";
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        if let Ok((_, songs)) = client.get_album(&album.id).await {
                            if songs.is_empty() {
                                return;
                            }
                            let songs = assign_collection_queue_meta(
                                songs,
                                QueueSourceKind::Album,
                                album.id.clone(),
                            );
                            now_playing.set(songs.first().cloned());
                            queue.set(songs);
                            queue_index.set(0);
                            is_playing.set(true);
                            if shuffle {
                                let _ = apply_collection_shuffle_mode(
                                    queue,
                                    queue_index,
                                    now_playing,
                                    true,
                                );
                            }
                        }
                    });
                }
                "unfavorite" => {
                    let server = servers.peek().iter().find(|s| s.id == album.server_id).cloned();
                    let Some(server) = server else { return };
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        match client.unstar(&album.id, "album").await {
                            Ok(()) => data.with_mut(|d| {
                                if let Some(d) = d.as_mut() {
                                    d.albums.retain(|a| {
                                        !(a.id == album.id && a.server_id == album.server_id)
                                    });
                                }
                            }),
                            Err(err) => notice.set(Some(format!("Couldn't update favorites: {err}"))),
                        }
                    });
                }
                _ => {}
            },
            TileMenu::Artist(artist, _) => match key.as_str() {
                "open" => navigation.navigate_to(AppView::ArtistDetailView {
                    artist_id: artist.id.clone(),
                    server_id: artist.server_id.clone(),
                }),
                "unfavorite" => {
                    let server = servers.peek().iter().find(|s| s.id == artist.server_id).cloned();
                    let Some(server) = server else { return };
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        match client.unstar(&artist.id, "artist").await {
                            Ok(()) => data.with_mut(|d| {
                                if let Some(d) = d.as_mut() {
                                    d.artists.retain(|a| {
                                        !(a.id == artist.id && a.server_id == artist.server_id)
                                    });
                                }
                            }),
                            Err(err) => notice.set(Some(format!("Couldn't update favorites: {err}"))),
                        }
                    });
                }
                _ => {}
            },
        }
    };

    let has_servers = servers.read().iter().any(|s| s.active);
    let active_tab = tab();
    let snapshot = data.read();
    let counts = snapshot
        .as_ref()
        .map(|d| (d.songs.len(), d.albums.len(), d.artists.len()));
    drop(snapshot);

    rsx! {
        div { class: "rs-view", "data-testid": "favorites",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Favorites" }
            }

            if !has_servers {
                EmptyState {
                    icon: "server".to_string(),
                    title: "No servers connected".to_string(),
                    message: "Add a Navidrome server to see the songs, albums and artists you've favorited.".to_string(),
                    Button {
                        label: "Add Server".to_string(),
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    }
                }
            } else {
                div { class: "rs-personal-bar",
                    Segmented {
                        options: vec!["Songs".to_string(), "Albums".to_string(), "Artists".to_string()],
                        selected: active_tab,
                        label: "Favorites type".to_string(),
                        testid: "favorites-tabs".to_string(),
                        on_select: move |index| tab.set(index),
                    }
                    if active_tab == 0 && counts.map(|c| c.0 > 0).unwrap_or(false) {
                        div { class: "rs-personal-tools",
                            Button {
                                label: "Play".to_string(),
                                icon: "play".to_string(),
                                testid: "favorites-play".to_string(),
                                onclick: move |_| play_songs(0, false),
                            }
                            Button {
                                label: "Shuffle".to_string(),
                                icon: "shuffle".to_string(),
                                variant: ButtonVariant::Tinted,
                                onclick: move |_| play_songs(0, true),
                            }
                        }
                    }
                }

                if let Some(message) = notice() {
                    p { class: "rs-personal-error", role: "alert", "{message}" }
                }

                match counts {
                    None => rsx! {
                        Skeleton { rows: 8, round: active_tab == 2 }
                    },
                    Some((songs, albums, artists)) => match active_tab {
                        0 if songs == 0 => rsx! {
                            EmptyState {
                                icon: "heart".to_string(),
                                title: "No favorite songs".to_string(),
                                message: "Tap Favorite (the heart) in a song’s menu and it shows up here.".to_string(),
                            }
                        },
                        0 => rsx! {
                            FavoriteSongs {
                                data,
                                current_key,
                                on_play: move |index| play_songs(index, false),
                            }
                            p { class: "rs-list-foot",
                                if songs == 1 { "1 song" } else { "{songs} songs" }
                            }
                        },
                        1 if albums == 0 => rsx! {
                            EmptyState {
                                icon: "album".to_string(),
                                title: "No favorite albums".to_string(),
                                message: "Choose Favorite in an album’s … menu to keep it here.".to_string(),
                            }
                        },
                        1 => rsx! {
                            ul { class: "rs-personal-covers", aria_label: "Favorite albums",
                                {
                                    let shown = album_limit().min(albums);
                                    let items: Vec<Album> = data
                                        .read()
                                        .as_ref()
                                        .map(|d| d.albums.iter().take(shown).cloned().collect())
                                        .unwrap_or_default();
                                    rsx! {
                                        for album in items {
                                            AlbumTile {
                                                key: "{album.server_id}:{album.id}",
                                                album: album.clone(),
                                                on_open: move |album: Album| navigation.navigate_to(AppView::AlbumDetailView {
                                                    album_id: album.id.clone(),
                                                    server_id: album.server_id.clone(),
                                                }),
                                                on_menu: move |(album, anchor)| tile_menu.set(Some(TileMenu::Album(album, anchor))),
                                            }
                                        }
                                    }
                                }
                            }
                            if album_limit() < albums {
                                MoreSentinel {
                                    key: "{album_limit()}",
                                    on_visible: move |_| album_limit.with_mut(|n| *n += GRID_PAGE),
                                }
                            }
                        },
                        _ if artists == 0 => rsx! {
                            EmptyState {
                                icon: "artist".to_string(),
                                title: "No favorite artists".to_string(),
                                message: "Choose Favorite in an artist’s … menu to keep them here.".to_string(),
                            }
                        },
                        _ => rsx! {
                            ul { class: "rs-personal-covers rs-personal-covers--round", aria_label: "Favorite artists",
                                {
                                    let shown = artist_limit().min(artists);
                                    let items: Vec<Artist> = data
                                        .read()
                                        .as_ref()
                                        .map(|d| d.artists.iter().take(shown).cloned().collect())
                                        .unwrap_or_default();
                                    rsx! {
                                        for artist in items {
                                            ArtistTile {
                                                key: "{artist.server_id}:{artist.id}",
                                                artist: artist.clone(),
                                                on_open: move |artist: Artist| navigation.navigate_to(AppView::ArtistDetailView {
                                                    artist_id: artist.id.clone(),
                                                    server_id: artist.server_id.clone(),
                                                }),
                                                on_menu: move |(artist, anchor)| tile_menu.set(Some(TileMenu::Artist(artist, anchor))),
                                            }
                                        }
                                    }
                                }
                            }
                            if artist_limit() < artists {
                                MoreSentinel {
                                    key: "{artist_limit()}",
                                    on_visible: move |_| artist_limit.with_mut(|n| *n += GRID_PAGE),
                                }
                            }
                        },
                    },
                }
            }

            if let Some(menu) = tile_menu() {
                {
                    let (title, anchor, actions) = match &menu {
                        TileMenu::Album(album, anchor) => (
                            album.name.clone(),
                            *anchor,
                            vec![
                                MenuAction::new("play", "Play", "play"),
                                MenuAction::new("shuffle", "Shuffle", "shuffle"),
                                MenuAction::new("add", "Add to Playlist…", "playlist"),
                                MenuAction::new("open", "Go to Album", "disc"),
                                MenuAction::new("unfavorite", "Remove from Favorites", "heart-filled"),
                            ],
                        ),
                        TileMenu::Artist(artist, anchor) => (
                            artist.name.clone(),
                            *anchor,
                            vec![
                                MenuAction::new("open", "Go to Artist", "artist"),
                                MenuAction::new("unfavorite", "Remove from Favorites", "heart-filled"),
                            ],
                        ),
                    };
                    rsx! {
                        Menu {
                            title,
                            actions,
                            anchor,
                            testid: "favorites-menu".to_string(),
                            on_close: move |_| tile_menu.set(None),
                            on_action: on_tile_action,
                        }
                    }
                }
            }
        }
    }
}

/// The songs list: windowed past `SONGS_WINDOW_AFTER` rows. Rows look songs
/// up by index in the shared signal (no per-row list clones).
#[component]
fn FavoriteSongs(
    data: Signal<Option<FavoritesData>>,
    current_key: Memo<Option<(String, String)>>,
    on_play: EventHandler<usize>,
) -> Element {
    let is_playing = use_context::<IsPlayingSignal>().0;
    let count = data.read().as_ref().map(|d| d.songs.len()).unwrap_or(0);
    let render_row = use_callback(move |index: usize| {
        let song = data
            .read()
            .as_ref()
            .and_then(|d| d.songs.get(index).cloned());
        let Some(song) = song else {
            return rsx! {};
        };
        let playing = current_key
            .read()
            .as_ref()
            .map(|(server, id)| *server == song.server_id && *id == song.id)
            .unwrap_or(false);
        rsx! {
            TrackRow {
                key: "{song.server_id}:{song.id}",
                song: song.clone(),
                index,
                playing,
                live: playing && is_playing(),
                on_play: move |_| on_play.call(index),
            }
        }
    });

    if count > SONGS_WINDOW_AFTER {
        return rsx! {
            VirtualList {
                item_count: count,
                row_height: TRACK_ROW_PX,
                render_row,
                class: "rs-list".to_string(),
                testid: "favorites-songs".to_string(),
            }
        };
    }
    rsx! {
        InsetList { label: "Favorite songs".to_string(), testid: "favorites-songs".to_string(),
            for index in 0..count {
                {render_row.call(index)}
            }
        }
    }
}

/// An empty block that asks for the next page when it scrolls into view.
/// Re-keyed after every page so the observer fires again if it is still
/// visible after the grid grew.
#[component]
pub(super) fn MoreSentinel(on_visible: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "rs-personal-more",
            aria_hidden: "true",
            onvisible: move |evt| {
                if evt.data().is_intersecting().unwrap_or(false) {
                    on_visible.call(());
                }
            },
            span { class: "rs-spinner rs-spinner--page" }
        }
    }
}

#[component]
fn AlbumTile(
    album: Album,
    on_open: EventHandler<Album>,
    on_menu: EventHandler<(Album, Option<(f64, f64)>)>,
) -> Element {
    let press = use_long_press();
    let subtitle = album.artist.clone();
    rsx! {
        li {
            button {
                r#type: "button",
                class: "rs-personal-tile",
                aria_label: "{album.name}, {subtitle}",
                onclick: {
                    let album = album.clone();
                    move |_| {
                        if press.take_fired() {
                            return;
                        }
                        on_open.call(album.clone());
                    }
                },
                oncontextmenu: {
                    let album = album.clone();
                    move |evt: MouseEvent| {
                        evt.prevent_default();
                        press.cancel();
                        let p = evt.client_coordinates();
                        on_menu.call((album.clone(), Some((p.x, p.y))));
                    }
                },
                onkeydown: {
                    let album = album.clone();
                    move |evt: KeyboardEvent| {
                        if evt.key() == Key::ContextMenu || (evt.key() == Key::F10 && evt.modifiers().shift()) {
                            evt.prevent_default();
                            on_menu.call((album.clone(), None));
                        }
                    }
                },
                onpointerdown: {
                    let album = album.clone();
                    move |evt: PointerEvent| {
                        let album = album.clone();
                        press.down(&evt, move |point| on_menu.call((album, Some(point))));
                    }
                },
                onpointermove: move |evt| press.moved(&evt),
                onpointerup: move |_| press.cancel(),
                onpointercancel: move |_| press.cancel(),
                span { class: "rs-personal-tile__art",
                    Artwork {
                        cover_art: album.cover_art.clone(),
                        server_id: album.server_id.clone(),
                        size_px: 180,
                        fill: true,
                        placeholder: "album".to_string(),
                    }
                }
                span { class: "rs-personal-tile__title", "{album.name}" }
                if !subtitle.is_empty() {
                    span { class: "rs-personal-tile__sub", "{subtitle}" }
                }
            }
        }
    }
}

#[component]
fn ArtistTile(
    artist: Artist,
    on_open: EventHandler<Artist>,
    on_menu: EventHandler<(Artist, Option<(f64, f64)>)>,
) -> Element {
    let press = use_long_press();
    let albums = match artist.album_count {
        0 => String::new(),
        1 => "1 album".to_string(),
        n => format!("{n} albums"),
    };
    rsx! {
        li {
            button {
                r#type: "button",
                class: "rs-personal-tile",
                aria_label: "{artist.name}",
                onclick: {
                    let artist = artist.clone();
                    move |_| {
                        if press.take_fired() {
                            return;
                        }
                        on_open.call(artist.clone());
                    }
                },
                oncontextmenu: {
                    let artist = artist.clone();
                    move |evt: MouseEvent| {
                        evt.prevent_default();
                        press.cancel();
                        let p = evt.client_coordinates();
                        on_menu.call((artist.clone(), Some((p.x, p.y))));
                    }
                },
                onkeydown: {
                    let artist = artist.clone();
                    move |evt: KeyboardEvent| {
                        if evt.key() == Key::ContextMenu || (evt.key() == Key::F10 && evt.modifiers().shift()) {
                            evt.prevent_default();
                            on_menu.call((artist.clone(), None));
                        }
                    }
                },
                onpointerdown: {
                    let artist = artist.clone();
                    move |evt: PointerEvent| {
                        let artist = artist.clone();
                        press.down(&evt, move |point| on_menu.call((artist, Some(point))));
                    }
                },
                onpointermove: move |evt| press.moved(&evt),
                onpointerup: move |_| press.cancel(),
                onpointercancel: move |_| press.cancel(),
                span { class: "rs-personal-tile__art",
                    Artwork {
                        cover_art: artist.cover_art.clone(),
                        server_id: artist.server_id.clone(),
                        size_px: 136,
                        fill: true,
                        round: true,
                        placeholder: "artist".to_string(),
                    }
                }
                span { class: "rs-personal-tile__title", "{artist.name}" }
                if !albums.is_empty() {
                    span { class: "rs-personal-tile__sub rs-personal-tnum", "{albums}" }
                }
            }
        }
    }
}
