//! Bookmarks: saved positions in songs (written by the audio controllers).
//! Tap resumes at the saved position; the row menu opens the album or
//! deletes the bookmark; the page menu refreshes or clears everything.

use crate::api::*;
use crate::components::ui::{
    format_clock, use_long_press, Menu, Artwork, EmptyState, IconButton, IconButtonSize,
    InsetList, MenuAction, Button, Skeleton,
};
use crate::components::views::artist_links::{parse_artist_names, resolve_artist_id_for_name};
use crate::components::{AppView, IsPlayingSignal, Navigation, PlaybackPositionSignal, SeekRequestSignal};
use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
struct RowMenu {
    bookmark: Bookmark,
    anchor: Option<(f64, f64)>,
}

#[component]
pub fn BookmarksView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let player = use_player();
    let mut refresh_key = use_signal(|| 0u32);
    let mut list = use_signal(|| None::<Vec<Bookmark>>);
    let mut header_menu = use_signal(|| None::<(f64, f64)>);
    let mut row_menu = use_signal(|| None::<RowMenu>);
    let mut confirm_clear = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_effect(move || {
        let _ = refresh_key();
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        spawn(async move {
            let mut items = Vec::new();
            for server in active {
                let client = NavidromeClient::new(server);
                if let Ok(mut found) = client.get_bookmarks().await {
                    items.append(&mut found);
                }
            }
            items.sort_by(|a, b| {
                b.changed
                    .as_deref()
                    .unwrap_or("")
                    .cmp(a.changed.as_deref().unwrap_or(""))
            });
            list.set(Some(items));
        });
    });

    let delete_one = move |bookmark: Bookmark| {
        let server = servers.peek().iter().find(|s| s.id == bookmark.server_id).cloned();
        let Some(server) = server else { return };
        spawn(async move {
            let client = NavidromeClient::new(server);
            match client.delete_bookmark(&bookmark.entry.id).await {
                Ok(()) => list.with_mut(|items| {
                    if let Some(items) = items.as_mut() {
                        items.retain(|b| {
                            !(b.entry.id == bookmark.entry.id && b.server_id == bookmark.server_id)
                        });
                    }
                }),
                Err(err) => error.set(Some(format!("Couldn't delete the bookmark: {err}"))),
            }
        });
    };

    let clear_all = move |_| {
        let items = list.peek().clone().unwrap_or_default();
        let servers = servers.peek().clone();
        spawn(async move {
            for bookmark in items {
                if let Some(server) = servers.iter().find(|s| s.id == bookmark.server_id).cloned() {
                    let _ = NavidromeClient::new(server)
                        .delete_bookmark(&bookmark.entry.id)
                        .await;
                }
            }
            refresh_key.with_mut(|k| *k += 1);
        });
    };

    let on_row_action = move |key: String| {
        let Some(menu) = row_menu.peek().clone() else { return };
        row_menu.set(None);
        let song = menu.bookmark.entry.clone();
        match key.as_str() {
            "resume" => resume(player, menu.bookmark.clone()),
            "album" => {
                if let Some(album_id) = song.album_id.clone() {
                    navigation.navigate_to(AppView::AlbumDetailView {
                        album_id,
                        server_id: song.server_id.clone(),
                    });
                }
            }
            "artist" => {
                if let Some(artist_id) = song.artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id: song.server_id.clone(),
                    });
                } else if let Some(name) = parse_artist_names(song.artist.as_deref().unwrap_or_default()).first().cloned() {
                    let server = servers.peek().iter().find(|s| s.id == song.server_id).cloned();
                    if let Some(server) = server {
                        spawn(async move {
                            if let Some(artist_id) = resolve_artist_id_for_name(server.clone(), name).await {
                                navigation.navigate_to(AppView::ArtistDetailView {
                                    artist_id,
                                    server_id: server.id.clone(),
                                });
                            }
                        });
                    }
                }
            }
            "delete" => delete_one(menu.bookmark.clone()),
            _ => {}
        }
    };

    let has_servers = servers.read().iter().any(|s| s.active);
    let items = list();
    let count = items.as_ref().map(|l| l.len()).unwrap_or(0);

    rsx! {
        div { class: "rs-view rs-view--narrow", "data-testid": "bookmarks",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Bookmarks" }
                if has_servers {
                    div { class: "rs-personal-head-actions",
                        IconButton {
                            icon: "more-horizontal".to_string(),
                            label: "Bookmark options".to_string(),
                            testid: "bookmarks-menu".to_string(),
                            onclick: move |evt: MouseEvent| {
                                let p = evt.client_coordinates();
                                header_menu.set(Some((p.x - 250.0, p.y + 14.0)));
                            },
                        }
                    }
                }
            }

            if let Some(message) = error() {
                p { class: "rs-personal-error", role: "alert", "{message}" }
            }

            if !has_servers {
                EmptyState {
                    icon: "server".to_string(),
                    title: "No servers connected".to_string(),
                    message: "Add a Navidrome server to see your bookmarks.".to_string(),
                    Button {
                        label: "Add Server".to_string(),
                        onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                    }
                }
            } else {
                match items {
                    None => rsx! { Skeleton { rows: 3 } },
                    Some(items) if items.is_empty() => rsx! {
                        EmptyState {
                            icon: "bookmark".to_string(),
                            title: "No bookmarks".to_string(),
                            message: "When you stop partway through a long track, RustySound saves your place here so you can pick it up later.".to_string(),
                            testid: "bookmarks-empty".to_string(),
                        }
                    },
                    Some(items) => rsx! {
                        InsetList { label: "Bookmarks".to_string(), testid: "bookmarks-list".to_string(),
                            for bookmark in items {
                                BookmarkRow {
                                    key: "{bookmark.server_id}:{bookmark.entry.id}",
                                    bookmark: bookmark.clone(),
                                    on_menu: move |(bookmark, anchor)| row_menu.set(Some(RowMenu { bookmark, anchor })),
                                }
                            }
                        }
                        p { class: "rs-list-foot",
                            if count == 1 { "1 bookmark" } else { "{count} bookmarks" }
                        }
                    },
                }
            }

            if let Some(anchor) = header_menu() {
                Menu {
                    title: "Bookmarks".to_string(),
                    anchor,
                    testid: "bookmarks-actions".to_string(),
                    actions: vec![
                        MenuAction::new("refresh", "Refresh", "refresh-cw"),
                        MenuAction::new("clear", "Clear All…", "trash").danger().disabled(count == 0),
                    ],
                    on_close: move |_| header_menu.set(None),
                    on_action: move |key: String| match key.as_str() {
                        "refresh" => refresh_key.with_mut(|k| *k += 1),
                        "clear" => confirm_clear.set(true),
                        _ => {}
                    },
                }
            }
            if let Some(menu) = row_menu() {
                Menu {
                    title: menu.bookmark.entry.title.clone(),
                    anchor: menu.anchor,
                    testid: "bookmark-menu".to_string(),
                    actions: vec![
                        MenuAction::new("resume", "Resume", "play"),
                        MenuAction::new("album", "Go to Album", "disc").disabled(menu.bookmark.entry.album_id.is_none()),
                        MenuAction::new("artist", "Go to Artist", "artist").disabled(menu.bookmark.entry.artist.as_deref().unwrap_or("").trim().is_empty()),
                        MenuAction::new("delete", "Delete Bookmark", "trash").danger(),
                    ],
                    on_close: move |_| row_menu.set(None),
                    on_action: on_row_action,
                }
            }
            if confirm_clear() {
                Menu {
                    title: "Clear All Bookmarks?".to_string(),
                    message: format!("{count} saved positions will be deleted from your server."),
                    testid: "bookmarks-confirm".to_string(),
                    actions: vec![MenuAction::new("clear", "Clear All", "trash").danger()],
                    on_close: move |_| confirm_clear.set(false),
                    on_action: clear_all,
                }
            }
        }
    }
}

