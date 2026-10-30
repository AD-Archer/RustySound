//! A horizontal shelf (title row, scroll-snapped row of cards, paging arrows
//! on wide layouts), its loading skeleton and the card every shelf uses.
//!
//! Card width comes from CSS (`--card-w` on `.rs-home`): about 2.3 cards per
//! screen on compact, so the next one peeks in; on wide the column count
//! grows with the window and cards fill the row edge to edge.

use crate::components::ui::{use_layout, use_long_press, IconButton, IconButtonSize};
use dioxus::html::geometry::PixelsVector2D;
use dioxus::html::ScrollBehavior;
use dioxus::prelude::*;
use std::rc::Rc;

/// A titled, horizontally scrolling row of cards.
/// * `testid`: on the section (`shelf-playlists`, …); also the heading id.
/// * `children`: the cards (each renders its own `role="listitem"`).
#[component]
pub fn Shelf(
    title: String,
    testid: String,
    children: Element,
) -> Element {
    let wide = !use_layout().is_compact();
    let row = use_signal(|| None::<Rc<MountedData>>);
    // Written from scroll events, read only by `ShelfArrows`.
    let mut at_start = use_signal(|| true);
    let mut at_end = use_signal(|| false);
    let heading = format!("{testid}-title");

    let measure = move || {
        let Some(el) = row.peek().clone() else {
            return;
        };
        spawn(async move {
            let (Ok(offset), Ok(size), Ok(rect)) = (
                el.get_scroll_offset().await,
                el.get_scroll_size().await,
                el.get_client_rect().await,
            ) else {
                return;
            };
            let start = offset.x <= 1.0;
            let end = offset.x + rect.width() >= size.width - 1.0;
            if start != *at_start.peek() {
                at_start.set(start);
            }
            if end != *at_end.peek() {
                at_end.set(end);
            }
        });
    };

    rsx! {
        section {
            class: "rs-shelf",
            "data-testid": "{testid}",
            aria_labelledby: "{heading}",
            div { class: "rs-shelf__hd",
                h2 { id: "{heading}", class: "rs-shelf__title", "{title}" }
                if wide {
                    ShelfArrows { title: title.clone(), row, at_start, at_end }
                }
            }
            div {
                class: "rs-shelf__row",
                role: "list",
                aria_labelledby: "{heading}",
                onmounted: move |evt| {
                    let mut row = row;
                    row.set(Some(evt.data()));
                    measure();
                },
                onresize: move |_| measure(),
                onscroll: move |evt| {
                    let left = evt.scroll_left();
                    let max = f64::from(evt.scroll_width() - evt.client_width());
                    let start = left <= 1.0;
                    let end = left >= max - 1.0;
                    if start != *at_start.peek() {
                        at_start.set(start);
                    }
                    if end != *at_end.peek() {
                        at_end.set(end);
                    }
                },
                {children}
            }
        }
    }
}

/// Previous / next page buttons (wide). Hidden when everything fits.
#[component]
fn ShelfArrows(
    title: String,
    row: Signal<Option<Rc<MountedData>>>,
    at_start: Signal<bool>,
    at_end: Signal<bool>,
) -> Element {
    let (start, end) = (at_start(), at_end());
    if start && end {
        return rsx! {};
    }
    let page = move |direction: f64| {
        let Some(el) = row.peek().clone() else {
            return;
        };
        spawn(async move {
            let (Ok(offset), Ok(rect)) = (el.get_scroll_offset().await, el.get_client_rect().await) else {
                return;
            };
            // One screenful minus the side padding; scroll-snap lines it up.
            let step = (rect.width() - 64.0).max(120.0);
            let x = (offset.x + direction * step).max(0.0);
            let _ = el.scroll(PixelsVector2D::new(x, offset.y), ScrollBehavior::Smooth).await;
        });
    };
    rsx! {
        div { class: "rs-shelf__arrows",
            IconButton {
                icon: "chevron-left".to_string(),
                label: format!("Previous {title}"),
                quiet: true,
                size: IconButtonSize::Small,
                disabled: start,
                onclick: move |_| page(-1.0),
            }
            IconButton {
                icon: "chevron-right".to_string(),
                label: format!("Next {title}"),
                quiet: true,
                size: IconButtonSize::Small,
                disabled: end,
                onclick: move |_| page(1.0),
            }
        }
    }
}

