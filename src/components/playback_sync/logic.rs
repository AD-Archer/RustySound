//! The pure half of playback sync: what to report, when to save the queue,
//! how much of it to save, and whether to offer another device's queue. No
//! signals, no network; `mod.rs` feeds these once a second.

use crate::api::PlaybackReportState;

/// Songs saved to the server at most, around the current one.
pub const QUEUE_SAVE_CAP: usize = 500;
/// Without `formPost` the ids ride in the URL, so fewer fit.
pub const QUEUE_SAVE_CAP_NO_FORM: usize = 150;
/// Of the saved window, how many come before the current song.
pub const QUEUE_SAVE_BEFORE: usize = 100;
/// Save at least this often while playing.
pub const SAVE_HEARTBEAT_MS: u64 = 30_000;
/// A trigger waits this long for more (rapid skips make one save).
pub const SAVE_DEBOUNCE_MS: u64 = 2_000;
/// And saves are at least this far apart (background saves excepted).
pub const SAVE_MIN_GAP_MS: u64 = 5_000;
/// A position jump bigger than this (beyond the clock) is a seek.
const SEEK_SLACK_MS: u64 = 3_000;

/// The song a report is about.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportSong {
    pub id: String,
    pub server_id: String,
    pub duration_ms: u64,
}

/// One look at the player.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSample {
    /// `None` when nothing reportable plays (nothing, radio, a preview).
    pub song: Option<ReportSong>,
    pub playing: bool,
    pub position_ms: u64,
    pub rate: f64,
    /// The song that just played to its end, if one did since the last look.
    pub finished: Option<(String, String)>,
    pub now_ms: u64,
}

/// One reportPlayback call to make.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackReport {
    pub song_id: String,
    pub server_id: String,
    pub state: PlaybackReportState,
    pub position_ms: u64,
    pub rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Session {
    song: ReportSong,
    state: PlaybackReportState,
    start_ms: u64,
    last_pos: u64,
    last_at: u64,
    rate: f64,
}

/// Turns samples into reportPlayback calls: `starting` when a song begins,
/// `playing` once it moves, `paused`, and `stopped` when it ends or another
/// song takes over. A seek or a speed change re-sends the current state with
/// the new position. Navidrome scrobbles on `stopped` (half the song or
/// 4 minutes), so a song that played to the end is stopped at its full
/// length. No heartbeat: the server runs its own clock from `playing`.
#[derive(Debug, Default)]
pub struct ReportTracker {
    session: Option<Session>,
}

impl ReportTracker {
    pub fn observe(&mut self, sample: &PlaybackSample) -> Vec<PlaybackReport> {
        let mut out = Vec::new();

        // A finished song is stopped at its end, so the server counts it.
        if let Some((server_id, id)) = &sample.finished {
            if let Some(session) = self.session.as_ref() {
                if &session.song.id == id && &session.song.server_id == server_id {
                    let end = session.song.duration_ms.max(session.last_pos);
                    out.push(Self::report(session, PlaybackReportState::Stopped, end));
                    self.session = None;
                }
            }
        }

        // Another song (or none): stop the old one where it was.
        let same_song = match (&self.session, &sample.song) {
            (Some(session), Some(song)) => session.song.id == song.id && session.song.server_id == song.server_id,
            _ => false,
        };
        if !same_song {
            if let Some(session) = self.session.take() {
                out.push(Self::report(&session, PlaybackReportState::Stopped, session.last_pos));
            }
        }

        let Some(song) = sample.song.clone() else {
            return out;
        };
        let pos = sample.position_ms;
        let rate = if sample.rate.is_finite() && sample.rate > 0.0 { sample.rate } else { 1.0 };

        let Some(session) = self.session.as_mut() else {
            // Only a song that plays opens a session: one loaded paused (a
            // resume) or left at its end isn't news.
            if sample.playing {
                let session = Session {
                    song,
                    state: PlaybackReportState::Starting,
                    start_ms: pos,
                    last_pos: pos,
                    last_at: sample.now_ms,
                    rate,
                };
                out.push(Self::report(&session, PlaybackReportState::Starting, pos));
                self.session = Some(session);
            }
            return out;
        };

        // Rewound from the end without an `ended` (repeat one on some
        // backends): the last play counts, and this is a new one.
        let near_end = session.song.duration_ms > 0
            && session.last_pos + SEEK_SLACK_MS >= session.song.duration_ms;
        if near_end && pos + SEEK_SLACK_MS < session.last_pos && pos < SEEK_SLACK_MS {
            let end = session.song.duration_ms;
            out.push(Self::report(session, PlaybackReportState::Stopped, end));
            self.session = None;
            out.extend(self.observe(&PlaybackSample { finished: None, ..sample.clone() }));
            return out;
        }

        let elapsed = sample.now_ms.saturating_sub(session.last_at);
        let expected = if session.state == PlaybackReportState::Playing {
            session.last_pos + (elapsed as f64 * session.rate) as u64
        } else {
            session.last_pos
        };
        let seeked = pos.abs_diff(expected) > SEEK_SLACK_MS;
        let rate_changed = (rate - session.rate).abs() > 0.01;

        let next = if !sample.playing {
            PlaybackReportState::Paused
        } else if session.state == PlaybackReportState::Starting && pos <= session.start_ms {
            // Still buffering.
            PlaybackReportState::Starting
        } else {
            PlaybackReportState::Playing
        };

        session.rate = rate;
        if next != session.state || seeked || rate_changed {
            session.state = next;
            out.push(Self::report(session, next, pos));
        }
        session.last_pos = pos;
        session.last_at = sample.now_ms;
        out
    }

