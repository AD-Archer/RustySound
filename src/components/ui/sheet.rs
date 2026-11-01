//! Sheet: bottom sheet on compact layouts, centred panel on wide ones; and
//! `MenuSheet`, the compact action sheet the menus share. On compact, both
//! can be swiped down to close (`sheet_swipe.js`).

use crate::components::ui::{IconButton, IconButtonSize};
use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

const SHEET_SWIPE_JS: &str = concat!(
    include_str!("gesture_core.js"),
    "\n",
    include_str!("sheet_swipe.js")
);

static NEXT_SHEET: AtomicU64 = AtomicU64::new(1);

/// Swipe-to-dismiss for one open sheet: returns the token to write as the
/// sheet element's `data-sheet-swipe`; a finished swipe calls `on_close`.
fn use_sheet_swipe(on_close: EventHandler<()>) -> u64 {
    let token = use_hook(|| NEXT_SHEET.fetch_add(1, Ordering::Relaxed));
    use_hook(move || {
        let mut script = document::eval(&SHEET_SWIPE_JS.replace("__SHEET__", &token.to_string()));
        spawn(async move {
            while let Ok(message) = script.recv::<String>().await {
                if message == "close" {
                    on_close.call(());
                }
            }
        });
    });
    use_drop(move || {
        let _ = document::eval(&format!(
            "window.__rsSheetSwipe && window.__rsSheetSwipe.unregister({token});"
        ));
    });
    token
}

/// Modal sheet with a scrim. The parent decides whether it is mounted.
/// * `on_close`: scrim tap, Escape, the close button, or (compact) a swipe
///   down.
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
    let swipe = use_sheet_swipe(on_close);
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
            "data-sheet-swipe": "{swipe}",
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

/// The compact action sheet behind `Menu` and the track menu: a scrim, the
/// menu card(s) given as children, and a separate Cancel card. Scrim, Cancel
/// and a swipe down call `on_close`. The wide layout uses popovers instead.
/// * `label`: the dialog's aria-label (the menu's subject).
#[component]
pub fn MenuSheet(label: String, on_close: EventHandler<()>, children: Element) -> Element {
    let swipe = use_sheet_swipe(on_close);
    rsx! {
        div {
            class: "rs-scrim",
            aria_hidden: "true",
            onclick: move |_| on_close.call(()),
        }
        div {
            class: "rs-sheet rs-sheet--menu",
            role: "dialog",
            aria_label: "{label}",
            "data-sheet-swipe": "{swipe}",
            div {
                class: "rs-sheet__head rs-sheet__head--menu",
                span { class: "rs-sheet__title", "{label}" }
                button {
                    r#type: "button",
                    class: "rs-iconbtn rs-iconbtn--quiet",
                    aria_label: "Close",
                    onclick: move |_| on_close.call(()),
                    crate::components::Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            }
            {children}
            div { class: "rs-menu",
                button {
                    r#type: "button",
                    class: "rs-menu__item rs-menu__cancel",
                    onclick: move |_| on_close.call(()),
                    "Cancel"
                }
            }
        }
    }
}
