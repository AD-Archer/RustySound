//! Text button: a rounded rectangle (style A), never a capsule.

use crate::components::Icon;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonVariant {
    /// Quiet grey fill with accent text. The default for every action.
    #[default]
    Tinted,
    /// Accent text (and optional icon) with no box: Preview, See All, Edit.
    Plain,
    /// Accent fill with light text. Only for the single primary action of a
    /// sheet or form (Save, Create, Add Server).
    Filled,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ButtonSize {
    /// 44px tall on compact layouts, 36px on wide ones.
    #[default]
    Regular,
    /// 32px tall (28px on wide), for buttons inside rows and toolbars.
    Small,
}

/// Text button.
/// * `label`: visible text. `icon`: optional leading icon name.
/// * `variant`: Tinted (default) | Plain | Filled. `size`: Regular | Small.
/// * `danger`: destructive (red text, or a red fill when `Filled`).
/// * `block`: stretch to fill the row (Play / Shuffle pairs).
/// * `submit`: `type="submit"` for forms.
/// * `id`, `testid`: optional element id and data-testid.
#[component]
pub fn Button(
    label: String,
    #[props(default)] icon: Option<String>,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] danger: bool,
    #[props(default)] block: bool,
    #[props(default)] submit: bool,
    #[props(default)] disabled: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
) -> Element {
    let variant_class = match variant {
        ButtonVariant::Tinted => "rs-btn--tinted",
        ButtonVariant::Plain => "rs-btn--plain",
        ButtonVariant::Filled => "rs-btn--filled",
    };
    let icon_class = if size == ButtonSize::Small || variant == ButtonVariant::Plain {
        "rs-ic rs-ic--xs"
    } else {
        "rs-ic rs-ic--sm"
    };
    rsx! {
        button {
            r#type: if submit { "submit" } else { "button" },
            id: id.clone(),
            "data-testid": testid.clone(),
            class: "rs-btn {variant_class}",
            class: if size == ButtonSize::Small { "rs-btn--small" },
            class: if danger { "rs-btn--danger" },
            class: if block { "rs-btn--block" },
            disabled,
            onclick: move |evt| {
                if let Some(handler) = onclick {
                    handler.call(evt);
                }
            },
            if let Some(name) = icon.clone() {
                Icon { name, class: icon_class.to_string() }
            }
            "{label}"
        }
    }
}
