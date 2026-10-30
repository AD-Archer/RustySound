//! Cover Flow ("stage") and the album shelves. Pure.

use super::reasons::{added_reason, because_reason};
use super::types::{DiscoverAlbum, ReasonKind};
use super::util::parse_timestamp;
use crate::api::models::Album;
use std::collections::HashSet;

/// Max Cover Flow items.
pub const STAGE_SIZE: usize = 9;

fn album_key(album: &Album) -> String {
    format!("{}::{}", album.server_id, album.id)
}

/// "New in your library": newest albums with an "Added …" reason.
pub fn new_in_library(newest: &[Album], now: i64, limit: usize) -> Vec<DiscoverAlbum> {
    let mut seen = HashSet::new();
    newest
        .iter()
        .filter(|album| seen.insert(album_key(album)))
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

/// Interleave the pools into the Cover Flow list: rewind, new, because, …
/// New albums the user already played are moved behind unplayed ones.
/// Albums appear once; at most [`STAGE_SIZE`] items.
pub fn compose_stage(
    rewind: &[DiscoverAlbum],
    new: &[DiscoverAlbum],
    because: &[DiscoverAlbum],
) -> Vec<DiscoverAlbum> {
    let mut new_sorted: Vec<&DiscoverAlbum> = new.iter().collect();
    new_sorted.sort_by_key(|a| a.album.play_count.unwrap_or(0) > 0);
    let pools: [Vec<&DiscoverAlbum>; 3] = [
        rewind.iter().collect(),
        new_sorted,
        because.iter().collect(),
    ];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let longest = pools.iter().map(Vec::len).max().unwrap_or(0);
    'outer: for i in 0..longest {
        for pool in &pools {
            if let Some(item) = pool.get(i) {
                if seen.insert(album_key(&item.album)) {
                    out.push((*item).clone());
                    if out.len() == STAGE_SIZE {
                        break 'outer;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: ReasonKind, plays: u32) -> DiscoverAlbum {
        DiscoverAlbum {
            album: Album {
                id: id.into(),
                server_id: "s1".into(),
                play_count: Some(plays),
                ..Default::default()
            },
            reason: String::new(),
            kind,
        }
    }

    #[test]
    fn interleaves_and_dedupes() {
        let rewind = vec![item("r1", ReasonKind::Rewind, 200), item("r2", ReasonKind::Rewind, 90)];
        let new = vec![
            item("n-played", ReasonKind::NewInLibrary, 3),
            item("n1", ReasonKind::NewInLibrary, 0),
            item("r1", ReasonKind::NewInLibrary, 0),
        ];
        let because = vec![item("b1", ReasonKind::BecauseYouPlay, 0)];
        let ids: Vec<String> = compose_stage(&rewind, &new, &because)
            .into_iter()
            .map(|a| a.album.id)
            .collect();
        assert_eq!(ids, vec!["r1", "n1", "b1", "r2", "n-played"]);
    }

    #[test]
    fn empty_pools_give_empty_stage() {
        assert!(compose_stage(&[], &[], &[]).is_empty());
    }

    #[test]
    fn caps_stage_size() {
        let many: Vec<DiscoverAlbum> =
            (0..20).map(|i| item(&format!("r{i}"), ReasonKind::Rewind, 1)).collect();
        assert_eq!(compose_stage(&many, &[], &[]).len(), STAGE_SIZE);
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
