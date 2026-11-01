//! Autoplay ("never stop the music"): when three or fewer songs are left
//! after the current one, similar songs are fetched in the background and
//! appended, so the queue never runs dry. On by default
//! (`AppSettings::autoplay_enabled`; Settings › Playback).
//!
//! One pipeline: server songs come from `generate_queue_extension_from_seed`
//! (similar songs, then the same genre, then random songs, as Quick Create
//! uses), seeded with the last song in the queue. A queue that ends with a
//! song on this device continues from the device's own library (same artist,
//! then genre) and never from a server; radio never autoplays. Nothing
//! already queued or played in the last ~100 songs comes back.
//!
//! Added songs carry `QueueSourceKind::Autoplay`, so Up Next can show where
//! the user's own queue ends. Right after the user removes songs ("Clear Up
//! Next") nothing is refilled until the next song starts. With Autoplay off, the older behaviour stays:
//! a single shuffled song turns into a similar queue when it ends
//! (`queue_should_generate_similar_on_end`).

use super::{assign_collection_queue_meta, generate_queue_extension_from_seed, queue_extension_song_key};
use crate::api::{QueueSourceKind, ServerConfig, Song};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

/// Fetch when this many songs (or fewer) are left after the current one.
pub const AUTOPLAY_LOOKAHEAD: usize = 3;
/// Songs added per fetch.
pub const AUTOPLAY_BATCH: usize = 10;
/// Recently played songs Autoplay won't bring back.
pub const AUTOPLAY_HISTORY: usize = 100;

static HISTORY: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

fn is_radio(song: &Song) -> bool {
    song.server_name == "Radio"
}

/// Notes a song as played (newest last), keeping the last `AUTOPLAY_HISTORY`.
pub fn remember_played(song: &Song) {
    if is_radio(song) || song.id.trim().is_empty() {
        return;
    }
    let key = queue_extension_song_key(song);
    if let Ok(mut history) = HISTORY.lock() {
        if history.back() == Some(&key) {
            return;
        }
        history.retain(|k| k != &key);
        history.push_back(key);
        while history.len() > AUTOPLAY_HISTORY {
            history.pop_front();
        }
    }
}

/// Keys (`server::id`) of the songs played lately.
pub fn recently_played_keys() -> HashSet<String> {
    HISTORY
        .lock()
        .map(|history| history.iter().cloned().collect())
        .unwrap_or_default()
}

/// Whether the queue needs topping up now: Autoplay on, repeat off, a
/// regular song playing, and few songs left after it.
pub fn autoplay_wanted(queue: &[Song], index: usize, enabled: bool, repeat: RepeatMode) -> bool {
    if !enabled || repeat != RepeatMode::Off {
        return false;
    }
    let Some(current) = queue.get(index) else {
        return false;
    };
    if is_radio(current) {
        return false;
    }
    queue.len() - index - 1 <= AUTOPLAY_LOOKAHEAD && autoplay_seed(queue).is_some()
}

/// The song new ones should follow: the last in the queue, unless it's radio.
pub fn autoplay_seed(queue: &[Song]) -> Option<&Song> {
    queue.last().filter(|song| !is_radio(song) && !song.id.trim().is_empty())
}

/// `candidates` minus anything queued or in `history`, without repeats, at
/// most `limit`.
pub fn unplayed_candidates(
    candidates: Vec<Song>,
    queue: &[Song],
    history: &HashSet<String>,
    limit: usize,
) -> Vec<Song> {
    let mut seen: HashSet<String> = queue.iter().map(queue_extension_song_key).collect();
    seen.extend(history.iter().cloned());
    candidates
        .into_iter()
        .filter(|song| !is_radio(song) && seen.insert(queue_extension_song_key(song)))
        .take(limit)
        .collect()
}

/// Similar songs to follow `seed`: from the device's library for a song on
/// this device, otherwise from the servers.
async fn fetch_autoplay(servers: Vec<ServerConfig>, seed: Song, queue: Vec<Song>) -> Vec<Song> {
    if crate::local_music::is_local_song(&seed) {
        let lookup = (AUTOPLAY_BATCH * 4) as u32;
        let found = crate::local_music::LocalLibrary
            .get_similar_songs(&seed, lookup)
            .await
            .unwrap_or_default();
        return unplayed_candidates(found, &queue, &recently_played_keys(), AUTOPLAY_BATCH);
    }
    generate_queue_extension_from_seed(servers, seed, queue, AUTOPLAY_BATCH).await
}

