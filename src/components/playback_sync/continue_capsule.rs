//! "Continue from iPhone · Song — 2:31": the kit's capsule, floating above
//! the player while another device's saved queue is on offer.

use super::{PlaybackSync, RestoreSignals};
use crate::components::ui::env::format_clock;
use crate::components::ui::CloseButton;
use dioxus::prelude::*;

/// Tapping the capsule loads the queue at its song and position, paused;
/// the cross hides it until the server's queue changes again. Renders
/// nothing while there's no offer.
#[component]
pub fn ContinueCapsule() -> Element {
    let sync = use_context::<PlaybackSync>();
    let signals = RestoreSignals::from_context();
    let Some(offer) = (sync.offer)() else {
        return rsx! {};
    };
    let title = offer.title();
    let at = format_clock(offer.queue.position_ms as f64 / 1000.0);

    rsx! {
        div {
            class: "rs-capsule rs-capsule--continue",
            role: "status",
            "data-testid": "continue-capsule",
            button {
                r#type: "button",
                class: "rs-capsule__main",
                "data-testid": "continue-capsule-accept",
                onclick: move |_| sync.accept(signals),
                span { class: "rs-capsule__text",
                    span { class: "rs-capsule__lead", "Continue from {offer.device}" }
                    " · {title} — {at}"
                }
            }
            CloseButton {
                label: "Dismiss".to_string(),
                testid: "continue-capsule-dismiss".to_string(),
                class: "rs-capsule__close".to_string(),
                onclick: move |_| sync.dismiss(),
            }
        }
    }
}
