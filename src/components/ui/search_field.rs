//! Rounded search field (the kit's `.rs-field`) with a clear button.

use crate::components::Icon;
use dioxus::prelude::*;

/// Search field. Fires `on_input` on every keystroke; debounce in the caller.
/// * `value`: current text. `placeholder`: also the accessible label.
/// * `testid`: data-testid of the `<input>`.
#[component]
pub fn SearchField(
    value: String,
    placeholder: String,
    on_input: EventHandler<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
) -> Element {
    let has_text = !value.is_empty();
    rsx! {
        label { class: "rs-field {class}",
            Icon { name: "search".to_string(), class: "rs-ic rs-ic--sm".to_string() }
            input {
                r#type: "search",
                value: "{value}",
                placeholder: "{placeholder}",
                aria_label: "{placeholder}",
                autocomplete: "off",
                spellcheck: "false",
                "data-testid": testid.clone(),
                oninput: move |evt| on_input.call(evt.value()),
                onkeydown: move |evt| {
                    if evt.key() == Key::Escape {
                        on_input.call(String::new());
                    }
                },
            }
            if has_text {
                button {
                    r#type: "button",
                    class: "rs-field__clear",
                    aria_label: "Clear search",
                    onclick: move |_| on_input.call(String::new()),
                    Icon { name: "x".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                }
            }
        }
    }
}
