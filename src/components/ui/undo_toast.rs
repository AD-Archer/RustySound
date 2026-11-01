//! The Undo toast shown after a song is removed from a list: the kit's
//! capsule look (a 10px rounded rectangle on the stage colour, never a
//! pill), floating above the player, with an accent "Undo" text button.

use crate::components::ui::env::sleep_ms;
use dioxus::prelude::*;

/// How long the toast stays up.
pub const UNDO_TOAST_MS: u64 = 5_000;

/// Mount while there is something to undo; it calls `on_close` after
/// [`UNDO_TOAST_MS`]. `generation` names the removal it is for: a new
/// removal (a new generation) restarts the timer.
#[component]
pub fn UndoToast(
    message: String,
    generation: u64,
    on_undo: EventHandler<()>,
    on_close: EventHandler<()>,
) -> Element {
    let mut shown = use_signal(|| 0u64);
    use_effect(use_reactive!(|generation| {
        shown.set(generation);
        spawn(async move {
            sleep_ms(UNDO_TOAST_MS).await;
            if *shown.peek() == generation {
                on_close.call(());
            }
        });
    }));
    rsx! {
        div {
            class: "rs-capsule rs-capsule--undo",
            role: "status",
            "data-testid": "undo-toast",
            span { class: "rs-capsule__text", "{message}" }
            button {
                r#type: "button",
                class: "rs-capsule__action",
                "data-testid": "undo-toast-undo",
                onclick: move |_| {
                    shown.set(0);
                    on_undo.call(());
                },
                "Undo"
            }
        }
    }
}
