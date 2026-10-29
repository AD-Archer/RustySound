//! Last.fm API (optional; needs a user-supplied API key).
//!
//! Shapes verified against https://www.last.fm/api/show/<method> (XML samples)
//! plus the JSON mapping rules at https://www.last.fm/api/rest: attributes
//! become plain string members (`@attr` for element attributes), text nodes
//! with attributes go under `#text`, repeated children become arrays. The JSON
//! error form is `{"error": 10, "message": "..."}` (confirmed live: HTTP 403).
//! Parsing is deliberately lenient: numbers may arrive as strings or numbers,
//! and a single child may arrive as an object instead of a one-item array.

use super::super::http::{get_external_json, Budget};
use serde_json::Value;

const ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
/// Last.fm's grey-star placeholder image hash; not real art.
const PLACEHOLDER_HASH: &str = "2a96cbd8b46e442fc41c2b86b821562f";

/// An artist from `user.getTopArtists` or `artist.getSimilar`.
#[derive(Debug, Clone, PartialEq)]
pub struct LfmArtist {
    pub name: String,
    pub url: Option<String>,
    /// Similarity 0..1 (`artist.getSimilar` only).
    pub match_score: f32,
    pub playcount: Option<u64>,
    pub image: Option<String>,
}

/// An album from `artist.getTopAlbums`.
#[derive(Debug, Clone, PartialEq)]
pub struct LfmAlbum {
    pub name: String,
    pub artist: String,
    pub url: Option<String>,
    pub playcount: Option<u64>,
    pub image: Option<String>,
}

/// A track from `track.getSimilar`.
#[derive(Debug, Clone, PartialEq)]
pub struct LfmTrack {
    pub name: String,
    pub artist: String,
    pub url: Option<String>,
    pub match_score: f32,
    pub image: Option<String>,
}

/// Build a Last.fm GET URL (`format=json`).
pub fn url(method: &str, api_key: &str, params: &[(&str, &str)]) -> String {
    let mut out = format!(
        "{ROOT}?method={method}&api_key={}&format=json",
        urlencoding::encode(api_key)
    );
    for (key, value) in params {
        out.push('&');
        out.push_str(key);
        out.push('=');
        out.push_str(&urlencoding::encode(value));
    }
    out
}

/// `Some(message)` when the body is a Last.fm error object.
pub fn error_message(value: &Value) -> Option<String> {
    let code = value.get("error")?;
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Last.fm error");
    Some(match code.as_i64() {
        Some(10) | Some(26) => "Invalid Last.fm API key".to_string(),
        Some(6) => format!("Last.fm: {message}"),
        Some(29) => "Last.fm rate limit reached".to_string(),
        _ => format!("Last.fm: {message}"),
    })
}

/// Array, single object, or nothing → list.
fn as_list(value: Option<&Value>) -> Vec<&Value> {
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
        Value::Object(map) => return text(map.get("#text")),
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn number_f32(value: Option<&Value>) -> f32 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) as f32,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn number_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Largest non-placeholder image from Last.fm's `image` array.
pub fn best_image(value: Option<&Value>) -> Option<String> {
    let rank = |size: &str| match size {
        "mega" => 5,
        "extralarge" => 4,
        "large" => 3,
        "medium" => 2,
        "small" => 1,
        _ => 0,
    };
    as_list(value)
        .into_iter()
        .filter_map(|img| {
            let url = text(img.get("#text"))?;
            if url.contains(PLACEHOLDER_HASH) {
                return None;
            }
            let size = img.get("size").and_then(Value::as_str).unwrap_or("").trim();
            Some((rank(size), url))
        })
        .max_by_key(|(r, _)| *r)
        .map(|(_, url)| url)
}

fn container<'a>(value: &'a Value, root: &str) -> Result<&'a Value, String> {
    if let Some(message) = error_message(value) {
        return Err(message);
    }
    value
        .get(root)
        .ok_or_else(|| format!("Last.fm: missing `{root}`"))
}

fn artist_name(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Object(map) => text(map.get("name")).or_else(|| text(map.get("#text"))),
        other => text(Some(other)),
    }
}

fn parse_artists(value: &Value, root: &str) -> Result<Vec<LfmArtist>, String> {
    let root = container(value, root)?;
    Ok(as_list(root.get("artist"))
        .into_iter()
        .filter_map(|a| {
            Some(LfmArtist {
                name: text(a.get("name"))?,
                url: text(a.get("url")),
                match_score: number_f32(a.get("match")),
                playcount: number_u64(a.get("playcount")),
                image: best_image(a.get("image")),
            })
        })
        .collect())
}

/// Parse `user.getTopArtists` → `{"topartists":{"artist":[…],"@attr":{…}}}`.
pub fn parse_top_artists(value: &Value) -> Result<Vec<LfmArtist>, String> {
    parse_artists(value, "topartists")
}

/// Parse `artist.getSimilar` → `{"similarartists":{"artist":[…],"@attr":{"artist":…}}}`.
pub fn parse_similar_artists(value: &Value) -> Result<Vec<LfmArtist>, String> {
    parse_artists(value, "similarartists")
}

/// Parse `artist.getTopAlbums` → `{"topalbums":{"album":[{"name","playcount","artist":{"name"},"image"}]}}`.
pub fn parse_top_albums(value: &Value) -> Result<Vec<LfmAlbum>, String> {
    let root = container(value, "topalbums")?;
    let fallback_artist = root
        .get("@attr")
        .and_then(|attr| text(attr.get("artist")));
    Ok(as_list(root.get("album"))
        .into_iter()
        .filter_map(|a| {
            let name = text(a.get("name"))?;
            if name == "(null)" {
                return None;
            }
            Some(LfmAlbum {
                name,
                artist: artist_name(a.get("artist")).or_else(|| fallback_artist.clone())?,
                url: text(a.get("url")),
                playcount: number_u64(a.get("playcount")),
                image: best_image(a.get("image")),
            })
        })
        .collect())
}

