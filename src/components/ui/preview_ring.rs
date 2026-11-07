//! Thin progress ring around the playing preview's button.

use dioxus::prelude::*;

/// Mount inside the button of the item previewing now (the button must be
/// `position: relative` or absolutely placed). It draws from CSS state the
/// preview script publishes (`--rs-preview-p`, `data-rs-preview` on
/// `<html>`, see `audio_manager/preview.rs`), so it never re-renders while
/// the clip plays; it breathes quietly while the clip buffers.
#[component]
pub fn PreviewRing() -> Element {
    rsx! {
        svg {
            class: "rs-pring",
            "viewBox": "0 0 36 36",
            "aria-hidden": "true",
            "data-testid": "preview-ring",
            circle { class: "rs-pring__track", cx: "18", cy: "18", r: "16.5" }
            circle {
                class: "rs-pring__bar",
                cx: "18",
                cy: "18",
                r: "16.5",
                "pathLength": "100",
            }
        }
    }
}
