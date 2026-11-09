//! Listening stats from a Last.fm account (username + the user's own API
//! key, from Settings › Discovery): the profile and its totals, top artists,
//! albums and tracks for a period, any year in review, and recent scrobbles.
//!
//! A year comes from the weekly charts asked for the whole year at once
//! (`from`/`to` need not be chart weeks; checked live), its scrobble count
//! from `user.getRecentTracks`' `total`. Album charts carry no art, so the
//! year's top albums get theirs from `album.getInfo`.
//!
//! Cached in the JSON cache: past years for a month, the profile, tops and
//! this year for an hour; recent scrobbles are always fresh. Parsing is lenient, like
//! Discover's: numbers may be strings, a single child may be an object.

use crate::api::discovery::{get_external_json, lastfm_best_image, lastfm_error, lastfm_url, Budget};
use crate::cache_service::{get_json as cache_get, put_json as cache_put};
use chrono::{Datelike, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Requests one page load may make (profile, three tops, recent, a year with
/// its album art).
const BUDGET: u32 = 24;
/// Top albums in a year that get their art looked up.
const YEAR_ALBUM_ART: usize = 6;

/// Who to ask: the user's key and name.
#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub api_key: String,
    pub user: String,
}

impl Account {
    /// The account from settings, when both fields are filled in.
    pub fn from_settings(settings: &crate::db::AppSettings) -> Option<Self> {
        let api_key = settings.lastfm_api_key.trim().to_string();
        let user = settings.lastfm_username.trim().to_string();
        (!api_key.is_empty() && !user.is_empty()).then_some(Self { api_key, user })
    }

    fn cache_key(&self, what: &str) -> String {
        format!("lastfm:stats:v1:{}:{what}", self.user.to_lowercase())
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub real_name: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub country: Option<String>,
    pub scrobbles: u64,
    pub artists: u64,
    pub albums: u64,
    pub tracks: u64,
    /// Unix seconds.
    pub registered: i64,
    pub loved: Option<u64>,
}

impl Profile {
    pub fn since_year(&self) -> i32 {
        Utc.timestamp_opt(self.registered, 0).single().map(|d| d.year()).unwrap_or_else(|| Utc::now().year())
    }

    /// Scrobbles per day since joining.
    pub fn per_day(&self, now: i64) -> f64 {
        let days = ((now - self.registered) as f64 / 86_400.0).max(1.0);
        self.scrobbles as f64 / days
    }

    /// The years with a recap: joining year to this one, newest first.
    pub fn years(&self, this_year: i32) -> Vec<i32> {
        (self.since_year().min(this_year)..=this_year).rev().collect()
    }
}

/// One ranked artist, album or track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Top {
    pub name: String,
    /// The artist, for albums and tracks.
    pub artist: Option<String>,
    pub plays: u64,
    pub image: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Artists,
    Albums,
    Tracks,
}

impl Kind {
    fn method(self) -> &'static str {
        match self {
            Kind::Artists => "user.gettopartists",
            Kind::Albums => "user.gettopalbums",
            Kind::Tracks => "user.gettoptracks",
        }
    }
    fn chart_method(self) -> &'static str {
        match self {
            Kind::Artists => "user.getweeklyartistchart",
            Kind::Albums => "user.getweeklyalbumchart",
            Kind::Tracks => "user.getweeklytrackchart",
        }
    }
    fn root(self) -> &'static str {
        match self {
            Kind::Artists => "topartists",
            Kind::Albums => "topalbums",
            Kind::Tracks => "toptracks",
        }
    }
    fn chart_root(self) -> &'static str {
        match self {
            Kind::Artists => "weeklyartistchart",
            Kind::Albums => "weeklyalbumchart",
            Kind::Tracks => "weeklytrackchart",
        }
    }
    fn item(self) -> &'static str {
        match self {
            Kind::Artists => "artist",
            Kind::Albums => "album",
            Kind::Tracks => "track",
        }
    }
}

/// Last.fm's periods for the tops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Period {
    Week,
    Month,
    Quarter,
    HalfYear,
    Year,
    AllTime,
}

impl Period {
    pub const ALL: [Period; 6] = [Period::Week, Period::Month, Period::Quarter, Period::HalfYear, Period::Year, Period::AllTime];

