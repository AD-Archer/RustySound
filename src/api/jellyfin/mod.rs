//! Jellyfin client: the `Jellyfin` variant of [`crate::api::MusicClient`],
//! with `NavidromeClient`'s method names so views call either the same way.
//!
//! Requests carry `Authorization: MediaBrowser …` (Jellyfin 12 turned the
//! `X-Emby-*` headers off). Media URLs, which `<audio>` and `<img>` load
//! without headers, carry `ApiKey=<token>` instead, never both. Lists are
//! paged with `startIndex` / `limit`, ask only for the `fields` they show,
//! and skip the total count. See `.redesign/research/jellyfin.md`.

pub mod dto;
mod library;
mod playback;

use crate::api::{device_label, ServerConfig};
use crate::cache_service::is_offline_mode;
use dto::{AuthenticationResult, PublicSystemInfo};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

const CLIENT_NAME: &str = "RustySound";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct JellyfinClient {
    pub server: ServerConfig,
}

/// The `Authorization` header value. Keys are case-sensitive and every
/// value is quoted; quotes and commas are dropped from values so they can't
/// break the list. Pure; used by tests.
pub fn auth_header(device: &str, device_id: &str, version: &str, token: Option<&str>) -> String {
    let clean = |value: &str| value.chars().filter(|c| !matches!(c, '"' | ',' | '\r' | '\n')).collect::<String>();
    let mut header = format!(
        "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
        clean(CLIENT_NAME),
        clean(device),
        clean(device_id),
        clean(version)
    );
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        header.push_str(&format!(", Token=\"{}\"", clean(token)));
    }
    header
}

/// Jellyfin keeps one token per DeviceId, so the id is per install *and*
/// user: two accounts on one device don't sign each other out. Stable, so a
/// fresh sign-in replaces this device's old session instead of adding one.
pub fn device_id_for(install_id: &str, username: &str) -> String {
    format!("{:x}", md5::compute(format!("{install_id}:{}", username.trim().to_lowercase())))
}

/// A random id made once per install and kept with the settings. Tests
/// use a fixed one, so they never touch the app's database.
#[cfg(test)]
fn install_id() -> &'static str {
    "test-install"
}

#[cfg(not(test))]
fn install_id() -> &'static str {
    const INSTALL_ID_KEY: &str = "jellyfin_install_id";
    // One per device, shared by every profile: Jellyfin ties tokens to it.
    static ID: Lazy<String> = Lazy::new(|| {
        crate::db::load_device_value::<String>(INSTALL_ID_KEY)
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = crate::db::save_device_value(INSTALL_ID_KEY, &id);
                id
            })
    });
    &ID
}

fn device_id(username: &str) -> String {
    device_id_for(install_id(), username)
}

fn base_url(url: &str) -> &str {
    url.trim().trim_end_matches('/')
}

/// `path?k=v&…` with every value encoded.
fn with_query(base: &str, path: &str, query: &[(&str, String)]) -> String {
    let mut url = format!("{}{}", base_url(base), path);
    for (i, (key, value)) in query.iter().enumerate() {
        url.push(if i == 0 { '?' } else { '&' });
        url.push_str(key);
        url.push('=');
        url.push_str(&urlencoding::encode(value));
    }
    url
}

async fn check(response: reqwest::Response) -> Result<reqwest::Response, String> {
    let status = response.status();
    if status.is_success() {
        Ok(response)
    } else {
        Err(status.to_string())
    }
}

impl JellyfinClient {
    pub fn new(server: ServerConfig) -> Self {
        Self { server }
    }

    fn device_id(&self) -> String {
        device_id(&self.server.username)
    }

    fn auth(&self) -> String {
        auth_header(device_label(), &self.device_id(), CLIENT_VERSION, Some(&self.server.access_token))
    }

    fn url(&self, path: &str, query: &[(&str, String)]) -> String {
        with_query(&self.server.url, path, query)
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        if is_offline_mode() {
            return Err("Offline mode is on.".to_string());
        }
        let mut request = HTTP_CLIENT
            .request(method, self.url(path, query))
            .header("Authorization", self.auth());
        if let Some(body) = body {
            request = request.json(&body);
        }
        check(request.send().await.map_err(|e| e.to_string())?).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T, String> {
        self.send(reqwest::Method::GET, path, query, None)
            .await?
            .json::<T>()
            .await
            .map_err(|e| e.to_string())
    }

    /// Asks an address whether Jellyfin answers there. `None` for anything
    /// else (Navidrome, a typo, no answer).
    pub async fn probe(url: &str) -> Option<PublicSystemInfo> {
        let response = HTTP_CLIENT
            .get(with_query(url, "/System/Info/Public", &[]))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json::<PublicSystemInfo>().await.ok().filter(PublicSystemInfo::is_jellyfin)
    }

    /// Signs in with a username and password. Returns the user id and the
    /// access token; the password isn't needed again.
    pub async fn authenticate(url: &str, username: &str, password: &str) -> Result<(String, String), String> {
        let header = auth_header(device_label(), &device_id(username), CLIENT_VERSION, None);
        let response = HTTP_CLIENT
            .post(with_query(url, "/Users/AuthenticateByName", &[]))
            .header("Authorization", header)
            .json(&serde_json::json!({ "Username": username, "Pw": password }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let result: AuthenticationResult = check(response).await?.json().await.map_err(|e| e.to_string())?;
        let token = result.access_token.filter(|t| !t.is_empty());
        let user_id = result.user.map(|u| u.id).filter(|id| !id.is_empty());
        match (user_id, token) {
            (Some(user_id), Some(token)) => Ok((user_id, token)),
            _ => Err("The server didn't return a session (missing field AccessToken).".to_string()),
        }
    }

    /// Checks the saved token still works.
    pub async fn ping(&self) -> Result<bool, String> {
        self.send(reqwest::Method::GET, "/Users/Me", &[], None).await.map(|_| true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_auth_header_is_quoted_and_carries_the_token() {
        assert_eq!(
            auth_header("Mac", "abc", "3.0.0", Some("tok")),
            r#"MediaBrowser Client="RustySound", Device="Mac", DeviceId="abc", Version="3.0.0", Token="tok""#
        );
        // Signing in: no token yet. Stray quotes can't break the list.
        assert_eq!(
            auth_header("Ro\"b's, Mac", "abc", "3.0.0", None),
            r#"MediaBrowser Client="RustySound", Device="Rob's Mac", DeviceId="abc", Version="3.0.0""#
        );
    }

    #[test]
    fn device_ids_are_stable_per_install_and_user() {
        let a = device_id_for("install-1", "Demo");
        assert_eq!(a, device_id_for("install-1", " demo "));
        assert_ne!(a, device_id_for("install-1", "other"));
        assert_ne!(a, device_id_for("install-2", "demo"));
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(
            with_query("http://h:8096/", "/Items", &[("searchTerm", "a b&c".into()), ("limit", "5".into())]),
            "http://h:8096/Items?searchTerm=a%20b%26c&limit=5"
        );
    }
}
