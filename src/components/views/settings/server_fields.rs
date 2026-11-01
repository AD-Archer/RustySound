//! The server sign-in form, shared by the first-run welcome and the Add /
//! Edit server sheet in Settings.
//!
//! One primary action tests the connection and saves in the same step, with
//! a progress state on the button. Fields have their labels above them,
//! placeholders that show the expected shape, the keyboard hints iOS needs
//! (no capitalising, URL keyboard, saved passwords), and Return moves to the
//! next field and finally signs in. HTTPS follows the typed address; the line
//! under the address says which one will be used and offers the other.
//!
//! Jellyfin is found, not chosen: leaving the address (or signing in) asks
//! it for `/System/Info/Public`, and a Jellyfin answer adds a quiet
//! "Jellyfin server" to that line and signs in with Jellyfin's own API,
//! keeping its access token rather than the password.

use super::server_form::{
    check_address, final_server_url, format_connection_error, https_for_url, resolve_server_name,
    url_host,
};
use crate::api::{JellyfinClient, NavidromeClient, ServerConfig, ServerKind};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{Button, ButtonVariant};
use crate::components::Icon;
use crate::db::save_servers_now;
use dioxus::prelude::*;
use futures_util::future::{select, Either};

/// How long a sign-in waits for the server before giving up.
const PING_TIMEOUT_MS: u64 = 15_000;
/// How long the Jellyfin probe waits. Anything slower is taken as Subsonic.
const PROBE_TIMEOUT_MS: u64 = 5_000;
/// The "Connected" state stays up this long before the form closes.
const SUCCESS_HOLD_MS: u64 = 450;

/// Where the form is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FormPlace {
    /// First run: no Name field (the host names the server), a full-width
    /// "Sign In".
    Welcome,
    /// Settings sheet: Name field, "Add Server" / "Save", and "Save Anyway"
    /// after a failed test.
    Sheet,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Url,
    Name,
    User,
    Pass,
}

