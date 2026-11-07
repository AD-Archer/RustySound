//! Vertical slider (the equalizer's bands).

use dioxus::prelude::*;

/// A vertical range control: a thin trough, a fill from `origin` to the
/// value, and a knob. The input underneath is a native range turned on its
/// side, so mouse drags, touch, the keyboard (arrows, Page Up/Down, Home,
/// End) and screen readers all work as they do for any slider.
///
/// Like `ProgressTrack`, the knob follows a drag locally; `on_input` fires
/// while dragging, `on_commit` once on release.
/// * `origin`: where the fill starts (0 for a ±12 dB band); defaults to `min`.
/// * `label`: aria-label. `value_text`: aria-valuetext ("+3 dB").
#[component]
pub fn VerticalSlider(
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 0.5)] step: f64,
    #[props(default)] origin: Option<f64>,
    #[props(default)] on_input: Option<EventHandler<f64>>,
    #[props(default)] on_commit: Option<EventHandler<f64>>,
    #[props(default)] disabled: bool,
    label: String,
    #[props(default)] value_text: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
) -> Element {
    let mut drag = use_signal(|| None::<f64>);
    let span = (max - min).max(f64::EPSILON);
    let shown = drag().unwrap_or(value).clamp(min, max);
    let origin = origin.unwrap_or(min).clamp(min, max);
    // 0 at the bottom, 1 at the top.
    let at = |v: f64| (v - min) / span;
    let (lo, hi) = if shown >= origin { (at(origin), at(shown)) } else { (at(shown), at(origin)) };

    rsx! {
        div {
            class: "rs-vslider {class}",
            class: if drag().is_some() { "rs-vslider--active" },
            class: if disabled { "rs-vslider--disabled" },
            "data-testid": testid.clone(),
            div { class: "rs-vslider__trough",
                // Fill and knob move by transform only.
                div {
                    class: "rs-vslider__fill",
                    style: "transform:translateY({(1.0 - hi) * 100.0:.3}%) scaleY({(hi - lo).max(0.0):.4})",
                }
                div { class: "rs-vslider__rail", style: "transform:translateY({(1.0 - at(shown)) * 100.0:.3}%)",
                    div { class: "rs-vslider__knob" }
                }
            }
            input {
                class: "rs-vslider__input",
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{shown}",
                disabled,
                aria_label: "{label}",
                aria_orientation: "vertical",
                aria_valuetext: value_text.clone(),
                oninput: move |evt| {
                    if let Ok(next) = evt.value().parse::<f64>() {
                        drag.set(Some(next));
                        if let Some(handler) = on_input {
                            handler.call(next);
                        }
                    }
                },
                onchange: move |evt| {
                    let committed = evt.value().parse::<f64>().ok().or(*drag.peek());
                    drag.set(None);
                    if let (Some(next), Some(handler)) = (committed, on_commit) {
                        handler.call(next);
                    }
                },
            }
        }
    }
}
