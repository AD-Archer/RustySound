//! iTunes Search API: art, 30-second previews and Apple Music links for
//! off-server items. No key; CORS `*` (both verified live 2026-10-29).
//!
//! Docs: https://performance-partners.apple.com/search-api — `term`, `country`,
//! `media=music`, `entity` (musicArtist | album | song | …), `attribute`
//! (artistTerm | albumTerm | songTerm | …), `limit` 1–200. Rate limit is
//! "approximately 20 calls per minute". Response: `{"resultCount", "results":[…]}`.
//! `musicArtist` results carry no artwork, so artists are resolved through a
//! `entity=song&attribute=artistTerm` search instead (gives art + a preview).

use super::super::http::{get_external_json, Budget};
use super::super::types::{OffServerItem, OffServerKind};
use super::super::util::same_name;
use serde_json::Value;

const ROOT: &str = "https://itunes.apple.com/search";

/// One search result (song or album).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItunesHit {
    pub artist_name: String,
    pub collection_name: Option<String>,
    pub track_name: Option<String>,
    /// Artwork upscaled to 600×600.
    pub artwork_url: Option<String>,
    /// 30-second AAC preview (songs only).
    pub preview_url: Option<String>,
    pub track_view_url: Option<String>,
    pub collection_view_url: Option<String>,
    pub artist_view_url: Option<String>,
    pub release_date: Option<String>,
}

/// Build a search URL. `attribute` narrows which field `term` matches.
pub fn search_url(term: &str, entity: &str, attribute: Option<&str>, limit: u32) -> String {
    let mut url = format!(
        "{ROOT}?term={}&media=music&entity={entity}&limit={limit}&country=US",
        urlencoding::encode(term)
    );
    if let Some(attribute) = attribute {
        url.push_str("&attribute=");
        url.push_str(attribute);
    }
    url
}

/// iTunes art URLs end in `/100x100bb.jpg`; any size up to ~3000 works.
pub fn upscale_artwork(url: &str) -> String {
    url.replace("/100x100bb.", "/600x600bb.")
        .replace("/60x60bb.", "/600x600bb.")
}

