//! Skip back: the one rule every "previous" control follows.
//!
//! - One press restarts the current song (seek to 0); playing or paused stays
//!   as it was. The press acts at once, never waiting for a second press.
//! - A press within [`DOUBLE_PRESS_WINDOW_MS`] of the last press goes back one
//!   song. Each further quick press goes back one more.
//! - If the song has only just started (under [`NEAR_START_SECS`]), the
//!   first press goes straight to the previous song, as most players do. A
//!   quick second press after that does nothing, so a double press always
//!   lands exactly one song back. A song you just restarted with skip back
//!   doesn't count as "just started", so slow presses keep restarting.
//! - With no previous song (start of the queue, repeat off) going back
//!   restarts instead. Repeat-all wraps to the last song. Repeat-one and
//!   shuffle change nothing: the queue is already in play order.
//! - Radio streams: nothing to restart, so skip back does nothing.
//!
//! Callers: `prev-btn` (player bar, mini player, macOS Playback menu, web
//! keyboard shortcuts, web MediaSession `previoustrack`), Now Playing's
//! `np-prev-btn` (`PlayerCtx::previous`), the native controller's remote
//! "previous" action (desktop media keys, MediaSession and SMTC) and the iOS
//! remote commands (`native_ios_remote.rs`). They share one machine, so a
//! press on the lock screen and one in the app count together.

use crate::api::Song;
use crate::components::ui::env::is_radio_song;
use crate::components::{ios_diag_log, seek_to};
use crate::db::RepeatMode;
use dioxus::prelude::*;
use std::sync::Mutex;

/// Presses this close together are a double press. Long enough for a
/// deliberate double tap on a phone (taps land ~150–400 ms apart); the first
/// press never waits for it, so it adds no lag.
pub(crate) const DOUBLE_PRESS_WINDOW_MS: f64 = 700.0;
/// A song under this position (seconds) has only just started.
pub(crate) const NEAR_START_SECS: f64 = 2.0;
/// Presses closer than this are one press delivered twice (menu shortcut and
/// key event, duplicate remote commands), not a double press.
pub(crate) const DUPLICATE_PRESS_MS: f64 = 80.0;

/// What a press does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkipBack {
    /// Seek the current song to 0.
    Restart,
    /// Play this queue index.
    Previous(usize),
    /// Do nothing.
    Ignore,
}

/// One press, as the caller sees the player at that moment.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Press<'a> {
    /// Monotonic milliseconds ([`monotonic_now_ms`]).
    pub now_ms: f64,
    /// Playback position of the current song, in seconds.
    pub position_secs: f64,
    /// Identifies the current queue entry ([`track_key`]).
    pub track: &'a str,
    /// The queue index skip back would go to ([`previous_index`]).
    pub previous: Option<usize>,
    /// Live radio: nothing to restart or go back to.
    pub is_live: bool,
}

#[derive(Debug, Clone)]
struct LastPress {
    at_ms: f64,
    /// Presses in the current quick burst, this one included.
    burst: u32,
    /// The burst's first press went back a song (song had just started).
    first_went_back: bool,
    /// The track this press restarted, if it restarted.
    restarted: Option<String>,
}

/// The pure state machine. Callers share one through [`decide`].
#[derive(Debug, Default)]
pub(crate) struct SkipBackMachine {
    last: Option<LastPress>,
}

impl SkipBackMachine {
    pub(crate) const fn new() -> Self {
        Self { last: None }
    }

    pub(crate) fn press(&mut self, press: Press<'_>) -> SkipBack {
        if press.is_live {
            self.last = None;
            return SkipBack::Ignore;
        }
        let back = || match press.previous {
            Some(index) => SkipBack::Previous(index),
            None => SkipBack::Restart,
        };
        let restarted =
            |outcome: SkipBack| (outcome == SkipBack::Restart).then(|| press.track.to_string());

        if let Some(last) = self.last.as_mut() {
            let dt = press.now_ms - last.at_ms;
            if (0.0..DUPLICATE_PRESS_MS).contains(&dt) {
                return SkipBack::Ignore;
            }
            if (0.0..=DOUBLE_PRESS_WINDOW_MS).contains(&dt) {
                last.at_ms = press.now_ms;
                last.burst += 1;
                if last.first_went_back && last.burst == 2 {
                    // The first press already went back a song.
                    last.restarted = None;
                    return SkipBack::Ignore;
                }
                let outcome = back();
                last.restarted = restarted(outcome);
                return outcome;
            }
        }

        // First press of a burst.
        let just_restarted = self
            .last
            .as_ref()
            .and_then(|last| last.restarted.as_deref())
            == Some(press.track);
        let near_start = press.position_secs < NEAR_START_SECS && !just_restarted;
        let outcome = if near_start {
            back()
        } else {
            SkipBack::Restart
        };
        self.last = Some(LastPress {
            at_ms: press.now_ms,
            burst: 1,
            first_went_back: matches!(outcome, SkipBack::Previous(_)),
            restarted: restarted(outcome),
        });
        outcome
    }
}

