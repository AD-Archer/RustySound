//! Settings → Discovery: Last.fm and ListenBrainz accounts (each with a real
//! Test), and the off-server / preview switches.

use super::discovery_check::{test_lastfm_key, test_lastfm_user, test_listenbrainz_user};
use super::shared::{use_settings_ctx, Footnote, GroupTitle, SwitchRow};
use crate::components::ui::{IconButton, IconButtonSize};
use crate::components::Icon;
use crate::db::AppSettings;
use dioxus::prelude::*;
use std::future::Future;

#[derive(Clone, PartialEq)]
enum Check {
    Idle,
    Running,
    Done(Result<String, String>),
}

#[component]
pub(super) fn DiscoverySection() -> Element {
    let ctx = use_settings_ctx();
    let settings = ctx.settings;
    let mut key = use_signal(|| settings.peek().lastfm_api_key.clone());
    let mut lastfm_user = use_signal(|| settings.peek().lastfm_username.clone());
    let mut lb_user = use_signal(|| settings.peek().listenbrainz_username.clone());
    let mut show_key = use_signal(|| false);
    let key_check = use_signal(|| Check::Idle);
    let user_check = use_signal(|| Check::Idle);
    let lb_check = use_signal(|| Check::Idle);

    // Run a test with the typed value, saving it first so a passing test
    // never leaves an unsaved field behind.
    fn run<F>(mut state: Signal<Check>, fut: F)
    where
        F: Future<Output = Result<String, String>> + 'static,
    {
        if *state.peek() == Check::Running {
            return;
        }
        state.set(Check::Running);
        spawn(async move {
            let result = fut.await;
            state.set(Check::Done(result));
        });
    }
    let save_text = move |apply: fn(&mut AppSettings, String), value: String| {
        ctx.update(|s| apply(s, value.trim().to_string()));
    };

    let current = settings.read();
    let offserver = current.discovery_offserver_enabled;
    let previews = current.discovery_previews_enabled;
    drop(current);

    rsx! {
        GroupTitle { title: "Last.fm".to_string() }
        ul { class: "rs-list rs-list--text rs-settings-fields",
            li {
                div { class: "rs-settings-field rs-settings-field--test",
                    label { r#for: "lastfm-key", "API key" }
                    input {
                        id: "lastfm-key",
                        r#type: if show_key() { "text" } else { "password" },
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        placeholder: "32-character key",
                        aria_describedby: "lastfm-key-result lastfm-help",
                        value: "{key}",
                        oninput: move |evt| key.set(evt.value()),
                        onchange: move |evt| save_text(|s, v| s.lastfm_api_key = v, evt.value()),
                    }
                    IconButton {
                        icon: "eye".to_string(),
                        label: if show_key() { "Hide API key".to_string() } else { "Show API key".to_string() },
                        pressed: Some(show_key()),
                        quiet: true,
                        size: IconButtonSize::Small,
                        onclick: move |_| show_key.toggle(),
                    }
                    TestButton {
                        label: "Test Last.fm API key".to_string(),
                        testid: "test-lastfm-key".to_string(),
                        running: key_check() == Check::Running,
                        onclick: move |_| {
                            save_text(|s, v| s.lastfm_api_key = v, key());
                            run(key_check, test_lastfm_key(key()));
                        },
                    }
                }
            }
            li {
                div { class: "rs-settings-field rs-settings-field--test",
                    label { r#for: "lastfm-user", "Username" }
                    input {
                        id: "lastfm-user",
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        placeholder: "Your Last.fm name",
                        aria_describedby: "lastfm-user-result",
                        value: "{lastfm_user}",
                        oninput: move |evt| lastfm_user.set(evt.value()),
                        onchange: move |evt| save_text(|s, v| s.lastfm_username = v, evt.value()),
                    }
                    TestButton {
                        label: "Test Last.fm username".to_string(),
                        testid: "test-lastfm-user".to_string(),
                        running: user_check() == Check::Running,
                        onclick: move |_| {
                            save_text(|s, v| s.lastfm_username = v, lastfm_user());
                            run(user_check, test_lastfm_user(key(), lastfm_user()));
                        },
                    }
                }
            }
        }
        CheckResult { id: "lastfm-key-result".to_string(), check: key_check() }
        CheckResult { id: "lastfm-user-result".to_string(), check: user_check() }
        Footnote {
            span { id: "lastfm-help",
                "Get a free API key at "
                a {
                    href: "https://www.last.fm/api/account/create",
                    target: "_blank",
                    rel: "noopener",
                    "last.fm/api/account/create"
                }
                " (any application name works), then paste the “API key” here, not the shared secret."
            }
        }

        GroupTitle { title: "ListenBrainz".to_string() }
        ul { class: "rs-list rs-list--text rs-settings-fields",
            li {
                div { class: "rs-settings-field rs-settings-field--test",
                    label { r#for: "listenbrainz-user", "Username" }
                    input {
                        id: "listenbrainz-user",
                        autocomplete: "off",
                        autocapitalize: "off",
                        spellcheck: "false",
                        placeholder: "Your ListenBrainz name",
                        aria_describedby: "listenbrainz-result",
                        value: "{lb_user}",
                        oninput: move |evt| lb_user.set(evt.value()),
                        onchange: move |evt| save_text(|s, v| s.listenbrainz_username = v, evt.value()),
                    }
                    TestButton {
                        label: "Test ListenBrainz username".to_string(),
                        testid: "test-listenbrainz".to_string(),
                        running: lb_check() == Check::Running,
                        onclick: move |_| {
                            save_text(|s, v| s.listenbrainz_username = v, lb_user());
                            run(lb_check, test_listenbrainz_user(lb_user()));
                        },
                    }
                }
            }
        }
        CheckResult { id: "listenbrainz-result".to_string(), check: lb_check() }
        Footnote { "Fresh releases from the artists you listen to. No token needed, just your public username." }

        GroupTitle { title: "On Discover".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "discovery-offserver".to_string(),
                title: "Outside your server".to_string(),
                subtitle: "Recommend artists and albums you don't own yet.".to_string(),
                checked: offserver,
                on_toggle: move |on| ctx.update(|s| s.discovery_offserver_enabled = on),
            }
            SwitchRow {
                id: "discovery-previews".to_string(),
                title: "30-second previews".to_string(),
                subtitle: "Find previews on the iTunes Store for those recommendations.".to_string(),
                checked: previews,
                on_toggle: move |on| ctx.update(|s| s.discovery_previews_enabled = on),
            }
        }
    }
}

#[component]
fn TestButton(label: String, testid: String, running: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "rs-btn rs-btn--plain rs-settings-field__test",
            aria_label: "{label}",
            "data-testid": "{testid}",
            // aria-disabled, not disabled, so the focused button keeps focus while it runs.
            aria_disabled: if running { "true" } else { "false" },
            onclick: move |evt| {
                if !running {
                    onclick.call(evt);
                }
            },
            if running { "Testing…" } else { "Test" }
        }
    }
}

/// The outcome line under a field group (live region).
#[component]
fn CheckResult(id: String, check: Check) -> Element {
    rsx! {
        div { class: "rs-settings-test rs-settings-test--inline", id: "{id}", role: "status", aria_live: "polite",
            match check {
                Check::Idle => rsx! {},
                Check::Running => rsx! {
                    span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                    span { "Testing…" }
                },
                Check::Done(Ok(message)) => rsx! {
                    span { class: "rs-settings-test__ok",
                        Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        "{message}"
                    }
                },
                Check::Done(Err(message)) => rsx! {
                    span { class: "rs-settings-test__err",
                        Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        "{message}"
                    }
                },
            }
        }
    }
}
