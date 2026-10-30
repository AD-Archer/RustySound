//! Artists: one inset list with round thumbs under A–Z headers, windowed
//! (libraries run to thousands of artists), with an A–Z scrubber on phones.

use crate::api::*;
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json};
use crate::components::ui::{use_layout, Artwork, EmptyState, SearchField, VirtualList};
use crate::components::views::library::shared::{
    use_servers,
    active_servers, use_debounced, ErrorState, ListSkeleton, NoServers,
};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

const ROW_H: f64 = 56.0;

/// Section letter for a name: A–Z, or "#" for digits and symbols.
fn section_of(name: &str) -> char {
    let trimmed = name.trim();
    let without_article = trimmed
        .strip_prefix("The ")
        .or_else(|| trimmed.strip_prefix("the "))
        .unwrap_or(trimmed);
    match without_article.chars().next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        Some(c) if c.is_alphabetic() => {
            // Fold accented letters (É → E) when the fold is plain ASCII.
            c.to_uppercase()
                .next()
                .filter(|u| u.is_ascii_alphabetic())
                .unwrap_or('#')
        }
        _ => '#',
    }
}

fn sort_key(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    lower
        .strip_prefix("the ")
        .map(str::to_string)
        .unwrap_or(lower)
}

#[derive(Clone, PartialEq)]
enum Row {
    Header(char),
    Artist(usize),
}

async fn load_artists(servers: Vec<ServerConfig>) -> Result<Vec<Artist>, String> {
    let mut ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
    ids.sort();
    let cache_key = format!("view:artists:v2:{}", ids.join("|"));
    if let Some(cached) = cache_get_json::<Vec<Artist>>(&cache_key) {
        if !cached.is_empty() {
            return Ok(cached);
        }
    }
    let mut artists = Vec::new();
    let mut last_error = None;
    for server in servers {
        match NavidromeClient::new(server).get_artists().await {
            Ok(list) => artists.extend(list),
            Err(err) => last_error = Some(err),
        }
    }
    if artists.is_empty() {
        if let Some(err) = last_error {
            return Err(err);
        }
    }
    artists.sort_by_cached_key(|a| sort_key(&a.name));
    let _ = cache_put_json(cache_key, &artists, Some(6));
    Ok(artists)
}