    pub fn label(self) -> &'static str {
        match self {
            Period::Week => "7 Days",
            Period::Month => "Month",
            Period::Quarter => "3 Months",
            Period::HalfYear => "6 Months",
            Period::Year => "Year",
            Period::AllTime => "All Time",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Period::Week => "7day",
            Period::Month => "1month",
            Period::Quarter => "3month",
            Period::HalfYear => "6month",
            Period::Year => "12month",
            Period::AllTime => "overall",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Tops {
    pub artists: Vec<Top>,
    pub albums: Vec<Top>,
    pub tracks: Vec<Top>,
}

/// One year in review.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Year {
    pub year: i32,
    pub scrobbles: u64,
    /// How many different artists, albums and tracks the year had.
    pub artist_count: u64,
    pub album_count: u64,
    pub track_count: u64,
    pub tops: Tops,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub name: String,
    pub artist: String,
    pub album: Option<String>,
    pub image: Option<String>,
    /// Unix seconds; `None` while it plays.
    pub played_at: Option<i64>,
    pub now_playing: bool,
}

// ---- Parsing ---------------------------------------------------------------

fn list(value: Option<&Value>) -> Vec<&Value> {
    match value {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        _ => Vec::new(),
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Object(map) => return text(map.get("#text")).or_else(|| text(map.get("name"))),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn number(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Object(map) => number(map.get("#text")),
        _ => None,
    }
}

fn root<'a>(value: &'a Value, name: &str) -> Result<&'a Value, String> {
    if let Some(message) = lastfm_error(value) {
        return Err(message);
    }
    value.get(name).ok_or_else(|| format!("Last.fm: missing `{name}`"))
}

pub fn parse_profile(value: &Value) -> Result<Profile, String> {
    let user = root(value, "user")?;
    Ok(Profile {
        name: text(user.get("name")).unwrap_or_default(),
        real_name: text(user.get("realname")),
        image: lastfm_best_image(user.get("image")),
        url: text(user.get("url")),
        country: text(user.get("country")).filter(|c| c != "None"),
        scrobbles: number(user.get("playcount")).unwrap_or(0),
        artists: number(user.get("artist_count")).unwrap_or(0),
        albums: number(user.get("album_count")).unwrap_or(0),
        tracks: number(user.get("track_count")).unwrap_or(0),
        registered: user
            .get("registered")
            .and_then(|r| number(r.get("unixtime")).or_else(|| number(Some(r))))
            .unwrap_or(0) as i64,
        loved: None,
    })
}

fn parse_item(item: &Value, kind: Kind) -> Option<Top> {
    let name = text(item.get("name"))?;
    if name == "(null)" {
        return None;
    }
    Some(Top {
        name,
        artist: match kind {
            Kind::Artists => None,
            _ => text(item.get("artist")),
        },
        plays: number(item.get("playcount")).unwrap_or(0),
        image: lastfm_best_image(item.get("image")),
        url: text(item.get("url")),
    })
}

/// `user.getTop*` and `user.getWeekly*Chart`: the items and how many there are.
pub fn parse_tops(value: &Value, kind: Kind, chart: bool) -> Result<(Vec<Top>, u64), String> {
    let root = root(value, if chart { kind.chart_root() } else { kind.root() })?;
    let items = list(root.get(kind.item()));
    let total = root
        .get("@attr")
        .and_then(|attr| number(attr.get("total")))
        .unwrap_or(items.len() as u64);
    Ok((items.into_iter().filter_map(|item| parse_item(item, kind)).collect(), total))
}

pub fn parse_recent(value: &Value) -> Result<(Vec<Recent>, u64), String> {
    let root = root(value, "recenttracks")?;
    let total = root.get("@attr").and_then(|attr| number(attr.get("total"))).unwrap_or(0);
    let tracks = list(root.get("track"))
        .into_iter()
        .filter_map(|track| {
            let now_playing = track
                .get("@attr")
                .and_then(|attr| attr.get("nowplaying"))
                .and_then(Value::as_str)
                == Some("true");
            Some(Recent {
                name: text(track.get("name"))?,
                artist: text(track.get("artist"))?,
                album: text(track.get("album")),
                image: lastfm_best_image(track.get("image")),
                played_at: track.get("date").and_then(|d| number(d.get("uts"))).map(|t| t as i64),
                now_playing,
            })
        })
        .collect();
    Ok((tracks, total))
}

fn parse_total(value: &Value, name: &str) -> Option<u64> {
    root(value, name).ok()?.get("@attr").and_then(|attr| number(attr.get("total")))
}