/// Watches the queue and appends Autoplay songs ahead of time. A fetch runs
/// once per queue tail (so an empty answer isn't retried in a loop) and is
/// dropped if the queue changed meanwhile. Never blocks playback.
pub fn use_autoplay(
    servers: Signal<Vec<ServerConfig>>,
    settings: Signal<AppSettings>,
    mut queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    now_playing: Signal<Option<Song>>,
    repeat: Signal<RepeatMode>,
) {
    let mut attempted = use_signal(|| None::<String>);
    // (queue length, playing song) at the last look, to tell a user's
    // removal ("Clear Up Next") from the queue moving on.
    let mut last_shape = use_signal(|| (0usize, String::new()));

    use_effect(move || {
        if let Some(song) = now_playing.read().as_ref() {
            remember_played(song);
        }
    });

    use_effect(move || {
        let (enabled, offline) = {
            let s = settings.read();
            (s.autoplay_enabled, s.offline_mode)
        };
        let repeat = repeat();
        let index = queue_index();
        let list = queue.read();
        let playing = list.get(index).map(queue_extension_song_key).unwrap_or_default();
        let shape = (list.len(), playing);
        let removed = {
            let last = last_shape.peek();
            shape.0 < last.0 && shape.1 == last.1
        };
        if *last_shape.peek() != shape {
            last_shape.set(shape);
        }
        let Some(seed) = autoplay_seed(&list).cloned() else {
            return;
        };
        let seed_key = queue_extension_song_key(&seed);
        // Per tail and position: a fetch that found nothing is retried at
        // the next song, not in a loop.
        let tail = format!("{}|{index}|{seed_key}", list.len());
        if removed {
            // The user just took songs out: don't refill at once. Autoplay
            // picks up again at the next song or when the queue grows.
            attempted.set(Some(tail));
            return;
        }
        if !autoplay_wanted(&list, index, enabled, repeat) {
            return;
        }
        if offline && !crate::local_music::is_local_song(&seed) {
            return;
        }
        if attempted.peek().as_deref() == Some(tail.as_str()) {
            return;
        }
        attempted.set(Some(tail));
        let snapshot = list.clone();
        drop(list);
        let len = snapshot.len();
        let servers_now = servers.peek().clone();
        spawn(async move {
            let additions = fetch_autoplay(servers_now, seed, snapshot).await;
            if additions.is_empty() {
                return;
            }
            let unchanged = {
                let list = queue.peek();
                list.len() == len && list.last().map(queue_extension_song_key) == Some(seed_key)
            };
            if !unchanged {
                return;
            }
            let marked =
                assign_collection_queue_meta(additions, QueueSourceKind::Autoplay, "autoplay".to_string());
            queue.with_mut(|items| items.extend(marked));
        });
    });
}

/// Whether Autoplay added this song (Up Next shows those under their own header).
pub fn is_autoplay_song(song: &Song) -> bool {
    song.queue_meta
        .as_ref()
        .is_some_and(|meta| meta.source_kind == QueueSourceKind::Autoplay)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song {
            id: id.to_string(),
            server_id: "srv".to_string(),
            server_name: "Mock".to_string(),
            ..Song::default()
        }
    }

    fn radio(id: &str) -> Song {
        Song {
            server_name: "Radio".to_string(),
            ..song(id)
        }
    }

    fn queue(n: usize) -> Vec<Song> {
        (0..n).map(|i| song(&format!("s{i}"))).collect()
    }

    #[test]
    fn fetches_only_near_the_end() {
        let q = queue(10);
        assert!(!autoplay_wanted(&q, 0, true, RepeatMode::Off));
        assert!(!autoplay_wanted(&q, 5, true, RepeatMode::Off));
        // Three songs left after the current one.
        assert!(autoplay_wanted(&q, 6, true, RepeatMode::Off));
        assert!(autoplay_wanted(&q, 9, true, RepeatMode::Off));
        // A short queue tops up at once.
        assert!(autoplay_wanted(&queue(1), 0, true, RepeatMode::Off));
    }

    #[test]
    fn never_with_autoplay_off_repeat_or_radio() {
        let q = queue(3);
        assert!(!autoplay_wanted(&q, 2, false, RepeatMode::Off));
        assert!(!autoplay_wanted(&q, 2, true, RepeatMode::All));
        assert!(!autoplay_wanted(&q, 2, true, RepeatMode::One));
        assert!(!autoplay_wanted(&[radio("r1")], 0, true, RepeatMode::Off));
        // Radio at the end of the queue isn't a seed either.
        let mut tail_radio = queue(2);
        tail_radio.push(radio("r1"));
        assert!(!autoplay_wanted(&tail_radio, 0, true, RepeatMode::Off));
        assert!(!autoplay_wanted(&[], 0, true, RepeatMode::Off));
        assert!(!autoplay_wanted(&q, 7, true, RepeatMode::Off));
    }

    #[test]
    fn seed_is_the_last_song() {
        let q = queue(4);
        assert_eq!(autoplay_seed(&q).map(|s| s.id.as_str()), Some("s3"));
        assert!(autoplay_seed(&[]).is_none());
    }

    #[test]
    fn candidates_skip_queued_played_and_repeated_songs() {
        let q = queue(3); // s0..s2
        let history: HashSet<String> = ["srv::h1".to_string()].into_iter().collect();
        let candidates = vec![
            song("s1"),
            song("h1"),
            song("n1"),
            song("n1"),
            radio("r9"),
            song("n2"),
            song("n3"),
        ];
        let picked = unplayed_candidates(candidates, &q, &history, 2);
        let ids: Vec<&str> = picked.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["n1", "n2"]);
    }

    #[test]
    fn history_keeps_the_latest_hundred() {
        for i in 0..(AUTOPLAY_HISTORY + 20) {
            remember_played(&Song {
                server_id: "hist".to_string(),
                ..song(&format!("h{i}"))
            });
        }
        remember_played(&radio("live"));
        let keys = recently_played_keys();
        let ours: Vec<_> = keys.iter().filter(|k| k.starts_with("hist::")).collect();
        assert_eq!(ours.len(), AUTOPLAY_HISTORY);
        assert!(keys.contains(&format!("hist::h{}", AUTOPLAY_HISTORY + 19)));
        assert!(!keys.contains("hist::h0"));
        assert!(!keys.iter().any(|k| k.ends_with("::live")));
    }

    #[test]
    fn autoplay_songs_are_marked() {
        let marked = assign_collection_queue_meta(queue(2), QueueSourceKind::Autoplay, "autoplay".to_string());
        assert!(marked.iter().all(is_autoplay_song));
        assert!(!is_autoplay_song(&song("x")));
    }
}
