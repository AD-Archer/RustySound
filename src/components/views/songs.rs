//! Songs: every song in the library as a windowed list of `TrackRow`s, with
//! one sort menu (field, direction, minimum rating), search and offset paging.
//! Tapping a song plays the visible list from that song.

use crate::api::*;
use crate::components::ui::{
    Menu, MenuGap, MenuItem, EmptyState, SearchField, TrackRow, VirtualList,
};
use crate::components::views::library::shared::{
    use_servers,
    active_servers, song_key, use_debounced, use_downloaded_keys, use_now_playing, use_pager,
    use_player, use_song_patches, ErrorState, ListSkeleton, LoadMore, NoServers, PageFoot,
};
use crate::components::Icon;
use dioxus::prelude::*;

const PAGE: u32 = 200;
const ROW_H: f64 = 58.0;
/// Placeholder rows after the loaded ones while the next page is on its way,
/// so a fling runs into rows that are about to fill in, not the list's end.
const GHOST_ROWS: usize = 24;
/// Ask for the next page this far (px) before the loaded rows run out: a whole
/// page, so the next page is requested as soon as the user starts into the
/// last one. Repeated flicks cover thousands of px while a server answers.
const PREFETCH_PX: u32 = (PAGE as f64 * ROW_H) as u32;
/// Playing from a row queues at most this many songs from that row on.
const CONTEXT_QUEUE_MAX: usize = 500;

const SORTS: [(&str, &str); 5] = [
    ("play_date", "Last Played"),
    ("play_count", "Most Played"),
    ("title", "A–Z"),
    ("rating", "Rating"),
    ("duration", "Duration"),
];

fn sort_label(value: &str) -> &'static str {
    SORTS
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, l)| *l)
        .unwrap_or("A–Z")
}

/// Ascending is the natural direction for A–Z only.
fn default_ascending(sort: &str) -> bool {
    sort == "title"
}

async fn fetch_song_page(
    servers: Vec<ServerConfig>,
    sort: String,
    ascending: bool,
    query: String,
    offset: u32,
) -> Result<(Vec<Song>, bool), String> {
    let mut songs = Vec::new();
    let mut more = false;
    let mut any_ok = false;
    let mut last_error = None;
    for server in servers {
        let client = NavidromeClient::new(server);
        let result = if query.is_empty() {
            match client
                .get_native_song_page(&sort, ascending, offset as usize, PAGE as usize)
                .await
            {
                Ok(page) => Ok(page),
                // Not Navidrome: page through an empty search instead (unsorted).
                Err(_) => client.search_page("", "song", PAGE, offset).await.map(|r| r.songs),
            }
        } else {
            client.search_page(&query, "song", PAGE, offset).await.map(|r| r.songs)
        };
        match result {
            Ok(page) => {
                any_ok = true;
                more |= page.len() as u32 >= PAGE;
                songs.extend(page);
            }
            Err(err) => last_error = Some(err),
        }
    }
    if !any_ok {
        if let Some(err) = last_error {
            return Err(err);
        }
    }
    Ok((songs, more))
}

