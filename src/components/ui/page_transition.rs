//! Native-feeling transitions between pages.
//!
//! Compact (iPhone): opening a deeper page is an iOS push (the new page
//! slides in from the right over the old one, which shifts left and dims);
//! going back is the reverse; switching tabs is a quick cross-fade. Wide
//! (Mac): a short fade with a 10px shift. Reduced motion: a plain fade.
//!
//! The animation is JavaScript (`page_transition.js`), transforms and
//! opacity only, so it runs the same in the WKWebView and does no Rust work
//! per frame. `Navigation` announces each route change before making it
//! (`PageMotion`), which lets the script keep a copy of the page on screen;
//! this component tells the script when the new page has rendered.

use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

const PAGE_TRANSITION_JS: &str = include_str!("page_transition.js");

/// Mount once in the app shell, inside the router. Renders nothing.
#[component]
pub fn PageTransition() -> Element {
    let navigation = use_context::<Navigation>();
    let route = use_route::<AppView>();

    use_hook(|| {
        let _ = document::eval(PAGE_TRANSITION_JS);
    });

    // After every route change the new page is in the DOM: play the
    // transition. A same-type detail change detours through Home for one
    // render; wait for the real target.
    use_effect(use_reactive!(|route| {
        let _ = route;
        if navigation.is_detouring() {
            return;
        }
        let _ = document::eval("window.__rsPageTx&&window.__rsPageTx.routed();");
    }));

    rsx! {}
}
