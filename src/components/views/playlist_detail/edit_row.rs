//! A playlist row in edit mode: art and title, then Move Up / Move Down /
//! Remove. Same height as a `TrackRow` so the windowed list stays exact.

use crate::api::Song;
use crate::components::ui::{Artwork, IconButton, IconButtonSize};
use dioxus::prelude::*;

#[component]
pub fn EditRow(
    song: Song,
    index: usize,
    last: usize,
    on_move: EventHandler<(usize, usize)>,
    on_remove: EventHandler<usize>,
) -> Element {
    let sub = song.artist.clone().unwrap_or_default();
    rsx! {
        li { "data-index": "{index}",
            div { class: "rs-trow rs-trow--art rs-library-editrow", "data-testid": "playlist-edit-row",
                Artwork { cover_art: song.cover_art.clone(), server_id: song.server_id.clone(), size_px: 44 }
                div { class: "rs-trow__meta",
                    div { class: "rs-trow__title", "{song.title}" }
                    if !sub.is_empty() {
                        div { class: "rs-trow__sub", "{sub}" }
                    }
                }
                IconButton {
                    icon: "chevron-up".to_string(),
                    label: format!("Move {} up", song.title),
                    quiet: true,
                    size: IconButtonSize::Small,
                    disabled: index == 0,
                    testid: "playlist-move-up".to_string(),
                    onclick: move |_| on_move.call((index, index - 1)),
                }
                IconButton {
                    icon: "chevron-down".to_string(),
                    label: format!("Move {} down", song.title),
                    quiet: true,
                    size: IconButtonSize::Small,
                    disabled: index >= last,
                    testid: "playlist-move-down".to_string(),
                    onclick: move |_| on_move.call((index, index + 1)),
                }
                IconButton {
                    icon: "trash".to_string(),
                    label: format!("Remove {} from the playlist", song.title),
                    quiet: true,
                    size: IconButtonSize::Small,
                    class: "rs-library-editrow__remove".to_string(),
                    testid: "playlist-remove".to_string(),
                    onclick: move |_| on_remove.call(index),
                }
            }
        }
    }
}
