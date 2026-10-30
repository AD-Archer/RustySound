//! Empty and error state block.

use crate::components::Icon;
use dioxus::prelude::*;

/// Centered empty state.
/// * `icon`: icon name shown in the tinted circle. `title`: one line.
/// * `message`: what happened and what to do next.
/// * `children`: action buttons (usually `PillButton`s).
#[component]
pub fn EmptyState(
    #[props(default = "music".to_string())] icon: String,
    title: String,
    #[props(default)] message: Option<String>,
    #[props(default)] testid: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: "rs-empty", "data-testid": testid.clone(),
            div { class: "rs-empty__icon",
                Icon { name: icon.clone(), class: "rs-ic rs-ic--lg".to_string() }
            }
            h2 { class: "rs-empty__title", "{title}" }
            if let Some(message) = message.clone() {
                p { class: "rs-empty__msg", "{message}" }
            }
            div { class: "rs-empty__actions", {children} }
        }
    }
}