/// The playback signals a resume writes.
#[derive(Clone, Copy)]
struct Player {
    queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    is_playing: Signal<bool>,
    position: Signal<f64>,
    seek: Signal<Option<(String, f64)>>,
}

fn use_player() -> Player {
    Player {
        queue: use_context(),
        queue_index: use_context(),
        now_playing: use_context(),
        is_playing: use_context::<IsPlayingSignal>().0,
        position: use_context::<PlaybackPositionSignal>().0,
        seek: use_context::<SeekRequestSignal>().0,
    }
}

/// Starts the bookmarked song at its saved position.
fn resume(player: Player, bookmark: Bookmark) {
    let Player {
        mut queue,
        mut queue_index,
        mut now_playing,
        mut is_playing,
        position: mut playback_position,
        seek: mut seek_request,
    } = player;
    let song = bookmark.entry.clone();
    let start_at = bookmark.position as f64 / 1000.0;
    seek_request.set(Some((song.id.clone(), start_at)));
    queue.set(vec![song.clone()]);
    queue_index.set(0);
    now_playing.set(Some(song));
    playback_position.set(start_at);
    is_playing.set(true);
}

#[component]
fn BookmarkRow(bookmark: Bookmark, on_menu: EventHandler<(Bookmark, Option<(f64, f64)>)>) -> Element {
    let press = use_long_press();
    let player = use_player();
    let song = bookmark.entry.clone();
    let at = format_clock((bookmark.position / 1000) as f64);
    let artist = song.artist.clone().filter(|a| !a.trim().is_empty());
    let resume_now = {
        let bookmark = bookmark.clone();
        move || resume(player, bookmark.clone())
    };
    rsx! {
        li { "data-testid": "bookmark-row",
            div { class: "rs-personal-row",
                button {
                    r#type: "button",
                    class: "rs-row",
                    aria_label: "Resume {song.title} at {at}",
                    onclick: {
                        let resume_now = resume_now.clone();
                        move |_| {
                            if press.take_fired() {
                                return;
                            }
                            resume_now();
                        }
                    },
                    oncontextmenu: {
                        let bookmark = bookmark.clone();
                        move |evt: MouseEvent| {
                            evt.prevent_default();
                            press.cancel();
                            let p = evt.client_coordinates();
                            on_menu.call((bookmark.clone(), Some((p.x, p.y))));
                        }
                    },
                    onpointerdown: {
                        let bookmark = bookmark.clone();
                        move |evt: PointerEvent| {
                            let bookmark = bookmark.clone();
                            press.down(&evt, move |p| on_menu.call((bookmark, Some(p))));
                        }
                    },
                    onpointermove: move |evt| press.moved(&evt),
                    onpointerup: move |_| press.cancel(),
                    onpointercancel: move |_| press.cancel(),
                    Artwork {
                        cover_art: song.cover_art.clone(),
                        server_id: song.server_id.clone(),
                        size_px: 44,
                    }
                    span { class: "rs-row__meta",
                        span { class: "rs-row__title", span { "{song.title}" } }
                        span { class: "rs-row__sub", style: "display:block",
                            span { class: "rs-personal-live", "Resume at {at}" }
                            if let Some(artist) = artist {
                                " · {artist}"
                            }
                        }
                        if let Some(comment) = bookmark.comment.clone().filter(|c| !c.trim().is_empty()) {
                            span { class: "rs-row__why", style: "display:block", "{comment}" }
                        }
                    }
                }
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: "More for {song.title}".to_string(),
                    quiet: true,
                    size: IconButtonSize::Small,
                    onclick: {
                        let bookmark = bookmark.clone();
                        move |evt: MouseEvent| {
                            let p = evt.client_coordinates();
                            on_menu.call((bookmark.clone(), Some((p.x - 250.0, p.y + 12.0))));
                        }
                    },
                }
            }
        }
    }
}
