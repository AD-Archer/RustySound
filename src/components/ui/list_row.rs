//! A row inside an `InsetList`.

use crate::components::ui::{IconButton, IconButtonSize};
use crate::components::Icon;
use dioxus::prelude::*;

/// Where a row or tile's menu should open, from the events that open it: a
/// right-click or long-press point, the "…" button (the popover opens under
/// it, to its left) or the keyboard (no point: centred).
pub(crate) fn menu_point(evt: &MouseEvent) -> (f64, f64) {
    let p = evt.client_coordinates();
    (p.x, p.y)
}

pub(crate) fn more_button_point(evt: &MouseEvent) -> (f64, f64) {
    let p = evt.client_coordinates();
    (p.x - 250.0, p.y + 12.0)
}

/// True for the keys that open a context menu (the menu key, Shift+F10).
pub(crate) fn is_menu_key(evt: &KeyboardEvent) -> bool {
    let key = evt.key();
    key == Key::ContextMenu || (key == Key::F10 && evt.modifiers().shift())
}

/// One list row (`li`). Clickable when `onclick` is set (renders a button).
/// * `title`, `subtitle`, `why`: the three text lines (why = faint reason line).
/// * `sub_line`: a richer second line (marks, a live label); replaces `subtitle`.
/// * `leading`: element before the text (an `Artwork`), or use `icon` for an
///   accent-coloured icon in that slot.
/// * `detail`: muted trailing text (counts, durations; tabular numerals).
/// * `chevron`: disclosure chevron. `fresh`: blue "updated" dot after the title.
/// * `trailing`: element at the end (play button, pill). Only put buttons here
///   when the row has no `onclick` (no nested buttons).
/// * `compact`: 44px minimum height instead of 58.
/// * `current`: marks the row `aria-current` (the playing station).
/// * `on_menu`: the row has a menu. It opens on a touch long-press (450 ms,
///   `row_gestures.js`), a right-click, the context-menu key / Shift+F10, and
///   a "…" button beside the row; the handler gets the point to anchor the
///   popover at (None from the keyboard). `more_label` names that button
///   (default "More for <title>"); its test id is `<testid>-more`.
/// * `testid`: on the `li`.
#[component]
pub fn ListRow(
    title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] sub_line: Option<Element>,
    #[props(default)] why: Option<String>,
    #[props(default)] leading: Option<Element>,
    #[props(default)] icon: Option<String>,
    #[props(default)] detail: Option<String>,
    #[props(default)] chevron: bool,
    #[props(default)] fresh: bool,
    #[props(default)] trailing: Option<Element>,
    #[props(default)] compact: bool,
    #[props(default)] current: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] on_menu: Option<EventHandler<Option<(f64, f64)>>>,
    #[props(default)] more_label: Option<String>,
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
            if let Some(line) = sub_line.clone() {
                span { class: "rs-row__sub", style: "display:block", {line} }
            } else if let Some(sub) = subtitle.clone() {
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
    let current = current.then_some("true");

    let Some(on_menu) = on_menu else {
        return rsx! {
            li { "data-testid": testid.clone(),
                if let Some(handler) = onclick {
                    button {
                        r#type: "button",
                        class: "{class}",
                        aria_current: current,
                        onclick: move |evt| handler.call(evt),
                        {body}
                    }
                } else {
                    div { class: "{class}", aria_current: current, {body} }
                }
            }
        };
    };

    // With a menu: the row and its "…" button side by side (no nesting).
    let more_label = more_label
        .clone()
        .unwrap_or_else(|| format!("More for {title}"));
    rsx! {
        li { "data-testid": testid.clone(),
            div { class: "rs-row-split",
                button {
                    r#type: "button",
                    class: "{class}",
                    aria_current: current,
                    "data-long-press": "true",
                    onclick: move |evt| {
                        if let Some(handler) = onclick {
                            handler.call(evt);
                        }
                    },
                    oncontextmenu: move |evt: MouseEvent| {
                        evt.prevent_default();
                        on_menu.call(Some(menu_point(&evt)));
                    },
                    onkeydown: move |evt: KeyboardEvent| {
                        if is_menu_key(&evt) {
                            evt.prevent_default();
                            on_menu.call(None);
                        }
                    },
                    {body}
                }
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: more_label,
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: testid.as_ref().map(|id| format!("{id}-more")),
                    onclick: move |evt: MouseEvent| on_menu.call(Some(more_button_point(&evt))),
                }
            }
        }
    }
}
