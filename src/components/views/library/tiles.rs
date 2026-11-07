//! The windowed album grid and the round artist bubbles. The tiles
//! themselves are the kit's (`ui::AlbumTile`, `ui::ArtistTile`).

use crate::api::Album;
use crate::components::ui::{use_layout, AlbumTile, Artwork, VirtualList};
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

/// Narrowest tile on wide windows (`.rs-library-covers` auto-fills these).
const WIDE_MIN_TILE_PX: f64 = 164.0;
/// Gaps between tiles, matching `.rs-library-covers` (compact, wide):
/// across (`--sp-6`, `--sp-7`) and down (`--sp-7`, `--sp-8`).
const GAP_X: (f64, f64) = (16.0, 20.0);
const GAP_Y: (f64, f64) = (20.0, 24.0);
/// The two text lines under the art (title, artist and year), with their
/// margins. Both are one line (they end in an ellipsis), so every row of
/// tiles is the same height.
const TILE_TEXT_PX: f64 = 48.0;
/// Width to lay out with before the grid has measured itself.
const GUESS_PX: (f64, f64) = (358.0, 1000.0);

/// Columns, and the height of one row of tiles (gap included), for a grid
/// `width` px wide.
fn grid_metrics(width: f64, wide: bool) -> (usize, f64) {
    let (gap_x, gap_y) = if wide { (GAP_X.1, GAP_Y.1) } else { (GAP_X.0, GAP_Y.0) };
    let cols = if wide {
        (((width + gap_x) / (WIDE_MIN_TILE_PX + gap_x)).floor() as usize).max(1)
    } else {
        2
    };
    let tile = ((width - gap_x * (cols as f64 - 1.0)) / cols as f64).max(1.0);
    (cols, (tile + TILE_TEXT_PX + gap_y).round())
}

/// A cover grid of albums (2 columns on compact, auto-fill on wide), windowed
/// by rows with the kit's `VirtualList`: only the rows near the viewport are
/// mounted, so a library of 20,000 albums costs a few screens of tiles.
/// * `albums`: pass the list's signal (no copy per render) or a plain list.
/// * `ghosts`: placeholder tiles after the albums, for a paged grid whose
///   next page is on its way (a fling runs into tiles, not the end).
#[component]
pub fn AlbumGrid(
    albums: ReadSignal<Vec<Album>>,
    #[props(default = true)] show_artist: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] ghosts: usize,
) -> Element {
    let wide = !use_layout().is_compact();
    let mut width = use_signal(|| 0.0f64);
    let measured = width();
    let width_now = if measured > 0.0 {
        measured
    } else if wide {
        GUESS_PX.1
    } else {
        GUESS_PX.0
    };
    let (cols, row_height) = grid_metrics(width_now, wide);
    let cells = albums.read().len() + ghosts;
    let rows = cells.div_ceil(cols);

    rsx! {
        div {
            class: "rs-library-grid",
            role: label.as_ref().map(|_| "region"),
            aria_label: label.clone(),
            onresize: move |evt: Event<ResizeData>| {
                let size = evt.get_content_box_size().or_else(|_| evt.get_border_box_size());
                if let Ok(size) = size {
                    // Whole pixels: sub-pixel jitter must not re-render.
                    let next = size.width.floor();
                    if next > 0.0 && next != *width.peek() {
                        width.set(next);
                    }
                }
            },
            VirtualList {
                item_count: rows,
                row_height,
                class: "rs-library-covers--windowed".to_string(),
                testid: "album-grid".to_string(),
                render_row: move |row: usize| {
                    let list = albums.read();
                    let first = row * cols;
                    let last = (first + cols).min(cells);
                    rsx! {
                        li {
                            class: "rs-library-covers rs-library-covers__row",
                            style: "height:{row_height}px;grid-template-columns:repeat({cols},minmax(0,1fr))",
                            for index in first..last {
                                if let Some(album) = list.get(index) {
                                    div {
                                        key: "{album.server_id}:{album.id}",
                                        class: "rs-library-covers__cell",
                                        AlbumTile { album: album.clone(), show_artist }
                                    }
                                } else {
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
                },
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

#[cfg(test)]
mod tests {
    use super::grid_metrics;

    #[test]
    fn compact_grids_have_two_columns() {
        let (cols, row) = grid_metrics(358.0, false);
        assert_eq!(cols, 2);
        // (358 - 16) / 2 = 171 art + text + gap.
        assert_eq!(row, (171.0 + 48.0 + 20.0f64).round());
    }

    #[test]
    fn wide_grids_fill_with_tiles_at_least_164px() {
        assert_eq!(grid_metrics(1000.0, true).0, 5);
        assert_eq!(grid_metrics(1100.0, true).0, 6);
        // Never fewer than one column, however narrow.
        assert_eq!(grid_metrics(100.0, true).0, 1);
        let (cols, row) = grid_metrics(1100.0, true);
        let tile = (1100.0 - 20.0 * (cols as f64 - 1.0)) / cols as f64;
        assert!(tile >= 164.0);
        assert_eq!(row, (tile + 48.0 + 24.0).round());
    }
}
