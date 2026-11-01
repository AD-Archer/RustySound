//! Cover tiles for grids: albums (square art) and artists (round art). Both
//! use the shared library tile classes (`.rs-library-tile*`, library.css)
//! and open their page when clicked.

use crate::api::{Album, Artist};
use crate::components::ui::list_row::{is_menu_key, menu_point};
use crate::components::ui::Artwork;
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

/// Context-menu wiring shared by the tiles: right-click and long-press
/// (`data-long-press`, row_gestures.js) open at the point, the menu key and
/// Shift+F10 open centred.
fn tile_menu_attrs(
    on_menu: Option<EventHandler<Option<(f64, f64)>>>,
) -> (
    Option<&'static str>,
    impl FnMut(MouseEvent) + 'static,
    impl FnMut(KeyboardEvent) + 'static,
) {
    let on_context = move |evt: MouseEvent| {
        if let Some(handler) = on_menu {
            evt.prevent_default();
            handler.call(Some(menu_point(&evt)));
        }
    };
    let on_key = move |evt: KeyboardEvent| {
        if let (Some(handler), true) = (on_menu, is_menu_key(&evt)) {
            evt.prevent_default();
            handler.call(None);
        }
    };
    (on_menu.map(|_| "true"), on_context, on_key)
}

/// One album in a cover grid: square art, title, artist, year, and a small
/// "On This Device" mark for the device's own albums (which fade in the first
/// time they're shown after the scan that found them). Opens the album.
/// * `show_artist`: false on an artist's own page.
/// * `on_menu`: the tile has a menu (long-press, right-click, menu key);
///   gets the point to anchor it at, None from the keyboard.
#[component]
pub fn AlbumTile(
    album: Album,
    #[props(default = true)] show_artist: bool,
    #[props(default)] on_menu: Option<EventHandler<Option<(f64, f64)>>>,
) -> Element {
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
    let local = crate::local_music::is_local(&album.server_id);
    let fresh = use_hook(|| local && crate::local_music::take_fresh(&album.id));
    let (long_press, on_context, on_key) = tile_menu_attrs(on_menu);
    rsx! {
        button {
            r#type: "button",
            class: "rs-library-tile",
            class: if fresh { "rs-fresh" },
            "data-testid": "album-tile",
            "data-long-press": long_press,
            onclick: move |_| navigation.navigate_to(target.clone()),
            oncontextmenu: on_context,
            onkeydown: on_key,
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
                if local {
                    span {
                        class: "rs-library-tile__src",
                        role: "img",
                        aria_label: crate::local_music::LOCAL_SOURCE_NAME,
                        title: crate::local_music::LOCAL_SOURCE_NAME,
                        Icon { name: "hard-drive".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                    }
                }
            }
        }
    }
}

/// One artist in a cover grid: round art, name, album count. Opens the
/// artist. `on_menu` as on `AlbumTile`.
#[component]
pub fn ArtistTile(
    artist: Artist,
    #[props(default)] on_menu: Option<EventHandler<Option<(f64, f64)>>>,
) -> Element {
    let navigation = use_context::<Navigation>();
    let target = AppView::ArtistDetailView {
        artist_id: artist.id.clone(),
        server_id: artist.server_id.clone(),
    };
    let albums = match artist.album_count {
        0 => String::new(),
        1 => "1 album".to_string(),
        n => format!("{n} albums"),
    };
    let (long_press, on_context, on_key) = tile_menu_attrs(on_menu);
    rsx! {
        button {
            r#type: "button",
            class: "rs-library-tile rs-library-tile--round",
            "data-testid": "artist-tile",
            "data-long-press": long_press,
            onclick: move |_| navigation.navigate_to(target.clone()),
            oncontextmenu: on_context,
            onkeydown: on_key,
            div { class: "rs-library-tile__art",
                Artwork {
                    cover_art: artist.cover_art.clone(),
                    server_id: artist.server_id.clone(),
                    size_px: 136,
                    fill: true,
                    round: true,
                    placeholder: "artist".to_string(),
                }
            }
            span { class: "rs-library-tile__title", "{artist.name}" }
            if !albums.is_empty() {
                span { class: "rs-library-tile__sub", "{albums}" }
            }
        }
    }
}
