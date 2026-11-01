//! The app-wide status capsule ("Link copied"): the kit's capsule at the
//! top, gone after 2.4 s. Provided by the shell, so it is also the scope for
//! tasks that must outlive the menu that started them (`StatusToast::spawn`).
//! Home keeps its own toast (`views/discover/actions.rs`), scoped to Home.

use crate::components::ui::env::sleep_ms;
use dioxus::core::{current_scope_id, Runtime, Task};
use dioxus::prelude::*;

const SHOW_MS: u64 = 2_400;

#[derive(Clone, PartialEq)]
struct Status {
    text: String,
    error: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub struct StatusToast {
    status: Signal<Option<Status>>,
    generation: Signal<u64>,
    scope: ScopeId,
}

impl StatusToast {
    /// Create in the shell's scope (as a context provider's init).
    pub fn new() -> Self {
        Self {
            status: Signal::new(None),
            generation: Signal::new(0),
            scope: current_scope_id(),
        }
    }

    /// Spawn on the shell's scope: survives the menu or row that started it.
    pub fn spawn(&self, fut: impl std::future::Future<Output = ()> + 'static) -> Task {
        Runtime::current().spawn(self.scope, fut)
    }

    pub fn show(&self, text: impl Into<String>, error: bool) {
        let (mut status, mut generation) = (self.status, self.generation);
        let current = *generation.peek() + 1;
        generation.set(current);
        status.set(Some(Status {
            text: text.into(),
            error,
        }));
        self.spawn(async move {
            sleep_ms(SHOW_MS).await;
            if *generation.peek() == current {
                status.set(None);
            }
        });
    }
}

/// Renders the current status, if any. Mounted once by the shell.
#[component]
pub fn StatusToastHost() -> Element {
    let toast = use_context::<StatusToast>();
    let Some(status) = (toast.status)() else {
        return rsx! {};
    };
    rsx! {
        div {
            class: "rs-capsule",
            class: if status.error { "rs-capsule--error" },
            role: "status",
            "data-testid": "status-toast",
            "{status.text}"
        }
    }
}
