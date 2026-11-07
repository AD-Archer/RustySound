//! Sleep timer: stop playback after 15–60 minutes or at the end of the song.
//!
//! The state lives in the shell (`use_sleep_timer_provider`), so it keeps
//! running while Now Playing is closed. A timed timer is one task in the
//! shell's scope that wakes only when the label would change (each minute,
//! then each second in the last one); at zero it fades the output down over
//! `FADE_MS`, pauses, and puts the volume setting back so the next play isn't
//! silent. "End of Track" is answered by the audio controllers when a song
//! ends (`take_end_of_track`), and on iOS by AVPlayer itself
//! (`set_stop_after_current`).

use super::{ramp_output_volume, set_output_volume, set_stop_after_current};
use crate::components::ui::env::sleep_ms;
use crate::components::{IsPlayingSignal, VolumeSignal};
use dioxus::core::{current_scope_id, Runtime, Task};
use dioxus::prelude::*;

/// How long the volume takes to fade out when time runs out.
pub const FADE_MS: u64 = 8000;

/// One entry of the sleep timer menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SleepChoice {
    Minutes(u32),
    EndOfTrack,
    Off,
}

/// The menu's choices, in order.
pub const SLEEP_CHOICES: [SleepChoice; 6] = [
    SleepChoice::Minutes(15),
    SleepChoice::Minutes(30),
    SleepChoice::Minutes(45),
    SleepChoice::Minutes(60),
    SleepChoice::EndOfTrack,
    SleepChoice::Off,
];

impl SleepChoice {
    pub fn label(&self) -> String {
        match self {
            SleepChoice::Minutes(60) => "1 hour".to_string(),
            SleepChoice::Minutes(m) => format!("{m} min"),
            SleepChoice::EndOfTrack => "End of Track".to_string(),
            SleepChoice::Off => "Off".to_string(),
        }
    }

    /// `sleep-choice-15`, `sleep-choice-end`, `sleep-choice-off`.
    pub fn testid(&self) -> String {
        match self {
            SleepChoice::Minutes(m) => format!("sleep-choice-{m}"),
            SleepChoice::EndOfTrack => "sleep-choice-end".to_string(),
            SleepChoice::Off => "sleep-choice-off".to_string(),
        }
    }
}

/// What the timer is doing.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SleepMode {
    #[default]
    Off,
    /// Stops at this wall-clock time (ms since the epoch).
    Until(f64),
    EndOfTrack,
}

impl SleepMode {
    /// Whether `choice` is the one in effect (for the menu's check mark).
    pub fn matches(&self, choice: SleepChoice, minutes: Option<u32>) -> bool {
        match (self, choice) {
            (SleepMode::Off, SleepChoice::Off) => true,
            (SleepMode::EndOfTrack, SleepChoice::EndOfTrack) => true,
            (SleepMode::Until(_), SleepChoice::Minutes(m)) => minutes == Some(m),
            _ => false,
        }
    }
}

// TODO(chunk A): moves to `src/time.rs`.
/// Wall clock in ms. Wall time (not monotonic) so a timer stays right after
/// the device sleeps; on the web Playwright's clock controls it.
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

/// Whole seconds left before `deadline_ms`, rounded up (0 once it passes).
pub fn remaining_secs(deadline_ms: f64, now_ms: f64) -> u64 {
    let left = deadline_ms - now_ms;
    if left.is_finite() && left > 0.0 {
        (left / 1000.0).ceil() as u64
    } else {
        0
    }
}

/// The indicator's text: whole minutes ("12 min", rounded up), then a
/// seconds countdown ("0:42") in the last minute.
pub fn remaining_label(secs: u64) -> String {
    if secs >= 60 {
        format!("{} min", secs.div_ceil(60))
    } else {
        format!("0:{secs:02}")
    }
}

/// Milliseconds until the label next changes, for `left_ms` remaining.
pub fn next_tick_ms(left_ms: f64) -> u64 {
    let left = left_ms.max(0.0).ceil() as u64;
    if left == 0 {
        return 0;
    }
    let unit = if left > 60_000 { 60_000 } else { 1000 };
    match left % unit {
        0 => unit,
        rest => rest,
    }
}

/// The shell's sleep timer (context).
#[derive(Clone, Copy)]
pub struct SleepTimer {
    pub mode: Signal<SleepMode>,
    /// The length picked for a timed timer, for the menu's check mark.
    pub minutes: Signal<Option<u32>>,
    /// Seconds left on a timed timer, refreshed when the label changes.
    pub remaining: Signal<u64>,
    task: Signal<Option<Task>>,
    fading: Signal<bool>,
    volume: Signal<f64>,
    is_playing: Signal<bool>,
    scope: ScopeId,
}

/// Creates the sleep timer and provides it as context. Call once, in the shell.
pub fn use_sleep_timer_provider() -> SleepTimer {
    let volume = use_context::<VolumeSignal>().0;
    let is_playing = use_context::<IsPlayingSignal>().0;
    use_context_provider(|| SleepTimer {
        mode: Signal::new(SleepMode::Off),
        minutes: Signal::new(None),
        remaining: Signal::new(0),
        task: Signal::new(None),
        fading: Signal::new(false),
        volume,
        is_playing,
        scope: current_scope_id(),
    })
}

