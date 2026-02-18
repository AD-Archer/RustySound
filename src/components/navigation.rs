use dioxus::prelude::*;

use crate::components::app_view::AppView;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlElement};

#[derive(Clone, Copy, Default)]
pub struct Navigation;

fn reset_main_scroll_position() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = window() {
            if let Some(document) = win.document() {
                if let Ok(Some(main)) = document.query_selector("main.main-scroll, .main-scroll") {
                    if let Some(element) = main.dyn_ref::<HtmlElement>() {
                        element.set_scroll_top(0);
                    }
                }
            }
            win.scroll_to_with_x_and_y(0.0, 0.0);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = document::eval(
            r#"
(() => {
  const main = document.querySelector("main.main-scroll, .main-scroll");
  if (main && typeof main.scrollTo === "function") {
    main.scrollTo({ top: 0, left: 0, behavior: "auto" });
  } else if (main) {
    main.scrollTop = 0;
  }
  if (typeof window !== "undefined" && typeof window.scrollTo === "function") {
    window.scrollTo(0, 0);
  }
  return true;
})();
            "#,
        );
    }
}

impl Navigation {
    pub fn new() -> Self {
        Self
    }

    pub fn navigate_to(&self, target: AppView) {
        let navigator = navigator();
        navigator.push(target);
        reset_main_scroll_position();
    }

    pub fn can_go_back(&self) -> bool {
        let navigator = navigator();
        navigator.can_go_back()
    }

    pub fn go_back(&self) -> Option<AppView> {
        let navigator = navigator();
        navigator.go_back();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }

    #[cfg(target_arch = "wasm32")]
    pub fn can_go_forward(&self) -> bool {
        let navigator = navigator();
        navigator.can_go_forward()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn go_forward(&self) -> Option<AppView> {
        let navigator = navigator();
        navigator.go_forward();
        reset_main_scroll_position();
        None // Router handles the navigation, we don't need to return the view
    }
}
