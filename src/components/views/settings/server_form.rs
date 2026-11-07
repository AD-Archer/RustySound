//! Server address helpers (scheme, host, validation, friendly connection
//! errors) and the Add / Edit server sheet (`data-testid="server-form"`) in
//! Settings → Servers. The form itself is [`ServerForm`], which the first-run
//! welcome shows too.

use super::server_fields::{FormPlace, ServerForm};
use crate::api::ServerConfig;
use crate::components::ui::Sheet;
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
pub(super) fn url_host(raw: &str) -> String {
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
pub(super) fn https_for_url(raw: &str) -> Option<bool> {
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

/// Checks a typed server address before any request. `Err` says what to fix.
pub(super) fn check_address(raw: &str) -> Result<(), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Enter your server's address.".to_string());
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err("An address can't contain spaces.".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if let Some(at) = lower.find("://") {
        if !matches!(&lower[..at], "http" | "https") {
            return Err("Use an address that starts with http:// or https://, or leave the start off.".to_string());
        }
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("HTTPS://"))
        .or_else(|| trimmed.strip_prefix("HTTP://"))
        .unwrap_or(trimmed);
    let authority = rest.split('/').next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default();
    if authority.starts_with('[') {
        // IPv6 literal: leave it to the request.
        return Ok(());
    }
    if url_host(trimmed).is_empty() {
        return Err("Add the server's name or IP address, like music.example.com.".to_string());
    }
    if let Some((_, port)) = authority.split_once(':') {
        if port.parse::<u16>().map(|p| p == 0).unwrap_or(true) {
            return Err("The port after the colon should be a number, like :4533.".to_string());
        }
    }
    Ok(())
}

pub(super) fn resolve_server_name(name: &str, url: &str) -> String {
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
        return "The HTTPS certificate was rejected. For a home-network server, choose Use HTTP under the address, or fix the certificate.".to_string();
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
        return "Couldn't reach the server. Check the address, the port, and whether the server uses HTTP or HTTPS.".to_string();
    }
    format!("Couldn't connect ({error}). Check the address and your network.")
}

/// Add/Edit server sheet: the shared [`ServerForm`] in a sheet.
/// * `editing`: the server to edit; `None` adds a new one.
/// * `on_saved`: called with the saved server after it is persisted.
/// * `title`: sheet title override.
/// * `on_delete`: shows "Delete Server…" under the form (edit mode).
#[component]
pub(super) fn ServerSheet(
    #[props(default)] editing: Option<ServerConfig>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<ServerConfig>,
    #[props(default)] title: Option<String>,
    #[props(default)] on_delete: Option<EventHandler<()>>,
) -> Element {
    let sheet_title = title.unwrap_or_else(|| {
        if editing.is_some() {
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
            ServerForm {
                place: FormPlace::Sheet,
                editing,
                on_saved: move |server| on_saved.call(server),
                on_delete,
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
    fn addresses_are_checked_before_any_request() {
        assert!(check_address("music.example.com").is_ok());
        assert!(check_address("https://music.example.com/").is_ok());
        assert!(check_address("192.168.1.10:4533").is_ok());
        assert!(check_address("http://user@nas.local:4533/app").is_ok());
        assert!(check_address("nas").is_ok());
        assert!(check_address("[::1]:4533").is_ok());
        assert_eq!(check_address("  ").unwrap_err(), "Enter your server's address.");
        assert!(check_address("music example.com").unwrap_err().contains("spaces"));
        assert!(check_address("ftp://x.y").unwrap_err().contains("http://"));
        assert!(check_address("https://").unwrap_err().contains("name or IP"));
        assert!(check_address("x.y:port").unwrap_err().contains("port"));
        assert!(check_address("x.y:99999").unwrap_err().contains("port"));
    }

    #[test]
    fn errors_name_the_fix() {
        assert!(format_connection_error("Wrong username or password").starts_with("Wrong username"));
        assert!(format_connection_error("error sending request for url").starts_with("Couldn't reach"));
    }
}
