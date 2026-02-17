use dioxus::prelude::*;

use crate::components::app_view::AppView;

#[derive(Clone, Copy, Default)]
pub struct Navigation;

impl Navigation {
    pub fn new() -> Self {
        Self
    }

    pub fn navigate_to(&self, target: AppView) {
        let navigator = navigator();
        navigator.push(target);
    }

    pub fn can_go_back(&self) -> bool {
        let navigator = navigator();
        navigator.can_go_back()
    }

    pub fn go_back(&self) -> Option<AppView> {
        let navigator = navigator();
        navigator.go_back();
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
        None // Router handles the navigation, we don't need to return the view
    }
}