static MACHINE: Mutex<SkipBackMachine> = Mutex::new(SkipBackMachine::new());

/// Runs `press` through the app-wide machine.
pub(crate) fn decide(press: Press<'_>) -> SkipBack {
    match MACHINE.lock() {
        Ok(mut machine) => machine.press(press),
        Err(poisoned) => poisoned.into_inner().press(press),
    }
}

/// The queue index "previous" goes to: one back, wrapping to the end under
/// repeat-all. `None` at the start of the queue (restart instead).
pub(crate) fn previous_index(index: usize, len: usize, repeat: RepeatMode) -> Option<usize> {
    if len == 0 {
        None
    } else if index > 0 {
        Some(index.min(len) - 1)
    } else if repeat == RepeatMode::All && len > 1 {
        Some(len - 1)
    } else {
        None
    }
}

/// Key for one queue entry (the same song twice in a queue is two entries).
pub(crate) fn track_key(index: usize, song_id: &str) -> String {
    format!("{index}:{song_id}")
}

/// Monotonic milliseconds: `performance.now()` on the web, `Instant` natively.
#[cfg(target_arch = "wasm32")]
pub(crate) fn monotonic_now_ms() -> f64 {
    use wasm_bindgen::JsCast;
    let global = js_sys::global();
    js_sys::Reflect::get(&global, &"performance".into())
        .ok()
        .and_then(|performance| {
            let now = js_sys::Reflect::get(&performance, &"now".into()).ok()?;
            let now: js_sys::Function = now.dyn_into().ok()?;
            now.call0(&performance).ok()?.as_f64()
        })
        .unwrap_or_else(js_sys::Date::now)
}

/// Monotonic milliseconds: `performance.now()` on the web, `Instant` natively.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn monotonic_now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// The playback signals skip back reads and writes.
#[derive(Clone, Copy)]
pub(crate) struct SkipBackSignals {
    pub queue: Signal<Vec<Song>>,
    pub queue_index: Signal<usize>,
    pub now_playing: Signal<Option<Song>>,
    pub is_playing: Signal<bool>,
    pub repeat: Signal<RepeatMode>,
    /// `PlaybackPositionSignal`.
    pub position: Signal<f64>,
    /// `AudioState::current_time`.
    pub current_time: Signal<f64>,
}

impl SkipBackSignals {
    /// From context; call inside a component or hook.
    pub(crate) fn from_context() -> Self {
        let audio = consume_context::<Signal<crate::components::AudioState>>();
        let current_time = audio.peek().current_time;
        Self {
            queue: consume_context(),
            queue_index: consume_context(),
            now_playing: consume_context(),
            is_playing: consume_context::<crate::components::IsPlayingSignal>().0,
            repeat: consume_context(),
            position: consume_context::<crate::components::PlaybackPositionSignal>().0,
            current_time,
        }
    }
}

/// One skip-back press: decides through the shared machine and applies it.
/// `resume` starts playback after the change (remote commands); otherwise
/// playing or paused stays as it was. Reads with `peek`, so it never
/// subscribes the caller. Returns what it did.
pub(crate) fn press_skip_back(s: SkipBackSignals, source: &str, resume: bool) -> SkipBack {
    let Some(song) = s.now_playing.peek().clone() else {
        return SkipBack::Ignore;
    };
    let index = *s.queue_index.peek();
    let len = s.queue.peek().len();
    let key = track_key(index, &song.id);
    // The web reads the audio element itself (the signal lags up to a poll);
    // native backends only report through the polled signal.
    #[cfg(target_arch = "wasm32")]
    let position = crate::components::get_current_time();
    #[cfg(not(target_arch = "wasm32"))]
    let position = *s.position.peek();
    let outcome = decide(Press {
        now_ms: monotonic_now_ms(),
        position_secs: position,
        track: &key,
        previous: previous_index(index, len, *s.repeat.peek()),
        is_live: is_radio_song(&song),
    });
    ios_diag_log(
        "ui.control",
        &format!("source={source}.skip-back idx={index} queue_len={len} pos={position:.2} outcome={outcome:?}"),
    );
    apply(s, outcome, resume);
    outcome
}

