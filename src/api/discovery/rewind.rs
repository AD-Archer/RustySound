//! "Rewind": things the user played a lot but hasn't played in a while. Pure.

use super::reasons::rewind_reason;
use super::types::{DiscoverAlbum, DiscoverSong, ReasonKind};
use super::util::{parse_timestamp, SeededRng};
use crate::api::models::{Album, Song};

/// Default Rewind window in months (the Discover segment's default).
pub const DEFAULT_REWIND_MONTHS: u32 = 6;
/// Minimum plays for an album to count as a "classic".
pub const MIN_ALBUM_PLAYS: u32 = 5;
/// Minimum plays for a song to count as a "classic".
pub const MIN_SONG_PLAYS: u32 = 3;

/// Not played for this many days ⇒ eligible for Rewind. 3 → 90, 6 → 180,
/// 12 → 365; other values are clamped to 1–24 months of 30 days.
pub fn cutoff_days_for_months(months: u32) -> i64 {
    match months {
        12 => 365,
        m => i64::from(m.clamp(1, 24)) * 30,
    }
}

/// Epoch seconds before which a last play counts as "a while ago".
pub fn rewind_cutoff(now: i64, cutoff_days: i64) -> i64 {
    now - cutoff_days * 86_400
}

/// True when something with these stats belongs in Rewind. A missing
/// `played` timestamp is never eligible: we only claim what the data shows.
pub fn is_rewind_candidate(
    played: Option<&str>,
    play_count: Option<u32>,
    min_plays: u32,
    now: i64,
    cutoff_days: i64,
) -> bool {
    let Some(last) = played.and_then(parse_timestamp) else {
        return false;
    };
    // Some servers report the epoch for "never"; treat pre-2000 as unknown.
    last > 946_684_800 && last < rewind_cutoff(now, cutoff_days) && play_count.unwrap_or(0) >= min_plays
}

/// Pick up to `limit` Rewind albums. Takes the top `limit * 2` by plays and
/// draws `limit` of them with the seed, so the shelf varies day to day but
/// always shows real classics. Result is ordered by plays, most first.
pub fn select_rewind_albums(
    albums: &[Album],
    now: i64,
    cutoff_days: i64,
    seed: u64,
    limit: usize,
) -> Vec<DiscoverAlbum> {
    let mut candidates: Vec<&Album> = albums
        .iter()
        .filter(|a| {
            is_rewind_candidate(a.played.as_deref(), a.play_count, MIN_ALBUM_PLAYS, now, cutoff_days)
        })
        .collect();
    candidates.sort_by_key(|a| std::cmp::Reverse(a.play_count.unwrap_or(0)));
    dedupe_by(&mut candidates, |a| format!("{}::{}", a.server_id, a.id));
    let mut picked = SeededRng::new(seed).pick_ordered(&candidates, limit * 2, limit);
    picked.sort_by_key(|a| std::cmp::Reverse(a.play_count.unwrap_or(0)));
    picked
        .into_iter()
        .map(|album| {
            let last = album.played.as_deref().and_then(parse_timestamp).unwrap_or(now);
            DiscoverAlbum {
                reason: rewind_reason(now, last, album.play_count.unwrap_or(0)),
                album: album.clone(),
                kind: ReasonKind::Rewind,
            }
        })
        .collect()
}

/// Rewind songs, most played first (no sampling: the list is short).
pub fn select_rewind_songs(songs: &[Song], now: i64, cutoff_days: i64, limit: usize) -> Vec<DiscoverSong> {
    let mut candidates: Vec<&Song> = songs
        .iter()
        .filter(|s| {
            is_rewind_candidate(s.played.as_deref(), s.play_count, MIN_SONG_PLAYS, now, cutoff_days)
        })
        .collect();
    candidates.sort_by_key(|s| std::cmp::Reverse(s.play_count.unwrap_or(0)));
    dedupe_by(&mut candidates, |s| format!("{}::{}", s.server_id, s.id));
    candidates
        .into_iter()
        .take(limit)
        .map(|song| {
            let last = song.played.as_deref().and_then(parse_timestamp).unwrap_or(now);
            DiscoverSong {
                reason: rewind_reason(now, last, song.play_count.unwrap_or(0)),
                song: song.clone(),
                kind: ReasonKind::Rewind,
                because_artist: None,
            }
        })
        .collect()
}