fn string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Parse a search response. Unknown shapes give an empty list.
pub fn parse_search(value: &Value) -> Vec<ItunesHit> {
    value
        .get("results")
        .and_then(Value::as_array)
        .map(|results| {
            results
                .iter()
                .filter_map(|r| {
                    Some(ItunesHit {
                        artist_name: string(r, "artistName")?,
                        collection_name: string(r, "collectionName"),
                        track_name: string(r, "trackName"),
                        artwork_url: string(r, "artworkUrl100")
                            .or_else(|| string(r, "artworkUrl60"))
                            .map(|u| upscale_artwork(&u)),
                        preview_url: string(r, "previewUrl"),
                        track_view_url: string(r, "trackViewUrl"),
                        collection_view_url: string(r, "collectionViewUrl"),
                        artist_view_url: string(r, "artistViewUrl")
                            .or_else(|| string(r, "artistLinkUrl")),
                        release_date: string(r, "releaseDate"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The best hit for an item, or `None` when nothing matches by name. We never
/// attach art or a preview for a different artist.
pub fn best_match<'a>(hits: &'a [ItunesHit], item: &OffServerItem) -> Option<&'a ItunesHit> {
    hits.iter().find(|hit| {
        same_name(&hit.artist_name, &item.artist)
            && match item.kind {
                OffServerKind::Artist => true,
                OffServerKind::Album => hit
                    .collection_name
                    .as_deref()
                    .is_some_and(|c| same_name(c, &item.title)),
                OffServerKind::Track => hit
                    .track_name
                    .as_deref()
                    .is_some_and(|t| same_name(t, &item.title)),
            }
    })
}

/// Fill `art_url`, `preview_url`, `preview_title` and `external_url` where
/// missing. `Ok(true)` when a match was applied, `Ok(false)` when iTunes had
/// no match. On `Err` the item is unchanged.
pub async fn enrich(item: &mut OffServerItem, previews: bool, budget: &Budget) -> Result<bool, String> {
    let (term, entity, attribute) = match item.kind {
        OffServerKind::Artist => (item.artist.clone(), "song", Some("artistTerm")),
        OffServerKind::Album => (format!("{} {}", item.artist, item.title), "album", None),
        OffServerKind::Track => (format!("{} {}", item.artist, item.title), "song", None),
    };
    let value = get_external_json(&search_url(&term, entity, attribute, 5), budget).await?;
    let hits = parse_search(&value);
    match best_match(&hits, item) {
        Some(hit) => {
            apply_hit(item, hit, previews);
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Copy fields from a matched hit onto the item (pure; used by tests).
pub fn apply_hit(item: &mut OffServerItem, hit: &ItunesHit, previews: bool) {
    if item.art_url.is_none() {
        item.art_url = hit.artwork_url.clone();
    }
    if previews && item.preview_url.is_none() && hit.preview_url.is_some() {
        item.preview_url = hit.preview_url.clone();
        item.preview_title = hit.track_name.clone();
    }
    if item.external_url.is_none() {
        item.external_url = match item.kind {
            OffServerKind::Artist => hit.artist_view_url.clone(),
            OffServerKind::Album => hit.collection_view_url.clone(),
            OffServerKind::Track => hit.track_view_url.clone(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::types::DiscoverySource;
    use super::*;

    fn item(kind: OffServerKind, title: &str, artist: &str) -> OffServerItem {
        OffServerItem {
            id: String::new(),
            kind,
            title: title.into(),
            artist: artist.into(),
            reason: String::new(),
            because_artist: None,
            source: DiscoverySource::LastFm,
            art_url: None,
            preview_url: None,
            preview_title: None,
            external_url: None,
            release_date: None,
        }
    }

    #[test]
    fn parses_live_song_search() {
        let value: Value =
            serde_json::from_str(include_str!("../fixtures/itunes_song_artistterm_live.json"))
                .unwrap();
        let hits = parse_search(&value);
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].artist_name, "Radiohead");
        assert!(hits[0].preview_url.as_deref().unwrap().ends_with(".m4a"));
        assert!(hits[0].artwork_url.as_deref().unwrap().contains("600x600bb"));

        let mut artist = item(OffServerKind::Artist, "Radiohead", "radiohead");
        let hit = best_match(&hits, &artist).unwrap().clone();
        apply_hit(&mut artist, &hit, true);
        assert!(artist.preview_url.is_some());
        assert_eq!(artist.preview_title, hits[0].track_name);
        assert!(artist.external_url.as_deref().unwrap().contains("/artist/"));

        let mut no_preview = item(OffServerKind::Artist, "Radiohead", "Radiohead");
        apply_hit(&mut no_preview, &hits[0], false);
        assert!(no_preview.preview_url.is_none(), "previews disabled");
        assert!(no_preview.art_url.is_some());
    }

    #[test]
    fn parses_live_album_search_and_matches_by_title() {
        let value: Value =
            serde_json::from_str(include_str!("../fixtures/itunes_album_live.json")).unwrap();
        let hits = parse_search(&value);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.preview_url.is_none()));
        let wanted = item(OffServerKind::Album, "OK Computer", "Radiohead");
        let hit = best_match(&hits, &wanted).unwrap();
        assert_eq!(hit.collection_name.as_deref(), Some("OK Computer"));
        let wrong_artist = item(OffServerKind::Album, "OK Computer", "Portishead");
        assert!(best_match(&hits, &wrong_artist).is_none());
    }

    #[test]
    fn garbage_is_empty_not_an_error() {
        assert!(parse_search(&Value::Null).is_empty());
        assert!(parse_search(&serde_json::json!({"results": [{"x": 1}]})).is_empty());
    }

    #[test]
    fn builds_urls() {
        let u = search_url("Kestrel & Vine", "song", Some("artistTerm"), 5);
        assert_eq!(
            u,
            "https://itunes.apple.com/search?term=Kestrel%20%26%20Vine&media=music&entity=song&limit=5&country=US&attribute=artistTerm"
        );
    }
}