    /// The report for the app going away: the playing song is stopped.
    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub fn stop(&mut self) -> Option<PlaybackReport> {
        let session = self.session.take()?;
        Some(Self::report(&session, PlaybackReportState::Stopped, session.last_pos))
    }

    fn report(session: &Session, state: PlaybackReportState, position_ms: u64) -> PlaybackReport {
        PlaybackReport {
            song_id: session.song.id.clone(),
            server_id: session.song.server_id.clone(),
            state,
            position_ms,
            rate: session.rate,
        }
    }
}

/// One look at the queue, for the save scheduler.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueSample {
    /// Changes whenever the queue, its order or the current song changes.
    pub signature: u64,
    /// The current song (server, id), if any.
    pub song: Option<(String, String)>,
    pub playing: bool,
    pub position_ms: u64,
    pub now_ms: u64,
}

/// Decides when to save the queue: on pause, on a track change or a queue
/// edit, every 30 s while playing, and when the app goes away. Triggers wait
/// [`SAVE_DEBOUNCE_MS`] and saves keep [`SAVE_MIN_GAP_MS`] apart, so skipping
/// through ten songs makes one save. Nothing is saved until this device has
/// played something: opening the app (or restoring a queue paused) must not
/// overwrite what another device saved.
#[derive(Debug, Default)]
pub struct SaveScheduler {
    played: bool,
    last_save_ms: Option<u64>,
    pending_since: Option<u64>,
    last_signature: Option<u64>,
    last_song: Option<(String, String)>,
    was_playing: bool,
    saved: Option<(u64, u64)>,
}

impl SaveScheduler {
    /// True when a save should start now.
    pub fn observe(&mut self, sample: &QueueSample) -> bool {
        if sample.playing {
            self.played = true;
        }
        let song_changed = self.last_song.is_some() && self.last_song != sample.song;
        let queue_changed = self.last_signature.is_some() && self.last_signature != Some(sample.signature);
        let paused = self.was_playing && !sample.playing;
        self.last_song = sample.song.clone();
        self.last_signature = Some(sample.signature);
        self.was_playing = sample.playing;

        if !self.played || sample.song.is_none() {
            return false;
        }
        if song_changed || queue_changed || paused {
            // Trailing debounce: each trigger restarts the wait.
            self.pending_since = Some(sample.now_ms);
        }
        let heartbeat_due = sample.playing
            && self
                .last_save_ms
                .map_or(true, |at| sample.now_ms.saturating_sub(at) >= SAVE_HEARTBEAT_MS);
        if heartbeat_due {
            self.pending_since.get_or_insert(sample.now_ms);
        }

        let Some(since) = self.pending_since else {
            return false;
        };
        let settled = heartbeat_due || sample.now_ms.saturating_sub(since) >= SAVE_DEBOUNCE_MS;
        let gap_ok = self
            .last_save_ms
            .map_or(true, |at| sample.now_ms.saturating_sub(at) >= SAVE_MIN_GAP_MS);
        if !(settled && gap_ok) {
            return false;
        }
        if self.saved == Some((sample.signature, sample.position_ms / 1000)) {
            // Nothing new since the last save (a pause right after one).
            self.pending_since = None;
            return false;
        }
        self.mark_saved(sample);
        true
    }