impl Field {
    fn id(self) -> &'static str {
        match self {
            Field::Url => "server-url",
            Field::Name => "server-name",
            Field::User => "server-user",
            Field::Pass => "server-pass",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Phase {
    Idle,
    Testing,
    Passed(String),
    Failed(String),
}

/// Which field a connection error is about, so focus can go there.
fn field_for_error(message: &str) -> Field {
    if message.starts_with("Wrong username or password") {
        Field::Pass
    } else if message.starts_with("The server denied access") {
        Field::User
    } else {
        Field::Url
    }
}

fn focus_field(field: Field, select_text: bool) {
    let id = field.id();
    let select = if select_text { "el.select && el.select();" } else { "" };
    let _ = document::eval(&format!(
        "const el = document.getElementById('{id}'); if (el) {{ el.focus(); {select} }}"
    ));
}

/// What a successful sign-in leaves to save.
#[derive(Clone, PartialEq, Debug)]
enum Credentials {
    /// Subsonic signs every request with the password, so it's kept.
    Password(String),
    /// Jellyfin: the session from `AuthenticateByName`; no password kept.
    Session { user_id: String, token: String },
}

/// Runs `request`, giving up after `ms`.
async fn with_timeout<T>(ms: u64, request: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    match select(Box::pin(request), Box::pin(sleep_ms(ms))).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err("timed out".to_string()),
    }
}

/// Pings with a timeout, so a server that never answers doesn't spin forever.
async fn ping_server(probe: ServerConfig) -> Result<(), String> {
    with_timeout(PING_TIMEOUT_MS, async move { NavidromeClient::new(probe).ping().await.map(|_| ()) }).await
}

/// Which API answers at `address`: Jellyfin when its public info does.
async fn probe_kind(address: String) -> ServerKind {
    let probe = async move { Ok(JellyfinClient::probe(&address).await) };
    match with_timeout(PROBE_TIMEOUT_MS, probe).await {
        Ok(Some(_)) => ServerKind::Jellyfin,
        _ => ServerKind::Subsonic,
    }
}

/// Signs in to Jellyfin with a timeout.
async fn sign_in_jellyfin(address: String, username: String, password: String) -> Result<Credentials, String> {
    let request = async move { JellyfinClient::authenticate(&address, &username, &password).await };
    let (user_id, token) = with_timeout(PING_TIMEOUT_MS, request).await?;
    Ok(Credentials::Session { user_id, token })
}

/// The form.
/// * `place`: Welcome (first run) or Sheet (Settings).
/// * `editing`: the server to edit; `None` adds one.
/// * `on_saved`: called with the saved server after it is persisted.
/// * `on_delete`: shows "Delete Server…" under the form (edit mode).
#[component]
pub fn ServerForm(
    place: FormPlace,
    #[props(default)] editing: Option<ServerConfig>,
    on_saved: EventHandler<ServerConfig>,
    #[props(default)] on_delete: Option<EventHandler<()>>,
) -> Element {
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let is_edit = editing.is_some();
    let welcome = place == FormPlace::Welcome;
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
    // Once the user picks HTTP or HTTPS, typing in the address no longer changes it.
    let mut https_touched = use_signal(|| is_edit);
    let mut show_pass = use_signal(|| false);
    let mut phase = use_signal(|| Phase::Idle);
    // Field errors show after a field is left (address) or on Sign In.
    let mut url_error = use_signal(|| None::<String>);
    let mut user_error = use_signal(|| None::<String>);
    let mut pass_error = use_signal(|| None::<String>);
    // The API found at an address (the probe runs once per address).
    let probed = use_signal(|| initial.as_ref().map(|s| (s.url.clone(), s.kind)));

    let busy = matches!(phase(), Phase::Testing | Phase::Passed(_));
    let failed = matches!(phase(), Phase::Failed(_));
    let address_ok = check_address(&url()).is_ok();
    let target = if address_ok { final_server_url(&url(), use_https()) } else { String::new() };
    // Shown only while the address is the one that answered as Jellyfin.
    let jellyfin = probed
        .read()
        .as_ref()
        .is_some_and(|(at, found)| address_ok && *at == target && *found == ServerKind::Jellyfin);

    // Asks the address which API it speaks, unless it was asked already.
    let detect = move |address: String| async move {
        let mut probed = probed;
        let known = probed.peek().clone().filter(|(at, _)| *at == address).map(|(_, k)| k);
        let found = match known {
            Some(found) => found,
            None => probe_kind(address.clone()).await,
        };
        probed.set(Some((address, found)));
        found
    };

    // An edited Jellyfin server keeps its session while its address and
    // user stay the same and no new password is typed.
    let kept_session = {
        let editing = editing.clone();
        move |address: &str, username: &str, password: &str| {
            editing
                .as_ref()
                .filter(|s| s.is_jellyfin() && !s.access_token.is_empty())
                .filter(|s| s.url == address && s.username == username && password.is_empty())
                .map(|s| Credentials::Session { user_id: s.user_id.clone(), token: s.access_token.clone() })
        }
    };

    // Persists the form's values and hands the server to the caller.
    let editing_for_save = editing.clone();
    let commit = move |credentials: Credentials| {
        let address = final_server_url(&url.peek(), *use_https.peek());
        let display = resolve_server_name(&name.peek(), &address);
        let username = user.peek().trim().to_string();
        let mut saved = match editing_for_save.clone() {
            Some(original) => original,
            None => ServerConfig::new(String::new(), String::new(), String::new(), String::new()),
        };
        saved.name = display;
        saved.url = address;
        saved.username = username;
        match credentials {
            Credentials::Password(password) => {
                saved.kind = ServerKind::Subsonic;
                saved.password = password;
                saved.user_id.clear();
                saved.access_token.clear();
            }
            Credentials::Session { user_id, token } => {
                saved.kind = ServerKind::Jellyfin;
                saved.password.clear();
                saved.user_id = user_id;
                saved.access_token = token;
            }
        }
        servers.with_mut(|list| match list.iter_mut().find(|s| s.id == saved.id) {
            Some(slot) => *slot = saved.clone(),
            None => list.push(saved.clone()),
        });
        let _ = save_servers_now(&servers.peek());
        on_saved.call(saved);
    };

    // Editing only the name doesn't need a round trip to the server.
    let editing_for_submit = editing.clone();
    let commit_after_test = commit.clone();
    let kept_session_for_submit = kept_session.clone();
    let submit = move || {
        if matches!(*phase.peek(), Phase::Testing | Phase::Passed(_)) {
            return;
        }
        let url_check = check_address(&url.peek()).err();
        let user_check = user
            .peek()
            .trim()
            .is_empty()
            .then(|| "Enter your username.".to_string());
        let keeps_session = url_check.is_none()
            && kept_session_for_submit(
                &final_server_url(&url.peek(), *use_https.peek()),
                user.peek().trim(),
                &pass.peek(),
            )
            .is_some();
        let pass_check = (pass.peek().is_empty() && !keeps_session).then(|| "Enter your password.".to_string());
        let first_invalid = if url_check.is_some() {
            Some(Field::Url)
        } else if user_check.is_some() {
            Some(Field::User)
        } else if pass_check.is_some() {
            Some(Field::Pass)
        } else {
            None
        };
        url_error.set(url_check);
        user_error.set(user_check);
        pass_error.set(pass_check);
        if let Some(field) = first_invalid {
            phase.set(Phase::Idle);
            focus_field(field, false);
            return;
        }

        let address = final_server_url(&url.peek(), *use_https.peek());
        let username = user.peek().trim().to_string();
        let password = pass.peek().clone();
        let mut commit = commit_after_test.clone();
        if let Some(session) = kept_session_for_submit(&address, &username, &password) {
            commit(session);
            return;
        }
        let unchanged = editing_for_submit
            .as_ref()
            .map(|s| !s.is_jellyfin() && s.url == address && s.username == username && s.password == password)
            .unwrap_or(false);
        if unchanged {
            commit(Credentials::Password(password));
            return;
        }

        phase.set(Phase::Testing);
        spawn(async move {
            let found = detect(address.clone()).await;
            let signed_in = match found {
                ServerKind::Jellyfin => sign_in_jellyfin(address, username.clone(), password).await,
                ServerKind::Subsonic => {
                    let probe = ServerConfig::new("Test".to_string(), address, username.clone(), password.clone());
                    ping_server(probe).await.map(|_| Credentials::Password(password))
                }
            };
            match signed_in {
                Ok(credentials) => {
                    phase.set(Phase::Passed(format!("Connected. Signed in as {username}.")));
                    sleep_ms(SUCCESS_HOLD_MS).await;
                    commit(credentials);
                }
                Err(error) => {
                    let message = format_connection_error(&error);
                    let field = field_for_error(&message);
                    phase.set(Phase::Failed(message));
                    focus_field(field, field == Field::Pass);
                }
            }
        });
    };

    // Return moves on; on the last field it signs in.
    let next_on_enter = move |evt: KeyboardEvent, next: Option<Field>, mut submit: Box<dyn FnMut()>| {
        if evt.key() != Key::Enter {
            return;
        }
        evt.prevent_default();
        match next {
            Some(field) => focus_field(field, false),
            None => submit(),
        }
    };

    let primary_label = match (place, is_edit, phase()) {
        (FormPlace::Welcome, _, Phase::Testing) => "Signing In…",
        (FormPlace::Welcome, _, Phase::Passed(_)) => "Signed In",
        (FormPlace::Sheet, _, Phase::Testing) => "Connecting…",
        (FormPlace::Sheet, _, Phase::Passed(_)) => "Connected",
        (FormPlace::Welcome, _, _) => "Sign In",
        (FormPlace::Sheet, true, _) => "Save",
        (FormPlace::Sheet, false, _) => "Add Server",
    };
    let host_hint = url_host(&url());
    let name_placeholder = if host_hint.is_empty() { "Home Server".to_string() } else { host_hint };
    let url_described = if url_error().is_some() || !target.is_empty() { "server-url-hint" } else { "server-url-help" };

    rsx! {
        div { class: "rs-signin-form", class: if welcome { "rs-signin-form--welcome" },
            // ---- Server address -------------------------------------------
            div { class: "rs-signin-group",
                div {
                    class: "rs-signin-field",
                    class: if url_error().is_some() { "rs-signin-field--error" },
                    label { r#for: "server-url", "Server address" }
                    input {
                        id: "server-url",
                        name: "server",
                        r#type: "url",
                        inputmode: "url",
                        autocomplete: "url",
                        autocapitalize: "none",
                        "autocorrect": "off",
                        spellcheck: "false",
                        "enterkeyhint": "next",
                        placeholder: "music.example.com",
                        readonly: busy,
                        aria_invalid: if url_error().is_some() { "true" } else { "false" },
                        aria_describedby: url_described,
                        value: "{url}",
                        oninput: move |evt| {
                            let value = evt.value();
                            if !*https_touched.peek() {
                                if let Some(secure) = https_for_url(&value) {
                                    use_https.set(secure);
                                }
                            }
                            // An error clears as soon as the address becomes valid.
                            if url_error.peek().is_some() && check_address(&value).is_ok() {
                                url_error.set(None);
                            }
                            url.set(value);
                            if failed {
                                phase.set(Phase::Idle);
                            }
                        },
                        onblur: move |_| {
                            let value = url.peek().clone();
                            if !value.trim().is_empty() {
                                let check = check_address(&value);
                                if check.is_ok() {
                                    let address = final_server_url(&value, *use_https.peek());
                                    spawn(async move {
                                        detect(address).await;
                                    });
                                }
                                url_error.set(check.err());
                            }
                        },
                        onkeydown: {
                            let submit = submit.clone();
                            move |evt| next_on_enter(evt, Some(if welcome { Field::User } else { Field::Name }), Box::new(submit.clone()))
                        },
                    }
                }
            }
            AddressHint {
                error: url_error(),
                target: target.clone(),
                secure: use_https(),
                jellyfin,
                busy,
                on_switch: move |_| {
                    https_touched.set(true);
                    use_https.toggle();
                    if failed {
                        phase.set(Phase::Idle);
                    }
                },
            }

            // ---- Name (Settings only) --------------------------------------
            if !welcome {
                div { class: "rs-signin-group",
                    div { class: "rs-signin-field",
                        label { r#for: "server-name", "Name "
                            span { class: "rs-signin-field__optional", "(optional)" }
                        }
                        input {
                            id: "server-name",
                            name: "server-name",
                            autocomplete: "off",
                            autocapitalize: "words",
                            "enterkeyhint": "next",
                            placeholder: "{name_placeholder}",
                            readonly: busy,
                            value: "{name}",
                            oninput: move |evt| name.set(evt.value()),
                            onkeydown: {
                                let submit = submit.clone();
                                move |evt| next_on_enter(evt, Some(Field::User), Box::new(submit.clone()))
                            },
                        }
                    }
                }
            }

            // ---- Account ---------------------------------------------------
            div { class: "rs-signin-group",
                div {
                    class: "rs-signin-field",
                    class: if user_error().is_some() { "rs-signin-field--error" },
                    label { r#for: "server-user", "Username" }
                    input {
                        id: "server-user",
                        name: "username",
                        autocomplete: "username",
                        autocapitalize: "none",
                        "autocorrect": "off",
                        spellcheck: "false",
                        "enterkeyhint": "next",
                        placeholder: if jellyfin { "Your Jellyfin username" } else { "Your Navidrome username" },
                        readonly: busy,
                        aria_invalid: if user_error().is_some() { "true" } else { "false" },
                        aria_describedby: "server-account-hint",
                        value: "{user}",
                        oninput: move |evt| {
                            if !evt.value().trim().is_empty() {
                                user_error.set(None);
                            }
                            user.set(evt.value());
                            if failed {
                                phase.set(Phase::Idle);
                            }
                        },
                        onkeydown: {
                            let submit = submit.clone();
                            move |evt| next_on_enter(evt, Some(Field::Pass), Box::new(submit.clone()))
                        },
                    }
                }
                div {
                    class: "rs-signin-field rs-signin-field--pass",
                    class: if pass_error().is_some() { "rs-signin-field--error" },
                    label { r#for: "server-pass", "Password" }
                    div { class: "rs-signin-field__row",
                        input {
                            id: "server-pass",
                            name: "password",
                            r#type: if show_pass() { "text" } else { "password" },
                            autocomplete: "current-password",
                            autocapitalize: "none",
                            "autocorrect": "off",
                            spellcheck: "false",
                            "enterkeyhint": "go",
                            placeholder: "Required",
                            readonly: busy,
                            aria_invalid: if pass_error().is_some() { "true" } else { "false" },
                            aria_describedby: "server-account-hint",
                            value: "{pass}",
                            oninput: move |evt| {
                                if !evt.value().is_empty() {
                                    pass_error.set(None);
                                }
                                pass.set(evt.value());
                                if failed {
                                    phase.set(Phase::Idle);
                                }
                            },
                            onkeydown: {
                                let submit = submit.clone();
                                move |evt| next_on_enter(evt, None, Box::new(submit.clone()))
                            },
                        }
                        button {
                            r#type: "button",
                            class: "rs-signin-reveal",
                            "data-testid": "server-pass-toggle",
                            aria_label: if show_pass() { "Hide password" } else { "Show password" },
                            aria_pressed: if show_pass() { "true" } else { "false" },
                            aria_controls: "server-pass",
                            onclick: move |_| show_pass.toggle(),
                            Icon {
                                name: if show_pass() { "eye-off".to_string() } else { "eye".to_string() },
                                class: "rs-ic rs-ic--sm".to_string(),
                            }
                        }
                    }
                }
            }
            {
                let account_error = user_error().or(pass_error());
                rsx! {
                    if let Some(error) = account_error {
                        p { class: "rs-signin-hint rs-signin-hint--error", id: "server-account-hint",
                            Icon { name: "alert".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                            span { "{error}" }
                        }
                    } else {
                        p { class: "rs-signin-hint", id: "server-account-hint",
                            if welcome && jellyfin {
                                "Your password is only sent to your server, to sign in. It isn't kept on this device."
                            } else if welcome {
                                "Your password stays on this device and is only ever sent to your server."
                            } else {
                                if jellyfin { "The account you sign in to Jellyfin with." } else { "The account you sign in to Navidrome with." }
                            }
                        }
                    }
                }
            }

            // ---- Outcome and actions ---------------------------------------
            SignInStatus { phase: phase(), host: url_host(&url()) }

            div { class: "rs-signin-actions",
                // A raw button so it can carry the spinner. Never disabled while
                // busy (that would drop focus out of a sheet); repeats are ignored.
                button {
                    r#type: "button",
                    class: "rs-btn rs-btn--filled rs-signin-primary",
                    class: if welcome { "rs-btn--block" },
                    class: if busy { "rs-signin-primary--busy" },
                    "data-testid": "server-save",
                    aria_busy: if busy { "true" } else { "false" },
                    onclick: {
                        let mut submit = submit.clone();
                        move |_| submit()
                    },
                    match phase() {
                        Phase::Testing => rsx! { span { class: "rs-signin-spinner", aria_hidden: "true" } },
                        Phase::Passed(_) => rsx! { Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() } },
                        _ => rsx! {},
                    }
                    "{primary_label}"
                }
                if failed && !welcome {
                    Button {
                        label: "Save Anyway".to_string(),
                        variant: ButtonVariant::Plain,
                        testid: "server-save-anyway".to_string(),
                        onclick: {
                            let mut commit = commit.clone();
                            let kept_session = kept_session.clone();
                            // A Jellyfin server saved untested keeps the session it had (if any).
                            move |_| {
                                let address = final_server_url(&url.peek(), *use_https.peek());
                                let credentials = if jellyfin {
                                    kept_session(&address, user.peek().trim(), "").unwrap_or(Credentials::Session {
                                        user_id: String::new(),
                                        token: String::new(),
                                    })
                                } else {
                                    Credentials::Password(pass.peek().clone())
                                };
                                commit(credentials)
                            }
                        },
                    }
                }
            }

            if let Some(handler) = on_delete {
                div { class: "rs-settings-form__delete",
                    Button {
                        label: "Delete Server…".to_string(),
                        icon: "trash".to_string(),
                        variant: ButtonVariant::Plain,
                        danger: true,
                        testid: "server-delete".to_string(),
                        onclick: move |_| handler.call(()),
                    }
                }
            }
        }
    }
}

/// The line under the address: what's wrong with it, or the exact address
/// that will be used with "Use HTTP" / "Use HTTPS", or what to type.
/// `jellyfin`: the address answered as Jellyfin, which the line says.
#[component]
fn AddressHint(
    error: Option<String>,
    target: String,
    secure: bool,
    jellyfin: bool,
    busy: bool,
    on_switch: EventHandler<()>,
) -> Element {
    if let Some(error) = error {
        return rsx! {
            p { class: "rs-signin-hint rs-signin-hint--error", id: "server-url-hint",
                Icon { name: "alert".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                span { "{error}" }
            }
        };
    }
    if target.is_empty() {
        return rsx! {
            p { class: "rs-signin-hint", id: "server-url-help",
                "The address you open Navidrome or Jellyfin at. A home server looks like 192.168.1.10:4533."
            }
        };
    }
    rsx! {
        p { class: "rs-signin-hint", id: "server-url-hint", "data-testid": "server-scheme",
            Icon {
                name: if secure { "lock".to_string() } else { "home".to_string() },
                class: "rs-ic rs-ic--xs".to_string(),
            }
            span { class: "rs-signin-hint__text",
                if secure { "Connects securely to " } else { "Connects without encryption to " }
                span { class: "rs-signin-hint__url", "{target}" }
                if jellyfin {
                    span { "data-testid": "server-kind", " · Jellyfin server" }
                }
            }
            button {
                r#type: "button",
                class: "rs-signin-hint__switch",
                "data-testid": "server-https",
                disabled: busy,
                onclick: move |_| on_switch.call(()),
                if secure { "Use HTTP" } else { "Use HTTPS" }
            }
        }
    }
}

/// The live outcome line: connecting, connected, or what went wrong and how
/// to fix it.
#[component]
fn SignInStatus(phase: Phase, host: String) -> Element {
    rsx! {
        div { class: "rs-signin-status", role: "status", aria_live: "polite",
            match phase {
                Phase::Idle => rsx! {},
                Phase::Testing => rsx! {
                    span { class: "rs-signin-status__line", "Connecting to {host}…" }
                },
                Phase::Passed(message) => rsx! {
                    span { class: "rs-signin-status__ok", "data-testid": "server-test-ok",
                        Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        "{message}"
                    }
                },
                Phase::Failed(message) => rsx! {
                    div { class: "rs-signin-status__err", "data-testid": "server-test-error",
                        Icon { name: "alert".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                        span { "{message}" }
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_point_at_their_field() {
        assert_eq!(field_for_error(&format_connection_error("401 Unauthorized")), Field::Pass);
        assert_eq!(field_for_error(&format_connection_error("403 forbidden")), Field::User);
        assert_eq!(field_for_error(&format_connection_error("connection refused")), Field::Url);
    }
}
