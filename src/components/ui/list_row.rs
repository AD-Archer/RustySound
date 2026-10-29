//! A row inside an `InsetList`.

use crate::components::Icon;
use dioxus::prelude::*;

/// One list row (`li`). Clickable when `onclick` is set (renders a button).
/// * `title`, `subtitle`, `why`: the three text lines (why = faint reason line).
/// * `leading`: element before the text (an `Artwork`), or use `icon` for an
///   accent-coloured icon in that slot.
/// * `detail`: muted trailing text (counts, durations; tabular numerals).
/// * `chevron`: disclosure chevron. `fresh`: blue "updated" dot after the title.
/// * `trailing`: element at the end (play button, pill). Only put buttons here
///   when the row has no `onclick` (no nested buttons).
/// * `compact`: 44px minimum height instead of 58.
#[component]
pub fn ListRow(
    title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] why: Option<String>,
    #[props(default)] leading: Option<Element>,
    #[props(default)] icon: Option<String>,
    #[props(default)] detail: Option<String>,
    #[props(default)] chevron: bool,
    #[props(default)] fresh: bool,
    #[props(default)] trailing: Option<Element>,
    #[props(default)] compact: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let class = if compact {
        "rs-row rs-row--compact"
    } else {
        "rs-row"
    };
    let body = rsx! {
        if let Some(name) = icon.clone() {
            span { class: "rs-row__lead",
                Icon { name, class: "rs-ic rs-ic--md".to_string() }
            }
        }
        if let Some(lead) = leading.clone() {
            {lead}
        }
        span { class: "rs-row__meta",
            span { class: "rs-row__title",
                span { "{title}" }
                if fresh {
                    span { class: "rs-row__fresh", role: "img", aria_label: "Updated" }
                }
            }
            if let Some(sub) = subtitle.clone() {
                span { class: "rs-row__sub", style: "display:block", "{sub}" }
            }
            if let Some(why) = why.clone() {
                span { class: "rs-row__why", style: "display:block", "{why}" }
            }
        }
        if let Some(detail) = detail.clone() {
            span { class: "rs-row__detail", "{detail}" }
        }
        if let Some(trail) = trailing.clone() {
            {trail}
        }
        if chevron {
            span { class: "rs-row__chev",
                Icon { name: "chevron-right".to_string(), class: "rs-ic rs-ic--sm".to_string() }
            }
        }
    };
    rsx! {
        li { "data-testid": testid.clone(),
            if let Some(handler) = onclick {
                button {
                    r#type: "button",
                    class: "{class}",
                    onclick: move |evt| handler.call(evt),
                    {body}
                }
            } else {
                div { class: "{class}", {body} }
            }
        }
    }
}