// ---- Requests --------------------------------------------------------------

async fn get(account: &Account, method: &str, params: &[(&str, &str)], budget: &Budget) -> Result<Value, String> {
    let mut all = vec![("user", account.user.as_str())];
    all.extend_from_slice(params);
    get_external_json(&lastfm_url(method, &account.api_key, &all), budget).await
}

async fn cached<T, F>(key: String, hours: u32, fetch: F) -> Result<T, String>
where
    T: Serialize + for<'de> Deserialize<'de>,
    F: std::future::Future<Output = Result<T, String>>,
{
    if let Some(hit) = cache_get::<T>(&key) {
        return Ok(hit);
    }
    let value = fetch.await?;
    let _ = cache_put(key, &value, Some(hours));
    Ok(value)
}

/// The profile, with the loved-tracks count.
pub async fn profile(account: &Account) -> Result<Profile, String> {
    cached(account.cache_key("profile"), 1, async {
        let budget = Budget::new(2);
        let mut profile = parse_profile(&get(account, "user.getinfo", &[], &budget).await?)?;
        profile.loved = get(account, "user.getlovedtracks", &[("limit", "1")], &budget)
            .await
            .ok()
            .and_then(|v| parse_total(&v, "lovedtracks"));
        Ok(profile)
    })
    .await
}

/// Top artists, albums and tracks for `period`.
pub async fn tops(account: &Account, period: Period, limit: u32) -> Result<Tops, String> {
    cached(account.cache_key(&format!("tops:{}:{limit}", period.key())), 1, async {
        let budget = Budget::new(3);
        let limit = limit.to_string();
        let fetch = |kind: Kind| {
            let budget = budget.clone();
            let limit = limit.clone();
            async move {
                let value = get(account, kind.method(), &[("period", period.key()), ("limit", &limit)], &budget).await?;
                parse_tops(&value, kind, false).map(|(items, _)| items)
            }
        };
        let (artists, albums, tracks) =
            futures_util::future::join3(fetch(Kind::Artists), fetch(Kind::Albums), fetch(Kind::Tracks)).await;
        Ok(Tops { artists: artists?, albums: albums?, tracks: tracks? })
    })
    .await
}

/// The year's start and end (UTC, Unix seconds).
fn year_range(year: i32) -> (i64, i64) {
    let start = Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single().map(|d| d.timestamp()).unwrap_or(0);
    let end = Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).single().map(|d| d.timestamp()).unwrap_or(0);
    (start, end.min(Utc::now().timestamp()))
}

/// One year in review: totals and the top `limit` of each kind.
pub async fn year(account: &Account, year: i32, limit: usize) -> Result<Year, String> {
    let past = year < Utc::now().year();
    let hours = if past { 24 * 30 } else { 1 };
    cached(account.cache_key(&format!("year:{year}:{limit}")), hours, async {
        let budget = Budget::new(BUDGET);
        let (from, to) = year_range(year);
        let (from, to) = (from.to_string(), to.to_string());
        let range = [("from", from.as_str()), ("to", to.as_str())];
        let chart = |kind: Kind| {
            let budget = budget.clone();
            async move {
                let value = get(account, kind.chart_method(), &range, &budget).await?;
                parse_tops(&value, kind, true)
            }
        };
        let scrobbles = async {
            let mut params = range.to_vec();
            params.push(("limit", "1"));
            let value = get(account, "user.getrecenttracks", &params, &budget).await?;
            parse_recent(&value).map(|(_, total)| total)
        };
        let (artists, albums, tracks, scrobbles) = futures_util::future::join4(
            chart(Kind::Artists),
            chart(Kind::Albums),
            chart(Kind::Tracks),
            scrobbles,
        )
        .await;
        let (artists, artist_count) = artists?;
        let (mut albums, album_count) = albums?;
        let (tracks, track_count) = tracks?;
        albums.truncate(limit);
        // The album chart has no art: ask for the top few.
        let art = futures_util::future::join_all(albums.iter().take(YEAR_ALBUM_ART).map(|album| {
            let budget = budget.clone();
            let artist = album.artist.clone().unwrap_or_default();
            let name = album.name.clone();
            async move {
                let url = lastfm_url(
                    "album.getinfo",
                    &account.api_key,
                    &[("artist", &artist), ("album", &name), ("autocorrect", "1")],
                );
                let value = get_external_json(&url, &budget).await.ok()?;
                lastfm_best_image(value.get("album")?.get("image"))
            }
        }))
        .await;
        for (album, image) in albums.iter_mut().zip(art) {
            album.image = album.image.take().or(image);
        }
        Ok(Year {
            year,
            scrobbles: scrobbles.unwrap_or(0),
            artist_count,
            album_count,
            track_count,
            tops: Tops {
                artists: artists.into_iter().take(limit).collect(),
                albums,
                tracks: tracks.into_iter().take(limit).collect(),
            },
        })
    })
    .await
}

