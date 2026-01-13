use dioxus::prelude::*;

use crate::components::app_view::AppView;

const NAV_HISTORY_LIMIT: usize = 64;

#[derive(Clone)]
pub struct Navigation {
    current_view: Signal<AppView>,
    history: Signal<Vec<AppView>>,
}

impl Navigation {
    pub fn new(current_view: Signal<AppView>, history: Signal<Vec<AppView>>) -> Self {
        Self {
            current_view,
            history,
        }
    }

    pub fn navigate_to(&self, target: AppView) {
        let mut current_view = self.current_view.clone();
        let previous = current_view();
        if previous == target {
            return;
        }

        let mut history = self.history.clone();
        let mut stack = history();
        stack.push(previous);
        if stack.len() > NAV_HISTORY_LIMIT {
            stack.remove(0);
        }
        history.set(stack);

        current_view.set(target);
    }

    pub fn can_go_back(&self) -> bool {
        let history = self.history.clone();
        !history().is_empty()
    }

    pub fn go_back(&self) -> Option<AppView> {
        let mut history = self.history.clone();
        let mut stack = history();
        let prev = stack.pop();
        history.set(stack);
        prev.map(|prev| {
            self.current_view.clone().set(prev.clone());
            prev
        })
    }
}
