//! Playback, queue and library actions: the one home for anything that
//! changes what plays (`PlayerCtx`). Now Playing, Up Next, the queue page,
//! the player bar and the add menu call it; lists start music with
//! [`PlayerCtx::play`], [`PlayerCtx::shuffle_play`] and
//! [`PlayerCtx::enqueue`].
//!
//! The player bar's buttons (`player/controls.rs`) and the sheet's own
//! buttons (different ids, see `transport.rs`) call the same methods, so
//! `play-pause-btn`, `prev-btn`, `next-btn`, `shuffle-btn` and `repeat-btn`
//! behave exactly like their `np-` copies. Handlers read with `peek`, so
//! calling them never subscribes a component to the queue.

use crate::api::{Capabilities, MusicClient, QueueSourceKind, ServerConfig, Song};
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta, normalize_manual_queue_songs,
    queue_should_generate_similar_on_end, spawn_shuffle_queue,
};
use crate::components::ui::{SongChange, StatusToast, TrackMenuController};
use crate::components::{
    ios_diag_log, seek_to, AudioState, IsPlayingSignal, PlaybackPositionSignal,
    ShuffleEnabledSignal, VolumeSignal,
};
use crate::db::{AppSettings, RepeatMode};
use dioxus::prelude::*;

/// Every playback signal, taken from context once. `Copy`, so closures and
/// tasks take it freely.
#[derive(Clone, Copy)]
pub(crate) struct PlayerCtx {
    pub servers: Signal<Vec<ServerConfig>>,
    pub queue: Signal<Vec<Song>>,
    pub queue_index: Signal<usize>,
    pub now_playing: Signal<Option<Song>>,
    pub is_playing: Signal<bool>,
    pub repeat: Signal<RepeatMode>,
    pub shuffle: Signal<bool>,
    pub settings: Signal<AppSettings>,
    pub audio: Signal<AudioState>,
    pub volume: Signal<f64>,
    pub position: Signal<f64>,
    pub track_menu: TrackMenuController,
    /// The shell's scope: favourite and rating calls finish there, so they
    /// survive the menu or button that started them closing.
    toast: StatusToast,
}

pub(crate) fn use_player() -> PlayerCtx {
    use_hook(|| PlayerCtx {
        servers: consume_context(),
        queue: consume_context(),
        queue_index: consume_context(),
        now_playing: consume_context(),
        is_playing: consume_context::<IsPlayingSignal>().0,
        repeat: consume_context(),
        shuffle: consume_context::<ShuffleEnabledSignal>().0,
        settings: consume_context(),
        audio: consume_context(),
        volume: consume_context::<VolumeSignal>().0,
        position: consume_context::<PlaybackPositionSignal>().0,
        track_menu: consume_context(),
        toast: consume_context(),
    })
}

/// Where [`PlayerCtx::enqueue`] puts songs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuePosition {
    /// Right after the playing song ("Play Next").
    Next,
    /// After the song that plays next, leaving Play Next's slot alone (the
    /// queue page's suggestions).
    // The queue page (views, chunk D) switches to this.
    #[allow(dead_code)]
    AfterNext,
    /// At the end ("Add to Queue").
    End,
}

impl QueuePosition {
    /// Where the songs go in a queue of `len` entries playing `current`.
    fn index(self, len: usize, current: usize) -> usize {
        match self {
            Self::Next => current.saturating_add(1).min(len),
            Self::AfterNext => current.saturating_add(2).min(len),
            Self::End => len,
        }
    }
}

pub(crate) const NOTHING_TO_PLAY: &str = "Nothing to play.";
pub(crate) const OFFLINE_NOTHING: &str =
    "You're offline, and none of these songs are downloaded. Connect to play them.";

/// The songs of `songs` that `can_play` accepts, and where `start` landed
/// among them (the first song when `start` was dropped). Errors say why
/// nothing is left.
fn keep_playable(
    songs: Vec<Song>,
    start: usize,
    offline: bool,
    can_play: impl Fn(&Song) -> bool,
) -> Result<(Vec<Song>, usize), String> {
    if songs.is_empty() {
        return Err(NOTHING_TO_PLAY.to_string());
    }
    if songs.iter().all(&can_play) {
        let start = start.min(songs.len() - 1);
        return Ok((songs, start));
    }
    let wanted = songs.get(start).map(song_key);
    let kept: Vec<Song> = songs.into_iter().filter(|song| can_play(song)).collect();
    if kept.is_empty() {
        return Err(if offline {
            OFFLINE_NOTHING.to_string()
        } else {
            format!("{}.", crate::local_music::NOT_PLAYABLE)
        });
    }
    let start = wanted
        .and_then(|key| kept.iter().position(|song| song_key(song) == key))
        .unwrap_or(0);
    Ok((kept, start))
}