/// The latest scrobbles (the first may be playing now). Not cached: the
/// page asks once per visit.
pub async fn recent(account: &Account, limit: u32) -> Result<Vec<Recent>, String> {
    let limit_text = limit.to_string();
    let value = get(account, "user.getrecenttracks", &[("limit", &limit_text)], &Budget::new(1)).await?;
    let (mut tracks, _) = parse_recent(&value)?;
    // Last.fm adds the playing track on top of `limit`.
    tracks.truncate(limit as usize + 1);
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_profile() {
        let value = json!({"user": {"name": "rj", "realname": "", "playcount": "5263", "artist_count": "812",
            "album_count": 1200, "track_count": "3001", "country": "None",
            "registered": {"unixtime": "1735689600", "#text": 1735689600},
            "image": [{"size": "large", "#text": "https://img/a.png"}]}});
        let profile = parse_profile(&value).unwrap();
        assert_eq!(profile.scrobbles, 5263);
        assert_eq!(profile.albums, 1200);
        assert_eq!(profile.real_name, None);
        assert_eq!(profile.country, None, "Last.fm says \"None\"");
        assert_eq!(profile.since_year(), 2025);
        assert_eq!(profile.years(2026), vec![2026, 2025]);
        assert!((profile.per_day(1735689600 + 10 * 86_400) - 526.3).abs() < 0.01);
    }

    #[test]
    fn reads_tops_and_charts() {
        let top = json!({"topalbums": {"album": [{"name": "DONDA 2", "playcount": "39",
            "artist": {"name": "DONDA", "url": "u"}, "image": []}], "@attr": {"total": "441"}}});
        let (albums, total) = parse_tops(&top, Kind::Albums, false).unwrap();
        assert_eq!(albums[0].artist.as_deref(), Some("DONDA"));
        assert_eq!(total, 441);
        // Charts name the artist as text and have no total: their length is it.
        let chart = json!({"weeklyalbumchart": {"album": [
            {"name": "A", "playcount": "3", "artist": {"#text": "X"}},
            {"name": "(null)", "playcount": "1", "artist": {"#text": "Y"}}],
            "@attr": {"from": "1", "to": "2"}}});
        let (albums, total) = parse_tops(&chart, Kind::Albums, true).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].artist.as_deref(), Some("X"));
        assert_eq!(total, 2);
        // A single artist arrives as an object.
        let one = json!({"weeklyartistchart": {"artist": {"name": "Solo", "playcount": 7}}});
        assert_eq!(parse_tops(&one, Kind::Artists, true).unwrap().0[0].plays, 7);
    }

    #[test]
    fn reads_recent_scrobbles() {
        let value = json!({"recenttracks": {"@attr": {"total": "1397"}, "track": [
            {"name": "Now", "artist": {"#text": "A"}, "album": {"#text": ""}, "@attr": {"nowplaying": "true"}},
            {"name": "Then", "artist": {"#text": "B"}, "album": {"#text": "C"}, "date": {"uts": "1700000000"}}]}});
        let (tracks, total) = parse_recent(&value).unwrap();
        assert_eq!(total, 1397);
        assert!(tracks[0].now_playing && tracks[0].played_at.is_none());
        assert_eq!(tracks[1].album.as_deref(), Some("C"));
        assert_eq!(tracks[1].played_at, Some(1_700_000_000));
    }

    #[test]
    fn errors_are_kept() {
        let value = json!({"error": 6, "message": "User not found"});
        assert_eq!(parse_profile(&value).unwrap_err(), "Last.fm: User not found");
    }

    #[test]
    fn a_year_ends_now_at_the_latest() {
        let (from, to) = year_range(2025);
        assert_eq!(from, 1735689600);
        assert_eq!(to, 1767225600.min(Utc::now().timestamp()));
    }
}
