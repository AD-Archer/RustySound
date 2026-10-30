//! ListenBrainz fresh releases (optional; needs only a public username).
//!
//! `GET https://api.listenbrainz.org/1/user/{user}/fresh_releases` — params
//! `sort` (release_date | artist_credit_name | release_name | confidence),
//! `past` / `future` (bool, default true), `days` (default 14, max 90). No
//! token needed and CORS `*` (verified live 2026-10-29). Response:
//! `{"payload":{"releases":[{artist_credit_name, release_name, release_date,
//! release_mbid, caa_id, caa_release_mbid, release_group_primary_type,
//! listen_count, …}], "user_id"}}`.

use super::super::http::{get_external_json, Budget};
use serde_json::Value;

const ROOT: &str = "https://api.listenbrainz.org/1/user";

/// A new release by an artist the user listens to.
#[derive(Debug, Clone, PartialEq)]
pub struct FreshRelease {
    pub artist: String,
    pub title: String,
    /// YYYY-MM-DD.
    pub release_date: Option<String>,
    pub release_mbid: Option<String>,
    /// Cover Art Archive front image (500 px), when the release has art.
    pub cover_url: Option<String>,
    /// "Album", "EP", "Single", …
    pub primary_type: Option<String>,
    /// How often the user already listened to it (0 = new to them).
    pub listen_count: u64,
    /// ListenBrainz's confidence that the user cares about this release.
    pub confidence: u64,
}

impl FreshRelease {
    /// MusicBrainz page for the release.
    pub fn external_url(&self) -> Option<String> {
        self.release_mbid
            .as_ref()
            .map(|mbid| format!("https://musicbrainz.org/release/{mbid}"))
    }
}

/// Fresh releases from the last 30 days, no future ones, most relevant first.
pub fn fresh_releases_url(user: &str) -> String {
    format!(
        "{ROOT}/{}/fresh_releases?days=30&past=true&future=false&sort=confidence",
        urlencoding::encode(user)
    )
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Parse the response. HTTP 204 / unknown shapes give an empty list.
pub fn parse_fresh_releases(value: &Value) -> Vec<FreshRelease> {
    let Some(releases) = value
        .get("payload")
        .and_then(|p| p.get("releases"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    releases
        .iter()
        .filter_map(|r| {
            let caa_id = r.get("caa_id").filter(|v| !v.is_null());
            let cover_url = match (caa_id, string(r, "caa_release_mbid")) {
                (Some(_), Some(mbid)) => {
                    Some(format!("https://coverartarchive.org/release/{mbid}/front-500"))
                }
                _ => None,
            };
            Some(FreshRelease {
                artist: string(r, "artist_credit_name")?,
                title: string(r, "release_name")?,
                release_date: string(r, "release_date"),
                release_mbid: string(r, "release_mbid"),
                cover_url,
                primary_type: string(r, "release_group_primary_type"),
                listen_count: r.get("listen_count").and_then(Value::as_u64).unwrap_or(0),
                confidence: r.get("confidence").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .collect()
}

/// Releases worth showing: not yet listened to, released on or before
/// `today` (YYYY-MM-DD), albums and EPs before singles.
pub fn select_fresh(mut releases: Vec<FreshRelease>, today: &str) -> Vec<FreshRelease> {
    releases.retain(|r| {
        r.listen_count == 0 && r.release_date.as_deref().is_none_or(|d| d <= today)
    });
    releases.sort_by_key(|r| {
        let rank = match r.primary_type.as_deref() {
            Some("Album") => 0,
            Some("EP") => 1,
            _ => 2,
        };
        (rank, std::cmp::Reverse(r.confidence))
    });
    releases
}

/// Fetch and parse the user's fresh releases.
pub async fn fresh_releases(user: &str, budget: &Budget) -> Result<Vec<FreshRelease>, String> {
    let value = get_external_json(&fresh_releases_url(user), budget).await?;
    if let Some(error) = string(&value, "error") {
        return Err(format!("ListenBrainz: {error}"));
    }
    Ok(parse_fresh_releases(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_fresh_releases() {
        let value: Value = serde_json::from_str(include_str!(
            "../fixtures/listenbrainz_fresh_releases_live.json"
        ))
        .unwrap();
        let releases = parse_fresh_releases(&value);
        assert_eq!(releases.len(), 3);
        assert_eq!(releases[0].artist, "Archive");
        assert_eq!(releases[0].title, "Not In My Name");
        assert_eq!(releases[0].primary_type.as_deref(), Some("EP"));
        assert!(releases[0]
            .cover_url
            .as_deref()
            .unwrap()
            .starts_with("https://coverartarchive.org/release/28a0632f"));
        assert!(releases[0].external_url().unwrap().contains("musicbrainz.org/release/"));

        let picked = select_fresh(releases, "2026-10-29");
        assert_eq!(picked[0].primary_type.as_deref(), Some("Album"));
        assert_eq!(picked[1].primary_type.as_deref(), Some("EP"));
    }

    #[test]
    fn filters_heard_and_future_releases() {
        let base = FreshRelease {
            artist: "A".into(),
            title: "T".into(),
            release_date: Some("2026-10-01".into()),
            release_mbid: None,
            cover_url: None,
            primary_type: Some("Album".into()),
            listen_count: 0,
            confidence: 1,
        };
        let heard = FreshRelease { listen_count: 4, ..base.clone() };
        let future = FreshRelease { release_date: Some("2026-11-20".into()), ..base.clone() };
        assert_eq!(select_fresh(vec![base, heard, future], "2026-10-29").len(), 1);
    }

    #[test]
    fn empty_and_204_are_empty() {
        assert!(parse_fresh_releases(&Value::Null).is_empty());
        assert!(parse_fresh_releases(&serde_json::json!({"payload": {}})).is_empty());
    }

    #[test]
    fn url_shape() {
        assert_eq!(
            fresh_releases_url("rob"),
            "https://api.listenbrainz.org/1/user/rob/fresh_releases?days=30&past=true&future=false&sort=confidence"
        );
    }
}