/// A loading shelf: the title and a few grey cards.
#[component]
pub fn ShelfSkeleton(title: String, testid: String) -> Element {
    rsx! {
        section {
            class: "rs-shelf rs-shelf--loading",
            "data-testid": "{testid}-loading",
            aria_busy: "true",
            aria_label: "Loading {title}",
            div { class: "rs-shelf__hd",
                h2 { class: "rs-shelf__title", "{title}" }
            }
            div { class: "rs-shelf__row", aria_hidden: "true",
                for index in 0..8 {
                    div { key: "{index}", class: "rs-hcard",
                        div { class: "rs-hcard__art rs-skel" }
                        div { class: "rs-skel rs-hcard__skel-line" }
                        div { class: "rs-skel rs-hcard__skel-line rs-hcard__skel-line--short" }
                    }
                }
            }
        }
    }
}

/// One card: square art, then a title, a subtitle and an optional quiet
/// reason line. The whole card is one button (`onclick`).
/// * `art`: the artwork element (an `Artwork` with `fill`, or a mosaic).
/// * `tag`: a small rectangle before the subtitle ("Mix", a source name).
/// * `overlay`: something over the art's corner (a play or preview button).
/// * `on_menu`: shows a "…" button, and opens on right-click or a touch
///   long-press; gets the pointer position for the popover.
#[component]
pub fn Card(
    title: String,
    #[props(default)] subtitle: String,
    #[props(default)] reason: Option<String>,
    art: Element,
    #[props(default)] tag: Option<String>,
    #[props(default)] overlay: Option<Element>,
    #[props(default)] testid: Option<String>,
    #[props(default)] pressed: Option<bool>,
    /// Round art box (artists).
    #[props(default)] round: bool,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] on_menu: Option<EventHandler<Option<(f64, f64)>>>,
) -> Element {
    let press = use_long_press();
    let has_menu = on_menu.is_some();
    rsx! {
        div {
            class: "rs-hcard",
            class: if has_menu { "rs-hcard--menu" },
            role: "listitem",
            "data-testid": testid.clone(),
            oncontextmenu: move |evt: MouseEvent| {
                let Some(handler) = on_menu else {
                    return;
                };
                evt.prevent_default();
                press.cancel();
                let point = evt.client_coordinates();
                handler.call(Some((point.x, point.y)));
            },
            onpointerdown: move |evt: PointerEvent| {
                if let Some(handler) = on_menu {
                    press.down(&evt, move |point| handler.call(Some(point)));
                }
            },
            onpointermove: move |evt: PointerEvent| press.moved(&evt),
            onpointerup: move |_| press.cancel(),
            onpointercancel: move |_| press.cancel(),
            button {
                r#type: "button",
                class: "rs-hcard__main",
                aria_pressed: pressed.map(|on| if on { "true" } else { "false" }),
                onclick: move |evt| {
                    if press.take_fired() {
                        return;
                    }
                    onclick.call(evt);
                },
                div {
                    class: "rs-hcard__art",
                    class: if round { "rs-hcard__art--round" },
                    {art}
                }
                span { class: "rs-hcard__title", "{title}" }
                if tag.is_some() || !subtitle.is_empty() {
                    span { class: "rs-hcard__sub",
                        if let Some(tag) = tag.clone() {
                            span { class: "rs-badge rs-hcard__tag", "{tag}" }
                        }
                        span { class: "rs-hcard__sub-text", "{subtitle}" }
                    }
                }
                if let Some(reason) = reason.clone().filter(|r| !r.is_empty()) {
                    span { class: "rs-hcard__why", "{reason}" }
                }
            }
            if let Some(overlay) = overlay {
                {overlay}
            }
            if let Some(handler) = on_menu {
                IconButton {
                    icon: "more-horizontal".to_string(),
                    label: format!("More for {title}"),
                    quiet: true,
                    size: IconButtonSize::Small,
                    class: "rs-hcard__more".to_string(),
                    testid: "card-more".to_string(),
                    onclick: move |evt: MouseEvent| {
                        let point = evt.client_coordinates();
                        let anchor = (point.x > 0.0 || point.y > 0.0).then_some((point.x, point.y));
                        handler.call(anchor);
                    },
                }
            }
        }
    }
}
