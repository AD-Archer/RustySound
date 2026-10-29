//! Mounts the open track menu: bottom sheet (compact) or popover (wide).

use crate::components::ui::env::use_layout;
use crate::components::ui::{TrackMenu, TrackMenuController};
use dioxus::prelude::*;

/// Mounts the open track menu, if any: a bottom sheet on compact layouts and
/// an anchored popover on wide ones. Rendered once by the shell.
#[component]
pub fn TrackMenuHost() -> Element {
    let mut controller = use_context::<TrackMenuController>();
    let layout = use_layout();
    let Some(request) = (controller.request)() else {
        return rsx! {};
    };
    let key = format!("{}:{}", request.song.server_id, request.song.id);

    if layout.is_compact() {
        return rsx! {
            div {
                class: "rs-scrim",
                aria_hidden: "true",
                onclick: move |_| controller.close(),
            }
            div { class: "rs-sheet rs-sheet--menu", role: "dialog", aria_label: "{request.song.title}",
                TrackMenu { key: "{key}", song: request.song.clone(), show_head: true }
                div { class: "rs-menu",
                    button {
                        r#type: "button",
                        class: "rs-menu__item rs-menu__cancel",
                        onclick: move |_| controller.close(),
                        "Cancel"
                    }
                }
            }
        };
    }

    let style = match request.anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, {x:.0}px, calc(100vw - 278px));top:clamp(8px, {y:.0}px, calc(100dvh - 480px))"
        ),
        None => "left:50%;top:50%;transform:translate(-50%,-50%)".to_string(),
    };
    rsx! {
        TrackMenu { key: "{key}", song: request.song.clone(), popover_style: style }
        div {
            class: "rs-scrim rs-scrim--clear",
            aria_hidden: "true",
            onclick: move |_| controller.close(),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                controller.close();
            },
        }
    }
}
