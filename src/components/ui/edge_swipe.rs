//! iOS-style edge swipe between pages on the compact layout.
//!
//! A rightward swipe from the left edge goes back; a leftward swipe from the
//! right edge goes forward when there is a forward entry. The page follows
//! the finger over a dimmed underlay and commits past 35% of the width or on
//! a flick, otherwise it springs back. Shelves, Cover Flow, sliders, open
//! menus and sheets, and Now Playing keep their own gestures.
//!
//! The finger tracking is JavaScript (`edge_swipe.js`) so it runs in the iOS
//! WKWebView too and does no Rust work per move. Rust only navigates when the
//! script posts "back" / "forward", and tells it when the new page is in.
//! On native builds it also passes the router's history flags, since the
//! webview's own history never changes there (MemoryHistory).

use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

const EDGE_SWIPE_JS: &str = include_str!("edge_swipe.js");

/// Mount once in the app shell, inside the router. Renders nothing.
#[component]
pub fn EdgeSwipe() -> Element {
    let navigation = use_context::<Navigation>();
    let route = use_route::<AppView>();

    use_hook(move || {
        // Created here, not in the task, so the script is installed before
        // the first route effect below syncs it.
        let mut gesture = document::eval(EDGE_SWIPE_JS);
        spawn(async move {
            while let Ok(message) = gesture.recv::<String>().await {
                match message.as_str() {
                    "back" if navigation.can_go_back() => {
                        navigation.go_back();
                    }
                    "forward" if navigation.can_go_forward() => {
                        navigation.go_forward();
                    }
                    _ => {}
                }
            }
        });
    });

    // After every route change: the new page has rendered, so the script can
    // bring it in; on native, refresh the history flags it gates on.
    use_effect(use_reactive!(|route| {
        let _ = route;
        let flags = if cfg!(target_arch = "wasm32") {
            "null,null".to_string()
        } else {
            format!("{},{}", navigation.can_go_back(), navigation.can_go_forward())
        };
        let _ = document::eval(&format!(
            "(()=>{{const s=window.__rsEdgeSwipe;if(s){{s.sync({flags});s.routed();}}}})();"
        ));
    }));

    rsx! {}
}
