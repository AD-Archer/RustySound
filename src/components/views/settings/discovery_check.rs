//! The "Test" calls behind Settings → Discovery: one small, real request per
//! account, turned into a sentence that says what works or how to fix it.

use crate::components::ui::env::sleep_ms;
use futures_util::future::{select, Either};
use serde_json::Value;

const LASTFM_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
const LISTENBRAINZ_ROOT: &str = "https://api.listenbrainz.org/1/user";
const TIMEOUT_MS: u64 = 8_000;

/// GET a JSON body, keeping non-2xx bodies that parse (both APIs explain
/// their errors in JSON). The URL is never echoed: it can hold the API key.
async fn get_json(url: &str) -> Result<(u16, Value), String> {
    if crate::cache_service::is_offline_mode() {
        return Err("Offline mode is on. Turn it off in Playback to run this test.".to_string());
    }
    let request = async {
        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_err(|_| "Couldn't reach the service. Check your internet connection.".to_string())?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .await
            .map_err(|_| "The service sent an unreadable answer. Try again.".to_string())?;
        match serde_json::from_str::<Value>(&text) {
            Ok(value) => Ok((status, value)),
            Err(_) => Err(format!("The service answered with HTTP {status} and no details. Try again later.")),
        }
    };
    let timer = sleep_ms(TIMEOUT_MS);
    match select(Box::pin(request), Box::pin(timer)).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err("The service took too long to answer. Try again.".to_string()),
    }
}

fn number(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

/// A Last.fm API key is 32 hexadecimal characters.
pub(super) fn looks_like_lastfm_key(key: &str) -> bool {
    let key = key.trim();
    key.len() == 32 && key.chars().all(|c| c.is_ascii_hexdigit())
}

/// Last.fm error envelope → what to do. `None` when the body isn't an error.
fn lastfm_error(value: &Value, user: Option<&str>) -> Option<String> {
    let code = value.get("error").and_then(number)?;
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    Some(match code {
        10 => "Last.fm rejected this API key (error 10). Copy the “API key”, not the “Shared secret”, from last.fm/api/accounts.".to_string(),
        26 => "Last.fm has suspended this API key (error 26). Create a new one at last.fm/api/account/create.".to_string(),
        29 => "Last.fm is rate-limiting this key (error 29). Wait a minute and test again.".to_string(),
        6 => match user {
            Some(name) => format!("Last.fm has no user called “{name}”. Use the name in your profile address, last.fm/user/…"),
            None => format!("Last.fm couldn't find that (error 6: {message})."),
        },
        11 | 16 => "Last.fm is having trouble right now. Try again in a few minutes.".to_string(),
        _ => format!("Last.fm said: {message} (error {code})."),
    })
}

/// `user.getInfo` body → "Connected as rj · 1,204 scrobbles".
pub(super) fn parse_lastfm_user(value: &Value, asked: &str) -> Result<String, String> {
    if let Some(error) = lastfm_error(value, Some(asked)) {
        return Err(error);
    }
    let user = value.get("user").ok_or("Last.fm sent an unexpected answer. Try again.")?;
    let name = user.get("name").and_then(Value::as_str).unwrap_or(asked);
    let plays = user.get("playcount").and_then(number).unwrap_or(0);
    Ok(format!(
        "Connected as {name} · {} scrobbles",
        super::shared::group_thousands(plays)
    ))
}

/// ListenBrainz `listen-count` body → "Found rj · 3,210 listens".
pub(super) fn parse_listen_count(status: u16, value: &Value, asked: &str) -> Result<String, String> {
    if status == 404 {
        return Err(format!(
            "ListenBrainz has no user called “{asked}”. Use the name from your profile address, listenbrainz.org/user/…"
        ));
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(format!("ListenBrainz said: {error} (HTTP {status})."));
    }
    let count = value
        .get("payload")
        .and_then(|p| p.get("count"))
        .and_then(number)
        .ok_or("ListenBrainz sent an unexpected answer. Try again.")?;
    Ok(format!(
        "Found {asked} · {} listens",
        super::shared::group_thousands(count)
    ))
}

/// Checks the API key alone with the smallest public call.
pub(super) async fn test_lastfm_key(key: String) -> Result<String, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("Paste your Last.fm API key first.".to_string());
    }
    if !looks_like_lastfm_key(&key) {
        return Err("That doesn't look like a Last.fm API key: it should be 32 letters and digits (0–9, a–f).".to_string());
    }
    let url = format!(
        "{LASTFM_ROOT}?method=chart.gettopartists&limit=1&format=json&api_key={}",
        urlencoding::encode(&key)
    );
    let (_, body) = get_json(&url).await?;
    if let Some(error) = lastfm_error(&body, None) {
        return Err(error);
    }
    if body.get("artists").is_some() {
        Ok("API key works.".to_string())
    } else {
        Err("Last.fm sent an unexpected answer. Try again.".to_string())
    }
}

/// Checks the username (needs the key: Last.fm requires one for every call).
pub(super) async fn test_lastfm_user(key: String, user: String) -> Result<String, String> {
    let user = user.trim().to_string();
    if user.is_empty() {
        return Err("Type your Last.fm username first.".to_string());
    }
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("Add your Last.fm API key above first: Last.fm needs it to look up users.".to_string());
    }
    let url = format!(
        "{LASTFM_ROOT}?method=user.getinfo&format=json&user={}&api_key={}",
        urlencoding::encode(&user),
        urlencoding::encode(&key)
    );
    let (_, body) = get_json(&url).await?;
    parse_lastfm_user(&body, &user)
}

/// Checks a ListenBrainz username (public endpoint, no token needed).
pub(super) async fn test_listenbrainz_user(user: String) -> Result<String, String> {
    let user = user.trim().to_string();
    if user.is_empty() {
        return Err("Type your ListenBrainz username first.".to_string());
    }
    let url = format!(
        "{LISTENBRAINZ_ROOT}/{}/listen-count",
        urlencoding::encode(&user)
    );
    let (status, body) = get_json(&url).await?;
    parse_listen_count(status, &body, &user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn key_shape() {
        assert!(looks_like_lastfm_key("0123456789abcdef0123456789ABCDEF"));
        assert!(!looks_like_lastfm_key("not-a-key"));
        assert!(!looks_like_lastfm_key("0123456789abcdef0123456789abcdeg"));
    }

    #[test]
    fn lastfm_user_ok() {
        let body = json!({"user": {"name": "rj", "playcount": "1204"}});
        assert_eq!(parse_lastfm_user(&body, "rj").unwrap(), "Connected as rj · 1,204 scrobbles");
    }

    #[test]
    fn lastfm_errors_name_the_fix() {
        let bad_key = json!({"error": 10, "message": "Invalid API key"});
        assert!(parse_lastfm_user(&bad_key, "rj").unwrap_err().contains("Shared secret"));
        let no_user = json!({"error": 6, "message": "User not found"});
        assert!(parse_lastfm_user(&no_user, "nobody").unwrap_err().contains("“nobody”"));
    }

    #[test]
    fn listenbrainz_count() {
        let body = json!({"payload": {"count": 3210}});
        assert_eq!(parse_listen_count(200, &body, "rj").unwrap(), "Found rj · 3,210 listens");
        let missing = json!({"code": 404, "error": "Cannot find user: x"});
        assert!(parse_listen_count(404, &missing, "x").unwrap_err().contains("no user called"));
    }
}
