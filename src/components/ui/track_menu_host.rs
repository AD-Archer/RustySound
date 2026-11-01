//! Mounts the open track menu: bottom sheet (compact) or popover (wide).

use crate::components::ui::env::use_layout;
use crate::components::ui::song_drop::use_song_drop_host;
use crate::components::ui::{MenuSheet, TrackMenu, TrackMenuController};
use dioxus::prelude::*;

/// Mounts the open track menu, if any: a bottom sheet on compact layouts and
/// an anchored popover on wide ones. Rendered once by the shell, so it also
/// installs the app's row gestures (long-press, browse swipes, carry drops),
/// which open this menu.
#[component]
pub fn TrackMenuHost() -> Element {
    use_song_drop_host();
    let mut controller = use_context::<TrackMenuController>();

    let layout = use_layout();
    let Some(request) = (controller.request)() else {
        return rsx! {};
    };
    let key = format!(
        "{}:{}:{:?}",
        request.song.server_id, request.song.id, request.player
    );

    if layout.is_compact() {
        return rsx! {
            MenuSheet { label: request.song.title.clone(), on_close: move |_| controller.close(),
                TrackMenu {
                    key: "{key}",
                    song: request.song.clone(),
                    show_head: true,
                    remove: request.remove.clone(),
                    player: request.player,
                }
            }
        };
    }

    let style = match request.anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, min({x:.0}px, calc(100vw - 286px)), calc(100vw - 286px));top:clamp(8px, min({y:.0}px, calc(100dvh - 520px)), calc(100dvh - 520px))"
        ),
        None => "left:50%;top:50%;transform:translate(-50%,-50%)".to_string(),
    };
    rsx! {
        TrackMenu {
            key: "{key}",
            song: request.song.clone(),
            popover_style: style,
            remove: request.remove.clone(),
            player: request.player,
        }
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