/// Remembers where a queue came from (album, playlist…), so the player's
/// shuffle can reorder and restore it; plain lists become manual entries.
fn tag_source(songs: Vec<Song>, source: Option<(QueueSourceKind, String)>) -> Vec<Song> {
    match source {
        Some((kind, id)) => assign_collection_queue_meta(songs, kind, id),
        None => normalize_manual_queue_songs(songs),
    }
}

/// What Next does at queue entry `index` of `len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextStep {
    /// Repeat one: back to the start of the song.
    Restart,
    Play(usize),
    /// The end of a one-song shuffle: grow a queue of similar songs.
    GrowSimilar,
    Stop,
}

/// Repeat-one restarts, the end of the queue wraps (repeat all), grows a
/// similar queue (`grows`, asked only at the end with repeat off), or stops.
fn next_step(index: usize, len: usize, repeat: RepeatMode, grows: impl FnOnce() -> bool) -> NextStep {
    if repeat == RepeatMode::One {
        NextStep::Restart
    } else if index + 1 < len {
        NextStep::Play(index + 1)
    } else if repeat == RepeatMode::All && len > 0 {
        NextStep::Play(0)
    } else if repeat == RepeatMode::Off && grows() {
        NextStep::GrowSimilar
    } else {
        NextStep::Stop
    }
}

/// Off → All → One → Off.
fn next_repeat(mode: RepeatMode) -> RepeatMode {
    match mode {
        RepeatMode::Off => RepeatMode::All,
        RepeatMode::All => RepeatMode::One,
        RepeatMode::One => RepeatMode::Off,
    }
}

/// The playing index after moving entry `from` to `to`, playing `current`.
fn index_after_move(from: usize, to: usize, current: usize) -> usize {
    if from == current {
        to
    } else if from < current && to >= current {
        current - 1
    } else if from > current && to <= current {
        current + 1
    } else {
        current
    }
}

/// Applies a favourite or rating change to one song (the same marks lists
/// use: `starred` is a marker, a rating of 0 clears it).
pub(crate) fn apply_song_change(song: &mut Song, change: &SongChange) {
    if let Some(on) = change.starred {
        song.starred = on.then(|| "local".to_string());
    }
    if let Some(value) = change.rating {
        song.user_rating = (value > 0).then_some(value);
    }
}

impl PlayerCtx {
    // ---- Starting music ---------------------------------------------------

    /// Offline, only downloaded songs and music on the device play; local
    /// files this device can't decode never join the queue. Call from
    /// handlers or tasks, never from render (it checks the filesystem).
    fn playable(self, songs: Vec<Song>, start: usize) -> Result<(Vec<Song>, usize), String> {
        let offline =
            self.settings.peek().offline_mode || crate::cache_service::is_offline_mode();
        keep_playable(songs, start, offline, |song| {
            crate::local_music::is_playable(song)
                && (!offline
                    || crate::local_music::is_local_song(song)
                    || crate::offline_audio::is_song_downloaded(song))
        })
    }

    /// Replaces the queue and plays entry `index`.
    fn start(mut self, songs: Vec<Song>, index: usize) {
        let first = songs.get(index).cloned();
        self.queue.set(songs);
        self.queue_index.set(index);
        self.now_playing.set(first);
        self.is_playing.set(true);
    }

    /// Replaces the queue with `songs` and plays `start`. With a `source`
    /// (album, playlist…, plus its id) the queue remembers where it came
    /// from, so the player's shuffle can reorder and restore it. Respects the
    /// current shuffle mode. Errors are ready to show (offline, nothing
    /// playable).
    pub fn play(
        self,
        songs: Vec<Song>,
        start: usize,
        source: Option<(QueueSourceKind, String)>,
    ) -> Result<(), String> {
        let (songs, start) = self.playable(songs, start)?;
        self.start(tag_source(songs, source), start);
        if *self.shuffle.peek() {
            apply_collection_shuffle_mode(self.queue, self.queue_index, self.now_playing, true);
        }
        Ok(())
    }

