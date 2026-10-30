//! The kit menu: page "…" menus, row menus, sort and filter menus and
//! confirmations. One look, the track menu's: an anchored popover on wide
//! layouts and a bottom action sheet with a Cancel card on compact ones.
//!
//! Rows come from either (or both) of:
//! * `actions`: a list of `MenuAction`s; picking one calls `on_action(key)`
//!   and then `on_close`.
//! * `children`: `MenuItem`s, `MenuGap`s and notes, for rows that carry their
//!   own handlers. A `MenuItem` does not close the menu by itself.

use crate::components::ui::env::use_layout;
use crate::components::Icon;
use dioxus::prelude::*;

/// One data-driven row of a `Menu`.
#[derive(Clone, PartialEq, Debug)]
pub struct MenuAction {
    /// Passed back to `on_action`; also the suffix of the row's test id.
    pub key: String,
    pub label: String,
    /// Trailing icon name (optional).
    pub icon: Option<String>,
    /// Red text (destructive).
    pub danger: bool,
    pub disabled: bool,
    /// Some(_) makes the row a radio choice (current sort, current filter);
    /// Some(true) shows the check mark.
    pub checked: Option<bool>,
    /// Muted trailing text (a count, a progress figure).
    pub detail: Option<String>,
}

impl MenuAction {
    pub fn new(key: &str, label: &str, icon: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            icon: (!icon.is_empty()).then(|| icon.to_string()),
            danger: false,
            disabled: false,
            checked: None,
            detail: None,
        }
    }

    /// Destructive variant (red).
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Makes the row a radio choice; `checked` shows the check mark.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Menu card with a scrim. The parent decides whether it is mounted (usually
/// `if let Some(anchor) = open_menu()`).
/// * `title`, `message`: optional head (the name of the thing, or a
///   confirmation question).
/// * `label`: aria-label; defaults to the title, then the message.
/// * `actions` + `on_action`: data-driven rows (see the module docs).
/// * `children`: `MenuItem`s / `MenuGap`s, rendered after `actions`.
/// * `anchor`: viewport point for the wide popover. None centres it over a
///   dimmed scrim (use None for confirmations).
/// * `on_close`: scrim, Escape or Cancel.
/// * `testid`: data-testid of the card; `actions` rows get `<testid>-<key>`.
#[component]
pub fn Menu(
    on_close: EventHandler<()>,
    #[props(default)] title: Option<String>,
    #[props(default)] message: Option<String>,
    #[props(default)] label: Option<String>,
    #[props(default)] actions: Vec<MenuAction>,
    #[props(default)] on_action: Option<EventHandler<String>>,
    #[props(default)] anchor: Option<(f64, f64)>,
    #[props(default)] testid: Option<String>,
    children: Element,
) -> Element {
    let layout = use_layout();
    let aria = label
        .clone()
        .or_else(|| title.clone())
        .or_else(|| message.clone())
        .unwrap_or_else(|| "Actions".to_string());
    let row_testid = |key: &str| testid.as_ref().map(|id| format!("{id}-{key}"));

    let card = |style: Option<String>, popover: bool| {
        rsx! {
            div {
                class: "rs-menu",
                class: if popover { "rs-menu--popover" },
                style,
                role: "menu",
                aria_label: "{aria}",
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
                if title.is_some() || message.is_some() {
                    div { class: "rs-menu__head",
                        div { class: "rs-menu__head-meta",
                            if let Some(title) = title.clone() {
                                div { class: "rs-menu__head-title", "{title}" }
                            }
                            if let Some(message) = message.clone() {
                                div { class: "rs-menu__head-sub rs-menu__head-sub--wrap", "{message}" }
                            }
                        }
                    }
                    div { class: "rs-menu__gap", aria_hidden: "true" }
                }
                for action in actions.iter().cloned() {
                    MenuItem {
                        key: "{action.key}",
                        label: action.label.clone(),
                        icon: action.icon.clone(),
                        checked: action.checked,
                        danger: action.danger,
                        disabled: action.disabled,
                        detail: action.detail.clone(),
                        testid: row_testid(&action.key),
                        onclick: {
                            let key = action.key.clone();
                            move |_| {
                                // Action first, so the parent can still read
                                // what the menu was opened for.
                                if let Some(handler) = on_action {
                                    handler.call(key.clone());
                                }
                                on_close.call(());
                            }
                        },
                    }
                }
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
            div { class: "rs-sheet rs-sheet--menu", role: "dialog", aria_label: "{aria}",
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

    // Same clamp as the track menu, so a popover near the window edge stays
    // on screen. Without an anchor it sits centred, a little above the middle
    // (no transform, so the pop-in animation can own it).
    let style = match anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, {x:.0}px, calc(100vw - 278px));top:clamp(8px, {y:.0}px, calc(100dvh - 380px))"
        ),
        None => "left:calc(50% - 135px);top:28vh".to_string(),
    };
    rsx! {
        div {
            class: "rs-scrim",
            class: if anchor.is_some() { "rs-scrim--clear" },
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

/// One row of a `Menu` with its own handler.
/// * `checked`: Some(bool) makes it a radio-style choice (check mark when on).
/// * `icon`: trailing icon (replaced by the check mark when checked).
/// * `danger`: destructive styling. `detail`: muted trailing text.
#[component]
pub fn MenuItem(
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
            span { class: "rs-menu__label", "{label}" }
            if let Some(text) = detail.clone() {
                span { class: "rs-menu__detail", "{text}" }
            }
            if let Some(name) = trailing {
                Icon { name, class: "rs-ic".to_string() }
            }
        }
    }
}

/// The thick gap between groups of a `Menu`.
#[component]
pub fn MenuGap() -> Element {
    rsx! {
        div { class: "rs-menu__gap", aria_hidden: "true" }
    }
}
