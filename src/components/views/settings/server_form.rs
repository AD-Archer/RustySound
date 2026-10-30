//! The Add / Edit server sheet (`data-testid="server-form"`), shared by
//! Settings → Servers and the first-run welcome.

use crate::api::{NavidromeClient, ServerConfig};
use crate::components::ui::{Button, ButtonVariant, Sheet, Switch};
use crate::components::Icon;
use crate::db::save_servers_now;
use dioxus::prelude::*;

fn is_local_http_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host == "127.0.0.1" || host == "::1" || host.ends_with(".local") {
        return true;
    }
    if host.starts_with("10.") || host.starts_with("192.168.") {
        return true;
    }
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some(octet) = rest.split('.').next() {
            if let Ok(value) = octet.parse::<u8>() {
                return (16..=31).contains(&value);
            }
        }
    }
    false
}

/// Host of a URL with or without a scheme ("https://a.b:4533/x" → "a.b").
fn url_host(raw: &str) -> String {
    let rest = raw
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    rest.split('/')
        .next()
        .unwrap_or_default()
        .split('@')
        .next_back()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The URL the app stores: trimmed, without a trailing slash or a reverse-proxy
/// `/app` path, with the scheme the "Use HTTPS" switch asks for.
pub(super) fn final_server_url(raw: &str, use_https: bool) -> String {
    let mut url = raw.trim().trim_end_matches('/').to_string();
    if let Some(pos) = url.find("/app") {
        url.truncate(pos);
    }
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(&url)
        .to_string();
    if without_scheme.is_empty() {
        return String::new();
    }
    let scheme = if use_https { "https" } else { "http" };
    format!("{scheme}://{without_scheme}")
}

/// What the HTTPS switch should start at for a typed URL: the typed scheme
/// wins; without one, remote hosts get HTTPS and home-network hosts HTTP.
fn https_for_url(raw: &str) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.starts_with("https://") {
        Some(true)
    } else if trimmed.starts_with("http://") {
        Some(false)
    } else if trimmed.contains('.') || trimmed.contains(':') {
        Some(!is_local_http_host(&url_host(trimmed)))
    } else {
        None
    }
}

fn resolve_server_name(name: &str, url: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        let host = url_host(url);
        if host.is_empty() {
            url.trim().to_string()
        } else {
            host
        }
    } else {
        trimmed.to_string()
    }
}

/// Turns a raw client error into what went wrong and what to do about it.
pub(super) fn format_connection_error(error: &str) -> String {
    let lower = error.to_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));

    if has(&["wrong username or password", "401", "unauthorized"]) {
        return "Wrong username or password. Check both and try again.".to_string();
    }
    if has(&["connection refused", "econnrefused", "reset by peer"]) {
        return "The server refused the connection. Check the port number and that Navidrome is running.".to_string();
    }
    if has(&[
        "name or service not known",
        "no address associated",
        "nodename nor servname",
        "failed to lookup address",
        "nxdomain",
        "getaddrinfo failed",
        "dns error",
    ]) {
        return "That host name doesn't resolve. Check the spelling of the address.".to_string();
    }
    if has(&["certificate", "ssl", "tls", "x509", "self signed"]) {
        return "The HTTPS certificate was rejected. Turn off Use HTTPS for a home-network server, or fix the certificate.".to_string();
    }
    if has(&["timeout", "timed out"]) {
        return "The server took too long to answer. Check your network and the address.".to_string();
    }
    if has(&["invalid url", "invalid scheme", "empty domain", "relative url", "builder error"]) {
        return "That isn't a valid address. Use something like https://music.example.com or http://192.168.1.10:4533.".to_string();
    }
    if has(&["error decoding", "expected value", "invalid type", "missing field"]) {
        return "Something answered at that address, but it isn't a Subsonic/Navidrome API. Check the URL (leave off /app).".to_string();
    }
    if has(&["403", "forbidden"]) {
        return "The server denied access. Check this user's permissions.".to_string();
    }
    if has(&["404", "not found"]) {
        return "No Subsonic API at that address. Check the URL and port.".to_string();
    }
    if has(&["500", "502", "503", "504", "internal server error"]) {
        return "The server returned an error. It may be restarting; try again shortly.".to_string();
    }
    if has(&["network unreachable", "enetunreach"]) {
        return "No network connection. Check that you're online.".to_string();
    }
    if has(&["error sending request", "failed to fetch", "network error"]) {
        return "Couldn't reach the server. Check the address, the port and Use HTTPS.".to_string();
    }
    format!("Couldn't connect ({error}). Check the address and your network.")
}