/// Parse `track.getSimilar` → `{"similartracks":{"track":[{"name","match","artist":{"name"},"image"}]}}`.
pub fn parse_similar_tracks(value: &Value) -> Result<Vec<LfmTrack>, String> {
    let root = container(value, "similartracks")?;
    Ok(as_list(root.get("track"))
        .into_iter()
        .filter_map(|t| {
            Some(LfmTrack {
                name: text(t.get("name"))?,
                artist: artist_name(t.get("artist"))?,
                url: text(t.get("url")),
                match_score: number_f32(t.get("match")),
                image: best_image(t.get("image")),
            })
        })
        .collect())
}

/// `user.getTopArtists` (3-month period).
pub async fn top_artists(
    api_key: &str,
    user: &str,
    limit: u32,
    budget: &Budget,
) -> Result<Vec<LfmArtist>, String> {
    let limit = limit.to_string();
    let value = get_external_json(
        &url(
            "user.gettopartists",
            api_key,
            &[("user", user), ("period", "3month"), ("limit", &limit)],
        ),
        budget,
    )
    .await?;
    parse_top_artists(&value)
}

/// `artist.getSimilar`.
pub async fn similar_artists(
    api_key: &str,
    artist: &str,
    limit: u32,
    budget: &Budget,
) -> Result<Vec<LfmArtist>, String> {
    let limit = limit.to_string();
    let value = get_external_json(
        &url(
            "artist.getsimilar",
            api_key,
            &[("artist", artist), ("autocorrect", "1"), ("limit", &limit)],
        ),
        budget,
    )
    .await?;
    parse_similar_artists(&value)
}

/// `artist.getTopAlbums`.
pub async fn top_albums(
    api_key: &str,
    artist: &str,
    limit: u32,
    budget: &Budget,
) -> Result<Vec<LfmAlbum>, String> {
    let limit = limit.to_string();
    let value = get_external_json(
        &url(
            "artist.gettopalbums",
            api_key,
            &[("artist", artist), ("autocorrect", "1"), ("limit", &limit)],
        ),
        budget,
    )
    .await?;
    parse_top_albums(&value)
}

/// `track.getSimilar`.
pub async fn similar_tracks(
    api_key: &str,
    artist: &str,
    track: &str,
    limit: u32,
    budget: &Budget,
) -> Result<Vec<LfmTrack>, String> {
    let limit = limit.to_string();
    let value = get_external_json(
        &url(
            "track.getsimilar",
            api_key,
            &[
                ("artist", artist),
                ("track", track),
                ("autocorrect", "1"),
                ("limit", &limit),
            ],
        ),
        budget,
    )
    .await?;
    parse_similar_tracks(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let text = match name {
            "similar_artists" => include_str!("../fixtures/lastfm_artist_getsimilar.json"),
            "top_artists" => include_str!("../fixtures/lastfm_user_gettopartists.json"),
            "top_albums" => include_str!("../fixtures/lastfm_artist_gettopalbums.json"),
            "similar_tracks" => include_str!("../fixtures/lastfm_track_getsimilar.json"),
            "error" => include_str!("../fixtures/lastfm_error_live.json"),
            _ => unreachable!(),
        };
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn parses_similar_artists() {
        let artists = parse_similar_artists(&fixture("similar_artists")).unwrap();
        assert_eq!(artists.len(), 3);
        assert_eq!(artists[0].name, "Sonny & Cher");
        assert!((artists[0].match_score - 1.0).abs() < 1e-6);
        assert!(artists[0].image.is_none(), "placeholder star is dropped");
        assert_eq!(artists[1].image.as_deref(), Some("https://lastfm.freetls.fastly.net/i/u/300x300/real.png"));
    }

    #[test]
    fn parses_top_artists_with_string_numbers() {
        let artists = parse_top_artists(&fixture("top_artists")).unwrap();
        assert_eq!(artists[0].name, "Radiohead");
        assert_eq!(artists[0].playcount, Some(1203));
        assert_eq!(artists.len(), 2);
    }

    #[test]
    fn parses_top_albums_and_skips_null_names() {
        let albums = parse_top_albums(&fixture("top_albums")).unwrap();
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].name, "Believe");
        assert_eq!(albums[0].artist, "Cher");
        assert_eq!(albums[0].playcount, Some(4_191_633));
        assert!(albums[0].image.as_deref().unwrap().contains("300x300"));
    }

    #[test]
    fn parses_similar_tracks_with_numeric_match_and_single_object() {
        let tracks = parse_similar_tracks(&fixture("similar_tracks")).unwrap();
        assert_eq!(tracks.len(), 1, "single track object instead of array");
        assert_eq!(tracks[0].artist, "Madonna");
        assert!((tracks[0].match_score - 0.98).abs() < 1e-3);
    }

    #[test]
    fn surfaces_error_without_panicking() {
        let err = parse_similar_artists(&fixture("error")).unwrap_err();
        assert_eq!(err, "Invalid Last.fm API key");
        assert!(parse_top_albums(&Value::Null).is_err());
        assert!(parse_similar_tracks(&serde_json::json!({"similartracks": {}}))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn url_encodes_params_and_requests_json() {
        let u = url("artist.getsimilar", "k e y", &[("artist", "Kestrel & Vine")]);
        assert!(u.starts_with("https://ws.audioscrobbler.com/2.0/?method=artist.getsimilar"));
        assert!(u.contains("api_key=k%20e%20y"));
        assert!(u.contains("format=json"));
        assert!(u.contains("artist=Kestrel%20%26%20Vine"));
    }
}