/// Goes straight to the previous song (restarting at the start of the
/// queue), skipping the press rule. For a press another layer already
/// decided: an iOS remote command that could not switch natively.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn go_to_previous_track(s: SkipBackSignals, resume: bool) {
    let index = *s.queue_index.peek();
    let len = s.queue.peek().len();
    let outcome = match previous_index(index, len, *s.repeat.peek()) {
        Some(target) => SkipBack::Previous(target),
        None => SkipBack::Restart,
    };
    apply(s, outcome, resume);
}

fn apply(mut s: SkipBackSignals, outcome: SkipBack, resume: bool) {
    match outcome {
        SkipBack::Ignore => {}
        SkipBack::Restart => {
            s.position.set(0.0);
            s.current_time.set(0.0);
            seek_to(0.0);
            if resume {
                s.is_playing.set(true);
            }
        }
        SkipBack::Previous(target) => {
            let song = s.queue.peek().get(target).cloned();
            if let Some(song) = song {
                let keep_playing = resume || *s.is_playing.peek();
                s.queue_index.set(target);
                s.now_playing.set(Some(song));
                if keep_playing {
                    s.is_playing.set(true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MID: f64 = 95.0;

    fn press(
        m: &mut SkipBackMachine,
        now_ms: f64,
        position_secs: f64,
        track: &str,
        previous: Option<usize>,
    ) -> SkipBack {
        m.press(Press {
            now_ms,
            position_secs,
            track,
            previous,
            is_live: false,
        })
    }

    #[test]
    fn one_press_mid_song_restarts() {
        let mut m = SkipBackMachine::new();
        assert_eq!(
            press(&mut m, 1_000.0, MID, "3:b", Some(2)),
            SkipBack::Restart
        );
    }

    #[test]
    fn quick_second_press_goes_back() {
        let mut m = SkipBackMachine::new();
        assert_eq!(
            press(&mut m, 1_000.0, MID, "3:b", Some(2)),
            SkipBack::Restart
        );
        // Position is ~0 now; a quick press goes back a song.
        assert_eq!(
            press(&mut m, 1_250.0, 0.2, "3:b", Some(2)),
            SkipBack::Previous(2)
        );
    }

    #[test]
    fn window_edges() {
        let mut m = SkipBackMachine::new();
        press(&mut m, 0.0, MID, "3:b", Some(2));
        assert_eq!(
            press(&mut m, DOUBLE_PRESS_WINDOW_MS, 0.7, "3:b", Some(2)),
            SkipBack::Previous(2)
        );

        let mut m = SkipBackMachine::new();
        press(&mut m, 0.0, MID, "3:b", Some(2));
        assert_eq!(
            press(&mut m, DOUBLE_PRESS_WINDOW_MS + 1.0, 0.7, "3:b", Some(2)),
            SkipBack::Restart
        );
    }

    #[test]
    fn slow_presses_keep_restarting() {
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, MID, "3:b", Some(2)), SkipBack::Restart);
        // 1.2 s later the song is 1.2 s in: "just started", but we restarted
        // it, so it restarts again rather than going back.
        assert_eq!(
            press(&mut m, 1_200.0, 1.2, "3:b", Some(2)),
            SkipBack::Restart
        );
        assert_eq!(
            press(&mut m, 2_500.0, 1.3, "3:b", Some(2)),
            SkipBack::Restart
        );
    }

    #[test]
    fn duplicate_delivery_counts_once() {
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, MID, "3:b", Some(2)), SkipBack::Restart);
        assert_eq!(press(&mut m, 30.0, 0.0, "3:b", Some(2)), SkipBack::Ignore);
        // A real second press still counts.
        assert_eq!(
            press(&mut m, 300.0, 0.2, "3:b", Some(2)),
            SkipBack::Previous(2)
        );
    }

    #[test]
    fn near_start_single_press_goes_back() {
        let mut m = SkipBackMachine::new();
        assert_eq!(
            press(&mut m, 0.0, 1.0, "3:b", Some(2)),
            SkipBack::Previous(2)
        );
    }

    #[test]
    fn near_start_double_press_lands_one_song_back() {
        let mut m = SkipBackMachine::new();
        assert_eq!(
            press(&mut m, 0.0, 1.0, "3:b", Some(2)),
            SkipBack::Previous(2)
        );
        // Now on index 2 at ~0 s; the quick second press is absorbed.
        assert_eq!(press(&mut m, 300.0, 0.0, "2:a", Some(1)), SkipBack::Ignore);
        // A third quick press goes back one more.
        assert_eq!(
            press(&mut m, 600.0, 0.0, "2:a", Some(1)),
            SkipBack::Previous(1)
        );
    }

    #[test]
    fn rapid_presses_step_back_one_each() {
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, MID, "3:d", Some(2)), SkipBack::Restart);
        assert_eq!(
            press(&mut m, 300.0, 0.1, "3:d", Some(2)),
            SkipBack::Previous(2)
        );
        assert_eq!(
            press(&mut m, 600.0, 0.0, "2:c", Some(1)),
            SkipBack::Previous(1)
        );
        assert_eq!(
            press(&mut m, 900.0, 0.0, "1:b", Some(0)),
            SkipBack::Previous(0)
        );
    }

    #[test]
    fn after_a_song_change_near_start_goes_back_again() {
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, MID, "3:b", Some(2)), SkipBack::Restart);
        // The restarted song plays through; the next one has just begun.
        assert_eq!(
            press(&mut m, 200_000.0, 0.8, "4:c", Some(3)),
            SkipBack::Previous(3)
        );
    }

    #[test]
    fn queue_start_restarts_on_every_press() {
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, MID, "0:a", None), SkipBack::Restart);
        assert_eq!(press(&mut m, 250.0, 0.1, "0:a", None), SkipBack::Restart);
        assert_eq!(press(&mut m, 500.0, 0.2, "0:a", None), SkipBack::Restart);
        // Near the start with nothing before it: restart too.
        let mut m = SkipBackMachine::new();
        assert_eq!(press(&mut m, 0.0, 0.5, "0:a", None), SkipBack::Restart);
    }

    #[test]
    fn radio_does_nothing() {
        let mut m = SkipBackMachine::new();
        let live = |now_ms| Press {
            now_ms,
            position_secs: MID,
            track: "0:radio",
            previous: None,
            is_live: true,
        };
        assert_eq!(m.press(live(0.0)), SkipBack::Ignore);
        assert_eq!(m.press(live(200.0)), SkipBack::Ignore);
        // A radio press doesn't start a burst for the next song.
        assert_eq!(press(&mut m, 400.0, MID, "1:b", Some(0)), SkipBack::Restart);
    }

    #[test]
    fn previous_index_rules() {
        assert_eq!(previous_index(3, 5, RepeatMode::Off), Some(2));
        assert_eq!(previous_index(0, 5, RepeatMode::Off), None);
        assert_eq!(previous_index(0, 0, RepeatMode::Off), None);
        // Repeat-all wraps; a one-song queue has nothing to wrap to.
        assert_eq!(previous_index(0, 5, RepeatMode::All), Some(4));
        assert_eq!(previous_index(0, 1, RepeatMode::All), None);
        // Repeat-one doesn't change where "previous" is.
        assert_eq!(previous_index(3, 5, RepeatMode::One), Some(2));
        assert_eq!(previous_index(0, 5, RepeatMode::One), None);
        // A stale index past the end still lands inside the queue.
        assert_eq!(previous_index(9, 5, RepeatMode::Off), Some(4));
    }

    #[test]
    fn repeat_one_double_press_still_goes_back() {
        let mut m = SkipBackMachine::new();
        let prev = previous_index(3, 5, RepeatMode::One);
        assert_eq!(press(&mut m, 0.0, MID, "3:b", prev), SkipBack::Restart);
        assert_eq!(
            press(&mut m, 250.0, 0.1, "3:b", prev),
            SkipBack::Previous(2)
        );
    }

    #[test]
    fn repeat_all_wraps_from_the_first_song() {
        let mut m = SkipBackMachine::new();
        let prev = previous_index(0, 5, RepeatMode::All);
        assert_eq!(press(&mut m, 0.0, MID, "0:a", prev), SkipBack::Restart);
        assert_eq!(
            press(&mut m, 250.0, 0.1, "0:a", prev),
            SkipBack::Previous(4)
        );
    }

    #[test]
    fn clock_going_backwards_starts_a_new_burst() {
        let mut m = SkipBackMachine::new();
        assert_eq!(
            press(&mut m, 5_000.0, MID, "3:b", Some(2)),
            SkipBack::Restart
        );
        assert_eq!(
            press(&mut m, 1_000.0, MID, "3:b", Some(2)),
            SkipBack::Restart
        );
    }
}
