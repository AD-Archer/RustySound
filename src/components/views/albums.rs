//! Albums: a cover grid (or Cover Flow on wide windows) with one sort menu,
//! a debounced search and offset paging that appends as you scroll.

use crate::api::*;
use crate::components::ui::{
    cover_art_url, use_layout, ActionMenu, ActionMenuItem, CoverFlow, CoverFlowItem, EmptyState,
    SearchField, Segmented, Stage,
};
use crate::components::views::library::shared::{
    use_servers,
    active_servers, use_debounced, use_pager, ErrorState, GridSkeleton, LoadMore, NoServers,
    PageFoot,
};
use crate::components::views::library::tiles::AlbumGrid;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

const PAGE: u32 = 48;

const SORTS: [(&str, &str); 5] = [
    ("recent", "Recently Played"),
    ("alphabeticalByName", "A–Z"),
    ("newest", "Newest"),
    ("frequent", "Most Played"),
    ("random", "Random"),
];

fn sort_label(value: &str) -> &'static str {
    SORTS
        .iter()
        .find(|(v, _)| *v == value)
        .map(|(_, label)| *label)
        .unwrap_or("A–Z")
}

#[component]
pub fn Albums() -> Element {
    rsx! {
        AlbumsView { genre: None }
    }
}

#[component]
pub fn AlbumsWithGenre(genre: String) -> Element {
    rsx! {
        AlbumsView { key: "{genre}", genre: Some(genre) }
    }
}

/// One page from every active server at `offset`.
async fn fetch_album_page(
    servers: Vec<ServerConfig>,
    sort: String,
    query: String,
    genre: Option<String>,
    offset: u32,
) -> Result<(Vec<Album>, bool), String> {
    let mut albums = Vec::new();
    let mut more = false;
    let mut last_error = None;
    let mut any_ok = false;
    for server in servers {
        let client = NavidromeClient::new(server);
        let result = if query.is_empty() {
            client.get_album_page(&sort, genre.as_deref(), PAGE, offset).await
        } else {
            client
                .search_page(&query, "album", PAGE, offset)
                .await
                .map(|r| r.albums)
        };
        match result {
            Ok(page) => {
                any_ok = true;
                more |= page.len() as u32 >= PAGE;
                albums.extend(page);
            }
            Err(err) => last_error = Some(err),
        }
    }
    if !any_ok {
        if let Some(err) = last_error {
            return Err(err);
        }
    }
    // Search can't filter by genre on the server.
    if !query.is_empty() {
        if let Some(genre) = genre.as_deref() {
            albums.retain(|a| a.genre.as_deref().map(|g| g.eq_ignore_ascii_case(genre)) == Some(true));
        }
    }
    Ok((albums, more))
}

