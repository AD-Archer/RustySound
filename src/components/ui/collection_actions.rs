//! The action row under an album, playlist or mix header: Play, Shuffle and
//! an optional "…".

use crate::components::ui::{Button, ButtonVariant, IconButton, IconButtonSize};
use dioxus::prelude::*;

/// Play and Shuffle (equal width on compact, 168px on wide), then extras
/// (`children`, e.g. a Download button) and an optional "…".
/// * `testid`: prefix for the buttons' test ids (`<testid>-play`,
///   `<testid>-shuffle`, `<testid>-more`): "album", "playlist", "mix".
/// * `disabled`: nothing to play (an empty playlist).
/// * `on_more`: shows the "…" button, labelled `more_label`.
#[component]
pub fn CollectionActions(
    testid: String,
    on_play: EventHandler<MouseEvent>,
    on_shuffle: EventHandler<MouseEvent>,
    #[props(default)] disabled: bool,
    #[props(default)] on_more: Option<EventHandler<MouseEvent>>,
    #[props(default = "More".to_string())] more_label: String,
    children: Element,
) -> Element {
    rsx! {
        div { class: "rs-collection-actions",
            Button {
                label: "Play".to_string(),
                icon: "play".to_string(),
                block: true,
                disabled,
                testid: format!("{testid}-play"),
                onclick: move |evt| on_play.call(evt),
            }
            Button {
                label: "Shuffle".to_string(),
                icon: "shuffle".to_string(),
                variant: ButtonVariant::Tinted,
                block: true,
                disabled,
                testid: format!("{testid}-shuffle"),
                onclick: move |evt| on_shuffle.call(evt),
            }
            {children}
            if let Some(handler) = on_more {
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: more_label.clone(),
                    size: IconButtonSize::Medium,
                    testid: format!("{testid}-more"),
                    onclick: move |evt| handler.call(evt),
                }
            }
        }
    }
}
