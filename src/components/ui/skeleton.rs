//! Loading placeholders.

use dioxus::prelude::*;

/// Shimmering placeholder.
/// * `rows`: when > 0, that many list-row skeletons (thumb + two lines) inside
///   an inset list; otherwise one block of `width` × `height` (CSS lengths).
/// * `round`: round thumbs (artists).
#[component]
pub fn Skeleton(
    #[props(default)] rows: usize,
    #[props(default = "100%".to_string())] width: String,
    #[props(default = "16px".to_string())] height: String,
    #[props(default)] round: bool,
) -> Element {
    if rows == 0 {
        return rsx! {
            div {
                class: "rs-skel",
                aria_hidden: "true",
                style: "width:{width};height:{height}",
            }
        };
    }
    let thumb_radius = if round { "50%" } else { "4px" };
    rsx! {
        ul { class: "rs-list", aria_busy: "true", aria_label: "Loading",
            for index in 0..rows {
                li { key: "{index}",
                    div { class: "rs-skel-row",
                        div {
                            class: "rs-skel",
                            style: "width:44px;height:44px;border-radius:{thumb_radius}",
                        }
                        div { class: "rs-skel-row__lines",
                            div {
                                class: "rs-skel",
                                style: "width:{55 + (index * 17) % 30}%;height:12px",
                            }
                            div {
                                class: "rs-skel",
                                style: "width:{30 + (index * 11) % 25}%;height:10px",
                            }
                        }
                    }
                }
            }
        }
    }
}