fn dedupe_by<T>(items: &mut Vec<T>, key: impl Fn(&T) -> String) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(key(item)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    // 2026-10-29T00:00:00Z
    const NOW: i64 = 1_793_232_000;

    fn iso(ts: i64) -> String {
        chrono::DateTime::from_timestamp(ts, 0).unwrap().to_rfc3339()
    }

    fn album(id: &str, plays: u32, days_ago: Option<i64>) -> Album {
        Album {
            id: id.to_string(),
            name: id.to_string(),
            play_count: Some(plays),
            played: days_ago.map(|d| iso(NOW - d * DAY)),
            server_id: "s1".to_string(),
            ..Default::default()
        }
    }

    const SIX: i64 = 180;

    #[test]
    fn month_choices_map_to_days() {
        assert_eq!(cutoff_days_for_months(3), 90);
        assert_eq!(cutoff_days_for_months(DEFAULT_REWIND_MONTHS), 180);
        assert_eq!(cutoff_days_for_months(12), 365);
        assert_eq!(cutoff_days_for_months(0), 30, "clamped to at least a month");
        assert_eq!(cutoff_days_for_months(99), 720, "clamped to two years");
    }

    #[test]
    fn cutoff_is_a_parameter() {
        assert_eq!(rewind_cutoff(NOW, SIX), NOW - 180 * DAY);
        let just_inside = iso(NOW - 179 * DAY);
        let just_outside = iso(NOW - 181 * DAY);
        assert!(!is_rewind_candidate(Some(&just_inside), Some(50), 5, NOW, SIX));
        assert!(is_rewind_candidate(Some(&just_outside), Some(50), 5, NOW, SIX));
        // Four months ago: inside a 3-month window, not 6 or 12.
        let four_months = iso(NOW - 120 * DAY);
        assert!(is_rewind_candidate(Some(&four_months), Some(50), 5, NOW, 90));
        assert!(!is_rewind_candidate(Some(&four_months), Some(50), 5, NOW, SIX));
        assert!(!is_rewind_candidate(Some(&iso(NOW - 300 * DAY)), Some(50), 5, NOW, 365));
    }

    #[test]
    fn needs_plays_and_a_real_timestamp() {
        let old = iso(NOW - 400 * DAY);
        assert!(!is_rewind_candidate(Some(&old), Some(4), 5, NOW, SIX));
        assert!(!is_rewind_candidate(None, Some(500), 5, NOW, SIX));
        assert!(!is_rewind_candidate(Some("1970-01-01T00:00:00Z"), Some(500), 5, NOW, SIX));
        assert!(!is_rewind_candidate(Some("not a date"), Some(500), 5, NOW, SIX));
    }

    #[test]
    fn selects_old_heavy_albums_with_reasons() {
        let albums = vec![
            album("recent-heavy", 300, Some(10)),
            album("old-heavy", 212, Some(426)),
            album("old-light", 2, Some(500)),
            album("never-dated", 99, None),
            album("old-medium", 77, Some(330)),
        ];
        let picked = select_rewind_albums(&albums, NOW, SIX, 42, 5);
        let ids: Vec<&str> = picked.iter().map(|a| a.album.id.as_str()).collect();
        assert_eq!(ids, vec!["old-heavy", "old-medium"]);
        // A one-year window drops the 11-month-old album.
        let year = select_rewind_albums(&albums, NOW, 365, 42, 5);
        let ids: Vec<&str> = year.iter().map(|a| a.album.id.as_str()).collect();
        assert_eq!(ids, vec!["old-heavy"]);
        assert_eq!(picked[0].reason, "Last played 14 months ago · 212 plays");
        assert_eq!(picked[0].kind, ReasonKind::Rewind);
    }

    #[test]
    fn album_selection_is_deterministic_per_seed() {
        let albums: Vec<Album> = (0..20)
            .map(|i| album(&format!("a{i}"), 100 + i, Some(200 + i64::from(i))))
            .collect();
        let a = select_rewind_albums(&albums, NOW, SIX, 7, 4);
        let b = select_rewind_albums(&albums, NOW, SIX, 7, 4);
        assert_eq!(a, b);
        assert_eq!(a.len(), 4);
        // Sorted by plays, drawn from the top 8.
        assert!(a.windows(2).all(|w| w[0].album.play_count >= w[1].album.play_count));
        assert!(a.iter().all(|x| x.album.play_count.unwrap() >= 112));
    }

    #[test]
    fn selects_rewind_songs() {
        let songs = vec![
            Song {
                id: "1".into(),
                play_count: Some(58),
                played: Some(iso(NOW - 800 * DAY)),
                ..Default::default()
            },
            Song {
                id: "2".into(),
                play_count: Some(90),
                played: Some(iso(NOW - 3 * DAY)),
                ..Default::default()
            },
        ];
        let picked = select_rewind_songs(&songs, NOW, SIX, 10);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].reason, "Last played 2 years ago · 58 plays");
    }
}