#[component]
pub fn AlbumsView(genre: Option<String>) -> Element {
    let servers = use_servers();
    let navigation = use_context::<Navigation>();
    let layout = use_layout();
    let mut sort = use_signal(|| {
        if genre.is_some() {
            "alphabeticalByName".to_string()
        } else {
            "recent".to_string()
        }
    });
    let mut search = use_signal(String::new);
    let query = use_debounced(search, 250);
    let mut flow_view = use_signal(|| false);
    let mut menu_at = use_signal(|| None::<Option<(f64, f64)>>);
    let pager = use_pager::<Album>();
    let genre_sig = use_signal(|| genre.clone());

    let load_more = move || {
        let servers = active_servers(&servers.peek());
        let sort_value = sort.peek().clone();
        let query_value = query.peek().clone();
        let genre_value = genre_sig.peek().clone();
        pager.load_more(PAGE, move |offset| {
            fetch_album_page(servers, sort_value, query_value, genre_value, offset)
        });
    };

    // A new sort, search or server set starts over from offset 0.
    use_effect(move || {
        let _ = (sort(), query(), servers());
        pager.reset();
        load_more();
    });

    // "Recently Played" is empty on a fresh server: fall back to A–Z once.
    let mut fell_back = use_signal(|| false);
    use_effect(move || {
        let ready = (pager.ready)();
        if ready
            && !*fell_back.peek()
            && pager.items.peek().is_empty()
            && pager.error.peek().is_none()
            && sort.peek().as_str() == "recent"
            && query.peek().is_empty()
        {
            fell_back.set(true);
            sort.set("alphabeticalByName".to_string());
        }
    });

    let has_servers = !servers.read().iter().all(|s| !s.active);
    let title = match genre.as_deref() {
        Some(g) => g.to_string(),
        None => "Albums".to_string(),
    };
    let wide = !layout.is_compact();
    let show_flow = wide && flow_view();
    let items = pager.items.read();
    let count = items.len();
    let loading = (pager.loading)();
    let ready = (pager.ready)();
    let error = (pager.error)();
    let done = (pager.done)();
    let offset = (pager.offset)();
    let searching = !query.read().is_empty();
    let current_sort = sort();

    let flow_items: Vec<CoverFlowItem> = if show_flow {
        let servers_now = servers.read();
        items
            .iter()
            .map(|album| CoverFlowItem {
                key: format!("{}:{}", album.server_id, album.id),
                title: album.name.clone(),
                subtitle: album.artist.clone(),
                reason: album.year.filter(|y| *y > 0).map(|y| y.to_string()).unwrap_or_default(),
                art: cover_art_url(
                    &servers_now,
                    &album.server_id,
                    album.cover_art.as_deref().or(Some(album.id.as_str())),
                    432,
                )
                .into_iter()
                .collect(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let flow_targets: Vec<AppView> = if show_flow {
        items
            .iter()
            .map(|a| AppView::AlbumDetailView {
                album_id: a.id.clone(),
                server_id: a.server_id.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let grid_albums: Vec<Album> = if show_flow { Vec::new() } else { items.clone() };
    drop(items);

    rsx! {
        div { class: "rs-view rs-library-albums", "data-testid": "albums",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "{title}" }
                div { class: "rs-library-head-tools",
                    if wide {
                        Segmented {
                            options: vec!["Grid".to_string(), "Cover Flow".to_string()],
                            selected: if flow_view() { 1 } else { 0 },
                            inline: true,
                            label: "Album view".to_string(),
                            testid: "albums-view-toggle".to_string(),
                            on_select: move |i: usize| flow_view.set(i == 1),
                        }
                    }
                    if genre.is_none() && !searching {
                        button {
                            r#type: "button",
                            class: "rs-library-menubtn",
                            aria_haspopup: "menu",
                            "data-testid": "albums-sort",
                            onclick: move |evt: MouseEvent| {
                                let p = evt.client_coordinates();
                                menu_at.set(Some(Some((p.x - 200.0, p.y + 16.0))));
                            },
                            span { class: "rs-library-menubtn__hint", "Sort" }
                            "{sort_label(&current_sort)}"
                            Icon { name: "chevron-down".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        }
                    }
                }
            }
            SearchField {
                value: search(),
                placeholder: if genre.is_some() { format!("Search {title} albums") } else { "Search albums".to_string() },
                testid: "albums-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }

            if !has_servers {
                NoServers {}
            } else if !ready {
                GridSkeleton { count: 12 }
            } else if count == 0 {
                if let Some(err) = error.clone() {
                    ErrorState {
                        title: "Couldn't load albums".to_string(),
                        error: err,
                        on_retry: move |_| {
                            pager.reset();
                            load_more();
                        },
                    }
                } else if searching {
                    EmptyState {
                        icon: "search".to_string(),
                        title: "No albums found".to_string(),
                        message: format!("Nothing matches “{}”. Check the spelling or try fewer words.", query.read()),
                    }
                } else {
                    EmptyState {
                        icon: "album".to_string(),
                        title: "No albums yet".to_string(),
                        message: "When your server has scanned some music, albums show up here.".to_string(),
                    }
                }
            } else if show_flow {
                Stage { class: "rs-library-flowstage".to_string(), label: "Albums in Cover Flow".to_string(),
                    CoverFlow {
                        items: flow_items,
                        cover_px: 216,
                        wide: true,
                        label: "Albums".to_string(),
                        on_open: move |index: usize| {
                            if let Some(target) = flow_targets.get(index) {
                                navigation.navigate_to(target.clone());
                            }
                        },
                        on_select: move |index: usize| {
                            if index + 12 >= count && !done {
                                load_more();
                            }
                        },
                    }
                }
                PageFoot {
                    loading,
                    error: error.clone(),
                    on_retry: move |_| load_more(),
                }
            } else {
                AlbumGrid { albums: grid_albums, label: title.clone() }
                if !done && error.is_none() {
                    LoadMore { key: "{offset}", on_near_end: move |_| load_more() }
                }
                PageFoot {
                    loading,
                    error: error.clone(),
                    on_retry: move |_| load_more(),
                }
            }

            if let Some(anchor) = menu_at() {
                ActionMenu {
                    label: "Sort albums".to_string(),
                    title: "Sort by".to_string(),
                    anchor,
                    on_close: move |_| menu_at.set(None),
                    testid: "albums-sort-menu".to_string(),
                    for (value, label) in SORTS {
                        ActionMenuItem {
                            key: "{value}",
                            label: label.to_string(),
                            checked: current_sort == value,
                            onclick: move |_| {
                                menu_at.set(None);
                                if sort.peek().as_str() != value {
                                    sort.set(value.to_string());
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
