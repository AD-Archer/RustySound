//! Cover tiles: album grid cells and round artist thumbs.

use crate::api::Album;
use crate::components::ui::Artwork;
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

/// One album in a cover grid: square art, title, artist, year.
/// * `show_artist`: false on an artist's own page.
#[component]
pub fn AlbumTile(album: Album, #[props(default = true)] show_artist: bool) -> Element {
    let navigation = use_context::<Navigation>();
    let target = AppView::AlbumDetailView {
        album_id: album.id.clone(),
        server_id: album.server_id.clone(),
    };
    let sub = if show_artist {
        album.artist.clone()
    } else {
        String::new()
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-library-tile",
            "data-testid": "album-tile",
            onclick: move |_| navigation.navigate_to(target.clone()),
            div { class: "rs-library-tile__art",
                Artwork {
                    cover_art: album.cover_art.clone(),
                    server_id: album.server_id.clone(),
                    size_px: 200,
                    fill: true,
                    placeholder: "album".to_string(),
                }
            }
            span { class: "rs-library-tile__title", "{album.name}" }
            span { class: "rs-library-tile__sub",
                if !sub.is_empty() {
                    span { class: "rs-library-tile__artist", "{sub}" }
                }
                if let Some(year) = album.year.filter(|y| *y > 0) {
                    span { class: "rs-library-tile__year", "{year}" }
                }
            }
        }
    }
}

/// A cover grid of albums (2 columns on compact, auto-fill on wide).
/// * `ghosts`: placeholder tiles after the albums, for a paged grid whose
///   next page is on its way (a fling runs into tiles, not the end).
#[component]
pub fn AlbumGrid(
    albums: Vec<Album>,
    #[props(default = true)] show_artist: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] ghosts: usize,
) -> Element {
    rsx! {
        div { class: "rs-library-covers", role: "list", aria_label: label.clone(),
            for album in albums {
                div { key: "{album.server_id}:{album.id}", role: "listitem", class: "rs-library-covers__cell",
                    AlbumTile { album: album.clone(), show_artist }
                }
            }
            for index in 0..ghosts {
                div {
                    key: "ghost-{index}",
                    class: "rs-library-covers__cell rs-library-covers__ghost",
                    aria_hidden: "true",
                    div { class: "rs-library-tile",
                        div { class: "rs-skel rs-library-tile__skel" }
                        div { class: "rs-skel", style: "width:70%;height:12px;margin-top:8px" }
                        div { class: "rs-skel", style: "width:45%;height:10px;margin-top:6px" }
                    }
                }
            }
        }
    }
}

/// A round artist thumb with the name under it (similar artists, shelves).
#[component]
pub fn ArtistBubble(
    artist_id: String,
    server_id: String,
    name: String,
    #[props(default)] cover_art: Option<String>,
    #[props(default)] image_url: Option<String>,
) -> Element {
    let navigation = use_context::<Navigation>();
    let target = AppView::ArtistDetailView {
        artist_id: artist_id.clone(),
        server_id: server_id.clone(),
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-library-bubble",
            onclick: move |_| navigation.navigate_to(target.clone()),
            Artwork {
                cover_art: cover_art.clone(),
                server_id: server_id.clone(),
                src: image_url.clone(),
                size_px: 76,
                round: true,
                placeholder: "artist".to_string(),
            }
            span { class: "rs-library-bubble__name", "{name}" }
        }
    }
}
