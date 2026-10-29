//! Pill-shaped text button.

use crate::components::Icon;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum PillVariant {
    /// Filled accent (Play).
    #[default]
    Primary,
    /// Outlined with accent text (Shuffle).
    Quiet,
    /// Small outlined (Preview, Add).
    Mini,
}

/// Pill button.
/// * `label`: visible text. `icon`: optional leading icon name.
/// * `variant`: Primary | Quiet | Mini. `block`: stretch to fill the row.
/// * `id`, `testid`: optional element id and data-testid.
#[component]
pub fn PillButton(
    label: String,
    #[props(default)] icon: Option<String>,
    #[props(default)] variant: PillVariant,
    #[props(default)] block: bool,
    #[props(default)] disabled: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] testid: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let variant_class = match variant {
        PillVariant::Primary => "rs-pill--primary",
        PillVariant::Quiet => "rs-pill--quiet",
        PillVariant::Mini => "rs-pill--mini",
    };
    let icon_class = if variant == PillVariant::Mini {
        "rs-ic rs-ic--xs"
    } else {
        "rs-ic rs-ic--sm"
    };
    rsx! {
        button {
            r#type: "button",
            id: id.clone(),
            "data-testid": testid.clone(),
            class: "rs-pill {variant_class}",
            class: if block { "rs-pill--block" },
            disabled,
            onclick: move |evt| onclick.call(evt),
            if let Some(name) = icon.clone() {
                Icon { name, class: icon_class.to_string() }
            }
            "{label}"
        }
    }
}
