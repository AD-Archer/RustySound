use dioxus::prelude::*;

use crate::components::app_view::AppView;

const NAV_HISTORY_LIMIT: usize = 64;

#[derive(Clone)]
pub struct Navigation {
    history: Signal<Vec<AppView>>,
}

impl Navigation {
    pub fn new(history: Signal<Vec<AppView>>) -> Self {
        Self { history }
    }

    pub fn navigate_to(&self, target: AppView) {
        let navigator = navigator();
        navigator.push(target);
    }

    pub fn can_go_back(&self) -> bool {
        let history = self.history.clone();
        !history().is_empty()
    }

    pub fn go_back(&self) -> Option<AppView> {
        let navigator = navigator();
        navigator.go_back();
        None // Router handles the navigation, we don't need to return the view
    }
}
