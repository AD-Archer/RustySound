//! Section title with an optional trailing action ("See All").

use dioxus::prelude::*;

/// 17/600 section title above a list.
/// * `action`: label of the trailing link-style button, shown when `on_action` is set.
/// * `testid`: data-testid of the action button.
#[component]
pub fn SectionHeader(
    title: String,
    #[props(default)] action: Option<String>,
    #[props(default)] on_action: Option<EventHandler<MouseEvent>>,
    #[props(default)] testid: Option<String>,
) -> Element {
    rsx! {
        div { class: "rs-hd",
            h2 { class: "rs-hd__title", "{title}" }
            if let (Some(label), Some(handler)) = (action.clone(), on_action) {
                button {
                    r#type: "button",
                    class: "rs-hd__action",
                    "data-testid": testid.clone(),
                    onclick: move |evt| handler.call(evt),
                    "{label}"
                }
            }
        }
    }
}
