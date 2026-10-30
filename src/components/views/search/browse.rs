//! "Browse" before typing: genre tiles built from a sample of the library's
//! albums (Subsonic genres come with the album, so one request per server).

use crate::api::{Album, NavidromeClient, ServerConfig};
use crate::components::ui::{Artwork, SectionHeader, Skeleton};
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;
use std::collections::HashMap;

const SAMPLE_PER_SERVER: u32 = 200;
const MAX_GENRES: usize = 12;

#[derive(Clone, PartialEq)]
pub(super) struct GenreTile {
    pub name: String,
    pub albums: usize,
    pub cover: Option<(String, String)>, // (cover_art, server_id)
}

/// Counts genres, most albums first. The genre string is kept whole because
/// the Albums genre route matches it exactly.
pub(super) fn genre_tiles(albums: &[Album]) -> Vec<GenreTile> {
    let mut map: HashMap<String, GenreTile> = HashMap::new();
    for album in albums {
        let Some(name) = album.genre.as_deref().filter(|g| !g.trim().is_empty()) else {
            continue;
        };
        {
            let tile = map.entry(name.to_string()).or_insert_with(|| GenreTile {
                name: name.to_string(),
                albums: 0,
                cover: None,
            });
            tile.albums += 1;
            if tile.cover.is_none() {
                if let Some(cover) = album.cover_art.clone().filter(|c| !c.is_empty()) {
                    tile.cover = Some((cover, album.server_id.clone()));
                }
            }
        }
    }
    let mut tiles: Vec<GenreTile> = map.into_values().collect();
    tiles.sort_by(|a, b| b.albums.cmp(&a.albums).then_with(|| a.name.cmp(&b.name)));
    tiles.truncate(MAX_GENRES);
    tiles
}

#[component]
pub(super) fn BrowseGenres() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();

    let tiles = use_resource(move || {
        let active: Vec<ServerConfig> = servers().into_iter().filter(|s| s.active).collect();
        async move {
            let mut albums = Vec::new();
            for server in active {
                if let Ok(mut list) = NavidromeClient::new(server)
                    .get_albums("frequent", SAMPLE_PER_SERVER, 0)
                    .await
                {
                    albums.append(&mut list);
                }
            }
            genre_tiles(&albums)
        }
    });

    let content = match tiles() {
        None => rsx! { Skeleton { width: "100%".to_string(), height: "132px".to_string() } },
        Some(list) if list.is_empty() => rsx! {
            p { class: "rs-list-foot", "Genres show up here once your albums have genre tags." }
        },
        Some(list) => rsx! {
            ul { class: "rs-search-genres", aria_label: "Genres",
                for tile in list {
                    li { key: "{tile.name}",
                        button {
                            r#type: "button",
                            class: "rs-search-genre",
                            "data-testid": "search-genre",
                            onclick: {
                                let genre = tile.name.clone();
                                move |_| navigation.navigate_to(AppView::AlbumsWithGenre { genre: genre.clone() })
                            },
                            Artwork {
                                cover_art: tile.cover.as_ref().map(|c| c.0.clone()),
                                server_id: tile.cover.as_ref().map(|c| c.1.clone()).unwrap_or_default(),
                                size_px: 48,
                                placeholder: "album".to_string(),
                            }
                            span { class: "rs-search-genre__meta",
                                span { class: "rs-search-genre__name", "{tile.name}" }
                                span { class: "rs-search-genre__count",
                                    if tile.albums == 1 { "1 album" } else { "{tile.albums} albums" }
                                }
                            }
                        }
                    }
                }
            }
        },
    };

    rsx! {
        SectionHeader { title: "Browse".to_string() }
        {content}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(genre: &str) -> Album {
        Album { genre: Some(genre.to_string()), ..Default::default() }
    }

    #[test]
    fn counts_genres_most_first() {
        let tiles = genre_tiles(&[album("Rock"), album("Jazz"), album("Rock"), album("")]);
        assert_eq!(tiles[0].name, "Rock");
        assert_eq!(tiles[0].albums, 2);
        assert_eq!(tiles.len(), 2);
    }
}