    /// Plays `songs` in a random order and switches shuffle on. Turning
    /// shuffle off later restores the original order.
    // Lists (views, chunk D) switch to this from their own copies.
    #[allow(dead_code)]
    pub fn shuffle_play(
        mut self,
        songs: Vec<Song>,
        source: Option<(QueueSourceKind, String)>,
    ) -> Result<(), String> {
        use rand::seq::SliceRandom;
        let (songs, _) = self.playable(songs, 0)?;
        let mut songs = tag_source(songs, source);
        songs.shuffle(&mut rand::thread_rng());
        if !*self.shuffle.peek() {
            self.shuffle.set(true);
        }
        self.start(songs, 0);
        Ok(())
    }

    /// Adds `songs` to the queue `at` a position; an empty, idle player
    /// starts playing the first one. Returns how many were added.
    pub fn enqueue(mut self, songs: Vec<Song>, at: QueuePosition) -> usize {
        let songs = normalize_manual_queue_songs(songs);
        let count = songs.len();
        if count == 0 {
            return 0;
        }
        if self.now_playing.peek().is_none() && self.queue.peek().is_empty() {
            self.start(songs, 0);
            return count;
        }
        let index = at.index(self.queue.peek().len(), *self.queue_index.peek());
        self.queue.with_mut(|items| {
            items.splice(index..index, songs);
        });
        count
    }

    // ---- Transport --------------------------------------------------------

    pub fn toggle_play(&mut self) {
        let current = *self.is_playing.peek();
        ios_diag_log("ui.control", &format!("source=play-pause current={current} next={}", !current));
        self.is_playing.set(!current);
    }

    /// Restart, or go back a song on a quick second press
    /// (`player/skip_back.rs`). `source` names the control in the logs.
    pub fn previous(&mut self, source: &str) {
        let current_time = self.audio.peek().current_time;
        crate::components::player::skip_back::press_skip_back(
            crate::components::player::skip_back::SkipBackSignals {
                queue: self.queue,
                queue_index: self.queue_index,
                now_playing: self.now_playing,
                is_playing: self.is_playing,
                repeat: self.repeat,
                position: self.position,
                current_time,
            },
            source,
            false,
        );
    }

    /// Repeat-one restarts, the end of the queue wraps (repeat all), grows a
    /// similar queue, or stops. Radio has no next. `source` names the control
    /// in the logs.
    pub fn next(&mut self, source: &str) {
        if self.now_playing.peek().as_ref().is_some_and(is_live_song) {
            return;
        }
        let was_playing = *self.is_playing.peek();
        let repeat = *self.repeat.peek();
        let index = *self.queue_index.peek();
        let len = self.queue.peek().len();
        let shuffle = *self.shuffle.peek();
        let step = next_step(index, len, repeat, || {
            queue_should_generate_similar_on_end(
                &self.queue.peek(),
                self.now_playing.peek().as_ref(),
                shuffle,
            )
        });
        ios_diag_log(
            "ui.control",
            &format!(
                "source={source} step={step:?} from_idx={index} queue_len={len} repeat={repeat:?} shuffle={shuffle} was_playing={was_playing}"
            ),
        );
        match step {
            NextStep::Restart => {
                seek_to(0.0);
                if was_playing {
                    self.is_playing.set(true);
                }
            }
            NextStep::Play(target) => {
                let song = self.queue.peek().get(target).cloned();
                self.queue_index.set(target);
                self.now_playing.set(song);
                if was_playing {
                    self.is_playing.set(true);
                }
            }
            NextStep::GrowSimilar => spawn_shuffle_queue(
                self.servers.peek().clone(),
                self.queue,
                self.queue_index,
                self.now_playing,
                self.is_playing,
                self.audio,
                self.now_playing.peek().clone(),
                Some(was_playing),
            ),
            NextStep::Stop => self.is_playing.set(false),
        }
    }

    /// Shuffle on or off. The shell saves the choice to the settings.
    pub fn toggle_shuffle(&mut self) {
        let next = !*self.shuffle.peek();
        self.shuffle.set(next);
        let _ = apply_collection_shuffle_mode(self.queue, self.queue_index, self.now_playing, next);
    }

