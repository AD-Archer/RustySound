//! Cover Flow: real 3D (perspective + rotateY + reflection), ported from the
//! Stage mock and the iPod reference's `layoutFlow`.

use crate::components::ui::env::sleep_ms;
use crate::components::Icon;
use dioxus::prelude::*;

/// One cover in the flow.
#[derive(Clone, PartialEq, Debug)]
pub struct CoverFlowItem {
    /// Stable key (album id, mix id).
    pub key: String,
    /// Caption line 1.
    pub title: String,
    /// Caption line 2 (artist, description).
    pub subtitle: String,
    /// Caption line 3, the reason ("Last played 14 months ago · 212 plays").
    pub reason: String,
    /// Resolved image URLs: one for an album, four for a 2×2 mix mosaic,
    /// none for a placeholder. Build them with `cover_art_url`.
    pub art: Vec<String>,
}

/// Covers rendered on each side of the centre; the rest are not mounted.
const VISIBLE_SIDE: usize = 8;
/// Covers further than this fade out (as in the mock).
const FADE_AFTER: usize = 7;

/// Cover Flow carousel on the stage.
/// * `items`: the covers. `initial`: index centred on first render.
/// * `on_open`: centre cover tapped, or Enter/Space pressed.
/// * `on_select`: the centred index changed (arrow keys, wheel, swipe, scrubber).
/// * `cover_px`: cover edge (148 phone, 216 Mac). `wide`: Mac variant (deeper
///   perspective and the range scrubber under the caption).
/// * `label`: aria-label of the carousel.
///
/// Input: ←/→ keys when focused, horizontal or vertical wheel, pointer swipe
/// (one cover per third of a cover width), tapping a side cover to centre it.
/// Transitions switch on only after the first layout (double rAF), so the
/// resting state never animates in; reduced motion removes them in CSS.
#[component]
pub fn CoverFlow(
    items: Vec<CoverFlowItem>,
    #[props(default)] initial: usize,
    on_open: EventHandler<usize>,
    #[props(default)] on_select: Option<EventHandler<usize>>,
    #[props(default = 148)] cover_px: u32,
    #[props(default)] wide: bool,
    #[props(default = "Covers".to_string())] label: String,
) -> Element {
    let mut selected = use_signal(|| initial);
    let mut ready = use_signal(|| false);
    let mut pulse = use_signal(|| false);
    // Gesture state lives in signals nobody renders from, so writes are free.
    let mut swipe_start = use_signal(|| None::<f64>);
    let mut dragged = use_signal(|| false);
    let mut wheel = use_signal(|| 0.0f64);

    use_hook(move || {
        spawn(async move {
            let _ = document::eval(
                "await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))); return true;",
            )
            .await;
            ready.set(true);
        });
    });

    let count = items.len();
    if count == 0 {
        return rsx! {};
    }
    let sel = selected().min(count - 1);
    let size = cover_px as f64;
    let swipe_step = size * if wide { 0.3 } else { 0.34 };

    let mut go = move |index: isize| {
        let clamped = index.clamp(0, count as isize - 1) as usize;
        if clamped != *selected.peek() {
            selected.set(clamped);
            if let Some(handler) = on_select {
                handler.call(clamped);
            }
        }
    };
    let mut open = move || {
        let current = (*selected.peek()).min(count - 1);
        pulse.set(true);
        spawn(async move {
            sleep_ms(180).await;
            pulse.set(false);
        });
        on_open.call(current);
    };

    let caption = items[sel].clone();
    let start = sel.saturating_sub(VISIBLE_SIDE);
    let end = (sel + VISIBLE_SIDE + 1).min(count);

    rsx! {
        div {
            class: "rs-flow",
            class: if wide { "rs-flow--wide" },
            class: if ready() { "rs-flow--ready" },
            style: "--cover:{cover_px}px",
            tabindex: "0",
            role: "region",
            aria_roledescription: "carousel",
            aria_label: "{label}",
            "data-testid": "coverflow",
            onkeydown: move |evt| {
                match evt.key() {
                    Key::ArrowLeft => go(*selected.peek() as isize - 1),
                    Key::ArrowRight => go(*selected.peek() as isize + 1),
                    Key::Home => go(0),
                    Key::End => go(count as isize - 1),
                    Key::Enter => open(),
                    Key::Character(ref c) if c == " " => open(),
                    _ => return,
                }
                evt.prevent_default();
            },
            onwheel: move |evt| {
                let delta = evt.delta().strip_units();
                let d = if delta.x.abs() > delta.y.abs() { delta.x } else { delta.y };
                evt.prevent_default();
                let total = *wheel.peek() + d;
                if total.abs() > 40.0 {
                    wheel.set(0.0);
                    go(*selected.peek() as isize + total.signum() as isize);
                } else {
                    wheel.set(total);
                }
            },
            onpointerdown: move |evt| {
                swipe_start.set(Some(evt.client_coordinates().x));
                dragged.set(false);
            },
            onpointermove: move |evt| {
                let Some(start_x) = *swipe_start.peek() else {
                    return;
                };
                let x = evt.client_coordinates().x;
                let dx = x - start_x;
                if dx.abs() > swipe_step {
                    go(*selected.peek() as isize - dx.signum() as isize);
                    swipe_start.set(Some(x));
                    dragged.set(true);
                }
            },
            onpointerup: move |_| swipe_start.set(None),
            onpointercancel: move |_| swipe_start.set(None),
            onpointerleave: move |_| swipe_start.set(None),
            for index in start..end {
                {
                    let item = &items[index];
                    let d = index as isize - sel as isize;
                    let a = d.unsigned_abs();
                    let dir = d.signum() as f64;
                    let transform = if a == 0 {
                        "translateZ(60px)".to_string()
                    } else {
                        let x = dir * (size * 0.62 + (a as f64 - 1.0) * size * 0.26);
                        format!(
                            "translateX({x:.1}px) translateZ(-{:.1}px) rotateY({:.0}deg)",
                            size * 0.35,
                            -dir * 62.0
                        )
                    };
                    let shade = if a == 0 { 0.0 } else { (0.12 + a as f64 * 0.06).min(0.45) };
                    let hidden = a > FADE_AFTER;
                    let style = format!(
                        "transform:{transform};z-index:{};opacity:{};--shade:{shade:.2};{}",
                        100 - a as i32,
                        if hidden { 0 } else { 1 },
                        if hidden { "pointer-events:none" } else { "" }
                    );
                    let aria = if item.subtitle.is_empty() {
                        item.title.clone()
                    } else {
                        format!("{}, {}", item.title, item.subtitle)
                    };
                    let mosaic = item.art.len() >= 4;
                    let art = item.art.clone();
                    rsx! {
                        button {
                            key: "{item.key}",
                            r#type: "button",
                            class: "rs-cover",
                            class: if mosaic { "rs-cover--mosaic" },
                            class: if a == 0 { "rs-cover--on" },
                            class: if a == 0 && pulse() { "rs-cover--pulse" },
                            style: "{style}",
                            tabindex: "-1",
                            aria_label: "{aria}",
                            aria_current: if a == 0 { Some("true") } else { None },
                            "data-testid": if a == 0 { "coverflow-center" } else { "coverflow-cover" },
                            onclick: move |evt| {
                                evt.stop_propagation();
                                if *dragged.peek() {
                                    dragged.set(false);
                                    return;
                                }
                                if index == *selected.peek() {
                                    open();
                                } else {
                                    go(index as isize);
                                }
                            },
                            if art.is_empty() {
                                div { class: "rs-cover__blank",
                                    Icon { name: "album".to_string(), class: "rs-ic rs-ic--xl".to_string() }
                                }
                            } else if mosaic {
                                for (tile, url) in art.into_iter().take(4).enumerate() {
                                    img {
                                        key: "{tile}",
                                        src: "{url}",
                                        alt: "",
                                        draggable: "false",
                                        decoding: "async",
                                        loading: if a <= 3 { "eager" } else { "lazy" },
                                    }
                                }
                            } else {
                                img {
                                    src: "{art[0]}",
                                    alt: "",
                                    width: "{cover_px}",
                                    height: "{cover_px}",
                                    draggable: "false",
                                    decoding: "async",
                                    loading: if a <= 3 { "eager" } else { "lazy" },
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "rs-flow-caption", aria_live: "polite",
            strong { "{caption.title}" }
            if !caption.subtitle.is_empty() {
                span { "{caption.subtitle}" }
            }
            if !caption.reason.is_empty() {
                em { "{caption.reason}" }
            }
        }
        if wide && count > 1 {
            input {
                class: "rs-flow-scrub",
                r#type: "range",
                min: "0",
                max: "{count - 1}",
                step: "1",
                value: "{sel}",
                aria_label: "Browse covers",
                oninput: move |evt| {
                    if let Ok(index) = evt.value().parse::<isize>() {
                        go(index);
                    }
                },
            }
        }
    }
}
