//! Page states shared by the browsing views: paging (`LoadMore`,
//! `PageFoot`), errors (`ErrorState`), no server (`NoServers`) and the cover
//! grid placeholder (`GridSkeleton`). Lists load with `Skeleton`.
//!
//! Moved from `views/library/shared.rs`; their classes still live in
//! library.css (`.rs-library-sentinel`, `.rs-library-foot`,
//! `.rs-library-covers`, `.rs-library-tile`).

use crate::components::ui::{Button, ButtonVariant, EmptyState};
use crate::db::AppSettings;
use dioxus::prelude::*;

/// Invisible marker near the end of a paged list. It sits `lead_px` above
/// the list's end and calls `on_near_end` when it scrolls into view. Give it
/// a `key` that changes per page so it re-observes after each append.
#[component]
pub fn LoadMore(on_near_end: EventHandler<()>, #[props(default = 900)] lead_px: u32) -> Element {
    rsx! {
        div {
            class: "rs-library-sentinel",
            style: "margin-top:-{lead_px}px;height:{lead_px}px",
            aria_hidden: "true",
            onvisible: move |evt| {
                if evt.is_intersecting().unwrap_or(false) {
                    on_near_end.call(());
                }
            },
        }
    }
}

/// Footer under a paged list: spinner while loading, Retry after an error.
#[component]
pub fn PageFoot(loading: bool, error: Option<String>, on_retry: EventHandler<()>) -> Element {
    if let Some(err) = error {
        return rsx! {
            div { class: "rs-library-foot rs-library-foot--error", role: "status",
                span { "Couldn't load more. {err}" }
                Button {
                    label: "Retry".to_string(),
                    variant: ButtonVariant::Plain,
                    onclick: move |_| on_retry.call(()),
                }
            }
        };
    }
    if loading {
        return rsx! {
            div { class: "rs-library-foot", role: "status", aria_label: "Loading more",
                span { class: "rs-spinner rs-spinner--page" }
            }
        };
    }
    rsx! {}
}

/// Error with a Retry button; the message mentions being offline when we are.
#[component]
pub fn ErrorState(
    title: String,
    error: String,
    on_retry: EventHandler<()>,
    #[props(default)] testid: Option<String>,
) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let offline = settings.read().offline_mode || crate::cache_service::is_offline_mode();
    let message = if offline {
        "You're offline. Downloaded music still plays; reconnect to browse the rest.".to_string()
    } else {
        format!("The server said: {error}")
    };
    rsx! {
        EmptyState {
            icon: if offline { "offline".to_string() } else { "server".to_string() },
            title,
            message,
            testid,
            Button {
                label: "Retry".to_string(),
                icon: "refresh-cw".to_string(),
                variant: ButtonVariant::Tinted,
                onclick: move |_| on_retry.call(()),
            }
        }
    }
}

/// "No servers" empty state with a way to Settings.
#[component]
pub fn NoServers() -> Element {
    let navigation = use_context::<crate::components::Navigation>();
    rsx! {
        EmptyState {
            icon: "server".to_string(),
            title: "No server connected".to_string(),
            message: "Add your Navidrome or Subsonic server to browse your library.".to_string(),
            Button {
                label: "Open Settings".to_string(),
                onclick: move |_| navigation.navigate_to(crate::components::AppView::SettingsView {}),
            }
        }
    }
}

/// Skeleton for a cover grid.
#[component]
pub fn GridSkeleton(#[props(default = 8)] count: usize) -> Element {
    rsx! {
        div { class: "rs-library-covers", aria_busy: "true", aria_label: "Loading",
            for index in 0..count {
                div { key: "{index}", class: "rs-library-tile",
                    div { class: "rs-skel rs-library-tile__skel" }
                    div { class: "rs-skel", style: "width:70%;height:12px;margin-top:8px" }
                    div { class: "rs-skel", style: "width:45%;height:10px;margin-top:6px" }
                }
            }
        }
    }
}
