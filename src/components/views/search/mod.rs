//! Search: one field over every active server.
//!
//! Before typing: recent searches and a Browse grid of genres. While typing
//! (debounced 220 ms): top hit, Artists, Albums and Songs, re-scored across
//! servers. The input keeps `id="global-search-input"`: the macOS Go menu
//! focuses it.

mod browse;
mod recent;
mod results;
mod scoring;

pub use results::ArtistCard;

use crate::api::{NavidromeClient, SearchResult, ServerConfig};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{
    use_layout, EmptyState, IconButton, IconButtonSize, InsetList, ListInset, ListRow,
    SectionHeader, Skeleton,
};
use crate::components::Icon;
use browse::BrowseGenres;
use dioxus::prelude::*;
use results::Results;
use scoring::{dedupe_search_results, filter_and_score_results, normalize_text, tokenize, top_hit};

const DEBOUNCE_MS: u64 = 220;

/// Searches every active server with the full query plus each significant
/// token (3+ characters), so "all falls down kanye" finds title + artist.
async fn search_servers(servers: Vec<ServerConfig>, query: String) -> SearchResult {
    let mut combined = SearchResult::default();
    let tokens = tokenize(&query);
    let mut queries: Vec<String> = vec![query.clone()];
    for token in &tokens {
        if token.len() >= 3 && *token != normalize_text(&query) {
            queries.push(token.clone());
        }
    }
    queries.dedup();
    for server in servers {
        let client = NavidromeClient::new(server);
        for q in &queries {
            if let Ok(result) = client.search(q, 24, 48, 96).await {
                combined.artists.extend(result.artists);
                combined.albums.extend(result.albums);
                combined.songs.extend(result.songs);
            }
        }
    }
    filter_and_score_results(dedupe_search_results(combined), &query)
}

#[component]
pub fn SearchView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let layout = use_layout();
    let wide = !layout.is_compact();

    let mut query = use_signal(String::new);
    let mut debounced = use_signal(String::new);
    let mut results = use_signal(|| None::<(String, SearchResult)>);
    let mut searching = use_signal(|| false);
    let mut generation = use_signal(|| 0u64);
    let mut recents = use_signal(Vec::<String>::new);

    // Recent searches load after the first render (file / localStorage).
    use_hook(move || {
        spawn(async move {
            recents.set(recent::load());
        });
    });

    // Debounce typing; clear immediately when the field empties.
    use_effect(move || {
        let text = query().trim().to_string();
        generation.with_mut(|g| *g = g.wrapping_add(1));
        let ticket = *generation.peek();
        if text.chars().count() < 2 {
            debounced.set(String::new());
            results.set(None);
            searching.set(false);
            return;
        }
        spawn(async move {
            sleep_ms(DEBOUNCE_MS).await;
            if *generation.peek() == ticket {
                debounced.set(text);
            }
        });
    });

    // Run the debounced query; drop answers that arrive after a newer query.
    use_effect(move || {
        let text = debounced();
        if text.is_empty() {
            return;
        }
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        let ticket = *generation.peek();
        searching.set(true);
        spawn(async move {
            let found = search_servers(active, text.clone()).await;
            if *generation.peek() == ticket {
                results.set(Some((text, found)));
                searching.set(false);
            }
        });
    });

    let mut remember_query = move || {
        let text = query.peek().trim().to_string();
        let next = recent::remember(&recents.peek(), &text);
        if next != *recents.peek() {
            recent::save(&next);
            recents.set(next);
        }
    };

    let text = query();
    let typed = text.trim().chars().count() >= 2;
    let shown = results();
    let busy = searching();

    let body = if !typed {
        let list = recents();
        rsx! {
            if !list.is_empty() {
                SectionHeader {
                    title: "Recent".to_string(),
                    action: "Clear".to_string(),
                    testid: "search-recent-clear".to_string(),
                    on_action: move |_| {
                        recent::save(&[]);
                        recents.set(Vec::new());
                    },
                }
                InsetList { inset: ListInset::Icon, label: "Recent searches".to_string(), testid: "search-recent".to_string(),
                    for item in list {
                        ListRow {
                            key: "{item}",
                            title: item.clone(),
                            icon: "clock".to_string(),
                            compact: true,
                            onclick: move |_| query.set(item.clone()),
                        }
                    }
                }
            }
            BrowseGenres {}
        }
    } else {
        match shown {
            Some((for_query, found)) => {
                let empty = found.artists.is_empty() && found.albums.is_empty() && found.songs.is_empty();
                if empty && !busy {
                    rsx! {
                        EmptyState {
                            icon: "search".to_string(),
                            title: format!("No results for “{for_query}”"),
                            message: "Check the spelling, or try fewer words.".to_string(),
                            testid: "search-empty".to_string(),
                        }
                    }
                } else {
                    let top = top_hit(&found, &for_query);
                    rsx! {
                        Results {
                            key: "{for_query}",
                            results: found,
                            top,
                            wide,
                            on_open: move |_| remember_query(),
                        }
                    }
                }
            }
            None => rsx! { Skeleton { rows: 6 } },
        }
    };

    let no_servers = !servers.read().iter().any(|s| s.active);

    rsx! {
        div { class: "rs-view rs-search", "data-testid": "search",
            div { class: "rs-page-head",
                h1 { class: "rs-large-title", "Search" }
            }
            div { class: "rs-search-bar",
                div { class: "rs-field rs-search-field",
                    label { class: "rs-search-sr", r#for: "global-search-input", "Search your library" }
                    Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    input {
                        id: "global-search-input",
                        "data-testid": "search-input",
                        r#type: "search",
                        enterkeyhint: "search",
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        placeholder: "Artists, albums, songs",
                        value: "{text}",
                        onmounted: move |evt| async move {
                            // Wide: arriving at Search (sidebar, ⌘L) puts the caret in the field.
                            if wide {
                                let _ = evt.set_focus(true).await;
                            }
                        },
                        oninput: move |evt| query.set(evt.value()),
                        onkeydown: move |evt| match evt.key() {
                            Key::Enter => remember_query(),
                            Key::Escape => query.set(String::new()),
                            _ => {}
                        },
                    }
                    if busy {
                        span { class: "rs-spinner rs-search-spin", aria_hidden: "true" }
                    }
                    if !text.is_empty() {
                        IconButton {
                            icon: "x".to_string(),
                            label: "Clear search".to_string(),
                            quiet: true,
                            size: IconButtonSize::Small,
                            onclick: move |_| query.set(String::new()),
                        }
                    }
                }
            }
            div { class: "rs-search-body", aria_busy: if busy { "true" } else { "false" },
                if no_servers {
                    EmptyState {
                        icon: "server".to_string(),
                        title: "No server is on".to_string(),
                        message: "Turn a server on in Settings to search your music.".to_string(),
                    }
                } else {
                    {body}
                }
            }
        }
    }
}
