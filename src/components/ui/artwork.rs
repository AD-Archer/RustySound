//! Cover art with fixed size, lazy loading and a placeholder.

use crate::api::ServerConfig;
use crate::components::ui::env::cover_art_src;
use crate::components::Icon;
use dioxus::prelude::*;

/// One image of cover art. Every `<img>` in the app should come through here.
/// * `cover_art` + `server_id`: Subsonic cover id on that server; the URL is
///   built here at `size_px * 2` (retina). The URL is only rebuilt when these
///   props change, so a parent that re-renders does not refetch.
/// * `src`: an explicit URL (off-server art, data URLs); wins over `cover_art`.
/// * `size_px`: rendered edge length; always written as width/height.
/// * `fill`: ignore `size_px` for layout and fill the parent box (heroes).
/// * `round`: circle (artists). `hero`: 3px radius instead of 4px.
/// * `alt`: alternative text ("" for decorative art next to a visible title).
/// * `eager`: load immediately (above-the-fold heroes); default is lazy.
/// * `placeholder`: icon name shown when there is no art or it fails.
#[component]
pub fn Artwork(
    #[props(default)] cover_art: Option<String>,
    #[props(default)] server_id: String,
    #[props(default)] src: Option<String>,
    #[props(default = 44)] size_px: u32,
    #[props(default)] fill: bool,
    #[props(default)] round: bool,
    #[props(default)] hero: bool,
    #[props(default)] alt: String,
    #[props(default)] eager: bool,
    #[props(default = "music".to_string())] placeholder: String,
    #[props(default)] class: String,
) -> Element {
    let servers = try_use_context::<Signal<Vec<ServerConfig>>>();
    let mut failed_url = use_signal(|| None::<String>);

    let url = match src.clone().filter(|value| !value.is_empty()) {
        Some(explicit) => Some(explicit),
        None => servers.and_then(|servers| {
            cover_art_src(
                &servers.read(),
                &server_id,
                cover_art.as_deref(),
                size_px.saturating_mul(2).max(64),
            )
        }),
    };
    let show_image = url.is_some() && *failed_url.read() != url;

    let box_style = if fill {
        "width:100%;height:100%".to_string()
    } else {
        format!("width:{size_px}px;height:{size_px}px")
    };
    let icon_px = (size_px / 2).clamp(14, 48);

    rsx! {
        div {
            class: "rs-art {class}",
            class: if round { "rs-art--round" },
            class: if hero { "rs-art--hero" },
            style: "{box_style}",
            if show_image {
                img {
                    src: url.clone().unwrap_or_default(),
                    alt: "{alt}",
                    width: "{size_px}",
                    height: "{size_px}",
                    loading: if eager { "eager" } else { "lazy" },
                    decoding: "async",
                    draggable: "false",
                    onerror: move |_| failed_url.set(url.clone()),
                }
            } else {
                div {
                    class: "rs-art__ph",
                    role: if alt.is_empty() { None } else { Some("img") },
                    aria_label: if alt.is_empty() { None } else { Some(alt.clone()) },
                    span { style: "width:{icon_px}px;height:{icon_px}px;display:block",
                        Icon {
                            name: placeholder.clone(),
                            class: "rs-art__ph-icon".to_string(),
                        }
                    }
                }
            }
        }
    }
}
