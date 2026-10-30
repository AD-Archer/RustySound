//! A generic action menu (page "…" menus, row menus, confirmations).
//!
//! Same material as the track menu: a bottom action sheet with a Cancel card
//! on compact layouts, an anchored popover on wide ones. Styles are the kit's
//! `rs-menu*` / `rs-sheet--menu` classes; nothing new in `components.css`.

use crate::components::ui::env::use_layout;
use crate::components::Icon;
use dioxus::prelude::*;

/// One entry in an `ActionMenu`.
#[derive(Clone, PartialEq, Debug)]
pub struct MenuAction {
    /// Passed back to `on_action`.
    pub key: String,
    pub label: String,
    /// Trailing icon name (optional).
    pub icon: Option<String>,
    /// Red text (destructive).
    pub danger: bool,
    pub disabled: bool,
    /// Shows a check mark (current sort, current choice).
    pub checked: bool,
}

impl MenuAction {
    pub fn new(key: &str, label: &str, icon: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            icon: (!icon.is_empty()).then(|| icon.to_string()),
            danger: false,
            disabled: false,
            checked: false,
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

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// Popover / action sheet with a list of actions. The parent decides whether
/// it is mounted (usually `if let Some(..) = open_menu()`).
/// * `title`, `message`: optional head (name of the thing, or a confirmation
///   question). `actions`: the rows, in order.
/// * `anchor`: viewport point for the wide popover; None centres it over a
///   dimmed scrim (use None for confirmations).
/// * `on_action`: called with the chosen action's `key`, then `on_close`.
/// * `on_close`: scrim, Escape or Cancel.
/// * `testid`: data-testid of the menu card; items get `<testid>-<key>`.
#[component]
pub fn ActionMenu(
    #[props(default)] title: Option<String>,
    #[props(default)] message: Option<String>,
    actions: Vec<MenuAction>,
    #[props(default)] anchor: Option<(f64, f64)>,
    on_action: EventHandler<String>,
    on_close: EventHandler<()>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let layout = use_layout();
    let aria = title
        .clone()
        .or_else(|| message.clone())
        .unwrap_or_else(|| "Actions".to_string());
    let item_testid = |key: &str| testid.as_ref().map(|id| format!("{id}-{key}"));

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
                                div { class: "rs-menu__head-sub", style: "white-space:normal", "{message}" }
                            }
                        }
                    }
                    div { class: "rs-menu__gap", aria_hidden: "true" }
                }
                for action in actions.iter().cloned() {
                    button {
                        key: "{action.key}",
                        r#type: "button",
                        class: "rs-menu__item",
                        class: if action.danger { "rs-menu__item--danger" },
                        role: if action.checked { "menuitemradio" } else { "menuitem" },
                        aria_checked: action.checked.then_some("true"),
                        disabled: action.disabled,
                        "data-testid": item_testid(&action.key),
                        onclick: {
                            let key = action.key.clone();
                            move |_| {
                                // Action first, so the parent can still read
                                // what the menu was opened for.
                                on_action.call(key.clone());
                                on_close.call(());
                            }
                        },
                        "{action.label}"
                        if action.checked {
                            Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        } else if let Some(icon) = action.icon.clone() {
                            Icon { name: icon, class: "rs-ic".to_string() }
                        }
                    }
                }
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

    let style = match anchor {
        Some((x, y)) => format!(
            "left:clamp(8px, {x:.0}px, calc(100vw - 278px));top:clamp(8px, {y:.0}px, calc(100dvh - 320px))"
        ),
        None => "left:calc(50% - 135px);top:28vh".to_string(),
    };
    rsx! {
        {card(Some(style), true)}
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
    }
}
