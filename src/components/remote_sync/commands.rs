//! Incoming `cmd`s, applied through the same player paths as this device's
//! own buttons (`PlayerCtx`), so a remote pause is exactly a local pause.

use super::protocol::Op;
use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::PlayerCtx;
use dioxus::prelude::*;
use futures_util::future::join_all;
use std::collections::HashMap;

/// Songs fetched at once while filling a transferred queue.
const FETCH_BATCH: usize = 8;

/// Runs `op` on this device. `server`: the room's music server (queue ids
/// are its song ids). `seek_request`: seeks a song once it has loaded.
pub(super) fn apply(mut player: PlayerCtx, seek_request: Signal<Option<(String, f64)>>, server: ServerConfig, op: Op) {
    match op {
        Op::Play => {
            if player.now_playing.peek().is_some() && !*player.is_playing.peek() {
                player.is_playing.set(true);
            }
        }
        Op::Pause => {
            if *player.is_playing.peek() {
                player.is_playing.set(false);
            }
        }
        Op::Seek { ms } => player.seek(ms as f64 / 1000.0),
        Op::Next => player.next("remote"),
        Op::Prev => player.previous("remote"),
        Op::Volume { v } => player.set_volume(v),
        Op::PlayQueue { ids, index, ms } => {
            spawn(play_queue(player, seek_request, server, ids, index as usize, ms));
        }
        // Never delivered; the server turns it into `play_queue` + `pause`.
        Op::Transfer => {}
    }
}

/// Where `wanted` landed in `songs` after unknown ids were dropped: the
/// entry with that id nearest its old place.
fn index_of(songs: &[Song], ids: &[String], wanted: usize) -> usize {
    let Some(id) = ids.get(wanted) else {
        return 0;
    };
    let before = ids[..wanted].iter().filter(|i| songs.iter().any(|s| &s.id == *i)).count();
    songs
        .iter()
        .enumerate()
        .filter(|(_, s)| &s.id == id)
        .min_by_key(|(i, _)| i.abs_diff(before))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Plays `ids[index]` from `ms` as soon as that one song is known (from the
/// queue here, else one `getSong`), then fills in the rest of the queue
/// around it without interrupting it.
async fn play_queue(
    player: PlayerCtx,
    mut seek_request: Signal<Option<(String, f64)>>,
    server: ServerConfig,
    ids: Vec<String>,
    index: usize,
    ms: u64,
) {
    let Some(first_id) = ids.get(index).cloned() else {
        return;
    };
    let mut known: HashMap<String, Song> = player
        .queue
        .peek()
        .iter()
        .chain(player.now_playing.peek().iter())
        .filter(|s| s.server_id == server.id)
        .map(|s| (s.id.clone(), s.clone()))
        .collect();
    let client = NavidromeClient::new(server.clone());
    let first = match known.get(&first_id) {
        Some(song) => song.clone(),
        None => match client.get_song(&first_id).await {
            Ok(song) => song,
            Err(_) => return,
        },
    };
    known.insert(first.id.clone(), first.clone());

    let seconds = ms as f64 / 1000.0;
    if seconds > 0.5 {
        seek_request.set(Some((first.id.clone(), seconds)));
    }
    let mut position = player.position;
    position.set(seconds);
    if player.play(vec![first.clone()], 0, None).is_err() {
        return;
    }

    // The rest, a few requests at a time.
    let missing: Vec<String> = ids.iter().filter(|id| !known.contains_key(*id)).cloned().collect();
    for batch in missing.chunks(FETCH_BATCH) {
        let fetched = join_all(batch.iter().map(|id| client.get_song(id))).await;
        for song in fetched.into_iter().flatten() {
            known.insert(song.id.clone(), song);
        }
    }
    let songs: Vec<Song> = ids.iter().filter_map(|id| known.get(id).cloned()).collect();
    if songs.len() < 2 {
        return;
    }
    // Only while that song still plays: the user may have moved on.
    let still = player
        .now_playing
        .peek()
        .as_ref()
        .is_some_and(|s| s.id == first.id && s.server_id == first.server_id);
    if still {
        let at = index_of(&songs, &ids, index);
        let (mut queue, mut queue_index) = (player.queue, player.queue_index);
        queue.set(songs);
        queue_index.set(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song { id: id.into(), ..Default::default() }
    }

    #[test]
    fn the_current_entry_is_found_after_unknown_ids_drop_out() {
        let ids: Vec<String> = ["a", "gone", "b", "c"].iter().map(|s| s.to_string()).collect();
        let songs = vec![song("a"), song("b"), song("c")];
        assert_eq!(index_of(&songs, &ids, 2), 1);
        assert_eq!(index_of(&songs, &ids, 0), 0);
        // A song queued twice: the copy nearest the old place.
        let ids: Vec<String> = ["a", "b", "a"].iter().map(|s| s.to_string()).collect();
        let songs = vec![song("a"), song("b"), song("a")];
        assert_eq!(index_of(&songs, &ids, 2), 2);
    }
}