impl SleepTimer {
    /// Starts, changes or cancels the timer.
    pub fn set(&self, choice: SleepChoice) {
        let mut this = *self;
        if let Some(task) = this.task.write().take() {
            task.cancel();
        }
        if *this.fading.peek() {
            // Cancelled mid-fade: back to the volume setting.
            this.fading.set(false);
            set_output_volume(*this.volume.peek());
        }
        set_stop_after_current(choice == SleepChoice::EndOfTrack);
        match choice {
            SleepChoice::Off => {
                this.mode.set(SleepMode::Off);
                this.minutes.set(None);
            }
            SleepChoice::EndOfTrack => {
                this.mode.set(SleepMode::EndOfTrack);
                this.minutes.set(None);
            }
            SleepChoice::Minutes(m) => {
                let deadline = now_ms() + f64::from(m) * 60_000.0;
                this.mode.set(SleepMode::Until(deadline));
                this.minutes.set(Some(m));
                this.remaining.set(u64::from(m) * 60);
                let task = Runtime::current().spawn(this.scope, this.run(deadline));
                this.task.set(Some(task));
            }
        }
    }

    /// The one timer task: wait (waking when the label changes), then fade
    /// out, pause and restore the volume setting.
    async fn run(self, deadline: f64) {
        let mut this = self;
        loop {
            let left = deadline - now_ms();
            if left <= 0.0 {
                break;
            }
            let secs = remaining_secs(deadline, now_ms());
            if *this.remaining.peek() != secs {
                this.remaining.set(secs);
            }
            sleep_ms(next_tick_ms(left)).await;
        }
        this.remaining.set(0);
        if *this.is_playing.peek() {
            this.fading.set(true);
            ramp_output_volume(*this.volume.peek(), 0.0, FADE_MS).await;
            this.is_playing.set(false);
            // Let the pause land before the volume comes back.
            sleep_ms(400).await;
        }
        set_output_volume(*this.volume.peek());
        this.fading.set(false);
        this.mode.set(SleepMode::Off);
        this.minutes.set(None);
        this.task.set(None);
    }

    /// Called by the audio controllers when a song ends by itself. True when
    /// "End of Track" was set: the caller pauses instead of moving on, and the
    /// timer turns off.
    pub fn take_end_of_track(&self) -> bool {
        let mut mode = self.mode;
        if *mode.peek() != SleepMode::EndOfTrack {
            return false;
        }
        mode.set(SleepMode::Off);
        set_stop_after_current(false);
        true
    }

    /// The indicator's text while a timer is set ("12 min", "End of Track").
    /// Subscribes the caller to the mode and the countdown.
    pub fn label(&self) -> Option<String> {
        match (self.mode)() {
            SleepMode::Off => None,
            SleepMode::EndOfTrack => Some("End of Track".to_string()),
            SleepMode::Until(_) => Some(remaining_label((self.remaining)())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_rounds_up_and_stops_at_zero() {
        assert_eq!(remaining_secs(10_000.0, 0.0), 10);
        assert_eq!(remaining_secs(10_000.0, 100.0), 10);
        assert_eq!(remaining_secs(10_000.0, 9_001.0), 1);
        assert_eq!(remaining_secs(10_000.0, 10_000.0), 0);
        assert_eq!(remaining_secs(10_000.0, 20_000.0), 0);
        assert_eq!(remaining_secs(f64::NAN, 0.0), 0);
    }

    #[test]
    fn labels_count_minutes_then_seconds() {
        assert_eq!(remaining_label(15 * 60), "15 min");
        assert_eq!(remaining_label(14 * 60 + 1), "15 min");
        assert_eq!(remaining_label(61), "2 min");
        assert_eq!(remaining_label(60), "1 min");
        assert_eq!(remaining_label(59), "0:59");
        assert_eq!(remaining_label(5), "0:05");
        assert_eq!(remaining_label(0), "0:00");
    }

    #[test]
    fn ticks_land_where_the_label_changes() {
        // 15 minutes: the next change is at 14 minutes left.
        assert_eq!(next_tick_ms(900_000.0), 60_000);
        assert_eq!(next_tick_ms(899_500.0), 59_500);
        // Down to exactly a minute, then seconds.
        assert_eq!(next_tick_ms(61_000.0), 1_000);
        assert_eq!(next_tick_ms(60_000.0), 1_000);
        assert_eq!(next_tick_ms(59_250.0), 250);
        assert_eq!(next_tick_ms(0.0), 0);
        // Following the ticks from 15 minutes ends exactly at zero.
        let mut left = 900_000.0;
        let mut wakes = 0;
        while left > 0.0 {
            left -= next_tick_ms(left) as f64;
            wakes += 1;
        }
        assert_eq!(left, 0.0);
        assert_eq!(wakes, 14 + 60);
    }

    #[test]
    fn choices_and_mode_match() {
        assert_eq!(SLEEP_CHOICES.len(), 6);
        assert_eq!(SleepChoice::Minutes(60).label(), "1 hour");
        assert_eq!(SleepChoice::Minutes(15).label(), "15 min");
        assert_eq!(SleepChoice::EndOfTrack.testid(), "sleep-choice-end");
        let timed = SleepMode::Until(1.0);
        assert!(timed.matches(SleepChoice::Minutes(30), Some(30)));
        assert!(!timed.matches(SleepChoice::Minutes(15), Some(30)));
        assert!(SleepMode::Off.matches(SleepChoice::Off, None));
        assert!(SleepMode::EndOfTrack.matches(SleepChoice::EndOfTrack, None));
        assert!(!SleepMode::Off.matches(SleepChoice::EndOfTrack, None));
    }
}
