//! Playback speed: the choices, their labels, and the hook that keeps the
//! output's rate in step with the setting. Both audio controllers call
//! `use_playback_rate_sync`; the backends keep the pitch (`preservesPitch`
//! on the `<audio>` element, a pitch algorithm on AVPlayer). Radio always
//! plays at 1x.

use super::set_playback_rate;
use crate::api::Song;
use crate::db::AppSettings;
use dioxus::prelude::*;

/// The speeds the Now Playing menu offers.
pub const PLAYBACK_SPEEDS: [f64; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];

/// The listed speed nearest to `raw` (1x for anything unusable), so a hand
/// edited or stale setting can't leave the player at an odd rate.
pub fn normalize_speed(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    PLAYBACK_SPEEDS
        .iter()
        .copied()
        .min_by(|a, b| (a - raw).abs().total_cmp(&(b - raw).abs()))
        .unwrap_or(1.0)
}

/// "1×", "1.25×", "0.75×".
pub fn speed_label(rate: f64) -> String {
    let text = format!("{rate:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}

/// The rate to play `song` at: radio streams ignore the setting.
pub fn effective_rate(setting: f64, song: Option<&Song>) -> f64 {
    let live = song.map(|s| s.server_name == "Radio").unwrap_or(false);
    if live {
        1.0
    } else {
        normalize_speed(setting)
    }
}

/// Saves a new speed; the controllers' sync hook applies it.
pub fn set_playback_speed(mut settings: Signal<AppSettings>, rate: f64) {
    let rate = normalize_speed(rate);
    if settings.peek().playback_rate == rate {
        return;
    }
    settings.with_mut(|s| s.playback_rate = rate);
    let snapshot = settings.peek().clone();
    spawn(async move {
        let _ = crate::db::save_settings(snapshot).await;
    });
}

/// Applies the speed when the setting changes or playback moves between
/// radio and library songs. Sends nothing when the rate is unchanged.
pub fn use_playback_rate_sync(settings: Signal<AppSettings>, now_playing: Signal<Option<Song>>) {
    let mut applied = use_signal(|| None::<f64>);
    use_effect(move || {
        let setting = settings.read().playback_rate;
        let rate = effective_rate(setting, now_playing.read().as_ref());
        if *applied.peek() != Some(rate) {
            applied.set(Some(rate));
            set_playback_rate(rate);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn radio() -> Song {
        Song {
            server_name: "Radio".to_string(),
            ..Song::default()
        }
    }

    #[test]
    fn speeds_are_ordered_and_include_normal() {
        assert!(PLAYBACK_SPEEDS.windows(2).all(|w| w[0] < w[1]));
        assert!(PLAYBACK_SPEEDS.contains(&1.0));
        assert_eq!(PLAYBACK_SPEEDS.first(), Some(&0.75));
        assert_eq!(PLAYBACK_SPEEDS.last(), Some(&2.0));
    }

    #[test]
    fn odd_settings_snap_to_a_listed_speed() {
        assert_eq!(normalize_speed(1.0), 1.0);
        assert_eq!(normalize_speed(1.3), 1.25);
        assert_eq!(normalize_speed(9.0), 2.0);
        assert_eq!(normalize_speed(0.1), 0.75);
        assert_eq!(normalize_speed(f64::NAN), 1.0);
        assert_eq!(normalize_speed(0.0), 1.0);
    }

    #[test]
    fn labels_drop_trailing_zeros() {
        let labels: Vec<_> = PLAYBACK_SPEEDS.iter().map(|r| speed_label(*r)).collect();
        assert_eq!(labels, ["0.75×", "1×", "1.25×", "1.5×", "2×"]);
    }

    #[test]
    fn radio_plays_at_normal_speed() {
        assert_eq!(effective_rate(1.5, Some(&radio())), 1.0);
        assert_eq!(effective_rate(1.5, Some(&Song::default())), 1.5);
        assert_eq!(effective_rate(1.5, None), 1.5);
    }
}
