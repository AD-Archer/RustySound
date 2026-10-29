//! First run: a welcome on the black stage with one action, "Add your server".
//! After a passing Test, Save lands straight in Discover.
//!
//! Mounted by the Discover route whenever no server is on. With servers that
//! are all switched off it says so and points to Settings instead.

use super::server_form::ServerSheet;
use crate::api::ServerConfig;
use crate::components::ui::{PillButton, Stage};
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

const MARK: Asset = asset!("/assets/favicon-96x96.png");

#[component]
pub fn FirstRun() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut adding = use_signal(|| false);
    let has_any = !servers.read().is_empty();

    rsx! {
        div { class: "rs-view rs-view--flush rs-firstrun", "data-testid": "first-run",
            Stage { tall: true, class: "rs-firstrun__stage".to_string(), label: "Welcome".to_string(),
                div { class: "rs-firstrun__inner",
                    img {
                        class: "rs-firstrun__mark",
                        src: MARK,
                        alt: "",
                        width: "72",
                        height: "72",
                    }
                    h1 { class: "rs-firstrun__title", "RustySound" }
                    if has_any {
                        p { class: "rs-firstrun__line",
                            "Every server is switched off. Turn one on in Settings to bring your music back."
                        }
                        div { class: "rs-firstrun__actions",
                            PillButton {
                                label: "Open Settings".to_string(),
                                icon: "settings".to_string(),
                                testid: "first-run-settings".to_string(),
                                onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                            }
                        }
                    } else {
                        p { class: "rs-firstrun__line",
                            "Your Navidrome library, with Cover Flow and fresh picks every day."
                        }
                        div { class: "rs-firstrun__actions",
                            PillButton {
                                label: "Add your server".to_string(),
                                icon: "plus".to_string(),
                                testid: "first-run-add".to_string(),
                                onclick: move |_| adding.set(true),
                            }
                        }
                        p { class: "rs-firstrun__note",
                            "Works with Navidrome and other Subsonic-compatible servers."
                        }
                    }
                }
            }
            if adding() {
                ServerSheet {
                    title: "Add your server".to_string(),
                    on_close: move |_| adding.set(false),
                    on_saved: move |_| {
                        adding.set(false);
                        navigation.navigate_to(AppView::HomeView {});
                    },
                }
            }
        }
    }
}
