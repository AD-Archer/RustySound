//! iOS-style on/off switch.
//!
//! Styles live in `views/settings.css` (`.rs-switch`) for now; they belong in
//! `components.css` and should move there when the kit is next touched.

use dioxus::prelude::*;

/// A `role="switch"` button.
/// * `checked`: current state. `on_toggle`: called with the NEW state.
/// * `label`: accessible name (use the visible row title). Alternatively pass
///   `labelled_by` with the id of the visible title.
/// * `disabled`, `id`, `testid`.
#[component]
pub fn Switch(
    checked: bool,
    on_toggle: EventHandler<bool>,
    #[props(default)] label: Option<String>,
    #[props(default)] labelled_by: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] testid: Option<String>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "rs-switch",
            role: "switch",
            id: id.clone(),
            "data-testid": testid.clone(),
            aria_checked: if checked { "true" } else { "false" },
            aria_label: label.clone(),
            aria_labelledby: labelled_by.clone(),
            disabled,
            onclick: move |_| on_toggle.call(!checked),
            span { class: "rs-switch__knob", aria_hidden: "true" }
        }
    }
}
