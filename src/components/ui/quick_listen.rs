//! The capsule shown while a library song's Quick Listen plays.

use crate::components::ui::{Artwork, PreviewRing};
use crate::components::{Icon, Preview};
use dioxus::prelude::*;

/// Floats above the player while a Quick Listen (track menu) plays: the
/// song, and a stop button carrying the clip's progress ring. Discover's
/// off-server previews show their ring on the card instead. Rendered once by
/// the shell; renders nothing otherwise.
#[component]
pub fn QuickListenCapsule() -> Element {
    let preview = use_context::<Preview>();
    let Some(song) = (preview.song)() else {
        return rsx! {};
    };
    rsx! {
        div {
            class: "rs-capsule rs-capsule--preview",
            role: "status",
            "data-testid": "quick-listen",
            Artwork {
                cover_art: song.cover_art.clone(),
                server_id: song.server_id.clone(),
                size_px: 28,
            }
            span { class: "rs-capsule__text",
                span { class: "rs-capsule__label", "Quick Listen" }
                "{song.title}"
            }
            button {
                r#type: "button",
                class: "rs-capsule__stop",
                aria_label: "Stop Quick Listen",
                title: "Stop",
                "data-testid": "quick-listen-stop",
                onclick: move |_| preview.stop(true),
                Icon { name: "pause".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                PreviewRing {}
            }
        }
    }
}