    /// Off → All → One → Off. The shell saves the choice to the settings.
    pub fn cycle_repeat(&mut self) {
        let next = next_repeat(*self.repeat.peek());
        self.repeat.set(next);
    }

    pub fn set_volume(&mut self, value: f64) {
        self.volume.set(value.clamp(0.0, 1.0));
    }

    pub fn seek(&mut self, seconds: f64) {
        let target = seconds.max(0.0);
        self.position.set(target);
        let mut current = self.audio.peek().current_time;
        current.set(target);
        seek_to(target);
    }

    /// One queue entry forward (`delta` 1) or back (-1), keeping the play
    /// state. The cover swipe uses it: no skip-back restart rule, no wrap and
    /// no queue growth, so one gesture moves exactly one song. False at an end.
    pub fn step(&mut self, delta: isize) -> bool {
        let index = *self.queue_index.peek();
        let Some(target) = index.checked_add_signed(delta) else {
            return false;
        };
        let song = self.queue.peek().get(target).cloned();
        let Some(song) = song else {
            return false;
        };
        let was_playing = *self.is_playing.peek();
        self.queue_index.set(target);
        self.now_playing.set(Some(song));
        if was_playing {
            self.is_playing.set(true);
        }
        true
    }

    // ---- Editing the queue ------------------------------------------------

    /// Jumps to queue entry `index` and plays it.
    pub fn play_index(&mut self, index: usize) {
        let song = self.queue.peek().get(index).cloned();
        if let Some(song) = song {
            self.queue_index.set(index);
            self.now_playing.set(Some(song));
            self.is_playing.set(true);
        }
    }

    /// Moves a queue entry; the playing entry keeps playing.
    pub fn move_entry(&mut self, from: usize, to: usize) {
        let len = self.queue.peek().len();
        if len < 2 || from >= len || to >= len || from == to {
            return;
        }
        let current = *self.queue_index.peek();
        self.queue.with_mut(|items| {
            let song = items.remove(from);
            items.insert(to, song);
        });
        let next_index = index_after_move(from, to, current);
        if next_index != current {
            self.queue_index.set(next_index);
        }
    }

    /// Removes a queue entry. Removing the playing entry plays the next one.
    pub fn remove_entry(&mut self, index: usize) {
        let len = self.queue.peek().len();
        if index >= len {
            return;
        }
        let current = *self.queue_index.peek();
        let was_playing = *self.is_playing.peek();
        self.queue.with_mut(|items| {
            items.remove(index);
        });
        let remaining = len - 1;
        if remaining == 0 {
            self.queue_index.set(0);
            self.now_playing.set(None);
            self.is_playing.set(false);
            return;
        }
        if index < current {
            self.queue_index.set(current - 1);
        } else if index == current {
            let next = index.min(remaining - 1);
            let song = self.queue.peek().get(next).cloned();
            self.queue_index.set(next);
            self.now_playing.set(song);
            self.is_playing.set(was_playing);
        }
    }

    /// Puts `song` back at `index` (Undo after a remove). The playing entry
    /// keeps playing; an emptied queue gets the song back, paused.
    pub fn insert_entry(&mut self, index: usize, song: Song) {
        let len = self.queue.peek().len();
        let at = index.min(len);
        self.queue.with_mut(|items| items.insert(at, song.clone()));
        if len == 0 {
            self.queue_index.set(0);
            self.now_playing.set(Some(song));
            return;
        }
        let current = *self.queue_index.peek();
        if at <= current {
            self.queue_index.set(current + 1);
        }
    }

    /// Clears the queue, keeping the playing song.
    pub fn clear_queue(&mut self) {
        let current = self.now_playing.peek().clone();
        match current {
            Some(song) => {
                self.queue.set(vec![song]);
                self.queue_index.set(0);
            }
            None => {
                self.queue.set(Vec::new());
                self.queue_index.set(0);
                self.is_playing.set(false);
            }
        }
    }

    // ---- Favourites and ratings -------------------------------------------

