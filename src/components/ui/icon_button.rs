//! Square icon-only button with an accessible name.

use crate::components::Icon;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum IconButtonSize {
    /// 32px.
    Small,
    /// 44px touch target. The default.
    #[default]
    Medium,
    /// 56px (Now Playing transport).
    Large,
}

/// Icon button.
/// * `icon`: icon name. `label`: aria-label and tooltip (its accessible name).
/// * `pressed`: Some(bool) makes it a toggle (`aria-pressed`, accent when on).
/// * `quiet`: muted colour until pressed.
/// * `size`: Small | Medium | Large. `icon_class`: override the icon size class.
/// * `id`, `testid`, `class`: element id, data-testid and extra classes.
#[component]
pub fn IconButton(
    icon: String,
    label: String,
    #[props(default)] pressed: Option<bool>,
    #[props(default)] quiet: bool,
    #[props(default)] size: IconButtonSize,
    #[props(default)] icon_class: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] id: Option<String>,
    #[props(default)] testid: Option<String>,
    #[props(default)] class: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let size_class = match size {
        IconButtonSize::Small => "rs-iconbtn--sm",
        IconButtonSize::Medium => "",
        IconButtonSize::Large => "rs-iconbtn--lg",
    };
    let icon_class = icon_class.unwrap_or_else(|| {
        match size {
            IconButtonSize::Small => "rs-ic rs-ic--sm",
            IconButtonSize::Medium => "rs-ic",
            IconButtonSize::Large => "rs-ic rs-ic--xl",
        }
        .to_string()
    });
    rsx! {
        button {
            r#type: "button",
            id: id.clone(),
            "data-testid": testid.clone(),
            class: "rs-iconbtn {size_class} {class}",
            class: if quiet { "rs-iconbtn--quiet" },
            aria_label: "{label}",
            title: "{label}",
            aria_pressed: pressed.map(|on| if on { "true" } else { "false" }),
            disabled,
            onclick: move |evt| onclick.call(evt),
            Icon { name: icon.clone(), class: icon_class }
        }
    }
}