    /// The app is going away: save now unless nothing changed or nothing
    /// played.
    pub fn force(&mut self, sample: &QueueSample) -> bool {
        if !self.played || sample.song.is_none() {
            return false;
        }
        if self.saved == Some((sample.signature, sample.position_ms / 1000)) {
            return false;
        }
        self.mark_saved(sample);
        true
    }

    /// After loading a queue from the server: it's saved already, and the
    /// next save waits for this device to play.
    pub fn reset_after_restore(&mut self) {
        *self = Self::default();
    }

    fn mark_saved(&mut self, sample: &QueueSample) {
        self.last_save_ms = Some(sample.now_ms);
        self.pending_since = None;
        self.saved = Some((sample.signature, sample.position_ms / 1000));
    }
}

/// The slice of a queue to save: at most `cap` songs, up to
/// [`QUEUE_SAVE_BEFORE`] (scaled to the cap) before the current one and the
/// rest after it. Returns `(start, end)` with `start <= current < end` when
/// the queue isn't empty.
pub fn trim_queue_window(len: usize, current: usize, cap: usize) -> (usize, usize) {
    if len == 0 || cap == 0 {
        return (0, 0);
    }
    let current = current.min(len - 1);
    if len <= cap {
        return (0, len);
    }
    let before = (QUEUE_SAVE_BEFORE * cap / QUEUE_SAVE_CAP).min(cap - 1);
    let mut start = current.saturating_sub(before);
    let end = (start + cap).min(len);
    if end - start < cap {
        start = end.saturating_sub(cap);
    }
    (start, end)
}

/// A server queue, as far as the Continue offer cares.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueOfferInput<'a> {
    pub has_entries: bool,
    pub changed_ms: Option<i64>,
    pub changed_by: &'a str,
}

/// What this device remembers about a server's queue.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueueMarks {
    /// When this device last saved (its own clock).
    #[serde(default)]
    pub last_save_ms: Option<i64>,
    /// The `changed` of a queue the user dismissed or took.
    #[serde(default)]
    pub handled_changed_ms: Option<i64>,
}

/// Offer "Continue from …" when the server's queue was saved by another
/// client after this device's last save, and the user hasn't already
/// dismissed or taken that very save.
pub fn should_offer_queue(queue: &QueueOfferInput, our_client: &str, marks: &QueueMarks) -> bool {
    let Some(changed) = queue.changed_ms else {
        return false;
    };
    queue.has_entries
        && !queue.changed_by.trim().eq_ignore_ascii_case(our_client)
        && marks.handled_changed_ms != Some(changed)
        && marks.last_save_ms.map_or(true, |saved| changed > saved)
}

/// On launch, with Resume on launch on: take the server queue over the
/// newest bookmark when this device saved it and it's at least as new.
/// (Another device's queue is offered with the banner instead.)
pub fn prefer_own_server_queue(queue: &QueueOfferInput, our_client: &str, newest_bookmark_ms: Option<i64>) -> bool {
    let Some(changed) = queue.changed_ms else {
        return false;
    };
    queue.has_entries
        && queue.changed_by.trim().eq_ignore_ascii_case(our_client)
        && newest_bookmark_ms.map_or(true, |bookmark| changed >= bookmark)
}

/// One getNowPlaying player, as the "Also playing on" line needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct OtherPlayer {
    pub device: String,
    pub title: String,
}

