//! First run: a welcome on the black stage, then signing in.
//!
//! The welcome shows what RustySound is: a slow Cover Flow of abstract covers
//! drifts behind the name, with one action, "Connect Your Server". That opens
//! the sign-in step in place (a panel rising under the flow on a phone, a
//! centred card on a wide window). Signing in tests the connection and saves
//! in one step, and lands straight in Home.
//!
//! Mounted by the Home route whenever no server is on. With servers that are
//! all switched off it says so and points to Settings instead.

use super::server_fields::{FormPlace, ServerForm};
use crate::api::ServerConfig;
use crate::components::ui::{use_layout, Button, ButtonVariant, Stage};
use crate::components::{AppView, Navigation};
use dioxus::prelude::*;

/// Covers in the welcome flow (one per slot; see `.rs-welcome-flow` in settings.css).
const WELCOME_COVERS: usize = 9;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    SignIn,
}

#[component]
pub fn FirstRun() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let wide = !use_layout().is_compact();
    let mut step = use_signal(|| Step::Welcome);
    let has_any = !servers.read().is_empty();
    let signing_in = step() == Step::SignIn && !has_any;

    rsx! {
        div {
            class: "rs-view rs-view--flush rs-firstrun",
            class: if signing_in { "rs-firstrun--signin" },
            "data-testid": "first-run",
            Stage { tall: true, class: "rs-firstrun__stage".to_string(), label: "Welcome to RustySound".to_string(),
                WelcomeFlow {}
                if signing_in {
                    section {
                        class: "rs-signin",
                        role: "region",
                        aria_labelledby: "signin-title",
                        "data-testid": "server-form",
                        onmounted: move |_| {
                            // On a phone the keyboard waits for a tap in a field;
                            // on a wide window typing can start at once.
                            if wide {
                                let _ = document::eval("document.getElementById('server-url')?.focus()");
                            }
                        },
                        onkeydown: move |evt| {
                            if evt.key() == Key::Escape {
                                evt.prevent_default();
                                step.set(Step::Welcome);
                            }
                        },
                        div { class: "rs-signin__head",
                            Button {
                                label: "Back".to_string(),
                                icon: "chevron-left".to_string(),
                                variant: ButtonVariant::Plain,
                                testid: "signin-back".to_string(),
                                onclick: move |_| step.set(Step::Welcome),
                            }
                        }
                        h2 { class: "rs-signin__title", id: "signin-title", "Sign in to your server" }
                        p { class: "rs-signin__lede",
                            "RustySound plays your library from Navidrome or any Subsonic-compatible server."
                        }
                        ServerForm {
                            place: FormPlace::Welcome,
                            on_saved: move |_| navigation.navigate_to(AppView::HomeView {}),
                        }
                    }
                } else {
                    div { class: "rs-firstrun__inner",
                        h1 { class: "rs-firstrun__title",
                            img {
                                class: "rs-firstrun__mark",
                                src: crate::components::ui::env::app_mark_src(),
                                alt: "",
                                width: "40",
                                height: "40",
                            }
                            "RustySound"
                        }
                        if has_any {
                            p { class: "rs-firstrun__line",
                                "Every server is switched off. Turn one on in Settings to bring your music back."
                            }
                            div { class: "rs-firstrun__actions",
                                Button {
                                    label: "Open Settings".to_string(),
                                    icon: "settings".to_string(),
                                    variant: ButtonVariant::Filled,
                                    testid: "first-run-settings".to_string(),
                                    onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
                                }
                            }
                        } else {
                            p { class: "rs-firstrun__line",
                                "Your Navidrome library, with Cover Flow and fresh picks every day."
                            }
                            div { class: "rs-firstrun__actions",
                                Button {
                                    label: "Connect Your Server".to_string(),
                                    variant: ButtonVariant::Filled,
                                    testid: "first-run-add".to_string(),
                                    onclick: move |_| step.set(Step::SignIn),
                                }
                            }
                            p { class: "rs-firstrun__note",
                                "Works with Navidrome and other Subsonic-compatible servers."
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The drifting Cover Flow behind the welcome: abstract covers (no server
/// yet, so no real artwork) that step one place to the left every few
/// seconds. Pure CSS; decorative, so hidden from assistive tech. Under
/// reduced motion the covers rest in place.
#[component]
fn WelcomeFlow() -> Element {
    rsx! {
        div { class: "rs-welcome-flow", aria_hidden: "true",
            for i in 0..WELCOME_COVERS {
                div { key: "{i}", class: "rs-welcome-flow__cover rs-welcome-flow__cover--{i}" }
            }
        }
    }
}