#[component]
pub fn ArtistsView() -> Element {
    let servers = use_servers();
    let navigation = use_context::<Navigation>();
    let layout = use_layout();
    let mut search = use_signal(String::new);
    let query = use_debounced(search, 220);
    let mut retry = use_signal(|| 0u32);

    let artists = use_resource(move || {
        let _ = retry();
        let servers = active_servers(&servers());
        async move { load_artists(servers).await }
    });

    // Flat rows (headers + artists) for the current filter.
    let rows = use_memo(move || {
        let q = query().to_lowercase();
        let guard = artists.read();
        let Some(Ok(list)) = guard.as_ref() else {
            return Vec::<Row>::new();
        };
        let mut rows = Vec::with_capacity(list.len() + 27);
        let mut current = None;
        for (index, artist) in list.iter().enumerate() {
            if !q.is_empty() && !artist.name.to_lowercase().contains(&q) {
                continue;
            }
            let letter = section_of(&artist.name);
            if q.is_empty() && current != Some(letter) {
                current = Some(letter);
                rows.push(Row::Header(letter));
            }
            rows.push(Row::Artist(index));
        }
        rows
    });
    let letters = use_memo(move || {
        rows.read()
            .iter()
            .enumerate()
            .filter_map(|(i, row)| match row {
                Row::Header(c) => Some((*c, i)),
                _ => None,
            })
            .collect::<Vec<(char, usize)>>()
    });

    let has_servers = servers.read().iter().any(|s| s.active);
    let compact = layout.is_compact();

    let scroll_to_row = move |row: usize| {
        let offset = row as f64 * ROW_H;
        let _ = document::eval(&format!(
            r#"const m=document.querySelector('.rs-main');const l=document.querySelector('[data-testid="artist-list"]');
if(m&&l){{m.scrollTop+=l.getBoundingClientRect().top-m.getBoundingClientRect().top+{offset};}}"#
        ));
    };

    let body = match &*artists.read() {
        _ if !has_servers => rsx! { NoServers {} },
        None => rsx! { ListSkeleton { rows: 10, round: true } },
        Some(Err(err)) => {
            let err = err.clone();
            rsx! {
                ErrorState {
                    title: "Couldn't load artists".to_string(),
                    error: err,
                    on_retry: move |_| retry += 1,
                }
            }
        }
        Some(Ok(_)) => {
            let total = rows.read().len();
            if total == 0 {
                let q = query();
                if q.is_empty() {
                    rsx! {
                        EmptyState {
                            icon: "artist".to_string(),
                            title: "No artists yet".to_string(),
                            message: "Artists appear here once your server has scanned some music.".to_string(),
                        }
                    }
                } else {
                    rsx! {
                        EmptyState {
                            icon: "search".to_string(),
                            title: "No artists found".to_string(),
                            message: format!("Nothing matches “{q}”."),
                        }
                    }
                }
            } else {
                rsx! {
                    VirtualList {
                        item_count: total,
                        row_height: ROW_H,
                        class: "rs-list rs-library-artists".to_string(),
                        testid: "artist-list".to_string(),
                        render_row: move |i: usize| {
                            let row = rows.read().get(i).cloned();
                            match row {
                                Some(Row::Header(letter)) => rsx! {
                                    li { class: "rs-library-letter", "data-index": "{i}",
                                        h2 { "{letter}" }
                                    }
                                },
                                Some(Row::Artist(index)) => {
                                    let found = match &*artists.read() {
                                        Some(Ok(list)) => list.get(index).cloned(),
                                        _ => None,
                                    };
                                    let Some(artist) = found else {
                                        return rsx! { li {} };
                                    };
                                    let target = AppView::ArtistDetailView {
                                        artist_id: artist.id.clone(),
                                        server_id: artist.server_id.clone(),
                                    };
                                    let albums = match artist.album_count {
                                        0 => String::new(),
                                        1 => "1 album".to_string(),
                                        n => format!("{n} albums"),
                                    };
                                    rsx! {
                                        li { "data-index": "{i}",
                                            button {
                                                r#type: "button",
                                                class: "rs-row rs-library-artrow",
                                                "data-testid": "artist-row",
                                                onclick: move |_| navigation.navigate_to(target.clone()),
                                                Artwork {
                                                    cover_art: artist.cover_art.clone(),
                                                    server_id: artist.server_id.clone(),
                                                    size_px: 40,
                                                    round: true,
                                                    placeholder: "artist".to_string(),
                                                }
                                                span { class: "rs-row__meta",
                                                    span { class: "rs-row__title", span { "{artist.name}" } }
                                                }
                                                if !albums.is_empty() {
                                                    span { class: "rs-row__detail", "{albums}" }
                                                }
                                                span { class: "rs-row__chev",
                                                    Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                                                }
                                            }
                                        }
                                    }
                                }
                                None => rsx! { li {} },
                            }
                        },
                    }
                }
            }
        }
    };

    // A–Z scrubber: a finger dragged down the strip jumps letter by letter.
    let mut index_node = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut index_rect = use_signal(|| None::<(f64, f64)>);
    let mut scrubbing = use_signal(|| false);
    let mut last_pick = use_signal(|| None::<usize>);
    let mut pick_at = move |y: f64| {
        let Some((top, height)) = *index_rect.peek() else {
            return;
        };
        let marks = letters.peek().clone();
        if marks.is_empty() || height <= 0.0 {
            return;
        }
        let slot = (((y - top) / height) * marks.len() as f64).floor();
        let slot = slot.clamp(0.0, (marks.len() - 1) as f64) as usize;
        if *last_pick.peek() != Some(slot) {
            last_pick.set(Some(slot));
            scroll_to_row(marks[slot].1);
        }
    };

    let index_letters = letters();
    let show_index = compact && query.read().is_empty() && index_letters.len() > 3;

    rsx! {
        div { class: "rs-view rs-library-artists-view", "data-testid": "artists",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Artists" }
            }
            SearchField {
                value: search(),
                placeholder: "Search artists".to_string(),
                testid: "artists-search".to_string(),
                class: "rs-library-search".to_string(),
                on_input: move |value: String| search.set(value),
            }
            {body}
            if show_index {
                nav {
                    class: "rs-library-index",
                    aria_label: "Jump to letter",
                    onmounted: move |evt| index_node.set(Some(evt.data())),
                    onpointerdown: move |evt: PointerEvent| {
                        scrubbing.set(true);
                        last_pick.set(None);
                        let y = evt.client_coordinates().y;
                        let node = index_node.peek().clone();
                        spawn(async move {
                            if let Some(node) = node {
                                if let Ok(rect) = node.get_client_rect().await {
                                    index_rect.set(Some((rect.origin.y, rect.size.height)));
                                    pick_at(y);
                                }
                            }
                        });
                    },
                    onpointermove: move |evt: PointerEvent| {
                        if *scrubbing.peek() {
                            pick_at(evt.client_coordinates().y);
                        }
                    },
                    onpointerup: move |_| scrubbing.set(false),
                    onpointercancel: move |_| scrubbing.set(false),
                    for (letter, row) in index_letters {
                        button {
                            key: "{letter}",
                            r#type: "button",
                            class: "rs-library-index__letter",
                            aria_label: "Jump to {letter}",
                            onclick: move |_| scroll_to_row(row),
                            "{letter}"
                        }
                    }
                }
            }
        }
    }
}
