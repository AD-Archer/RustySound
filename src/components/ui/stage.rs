//! The black stage material, where artwork lives.

use dioxus::prelude::*;

/// Black stage container with the radial lift.
/// * `tall`: use the taller gradient (full-screen surfaces like Now Playing).
/// * `label`: optional aria-label; when set the stage is a labelled region.
/// * `class`: extra classes for the view's own layout.
#[component]
pub fn Stage(
    #[props(default)] tall: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    rsx! {
        section {
            class: "rs-stage {class}",
            class: if tall { "rs-stage--tall" },
            aria_label: label.clone(),
            {children}
        }
    }
}