#[component]
pub fn SongsView() -> Element {
    let servers = use_servers();
    let player = use_player();
    let mut search = use_signal(String::new);
    let query = use_debounced(search, 250);
    let mut sort = use_signal(|| "play_date".to_string());
    let mut ascending = use_signal(|| false);
    let mut min_rating = use_signal(|| 0u32);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let mut status = use_signal(|| None::<String>);
    let pager = use_pager::<Song>();
    use_song_patches(pager.items);
    let downloaded = use_downloaded_keys(pager.items);
    let (playing_key, live) = use_now_playing();

    let load_more = move || {
        let servers = active_servers(&servers.peek());
        let (s, asc, q) = (sort.peek().clone(), *ascending.peek(), query.peek().clone());
        pager.load_more(PAGE, move |offset| fetch_song_page(servers, s, asc, q, offset));
    };

    use_effect(move || {
        let _ = (sort(), ascending(), query(), servers());
        pager.reset();
        load_more();
    });

    // The rating floor filters loaded rows; indexes map back to them.
    let visible = use_memo(move || {
        let floor = min_rating();
        let rating_sort = sort() == "rating";
        let items = pager.items.read();
        if floor == 0 || !rating_sort {
            return (0..items.len()).collect::<Vec<usize>>();
        }
        items
            .iter()
            .enumerate()
            .filter(|(_, s)| s.user_rating.unwrap_or(0) >= floor)
            .map(|(i, _)| i)
            .collect()
    });

    // Sorted by rating, high to low: once rows drop below the floor, stop.
    let below_floor = {
        let floor = min_rating();
        let items = pager.items.read();
        floor > 0
            && sort() == "rating"
            && !ascending()
            && items.last().map(|s| s.user_rating.unwrap_or(0) < floor).unwrap_or(false)
    };

    let mut play_at = move |row: usize| {
        let items = pager.items.peek();
        let order = visible.peek();
        let list: Vec<Song> = order
            .iter()
            .skip(row)
            .take(CONTEXT_QUEUE_MAX)
            .filter_map(|i| items.get(*i).cloned())
            .collect();
        drop(items);
        drop(order);
        status.set(player.play(list, 0, None).err());
    };

    let has_servers = servers.read().iter().any(|s| s.active);
    let ready = (pager.ready)();
    let loading = (pager.loading)();
    let error = (pager.error)();
    let done = (pager.done)() || below_floor;
    let offset = (pager.offset)();
    let rows = visible.read().len();
    let current_sort = sort();
    let asc = ascending();
    let floor = min_rating();
    let searching = !query.read().is_empty();
    let sort_text = if floor > 0 && current_sort == "rating" {
        format!("{} · {}+", sort_label(&current_sort), floor)
    } else {
        sort_label(&current_sort).to_string()
    };

    rsx! {
        div { class: "rs-view rs-library-songs", "data-testid": "songs",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Songs" }
                if !searching {
                    button {
                        r#type: "button",
                        class: "rs-library-menubtn",
                        aria_haspopup: "menu",
                        "data-testid": "songs-sort",
                        onclick: move |evt: MouseEvent| {
                            let p = evt.client_coordinates();
                            menu_at.set(Some(Some((p.x - 200.0, p.y + 16.0))));
                        },
                        span { class: "rs-library-menubtn__hint", "Sort" }
                        "{sort_text}"
                        Icon {
                            name: if asc { "chevron-up".to_string() } else { "chevron-down".to_string() },
                            class: "rs-ic rs-ic--sm".to_string(),
                        }
                    }
                }
            }
            SearchField {
                value: search(),
                placeholder: "Search songs".to_string(),
                testid: "songs-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }
            if let Some(text) = status() {
                p { class: "rs-library-status", role: "status", "{text}" }
            }

            if !has_servers {
                NoServers {}
            } else if !ready {
                ListSkeleton { rows: 12 }
            } else if rows == 0 && (done || error.is_some()) {
                if let Some(err) = error.clone() {
                    ErrorState {
                        title: "Couldn't load songs".to_string(),
                        error: err,
                        on_retry: move |_| {
                            pager.reset();
                            load_more();
                        },
                    }
                } else if searching {
                    EmptyState {
                        icon: "search".to_string(),
                        title: "No songs found".to_string(),
                        message: format!("Nothing matches “{}”.", query.read()),
                    }
                } else if floor > 0 {
                    EmptyState {
                        icon: "star".to_string(),
                        title: "No songs rated {floor} or more".to_string(),
                        message: "Lower the minimum rating in the sort menu.".to_string(),
                    }
                } else {
                    EmptyState {
                        icon: "music".to_string(),
                        title: "No songs yet".to_string(),
                        message: "Songs appear here once your server has scanned some music.".to_string(),
                    }
                }
            } else {
                VirtualList {
                    item_count: rows,
                    row_height: ROW_H,
                    class: "rs-list rs-library-songlist".to_string(),
                    testid: "song-list".to_string(),
                    ghost_rows: if !done && error.is_none() { GHOST_ROWS } else { 0 },
                    render_row: move |row: usize| {
                        let index = visible.read().get(row).copied();
                        let song = index.and_then(|i| pager.items.read().get(i).cloned());
                        let Some(song) = song else {
                            return rsx! { li {} };
                        };
                        let key = song_key(&song);
                        let playing = playing_key.read().as_deref() == Some(key.as_str());
                        let is_downloaded = downloaded.read().contains(&key);
                        let subtitle = match song.album.as_deref().filter(|a| !a.is_empty()) {
                            Some(album) => format!("{} · {}", song.artist.clone().unwrap_or_default(), album),
                            None => song.artist.clone().unwrap_or_default(),
                        };
                        rsx! {
                            TrackRow {
                                key: "{key}",
                                song,
                                subtitle,
                                playing,
                                live: playing && live(),
                                downloaded: is_downloaded,
                                index: row,
                                on_play: move |_| play_at(row),
                            }
                        }
                    },
                }
                if !done && error.is_none() {
                    LoadMore {
                        key: "{offset}",
                        lead_px: PREFETCH_PX + (GHOST_ROWS as f64 * ROW_H) as u32,
                        on_near_end: move |_| load_more(),
                    }
                }
                PageFoot { loading, error: error.clone(), on_retry: move |_| load_more() }
            }

            if let Some(anchor) = menu_at() {
                Menu {
                    label: "Sort songs".to_string(),
                    title: "Sort by".to_string(),
                    anchor,
                    on_close: move |_| menu_at.set(None),
                    testid: "songs-sort-menu".to_string(),
                    for (value, label) in SORTS {
                        MenuItem {
                            key: "{value}",
                            label: label.to_string(),
                            checked: current_sort == value,
                            onclick: move |_| {
                                menu_at.set(None);
                                if sort.peek().as_str() != value {
                                    ascending.set(default_ascending(value));
                                    sort.set(value.to_string());
                                }
                            },
                        }
                    }
                    MenuGap {}
                    MenuItem {
                        label: if current_sort == "title" { "A to Z".to_string() } else { "Ascending".to_string() },
                        checked: asc,
                        onclick: move |_| {
                            menu_at.set(None);
                            ascending.set(true);
                        },
                    }
                    MenuItem {
                        label: if current_sort == "title" { "Z to A".to_string() } else { "Descending".to_string() },
                        checked: !asc,
                        onclick: move |_| {
                            menu_at.set(None);
                            ascending.set(false);
                        },
                    }
                    if current_sort == "rating" {
                        MenuGap {}
                        div { class: "rs-menu__note", "Minimum rating" }
                        div { class: "rs-menu__stars", role: "group", aria_label: "Minimum rating",
                            for value in 1..=5u32 {
                                button {
                                    key: "{value}",
                                    r#type: "button",
                                    class: "rs-menu__star",
                                    class: if value <= floor { "rs-menu__star--on" },
                                    aria_label: "{value} stars or more",
                                    aria_pressed: if value == floor { "true" } else { "false" },
                                    onclick: move |_| min_rating.set(value),
                                    Icon {
                                        name: if value <= floor { "star-filled".to_string() } else { "star".to_string() },
                                        class: "rs-ic rs-ic--md".to_string(),
                                    }
                                }
                            }
                            button {
                                r#type: "button",
                                class: "rs-menu__star",
                                aria_label: "Any rating",
                                onclick: move |_| min_rating.set(0),
                                Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                            }
                        }
                    }
                }
            }
        }
    }
}