    /// Applies `change` to every queued copy of its song and to the playing
    /// one, then publishes it (`TrackMenuController::last_change`) so lists
    /// update their marks.
    pub fn apply_change(mut self, change: SongChange) {
        self.queue.with_mut(|items| {
            for item in items.iter_mut().filter(|song| change.matches(song)) {
                apply_song_change(item, &change);
            }
        });
        let playing = self.now_playing.peek().as_ref().is_some_and(|song| change.matches(song));
        if playing {
            self.now_playing.with_mut(|current| {
                if let Some(song) = current.as_mut() {
                    apply_song_change(song, &change);
                }
            });
        }
        let mut last_change = self.track_menu.last_change;
        last_change.set(Some(change));
    }

    fn change_for(song: &Song) -> SongChange {
        SongChange {
            server_id: song.server_id.clone(),
            song_id: song.id.clone(),
            starred: None,
            rating: None,
            downloaded: None,
        }
    }

    /// Stars or unstars `song` where it lives (its server, or the device's
    /// index for music on the device), then [`Self::apply_change`].
    pub fn toggle_favorite(self, song: Song) {
        let Some(client) = MusicClient::for_source(&self.servers.peek(), &song.server_id) else {
            return;
        };
        let star = song.starred.is_none();
        self.toast.spawn(async move {
            let result = if star {
                client.star(&song.id, "song").await
            } else {
                client.unstar(&song.id, "song").await
            };
            if result.is_ok() {
                let mut change = Self::change_for(&song);
                change.starred = Some(star);
                self.apply_change(change);
            }
        });
    }

    /// Sets a 0–5 rating (0 clears) where the song's source keeps star
    /// ratings ([`Capabilities::star_ratings`]; not Jellyfin), then
    /// [`Self::apply_change`].
    pub fn set_rating(self, song: Song, value: u32) {
        let servers = self.servers.peek();
        if !Capabilities::for_source(&servers, &song.server_id).star_ratings {
            return;
        }
        let Some(client) = MusicClient::for_source(&servers, &song.server_id) else {
            return;
        };
        drop(servers);
        let value = value.min(5);
        self.toast.spawn(async move {
            if client.set_rating(&song.id, value).await.is_ok() {
                let mut change = Self::change_for(&song);
                change.rating = Some(value);
                self.apply_change(change);
            }
        });
    }
}

/// `server::id`, used for keys and dedup.
// TODO(chunk A): `Song::key()` in `api/models.rs` replaces this.
pub(crate) fn song_key(song: &Song) -> String {
    format!("{}::{}", song.server_id, song.id)
}

// TODO(chunk A): `Song::same_as()` in `api/models.rs` replaces this.
pub(crate) fn same_song(a: &Song, b: &Song) -> bool {
    a.id == b.id && a.server_id == b.server_id
}

/// Radio entries: no seeking, no queue actions.
// TODO(chunk A): `Song::is_radio()` in `api/models.rs` replaces this.
pub(crate) fn is_live_song(song: &Song) -> bool {
    song.server_name == "Radio"
        || song
            .stream_url
            .as_ref()
            .map(|url| !url.trim().is_empty())
            .unwrap_or(false)
}

