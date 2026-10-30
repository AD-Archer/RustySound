//! Glossy, scrubbable progress track.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TrackVariant {
    /// 7px glossy blue on the dark stage trough (player, Now Playing).
    #[default]
    Stage,
    /// Same gloss on a page-coloured trough.
    Page,
    /// 4px flat light fill (volume).
    Thin,
}

/// A range control drawn as the Stage progress track.
///
/// While the user drags, the fill follows the finger locally; `on_commit`
/// fires once, on release (the input's `change` event), so seeking happens on
/// commit and not on every drag event. Keyboard steps commit immediately.
/// * `value` / `max`: current position and range (e.g. seconds). `step`: input step.
/// * `on_commit`: receives the committed value. Omit for display-only.
/// * `live`: radio stream; shows a full dim fill and disables input.
/// * `label`: aria-label. `value_text`: aria-valuetext ("1:38 of 4:12").
#[component]
pub fn ProgressTrack(
    value: f64,
    max: f64,
    #[props(default = 0.1)] step: f64,
    #[props(default)] on_commit: Option<EventHandler<f64>>,
    #[props(default)] on_input: Option<EventHandler<f64>>,
    #[props(default)] disabled: bool,
    #[props(default)] live: bool,
    #[props(default)] variant: TrackVariant,
    #[props(default)] label: String,
    #[props(default)] value_text: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
) -> Element {
    let mut drag = use_signal(|| None::<f64>);
    let max = if max.is_finite() && max > 0.0 { max } else { 0.0 };
    let shown = drag().unwrap_or(value).clamp(0.0, max.max(0.0));
    let percent = if max > 0.0 { shown / max * 100.0 } else { 0.0 };
    let interactive = !disabled && !live && max > 0.0 && on_commit.is_some();
    let variant_class = match variant {
        TrackVariant::Stage => "",
        TrackVariant::Page => "rs-track--page",
        TrackVariant::Thin => "rs-track--thin",
    };

    rsx! {
        div {
            class: "rs-track {variant_class} {class}",
            class: if drag().is_some() { "rs-track--active" },
            class: if live { "rs-track--live" },
            "data-testid": testid.clone(),
            // Fill and knob move with transforms only, so a playback tick never
            // repaints (on Home a repaint re-layerised the whole Cover Flow).
            div { class: "rs-track__clip",
                div {
                    class: "rs-track__fill",
                    style: if live { "transform:none".to_string() } else { format!("transform:scaleX({:.4})", percent / 100.0) },
                }
            }
            if interactive {
                div { class: "rs-track__rail", style: "transform:translateX({percent:.3}%)",
                    div { class: "rs-track__knob" }
                }
            }
            input {
                class: "rs-track__input",
                r#type: "range",
                min: "0",
                max: "{max}",
                step: "{step}",
                value: "{shown}",
                disabled: !interactive,
                aria_label: "{label}",
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
