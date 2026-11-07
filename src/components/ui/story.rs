//! Stories: full-screen cards shown one at a time (Wrapped).
//!
//! `Story` is the shell: a progress bar of segments, a header, the stage
//! with the current card, tap zones at both sides and a close button. Cards
//! change by swipe (left/right), tap or click on either side, arrow keys, or
//! a mouse drag; a swipe down, Escape or the close button closes it.
//!
//! The finger and mouse tracking is JavaScript (`story.js`, on top of
//! `gesture_core.js`), like the other gestures, so it runs in the iOS
//! WKWebView too and does no Rust work per move. Rust only acts on what the
//! script posts: "next", "prev" and "close".

use crate::components::ui::env::sleep_ms;
use crate::components::ui::{IconButton, IconButtonSize};
use crate::components::Icon;
use dioxus::prelude::*;

const STORY_JS: &str = concat!(include_str!("gesture_core.js"), "\n", include_str!("story.js"));
/// The outgoing card stays this long for its exit animation (story.js).
const LEAVE_MS: u64 = 420;

/// The story shell. The parent owns `index` and renders each card with
/// `card` (called for the current card, and the outgoing one while it
/// animates away, which is inert).
/// * `count`: how many cards there are now (it may grow once data loads).
/// * `header`: shown next to the close button (a picker, say).
#[component]
pub fn Story(
    count: usize,
    index: Signal<usize>,
    label: String,
    on_close: EventHandler<()>,
    card: Callback<usize, Element>,
    #[props(default = VNode::empty())] header: Element,
    #[props(default)] testid: Option<String>,
) -> Element {
    let mut index = index;
    let mut leaving = use_signal(|| None::<usize>);
    let mut forward = use_signal(|| true);
    let mut generation = use_signal(|| 0u64);
    // The count as the gesture task sees it.
    let mut total = use_signal(|| count);
    use_effect(use_reactive!(|count| total.set(count)));

    let mut step = move |delta: isize| {
        let current = *index.peek();
        let Some(next) = current.checked_add_signed(delta).filter(|n| *n < *total.peek()) else {
            return;
        };
        forward.set(delta > 0);
        leaving.set(Some(current));
        index.set(next);
        generation += 1;
        let mine = *generation.peek();
        spawn(async move {
            sleep_ms(LEAVE_MS).await;
            if *generation.peek() == mine {
                leaving.set(None);
            }
        });
    };

    use_hook(move || {
        let mut script = document::eval(STORY_JS);
        spawn(async move {
            while let Ok(message) = script.recv::<String>().await {
                match message.as_str() {
                    "next" => step(1),
                    "prev" => step(-1),
                    "close" => on_close.call(()),
                    _ => {}
                }
            }
        });
    });
    use_drop(|| {
        let _ = document::eval("window.__rsStory && window.__rsStory.setSender(null);");
    });

    let current = index().min(count.saturating_sub(1));
    let outgoing = leaving().filter(|old| *old != current && *old < count);
    // Ascending order, so neither card moves in the DOM when the other comes or goes.
    let mut shown: Vec<usize> = outgoing.into_iter().chain(std::iter::once(current)).collect();
    shown.sort_unstable();
    let compact_hint = if current == 0 { "Swipe or tap the right side for more." } else { "" };

    rsx! {
        div {
            class: "rs-story",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{label}",
            tabindex: "-1",
            "data-testid": testid.clone(),
            "data-story-index": "{current}",
            "data-story-count": "{count}",
            "data-story-dir": if forward() { "next" } else { "prev" },
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
                let _ = document::eval("window.__rsStory && window.__rsStory.mounted();");
            },
            onkeydown: move |evt| match evt.key() {
                Key::ArrowRight => {
                    evt.prevent_default();
                    step(1);
                }
                Key::ArrowLeft => {
                    evt.prevent_default();
                    step(-1);
                }
                Key::Escape => {
                    evt.prevent_default();
                    on_close.call(());
                }
                _ => {}
            },
            div { class: "rs-story__frame",
                div { class: "rs-story__top",
                    StoryProgress { count, index: current }
                    div { class: "rs-story__bar",
                        div { class: "rs-story__header", {header} }
                        IconButton {
                            icon: "x".to_string(),
                            label: format!("Close {label}"),
                            size: IconButtonSize::Small,
                            testid: "story-close".to_string(),
                            onclick: move |_| on_close.call(()),
                        }
                    }
                }
                div { class: "rs-story__stage",
                    button {
                        r#type: "button",
                        class: "rs-story__edge rs-story__edge--prev",
                        tabindex: "-1",
                        aria_label: "Previous card",
                        disabled: current == 0,
                        "data-story-edge": "prev",
                        "data-testid": "story-prev",
                        onclick: move |_| step(-1),
                        Icon { name: "chevron-left".to_string(), class: "rs-ic rs-story__edge-ic".to_string() }
                    }
                    button {
                        r#type: "button",
                        class: "rs-story__edge rs-story__edge--next",
                        tabindex: "-1",
                        aria_label: "Next card",
                        disabled: current + 1 >= count,
                        "data-story-edge": "next",
                        "data-testid": "story-next",
                        onclick: move |_| step(1),
                        Icon { name: "chevron-right".to_string(), class: "rs-ic rs-story__edge-ic".to_string() }
                    }
                    div { class: "rs-story__track",
                        for i in shown {
                            div {
                                key: "{i}",
                                class: "rs-story__slot",
                                "data-story-card": "{i}",
                                "data-story-leaving": if i != current { "true" },
                                aria_hidden: if i != current { "true" },
                                "inert": if i != current { "true" },
                                {card.call(i)}
                            }
                        }
                    }
                }
                p { class: "rs-sr-only", aria_live: "polite", "Card {current + 1} of {count}. {compact_hint}" }
            }
        }
    }
}

/// Story segments: done, current, to come. Static (no timer).
#[component]
pub fn StoryProgress(count: usize, index: usize) -> Element {
    rsx! {
        div {
            class: "rs-story-progress",
            role: "progressbar",
            aria_label: "Cards",
            aria_valuemin: "1",
            aria_valuemax: "{count.max(1)}",
            aria_valuenow: "{index + 1}",
            "data-testid": "story-progress",
            for i in 0..count {
                span {
                    key: "{i}",
                    class: "rs-story-progress__seg",
                    class: if i < index { "rs-story-progress__seg--done" },
                    class: if i == index { "rs-story-progress__seg--on" },
                }
            }
        }
    }
}

/// One story card on the stage, with an optional album-art colour wash
/// (the cover, blurred far out of focus behind a dark veil).
/// * `kicker`: the small label at the top ("Your top artist").
#[component]
pub fn StoryCard(
    #[props(default)] wash: Option<String>,
    #[props(default)] kicker: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    rsx! {
        article { class: "rs-story-card {class}", "data-testid": testid.clone(),
            if let Some(url) = wash {
                img { class: "rs-story-card__wash", src: "{url}", alt: "", aria_hidden: "true" }
            }
            div { class: "rs-story-card__veil", aria_hidden: "true" }
            if let Some(kicker) = kicker {
                p { class: "rs-story-card__kicker", "{kicker}" }
            }
            div { class: "rs-story-card__body", {children} }
        }
    }
}