/// The first other player of this user that is playing now. Players
/// without a name can't be told apart from this one, so they're skipped;
/// entries without a state (servers without `playbackReport`) count while
/// fresh.
pub fn pick_other_player(entries: &[crate::api::NowPlayingEntry], username: &str, our_client: &str) -> Option<OtherPlayer> {
    entries
        .iter()
        .filter(|e| e.username.eq_ignore_ascii_case(username))
        .filter(|e| match e.state.as_deref() {
            Some(state) => state == "playing" || state == "starting",
            None => e.minutes_ago < 10,
        })
        .find_map(|e| {
            let player = e.player_name.as_deref()?.trim();
            if player.is_empty() || player.eq_ignore_ascii_case(our_client) {
                return None;
            }
            Some(OtherPlayer {
                device: crate::api::device_from_client_name(player),
                title: e.title.clone(),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::NowPlayingEntry;
    use PlaybackReportState::*;

    fn song(id: &str) -> Option<ReportSong> {
        Some(ReportSong { id: id.into(), server_id: "s".into(), duration_ms: 200_000 })
    }

    fn sample(song_id: Option<&str>, playing: bool, pos: u64, now: u64) -> PlaybackSample {
        PlaybackSample { song: song_id.and_then(song), playing, position_ms: pos, rate: 1.0, finished: None, now_ms: now }
    }

    fn states(reports: &[PlaybackReport]) -> Vec<(String, PlaybackReportState, u64)> {
        reports.iter().map(|r| (r.song_id.clone(), r.state, r.position_ms)).collect()
    }

    #[test]
    fn a_song_starts_plays_pauses_and_stops() {
        let mut t = ReportTracker::default();
        assert_eq!(states(&t.observe(&sample(Some("a"), true, 0, 0))), vec![("a".into(), Starting, 0)]);
        // Buffering: nothing new.
        assert!(t.observe(&sample(Some("a"), true, 0, 1_000)).is_empty());
        assert_eq!(states(&t.observe(&sample(Some("a"), true, 900, 2_000))), vec![("a".into(), Playing, 900)]);
        // Plays on with the clock: no heartbeat.
        assert!(t.observe(&sample(Some("a"), true, 1_900, 3_000)).is_empty());
        assert!(t.observe(&sample(Some("a"), true, 2_900, 4_000)).is_empty());
        assert_eq!(states(&t.observe(&sample(Some("a"), false, 3_000, 5_000))), vec![("a".into(), Paused, 3_000)]);
        assert!(t.observe(&sample(Some("a"), false, 3_000, 6_000)).is_empty());
        assert_eq!(states(&t.observe(&sample(Some("a"), true, 3_000, 7_000))), vec![("a".into(), Playing, 3_000)]);
        assert_eq!(states(&t.stop().into_iter().collect::<Vec<_>>()), vec![("a".into(), Stopped, 3_000)]);
    }

    #[test]
    fn skipping_stops_the_old_song_where_it_was() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        let r = t.observe(&sample(Some("b"), true, 0, 2_000));
        assert_eq!(states(&r), vec![("a".into(), Stopped, 1_000), ("b".into(), Starting, 0)]);
    }

    #[test]
    fn a_finished_song_is_stopped_at_its_full_length() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        let mut next = sample(Some("b"), true, 0, 2_000);
        next.finished = Some(("s".into(), "a".into()));
        assert_eq!(states(&t.observe(&next)), vec![("a".into(), Stopped, 200_000), ("b".into(), Starting, 0)]);
    }

    #[test]
    fn the_last_song_ending_stops_once_and_stays_quiet() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        let mut end = sample(Some("a"), false, 200_000, 2_000);
        end.finished = Some(("s".into(), "a".into()));
        assert_eq!(states(&t.observe(&end)), vec![("a".into(), Stopped, 200_000)]);
        assert!(t.observe(&sample(Some("a"), false, 200_000, 3_000)).is_empty());
    }

    #[test]
    fn repeat_one_counts_each_play() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        let mut again = sample(Some("a"), true, 500, 2_000);
        again.finished = Some(("s".into(), "a".into()));
        assert_eq!(states(&t.observe(&again)), vec![("a".into(), Stopped, 200_000), ("a".into(), Starting, 500)]);
    }

    #[test]
    fn a_rewind_from_the_end_without_ended_counts_too() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 199_000, 1_000));
        let r = t.observe(&sample(Some("a"), true, 400, 2_000));
        assert_eq!(states(&r), vec![("a".into(), Stopped, 200_000), ("a".into(), Starting, 400)]);
    }

    #[test]
    fn a_seek_resends_the_state_with_the_new_position() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        assert_eq!(states(&t.observe(&sample(Some("a"), true, 90_000, 2_000))), vec![("a".into(), Playing, 90_000)]);
        t.observe(&sample(Some("a"), false, 90_500, 2_500));
        assert_eq!(states(&t.observe(&sample(Some("a"), false, 10_000, 3_000))), vec![("a".into(), Paused, 10_000)]);
    }

    #[test]
    fn a_speed_change_resends_with_the_new_rate() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        let mut fast = sample(Some("a"), true, 2_000, 2_000);
        fast.rate = 1.5;
        let r = t.observe(&fast);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].rate, 1.5);
        // At 1.5x the clock runs faster: no false seek.
        let mut later = sample(Some("a"), true, 3_500, 3_000);
        later.rate = 1.5;
        assert!(t.observe(&later).is_empty());
    }

    #[test]
    fn a_song_loaded_paused_is_not_reported_until_it_plays() {
        let mut t = ReportTracker::default();
        assert!(t.observe(&sample(Some("a"), false, 30_000, 0)).is_empty());
        assert_eq!(states(&t.observe(&sample(Some("a"), true, 30_000, 1_000))), vec![("a".into(), Starting, 30_000)]);
    }

    #[test]
    fn radio_or_nothing_stops_the_session() {
        let mut t = ReportTracker::default();
        t.observe(&sample(Some("a"), true, 0, 0));
        t.observe(&sample(Some("a"), true, 1_000, 1_000));
        assert_eq!(states(&t.observe(&sample(None, true, 0, 2_000))), vec![("a".into(), Stopped, 1_000)]);
        assert!(t.observe(&sample(None, true, 0, 3_000)).is_empty());
    }

    fn q(sig: u64, song: &str, playing: bool, pos: u64, now: u64) -> QueueSample {
        QueueSample { signature: sig, song: Some(("s".into(), song.into())), playing, position_ms: pos, now_ms: now }
    }

    #[test]
    fn nothing_is_saved_before_this_device_plays() {
        let mut s = SaveScheduler::default();
        assert!(!s.observe(&q(1, "a", false, 30_000, 0)));
        assert!(!s.observe(&q(2, "b", false, 0, 10_000)));
        assert!(!s.force(&q(2, "b", false, 0, 20_000)));
    }

    #[test]
    fn playing_saves_every_thirty_seconds() {
        let mut s = SaveScheduler::default();
        assert!(s.observe(&q(1, "a", true, 0, 0)), "first save when playback starts");
        assert!(!s.observe(&q(1, "a", true, 10_000, 10_000)));
        assert!(!s.observe(&q(1, "a", true, 29_000, 29_000)));
        assert!(s.observe(&q(1, "a", true, 30_000, 30_000)));
    }

    #[test]
    fn a_pause_saves_after_the_debounce() {
        let mut s = SaveScheduler::default();
        s.observe(&q(1, "a", true, 0, 0));
        assert!(!s.observe(&q(1, "a", false, 8_000, 8_000)));
        assert!(!s.observe(&q(1, "a", false, 8_000, 9_000)));
        assert!(s.observe(&q(1, "a", false, 8_000, 10_000)));
        assert!(!s.observe(&q(1, "a", false, 8_000, 20_000)), "paused and unchanged: no more saves");
    }

    #[test]
    fn rapid_skips_make_one_save() {
        let mut s = SaveScheduler::default();
        s.observe(&q(1, "a", true, 0, 0));
        let mut saves = 0;
        for (i, song) in ["b", "c", "d", "e"].iter().enumerate() {
            let now = 6_000 + i as u64 * 500;
            saves += s.observe(&q(10 + i as u64, song, true, 0, now)) as u32;
        }
        assert_eq!(saves, 0, "still skipping");
        assert!(s.observe(&q(13, "e", true, 1_000, 9_600)));
    }

    #[test]
    fn saves_keep_their_distance_but_background_saves_at_once() {
        let mut s = SaveScheduler::default();
        s.observe(&q(1, "a", true, 0, 0));
        s.observe(&q(2, "b", true, 0, 500));
        assert!(!s.observe(&q(2, "b", true, 2_600, 2_600)), "debounced but inside the 5 s gap");
        assert!(s.observe(&q(2, "b", true, 5_000, 5_000)));
        assert!(s.force(&q(2, "b", true, 6_000, 6_000)));
        assert!(!s.force(&q(2, "b", true, 6_000, 6_100)), "nothing new");
        assert_eq!(s.last_save_ms, Some(6_000));
    }

    #[test]
    fn a_restore_waits_for_local_play_again() {
        let mut s = SaveScheduler::default();
        s.observe(&q(1, "a", true, 0, 0));
        s.reset_after_restore();
        assert!(!s.observe(&q(9, "z", false, 151_000, 10_000)));
        assert!(!s.force(&q(9, "z", false, 151_000, 11_000)));
    }

    #[test]
    fn the_saved_window_keeps_the_current_song_and_favours_what_is_next() {
        assert_eq!(trim_queue_window(0, 0, 500), (0, 0));
        assert_eq!(trim_queue_window(10, 3, 500), (0, 10));
        assert_eq!(trim_queue_window(2_000, 0, 500), (0, 500));
        assert_eq!(trim_queue_window(2_000, 1_000, 500), (900, 1_400));
        assert_eq!(trim_queue_window(2_000, 1_999, 500), (1_500, 2_000));
        assert_eq!(trim_queue_window(2_000, 50, 500), (0, 500));
        assert_eq!(trim_queue_window(1_000, 999, 150), (850, 1_000));
        assert_eq!(trim_queue_window(1_000, 500, 150), (470, 620));
        let (start, end) = trim_queue_window(5, 99, 3);
        assert!(start <= 4 && 4 < end && end - start == 3);
    }

    fn offer(changed: Option<i64>, by: &str) -> QueueOfferInput<'_> {
        QueueOfferInput { has_entries: true, changed_ms: changed, changed_by: by }
    }

    #[test]
    fn another_devices_newer_queue_is_offered() {
        let me = "RustySound (Mac)";
        let marks = QueueMarks { last_save_ms: Some(1_000), handled_changed_ms: None };
        assert!(should_offer_queue(&offer(Some(2_000), "RustySound (iPhone)"), me, &marks));
        assert!(should_offer_queue(&offer(Some(2_000), "NavidromeUI"), me, &QueueMarks::default()));
    }

    #[test]
    fn our_own_older_or_dismissed_queues_are_not() {
        let me = "RustySound (Mac)";
        let marks = QueueMarks { last_save_ms: Some(3_000), handled_changed_ms: Some(2_500) };
        assert!(!should_offer_queue(&offer(Some(4_000), me), me, &marks), "ours");
        assert!(!should_offer_queue(&offer(Some(4_000), "rustysound (mac)"), me, &marks), "ours, any case");
        assert!(!should_offer_queue(&offer(Some(2_000), "RustySound (iPhone)"), me, &marks), "older than our save");
        let dismissed = QueueMarks { last_save_ms: None, handled_changed_ms: Some(4_000) };
        assert!(!should_offer_queue(&offer(Some(4_000), "RustySound (iPhone)"), me, &dismissed), "dismissed");
        assert!(should_offer_queue(&offer(Some(4_001), "RustySound (iPhone)"), me, &dismissed), "changed again");
        assert!(!should_offer_queue(&offer(None, "RustySound (iPhone)"), me, &QueueMarks::default()));
        let empty = QueueOfferInput { has_entries: false, ..offer(Some(9_000), "RustySound (iPhone)") };
        assert!(!should_offer_queue(&empty, me, &QueueMarks::default()));
    }

    #[test]
    fn our_own_server_queue_beats_an_older_bookmark_on_launch() {
        let me = "RustySound (iPhone)";
        assert!(prefer_own_server_queue(&offer(Some(5_000), me), me, Some(4_000)));
        assert!(prefer_own_server_queue(&offer(Some(5_000), me), me, None));
        assert!(!prefer_own_server_queue(&offer(Some(3_000), me), me, Some(4_000)), "bookmark is newer");
        assert!(!prefer_own_server_queue(&offer(Some(5_000), "RustySound (Mac)"), me, Some(4_000)), "not ours: banner");
    }

    fn np(user: &str, player: Option<&str>, state: Option<&str>, minutes: u32) -> NowPlayingEntry {
        NowPlayingEntry {
            song_id: "x".into(),
            title: "Song".into(),
            username: user.into(),
            player_name: player.map(Into::into),
            state: state.map(Into::into),
            position_ms: None,
            minutes_ago: minutes,
        }
    }

    #[test]
    fn other_players_skip_this_one_other_users_and_paused_ones() {
        let me = "RustySound (iPhone)";
        let entries = vec![
            np("demo", Some(me), Some("playing"), 0),
            np("other", Some("RustySound (Mac)"), Some("playing"), 0),
            np("demo", Some("RustySound (Mac)"), Some("paused"), 0),
            np("demo", None, Some("playing"), 0),
            np("demo", Some("RustySound (Web)"), Some("playing"), 0),
        ];
        assert_eq!(pick_other_player(&entries, "demo", me), Some(OtherPlayer { device: "Web".into(), title: "Song".into() }));
        assert_eq!(pick_other_player(&entries[..4], "demo", me), None);
        assert!(pick_other_player(&[np("demo", Some("NavidromeUI"), None, 2)], "DEMO", me).is_some());
        assert!(pick_other_player(&[np("demo", Some("NavidromeUI"), None, 30)], "demo", me).is_none());
    }
}
