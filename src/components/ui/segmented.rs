//! Segmented control.

use dioxus::prelude::*;

/// iOS segmented control; segments expose `aria-pressed`.
/// * `options`: segment labels. `selected`: active index.
/// * `on_select`: called with the tapped index.
/// * `stage`: dark variant for the stage. `inline`: shrink-to-fit (Mac toolbar).
/// * `label`: aria-label of the group.
#[component]
pub fn Segmented(
    options: Vec<String>,
    selected: usize,
    on_select: EventHandler<usize>,
    #[props(default)] stage: bool,
    #[props(default)] inline: bool,
    #[props(default)] label: Option<String>,
    #[props(default)] testid: Option<String>,
) -> Element {
    rsx! {
        div {
            class: "rs-seg",
            class: if stage { "rs-seg--stage" },
            class: if inline { "rs-seg--inline" },
            role: "group",
            aria_label: label.clone(),
            "data-testid": testid.clone(),
            for (index, option) in options.iter().enumerate() {
                button {
                    key: "{index}",
                    r#type: "button",
                    class: "rs-seg__btn",
                    aria_pressed: if index == selected { "true" } else { "false" },
                    onclick: move |_| on_select.call(index),
                    "{option}"
                }
            }
        }
    }
}
