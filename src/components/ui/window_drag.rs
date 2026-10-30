//! Window drag region for the macOS desktop app, whose titlebar is
//! transparent and sits over the web content. Pressing on the region moves
//! the window; double-clicking zooms it, like a native titlebar. Renders
//! nothing on other platforms.

use dioxus::prelude::*;

/// An empty strip that drags the window. `class` sets its size and place.
#[component]
pub fn WindowDragRegion(class: String) -> Element {
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    {
        use dioxus::html::input_data::MouseButton;
        rsx! {
            div {
                class: "rs-drag {class}",
                aria_hidden: "true",
                "data-testid": "window-drag",
                onmousedown: move |evt| {
                    if evt.trigger_button() == Some(MouseButton::Primary) {
                        dioxus::desktop::window().drag();
                    }
                },
                ondoubleclick: move |_| {
                    dioxus::desktop::window().toggle_maximized();
                },
            }
        }
    }
    #[cfg(not(all(feature = "desktop", target_os = "macos")))]
    {
        let _ = class;
        rsx! {}
    }
}
