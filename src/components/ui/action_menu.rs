//! A small action menu (sort and filter menus, "…" menus on headers and rows).
//! Same materials as the track menu: an anchored popover on wide layouts and
//! a bottom sheet with a Cancel row on compact ones.

use crate::components::ui::env::use_layout;
use crate::components::Icon;
use dioxus::prelude::*;

/// Menu card with a scrim. The parent decides whether it is mounted.
/// * `anchor`: viewport point to anchor the popover at (wide). None centres it.
/// * `on_close`: scrim tap, Escape, Cancel.
/// * `label`: aria-label of the menu. `title`: optional muted header line.
/// * `children`: `ActionMenuItem`s, `ActionMenuGap`s, notes.
#[component]
pub fn ActionMenu(
    on_close: EventHandler<()>,
    label: String,
    #[props(default)] anchor: Option<(f64, f64)>,
    #[props(default)] title: Option<String>,
    #[props(default)] testid: Option<String>,
    children: Element,
) -> Element {
    let layout = use_layout();
    let head = title.clone().map(|text| {
        rsx! {
            div { class: "rs-menu__note", "{text}" }
            div { class: "rs-menu__gap", aria_hidden: "true" }
        }
    });

    let card = |style: Option<String>, popover: bool| {
        rsx! {
            div {
                class: "rs-menu",
                class: if popover { "rs-menu--popover" },
                style,
                role: "menu",
                aria_label: "{label}",
                tabindex: "-1",
                "data-testid": testid.clone(),
                onmounted: move |evt| async move {
                    let _ = evt.set_focus(true).await;
                },
                onkeydown: move |evt| {
                    if evt.key() == Key::Escape {
                        evt.prevent_default();
                        on_close.call(());
                    }
                },
                {head.clone()}
                {children.clone()}
            }
        }
    };

    if layout.is_compact() {
        return rsx! {
            div {
                class: "rs-scrim",
                aria_hidden: "true",
                onclick: move |_| on_close.call(()),
            }
            div { class: "rs-sheet rs-sheet--menu", role: "dialog", aria_label: "{label}",
                {card(None, false)}
                div { class: "rs-menu",
                    button {
                        r#type: "button",
                        class: "rs-menu__item rs-menu__cancel",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                }
            }
        };
    }

    let style = match anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, {x:.0}px, calc(100vw - 278px));top:clamp(8px, {y:.0}px, calc(100dvh - 420px))"
        ),
        None => "left:50%;top:50%;transform:translate(-50%,-50%)".to_string(),
    };
    rsx! {
        div {
            class: "rs-scrim rs-scrim--clear",
            aria_hidden: "true",
            onclick: move |_| on_close.call(()),
            oncontextmenu: move |evt| {
                evt.prevent_default();
                on_close.call(());
            },
        }
        {card(Some(style), true)}
    }
}

/// One row of an `ActionMenu`.
/// * `checked`: Some(bool) makes it a radio-style choice (check mark when on).
/// * `icon`: trailing icon (ignored when `checked` is Some(true)).
/// * `danger`: destructive styling. `detail`: muted trailing text (progress).
#[component]
pub fn ActionMenuItem(
    label: String,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] icon: Option<String>,
    #[props(default)] checked: Option<bool>,
    #[props(default)] danger: bool,
    #[props(default)] disabled: bool,
    #[props(default)] detail: Option<String>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let role = if checked.is_some() {
        "menuitemradio"
    } else {
        "menuitem"
    };
    let trailing = match checked {
        Some(true) => Some("check".to_string()),
        Some(false) => None,
        None => icon.clone(),
    };
    rsx! {
        button {
            r#type: "button",
            class: "rs-menu__item",
            class: if danger { "rs-menu__item--danger" },
            role,
            aria_checked: checked.map(|on| if on { "true" } else { "false" }),
            disabled,
            "data-testid": testid.clone(),
            onclick: move |evt| onclick.call(evt),
            span { "{label}" }
            if let Some(text) = detail.clone() {
                span { class: "rs-menu__sub", style: "margin-left:auto;font-variant-numeric:tabular-nums", "{text}" }
            }
            if let Some(name) = trailing {
                Icon { name, class: "rs-ic".to_string() }
            }
        }
    }
}

/// The thick gap between groups of an `ActionMenu`.
#[component]
pub fn ActionMenuGap() -> Element {
    rsx! {
        div { class: "rs-menu__gap", aria_hidden: "true" }
    }
}