/// "FLAC 24/96"-style badge text when the format is known. Subsonic gives
/// the container suffix and bitrate; sample rate and depth are not in the
/// song model, so lossy files show the bitrate instead.
// TODO(chunk A): moves to `src/format.rs`.
pub(crate) fn format_badge(song: &Song) -> Option<String> {
    let suffix = song
        .suffix
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .or_else(|| {
            song.content_type
                .as_deref()
                .and_then(|ct| ct.split('/').nth(1))
                .map(|s| s.trim_start_matches("x-").to_ascii_uppercase())
        })?;
    let lossless = matches!(suffix.as_str(), "FLAC" | "ALAC" | "WAV" | "AIFF" | "APE" | "WV" | "DSF");
    match song.bitrate.filter(|b| *b > 0) {
        Some(kbps) if !lossless => Some(format!("{suffix} {kbps}")),
        _ => Some(suffix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str) -> Song {
        Song {
            id: id.to_string(),
            server_id: "srv".to_string(),
            ..Song::default()
        }
    }

    fn ids(songs: &[Song]) -> Vec<&str> {
        songs.iter().map(|s| s.id.as_str()).collect()
    }

    #[test]
    fn queue_positions() {
        // Playing entry 1 of 4.
        assert_eq!(QueuePosition::Next.index(4, 1), 2);
        assert_eq!(QueuePosition::AfterNext.index(4, 1), 3);
        assert_eq!(QueuePosition::End.index(4, 1), 4);
        // At the last entry everything lands at the end.
        assert_eq!(QueuePosition::Next.index(4, 3), 4);
        assert_eq!(QueuePosition::AfterNext.index(4, 3), 4);
        // An index past the end (stale) never overflows the queue.
        assert_eq!(QueuePosition::AfterNext.index(2, 9), 2);
        assert_eq!(QueuePosition::Next.index(0, usize::MAX), 0);
    }

    #[test]
    fn playable_keeps_everything_when_all_play() {
        let songs = vec![song("a"), song("b"), song("c")];
        let (kept, start) = keep_playable(songs, 7, false, |_| true).unwrap();
        assert_eq!(ids(&kept), ["a", "b", "c"]);
        assert_eq!(start, 2, "a start past the end clamps to the last song");
    }

    #[test]
    fn playable_drops_songs_and_follows_the_start() {
        let songs = vec![song("a"), song("b"), song("c"), song("d")];
        let (kept, start) = keep_playable(songs.clone(), 2, true, |s| s.id != "b").unwrap();
        assert_eq!(ids(&kept), ["a", "c", "d"]);
        assert_eq!(start, 1, "still starts on c");
        // The start itself was dropped: play from the top.
        let (_, start) = keep_playable(songs, 1, true, |s| s.id != "b").unwrap();
        assert_eq!(start, 0);
    }

    #[test]
    fn playable_errors_say_why() {
        assert_eq!(keep_playable(Vec::new(), 0, false, |_| true).unwrap_err(), NOTHING_TO_PLAY);
        assert_eq!(keep_playable(vec![song("a")], 0, true, |_| false).unwrap_err(), OFFLINE_NOTHING);
        let online = keep_playable(vec![song("a")], 0, false, |_| false).unwrap_err();
        assert!(online.starts_with(crate::local_music::NOT_PLAYABLE));
    }

    #[test]
    fn next_follows_repeat() {
        let never = || false;
        assert_eq!(next_step(0, 3, RepeatMode::Off, never), NextStep::Play(1));
        assert_eq!(next_step(1, 3, RepeatMode::One, never), NextStep::Restart);
        assert_eq!(next_step(2, 3, RepeatMode::All, never), NextStep::Play(0));
        assert_eq!(next_step(2, 3, RepeatMode::Off, never), NextStep::Stop);
        assert_eq!(next_step(0, 1, RepeatMode::Off, || true), NextStep::GrowSimilar);
        // Growth is only considered with repeat off at the end.
        assert_eq!(next_step(0, 1, RepeatMode::All, || panic!("not asked")), NextStep::Play(0));
        assert_eq!(next_step(0, 0, RepeatMode::All, never), NextStep::Stop);
    }

    #[test]
    fn repeat_cycles_off_all_one() {
        assert_eq!(next_repeat(RepeatMode::Off), RepeatMode::All);
        assert_eq!(next_repeat(RepeatMode::All), RepeatMode::One);
        assert_eq!(next_repeat(RepeatMode::One), RepeatMode::Off);
    }

    #[test]
    fn moving_entries_keeps_the_playing_one() {
        // Playing entry 2.
        assert_eq!(index_after_move(2, 0, 2), 0, "the playing entry moves");
        assert_eq!(index_after_move(0, 3, 2), 1, "an earlier entry moves past it");
        assert_eq!(index_after_move(4, 1, 2), 3, "a later entry moves before it");
        assert_eq!(index_after_move(3, 4, 2), 2, "after it, untouched");
        assert_eq!(index_after_move(0, 1, 2), 2, "before it, untouched");
    }

    #[test]
    fn changes_mark_songs_like_lists_do() {
        let mut target = song("a");
        let mut change = PlayerCtx::change_for(&target);
        change.starred = Some(true);
        apply_song_change(&mut target, &change);
        assert!(target.starred.is_some());
        assert_eq!(target.user_rating, None, "a favourite leaves the rating alone");

        change.starred = None;
        change.rating = Some(4);
        apply_song_change(&mut target, &change);
        assert_eq!(target.user_rating, Some(4));
        assert!(target.starred.is_some());

        change.starred = Some(false);
        change.rating = Some(0);
        apply_song_change(&mut target, &change);
        assert_eq!(target.starred, None);
        assert_eq!(target.user_rating, None, "0 clears the rating");
    }
}
