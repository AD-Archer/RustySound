//! The album shelves: "New in your library" and "Because you play". Pure.

use super::reasons::{added_reason, because_reason};
use super::types::{DiscoverAlbum, ReasonKind};
use super::util::parse_timestamp;
use crate::api::models::Album;
use std::collections::HashSet;

/// "New in your library": newest albums with an "Added …" reason.
pub fn new_in_library(newest: &[Album], now: i64, limit: usize) -> Vec<DiscoverAlbum> {
    let mut seen = HashSet::new();
    newest
        .iter()
        .filter(|album| seen.insert(album.key()))
        .take(limit)
        .map(|album| DiscoverAlbum {
            reason: added_reason(now, album.created.as_deref().and_then(parse_timestamp)),
            album: album.clone(),
            kind: ReasonKind::NewInLibrary,
        })
        .collect()
}

/// For a similar in-library artist, the album most worth trying: fewest plays
/// first (unplayed wins), then newest year. `None` if the artist has no albums.
pub fn pick_because_album(albums: &[Album], because_artist: &str) -> Option<DiscoverAlbum> {
    let album = albums.iter().min_by(|a, b| {
        a.play_count
            .unwrap_or(0)
            .cmp(&b.play_count.unwrap_or(0))
            .then_with(|| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)))
            .then_with(|| a.name.cmp(&b.name))
    })?;
    Some(DiscoverAlbum {
        album: album.clone(),
        reason: because_reason(because_artist),
        kind: ReasonKind::BecauseYouPlay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_in_library_dedupes_and_caps() {
        let album = |id: &str, server: &str| Album {
            id: id.into(),
            server_id: server.into(),
            ..Default::default()
        };
        let newest = vec![album("a", "s1"), album("a", "s1"), album("a", "s2"), album("b", "s1")];
        let ids: Vec<(String, String)> = new_in_library(&newest, 0, 2)
            .into_iter()
            .map(|item| (item.album.server_id, item.album.id))
            .collect();
        assert_eq!(ids, vec![("s1".into(), "a".into()), ("s2".into(), "a".into())]);
        let first = &new_in_library(&newest, 0, 1)[0];
        assert_eq!(first.kind, ReasonKind::NewInLibrary);
        assert_eq!(first.reason, "New in your library");
    }

    #[test]
    fn because_prefers_unplayed() {
        let albums = vec![
            Album { id: "a".into(), play_count: Some(12), ..Default::default() },
            Album { id: "b".into(), play_count: None, year: Some(2019), ..Default::default() },
            Album { id: "c".into(), play_count: Some(0), year: Some(2024), ..Default::default() },
        ];
        let pick = pick_because_album(&albums, "Mira Okafor").unwrap();
        assert_eq!(pick.album.id, "c");
        assert_eq!(pick.reason, "Because you play Mira Okafor");
        assert!(pick_because_album(&[], "x").is_none());
    }
}