#[derive(Clone, PartialEq)]
enum TestState {
    Idle,
    Testing,
    Passed(String),
    Failed(String),
}

/// Add/Edit server sheet.
/// * `editing`: the server to edit; `None` adds a new one.
/// * `on_saved`: called with the saved server after it is persisted.
/// * `title`: sheet title override (first run says "Add your server").
#[component]
pub(super) fn ServerSheet(
    #[props(default)] editing: Option<ServerConfig>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<ServerConfig>,
    #[props(default)] title: Option<String>,
) -> Element {
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let is_edit = editing.is_some();
    let initial = editing.clone();
    let mut url = use_signal(|| initial.as_ref().map(|s| s.url.clone()).unwrap_or_default());
    let mut name = use_signal(|| initial.as_ref().map(|s| s.name.clone()).unwrap_or_default());
    let mut user = use_signal(|| initial.as_ref().map(|s| s.username.clone()).unwrap_or_default());
    let mut pass = use_signal(|| initial.as_ref().map(|s| s.password.clone()).unwrap_or_default());
    let mut use_https = use_signal(|| {
        initial
            .as_ref()
            .map(|s| s.url.starts_with("https://"))
            .unwrap_or(false)
    });
    // Once the user flips the switch, typing in the URL no longer moves it.
    let mut https_touched = use_signal(|| is_edit);
    let mut test = use_signal(|| TestState::Idle);

    let complete = !url().trim().is_empty() && !user().trim().is_empty() && !pass().is_empty();
    let passed = matches!(test(), TestState::Passed(_));
    let testing = test() == TestState::Testing;
    // Adding needs a passing test first; editing may save straight away.
    let can_save = complete && !testing && (is_edit || passed);

    let run_test = move |_| {
        if testing || !complete {
            return;
        }
        let address = final_server_url(&url(), use_https());
        let username = user().trim().to_string();
        let password = pass();
        test.set(TestState::Testing);
        spawn(async move {
            let probe = ServerConfig::new("Test".to_string(), address, username.clone(), password);
            let outcome = NavidromeClient::new(probe).ping().await;
            test.set(match outcome {
                Ok(_) => TestState::Passed(format!("Connected. Signed in as {username}.")),
                Err(error) => TestState::Failed(format_connection_error(&error)),
            });
        });
    };

    let editing_for_save = editing.clone();
    let save = move |_| {
        if !can_save {
            return;
        }
        let address = final_server_url(&url(), use_https());
        let display = resolve_server_name(&name(), &address);
        let username = user().trim().to_string();
        let password = pass();
        let saved = match editing_for_save.clone() {
            Some(original) => {
                let mut updated = original.clone();
                updated.name = display;
                updated.url = address;
                updated.username = username;
                updated.password = password;
                servers.with_mut(|list| {
                    if let Some(slot) = list.iter_mut().find(|s| s.id == original.id) {
                        *slot = updated.clone();
                    }
                });
                updated
            }
            None => {
                let created = ServerConfig::new(display, address, username, password);
                servers.with_mut(|list| list.push(created.clone()));
                created
            }
        };
        let _ = save_servers_now(&servers.peek());
        on_saved.call(saved);
    };

    let sheet_title = title.unwrap_or_else(|| {
        if is_edit {
            "Edit Server".to_string()
        } else {
            "Add Server".to_string()
        }
    });

    rsx! {
        Sheet {
            title: sheet_title,
            testid: "server-form".to_string(),
            on_close: move |_| on_close.call(()),
            div { class: "rs-settings-form",
                ul { class: "rs-list rs-list--text rs-settings-fields",
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "server-url", "URL" }
                            input {
                                id: "server-url",
                                r#type: "url",
                                inputmode: "url",
                                autocomplete: "url",
                                autocapitalize: "off",
                                spellcheck: "false",
                                placeholder: "https://navidrome.example.com",
                                aria_describedby: "server-url-hint",
                                value: "{url}",
                                oninput: move |evt| {
                                    let value = evt.value();
                                    if !*https_touched.peek() {
                                        if let Some(secure) = https_for_url(&value) {
                                            use_https.set(secure);
                                        }
                                    }
                                    url.set(value);
                                    test.set(TestState::Idle);
                                },
                            }
                        }
                    }
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "server-name", "Name" }
                            input {
                                id: "server-name",
                                autocapitalize: "words",
                                placeholder: "My Navidrome Server",
                                value: "{name}",
                                oninput: move |evt| name.set(evt.value()),
                            }
                        }
                    }
                    li {
                        div { class: "rs-settings-field rs-settings-field--switch",
                            span { id: "server-https-label", "Use HTTPS" }
                            Switch {
                                checked: use_https(),
                                labelled_by: "server-https-label".to_string(),
                                testid: "server-https".to_string(),
                                on_toggle: move |on| {
                                    https_touched.set(true);
                                    use_https.set(on);
                                    test.set(TestState::Idle);
                                },
                            }
                        }
                    }
                }
                p { class: "rs-list-foot rs-settings-foot", id: "server-url-hint",
                    "Use HTTPS for servers outside your home network. Home-network addresses like 192.168.1.10:4533 usually use HTTP."
                }

                ul { class: "rs-list rs-list--text rs-settings-fields",
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "server-user", "Username" }
                            input {
                                id: "server-user",
                                autocomplete: "username",
                                autocapitalize: "off",
                                spellcheck: "false",
                                placeholder: "admin",
                                value: "{user}",
                                oninput: move |evt| {
                                    user.set(evt.value());
                                    test.set(TestState::Idle);
                                },
                            }
                        }
                    }
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "server-pass", "Password" }
                            input {
                                id: "server-pass",
                                r#type: "password",
                                autocomplete: "current-password",
                                placeholder: "••••••••",
                                value: "{pass}",
                                oninput: move |evt| {
                                    pass.set(evt.value());
                                    test.set(TestState::Idle);
                                },
                            }
                        }
                    }
                }

                div { class: "rs-settings-test", role: "status", aria_live: "polite",
                    match test() {
                        TestState::Idle => rsx! {
                            if !is_edit {
                                span { class: "rs-settings-test__hint",
                                    "Test the connection to enable Save."
                                }
                            }
                        },
                        TestState::Testing => rsx! {
                            span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                            span { "Testing connection…" }
                        },
                        TestState::Passed(message) => rsx! {
                            span { class: "rs-settings-test__ok",
                                Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                                "{message}"
                            }
                        },
                        TestState::Failed(message) => rsx! {
                            span { class: "rs-settings-test__err", "data-testid": "server-test-error",
                                Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                                "{message}"
                            }
                        },
                    }
                }

                div { class: "rs-settings-form__actions",
                    Button {
                        label: "Test Connection".to_string(),
                        icon: "server".to_string(),
                        variant: ButtonVariant::Tinted,
                        // Not disabled while testing: disabling the focused button would drop
                        // focus out of the sheet (Escape stops working). run_test ignores repeats.
                        disabled: !complete,
                        testid: "server-test".to_string(),
                        onclick: run_test,
                    }
                    Button {
                        label: "Save".to_string(),
                        variant: ButtonVariant::Filled,
                        disabled: !can_save,
                        testid: "server-save".to_string(),
                        onclick: save,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_url_follows_the_switch() {
        assert_eq!(final_server_url("https://music.example.com/", false), "http://music.example.com");
        assert_eq!(final_server_url("music.example.com", true), "https://music.example.com");
        assert_eq!(final_server_url("http://192.168.1.10:4533/app/#/album", false), "http://192.168.1.10:4533");
        assert_eq!(final_server_url("  ", true), "");
    }

    #[test]
    fn https_guess_from_the_typed_url() {
        assert_eq!(https_for_url("https://x.y"), Some(true));
        assert_eq!(https_for_url("http://x.y"), Some(false));
        assert_eq!(https_for_url("music.example.com"), Some(true));
        assert_eq!(https_for_url("192.168.1.10:4533"), Some(false));
        assert_eq!(https_for_url("nas"), None);
    }

    #[test]
    fn name_falls_back_to_the_host() {
        assert_eq!(resolve_server_name("  ", "https://music.example.com:443"), "music.example.com");
        assert_eq!(resolve_server_name("Home", "https://x.y"), "Home");
    }

    #[test]
    fn errors_name_the_fix() {
        assert!(format_connection_error("Wrong username or password").starts_with("Wrong username"));
        assert!(format_connection_error("error sending request for url").starts_with("Couldn't reach"));
    }
}
