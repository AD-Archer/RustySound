//! 2×2 cover mosaic for mixes and playlists.

use crate::api::ServerConfig;
use crate::components::ui::env::cover_art_src;
use crate::components::ui::Artwork;
use dioxus::prelude::*;

/// Four covers in a 2×2 grid (mixes). Falls back to a single `Artwork` when
/// fewer than four covers are known.
/// * `covers`: `(server_id, cover_art_id)` pairs; the first four are used.
/// * `size_px`: edge of the whole mosaic. `alt`: text for the group.
#[component]
pub fn ArtworkMosaic(
    covers: Vec<(String, String)>,
    #[props(default = 44)] size_px: u32,
    #[props(default)] alt: String,
) -> Element {
    let servers = try_use_context::<Signal<Vec<ServerConfig>>>();
    if covers.len() < 4 {
        let first = covers.first().cloned();
        return rsx! {
            Artwork {
                server_id: first.as_ref().map(|c| c.0.clone()).unwrap_or_default(),
                cover_art: first.map(|c| c.1),
                size_px,
                alt: alt.clone(),
                placeholder: "playlist".to_string(),
            }
        };
    }
    let tile = (size_px / 2).max(1);
    let urls: Vec<Option<String>> = covers
        .iter()
        .take(4)
        .map(|(server_id, cover)| {
            servers.and_then(|servers| {
                cover_art_src(&servers.read(), server_id, Some(cover), tile * 2)
            })
        })
        .collect();
    rsx! {
        div {
            class: "rs-mosaic",
            // Decorative without `alt`: role="img" with an empty name fails axe role-img-alt.
            role: if !alt.is_empty() { Some("img") } else { None },
            aria_label: if !alt.is_empty() { Some(alt.clone()) } else { None },
            aria_hidden: if alt.is_empty() { Some("true") } else { None },
            style: "width:{size_px}px;height:{size_px}px",
            for (index, url) in urls.into_iter().enumerate() {
                if let Some(url) = url {
                    img {
                        key: "{index}",
                        src: "{url}",
                        alt: "",
                        width: "{tile}",
                        height: "{tile}",
                        loading: "lazy",
                        decoding: "async",
                        draggable: "false",
                    }
                } else {
                    span { key: "{index}" }
                }
            }
        }
    }
}
