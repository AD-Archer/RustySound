//! Sheet: bottom sheet on compact layouts, centred panel on wide ones.

use crate::components::ui::{IconButton, IconButtonSize};
use dioxus::prelude::*;

/// Modal sheet with a scrim. The parent decides whether it is mounted.
/// * `on_close`: scrim tap, Escape, or the close button.
/// * `title`: header text; omit for a headerless sheet (menus).
/// * `menu`: transparent container for a `TrackMenu`-style card stack.
/// * `label`: aria-label when there is no title. `testid`: data-testid.
#[component]
pub fn Sheet(
    on_close: EventHandler<()>,
    #[props(default)] title: Option<String>,
    #[props(default)] menu: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] testid: Option<String>,
    children: Element,
) -> Element {
    let aria = title.clone().or(label.clone()).unwrap_or_default();
    rsx! {
        div {
            class: "rs-scrim",
            aria_hidden: "true",
            onclick: move |_| on_close.call(()),
        }
        div {
            class: "rs-sheet",
            class: if menu { "rs-sheet--menu" },
            role: "dialog",
            aria_modal: "true",
            aria_label: "{aria}",
            tabindex: "-1",
            "data-testid": testid.clone(),
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                }
            },
            if !menu {
                div { class: "rs-sheet__grab", aria_hidden: "true" }
            }
            if let Some(title) = title.clone() {
                div { class: "rs-sheet__head",
                    h2 { class: "rs-sheet__title", "{title}" }
                    IconButton {
                        icon: "x".to_string(),
                        label: "Close".to_string(),
                        quiet: true,
                        size: IconButtonSize::Small,
                        onclick: move |_| on_close.call(()),
                    }
                }
            }
            if menu {
                {children}
            } else {
                div { class: "rs-sheet__body", {children} }
            }
        }
    }
}
