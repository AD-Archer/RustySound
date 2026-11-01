//! Volume levelling (ReplayGain). Each song's OpenSubsonic `replayGain` turns
//! into a multiplier on the output volume, kept below the point where the
//! song's peak would clip. `VolumeSignal` stays the user's volume: the
//! multiplier is applied on the way out (`set_output_volume`), so the sliders
//! never show the levelled value.
//!
//! "Auto" uses album gain while the song before or after it in the queue is
//! from the same album (so an album keeps its own loud and quiet songs), and
//! track gain otherwise. Radio is never levelled.

use super::{apply_output_volume, set_levelling_gain};
use crate::api::{ReplayGain, Song};
use crate::db::{AppSettings, VolumeLevelling};
use dioxus::prelude::*;

/// A gain change in the middle of a song (settings, queue edits) ramps over
/// this long, so it doesn't click.
const LEVEL_RAMP_MS: u64 = 80;
/// Bounds for the multiplier: -24 dB to +12 dB.
const MIN_MULTIPLIER: f64 = 0.063;
const MAX_MULTIPLIER: f64 = 3.98;

/// Which of a song's gains to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GainSource {
    Track,
    Album,
}

/// Same album, same server. Radio has no album.
pub fn same_album(a: &Song, b: &Song) -> bool {
    if a.server_name == "Radio" || b.server_name == "Radio" {
        return false;
    }
    match (a.album_id.as_deref(), b.album_id.as_deref()) {
        (Some(x), Some(y)) => !x.trim().is_empty() && x == y && a.server_id == b.server_id,
        _ => false,
    }
}

/// The gain to use for `current`, given its neighbours in the queue.
pub fn gain_source(
    mode: VolumeLevelling,
    prev: Option<&Song>,
    current: &Song,
    next: Option<&Song>,
) -> Option<GainSource> {
    match mode {
        VolumeLevelling::Off => None,
        VolumeLevelling::Track => Some(GainSource::Track),
        VolumeLevelling::Album => Some(GainSource::Album),
        VolumeLevelling::Auto => {
            let album_run = prev.is_some_and(|p| same_album(p, current))
                || next.is_some_and(|n| same_album(current, n));
            Some(if album_run {
                GainSource::Album
            } else {
                GainSource::Track
            })
        }
    }
}

/// `10^((gain + preamp) / 20)`, clamped so `multiplier * peak` stays at or
/// below full scale. Falls back to the other gain (with its own peak), then to
/// the server's fallback gain; 1.0 when there's nothing to go on.
pub fn gain_multiplier(rg: &ReplayGain, source: GainSource, preamp_db: f64) -> f64 {
    let track = rg.track_gain.map(|gain| (gain, rg.track_peak));
    let album = rg.album_gain.map(|gain| (gain, rg.album_peak));
    let picked = match source {
        GainSource::Track => track.or(album),
        GainSource::Album => album.or(track),
    }
    .or_else(|| rg.fallback_gain.map(|gain| (gain, None)));
    let Some((gain_db, peak)) = picked.filter(|(gain, _)| gain.is_finite()) else {
        return 1.0;
    };
    let preamp = if preamp_db.is_finite() {
        preamp_db
    } else {
        0.0
    };
    let mut multiplier = 10f64.powf((gain_db + preamp) / 20.0);
    if let Some(peak) = peak.filter(|p| p.is_finite() && *p > 0.0) {
        multiplier = multiplier.min(1.0 / peak);
    }
    multiplier.clamp(MIN_MULTIPLIER, MAX_MULTIPLIER)
}

/// The output multiplier for `current` (1.0 when levelling is off, for radio,
/// and for songs without ReplayGain data).
pub fn levelling_gain(
    settings: &AppSettings,
    prev: Option<&Song>,
    current: &Song,
    next: Option<&Song>,
) -> f64 {
    if current.server_name == "Radio" {
        return 1.0;
    }
    let Some(rg) = current.replay_gain.as_ref() else {
        return 1.0;
    };
    match gain_source(settings.volume_levelling, prev, current, next) {
        Some(source) => gain_multiplier(rg, source, settings.replay_gain_preamp_db),
        None => 1.0,
    }
}

/// The songs before and after `current` in the queue. `idx` is a hint; it is
/// checked against `current` (the index signal can trail `now_playing`).
pub fn queue_neighbours<'a>(
    queue: &'a [Song],
    idx: usize,
    current: &Song,
) -> (Option<&'a Song>, Option<&'a Song>) {
    let at = if queue.get(idx).is_some_and(|song| song.id == current.id) {
        Some(idx)
    } else {
        super::find_song_instance_index(queue, current)
    };
    let Some(at) = at else {
        return (None, None);
    };
    let prev = at.checked_sub(1).and_then(|i| queue.get(i));
    (prev, queue.get(at + 1))
}

/// Keeps the output multiplier in step with the song, its neighbours and the
/// settings. Both audio controllers call this once.
pub fn use_volume_levelling(
    settings: Signal<AppSettings>,
    now_playing: Signal<Option<Song>>,
    queue: Signal<Vec<Song>>,
    queue_index: Signal<usize>,
    volume: Signal<f64>,
) {
    let mut applied = use_signal(|| None::<(String, f64)>);
    use_effect(move || {
        let settings = settings.read();
        let current = now_playing.read();
        let Some(song) = current.as_ref() else {
            return;
        };
        let queue = queue.read();
        let (prev, next) = queue_neighbours(&queue, queue_index(), song);
        let gain = levelling_gain(&settings, prev, song, next);
        let previous = applied.peek().clone();
        if previous
            .as_ref()
            .is_some_and(|(id, g)| *id == song.id && *g == gain)
        {
            return;
        }
        let same_song = previous.is_some_and(|(id, _)| id == song.id);
        applied.set(Some((song.id.clone(), gain)));
        set_levelling_gain(gain);
        // A new song starts at its own level; a change mid-song ramps.
        apply_output_volume(*volume.peek(), if same_song { LEVEL_RAMP_MS } else { 0 });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, album: &str) -> Song {
        Song {
            id: id.to_string(),
            album_id: Some(album.to_string()),
            server_id: "s".to_string(),
            ..Default::default()
        }
    }

    fn rg(track: f64, track_peak: f64, album: f64, album_peak: f64) -> ReplayGain {
        ReplayGain {
            track_gain: Some(track),
            track_peak: Some(track_peak),
            album_gain: Some(album),
            album_peak: Some(album_peak),
            ..Default::default()
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn gain_turns_into_a_multiplier() {
        let gains = rg(-6.0, 0.5, -3.0, 0.5);
        assert!(close(
            gain_multiplier(&gains, GainSource::Track, 0.0),
            0.501
        ));
        assert!(close(
            gain_multiplier(&gains, GainSource::Album, 0.0),
            0.708
        ));
        // The pre-amp adds to the gain.
        assert!(close(gain_multiplier(&gains, GainSource::Track, 6.0), 1.0));
    }

    #[test]
    fn peaks_stop_positive_gain_from_clipping() {
        // +6 dB would be ~2.0, but a 0.8 peak only allows 1.25.
        let gains = rg(6.0, 0.8, 6.0, 0.8);
        assert!(close(gain_multiplier(&gains, GainSource::Track, 0.0), 1.25));
        // A full-scale peak allows no boost at all.
        let loud = rg(4.0, 1.0, 4.0, 1.0);
        assert!(close(gain_multiplier(&loud, GainSource::Album, 0.0), 1.0));
        // Unknown (zero) peaks don't clamp.
        let unknown = rg(6.0, 0.0, 6.0, 0.0);
        assert!(close(
            gain_multiplier(&unknown, GainSource::Track, 0.0),
            1.995
        ));
    }

    #[test]
    fn missing_gains_fall_back() {
        let only_track = ReplayGain {
            track_gain: Some(-6.0),
            ..Default::default()
        };
        assert!(close(
            gain_multiplier(&only_track, GainSource::Album, 0.0),
            0.501
        ));
        let fallback = ReplayGain {
            fallback_gain: Some(-6.0),
            ..Default::default()
        };
        assert!(close(
            gain_multiplier(&fallback, GainSource::Track, 0.0),
            0.501
        ));
        assert_eq!(
            gain_multiplier(&ReplayGain::default(), GainSource::Track, 0.0),
            1.0
        );
        let silly = ReplayGain {
            track_gain: Some(-90.0),
            ..Default::default()
        };
        assert_eq!(
            gain_multiplier(&silly, GainSource::Track, 0.0),
            MIN_MULTIPLIER
        );
    }

    #[test]
    fn auto_uses_album_gain_inside_an_album_run() {
        let a1 = song("1", "a");
        let a2 = song("2", "a");
        let b1 = song("3", "b");
        let auto = VolumeLevelling::Auto;
        assert_eq!(
            gain_source(auto, Some(&a1), &a2, None),
            Some(GainSource::Album)
        );
        assert_eq!(
            gain_source(auto, None, &a1, Some(&a2)),
            Some(GainSource::Album)
        );
        assert_eq!(
            gain_source(auto, Some(&a2), &b1, Some(&a1)),
            Some(GainSource::Track)
        );
        assert_eq!(gain_source(auto, None, &b1, None), Some(GainSource::Track));
        assert_eq!(
            gain_source(VolumeLevelling::Off, Some(&a1), &a2, None),
            None
        );
        assert_eq!(
            gain_source(VolumeLevelling::Track, Some(&a1), &a2, None),
            Some(GainSource::Track)
        );
        assert_eq!(
            gain_source(VolumeLevelling::Album, None, &b1, None),
            Some(GainSource::Album)
        );
    }

    #[test]
    fn same_album_needs_an_id_and_the_same_server() {
        let a = song("1", "a");
        let mut other_server = song("2", "a");
        other_server.server_id = "t".to_string();
        assert!(!same_album(&a, &other_server));
        let blank = song("3", "");
        assert!(!same_album(&blank, &song("4", "")));
        let mut radio = song("5", "a");
        radio.server_name = "Radio".to_string();
        assert!(!same_album(&a, &radio));
    }

    #[test]
    fn radio_and_untagged_songs_are_not_levelled() {
        let settings = AppSettings::default();
        let mut tagged = song("1", "a");
        tagged.replay_gain = Some(rg(-6.0, 0.5, -6.0, 0.5));
        assert!(close(levelling_gain(&settings, None, &tagged, None), 0.501));
        let mut radio = tagged.clone();
        radio.server_name = "Radio".to_string();
        assert_eq!(levelling_gain(&settings, None, &radio, None), 1.0);
        assert_eq!(levelling_gain(&settings, None, &song("2", "a"), None), 1.0);
        let off = AppSettings {
            volume_levelling: VolumeLevelling::Off,
            ..AppSettings::default()
        };
        assert_eq!(levelling_gain(&off, None, &tagged, None), 1.0);
    }

    #[test]
    fn opensubsonic_replay_gain_parses_and_survives_a_round_trip() {
        let raw = r#"{"id":"1","title":"t","replayGain":{"trackGain":-6.5,"albumGain":-4,"trackPeak":0.9,"albumPeak":0.95}}"#;
        let parsed: Song = serde_json::from_str(raw).unwrap();
        let rg = parsed.replay_gain.clone().unwrap();
        assert_eq!((rg.track_gain, rg.album_gain), (Some(-6.5), Some(-4.0)));
        assert_eq!((rg.track_peak, rg.album_peak), (Some(0.9), Some(0.95)));
        // Saved queues store it in snake_case and read it back.
        let again: Song = serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(again.replay_gain, parsed.replay_gain);
        let plain: Song = serde_json::from_str(r#"{"id":"2","title":"t"}"#).unwrap();
        assert!(plain.replay_gain.is_none());
    }

    #[test]
    fn neighbours_follow_the_current_song() {
        let queue = vec![song("1", "a"), song("2", "a"), song("3", "b")];
        let (prev, next) = queue_neighbours(&queue, 1, &queue[1]);
        assert_eq!(prev.map(|s| s.id.as_str()), Some("1"));
        assert_eq!(next.map(|s| s.id.as_str()), Some("3"));
        // A stale index is corrected by looking the song up.
        let (prev, next) = queue_neighbours(&queue, 0, &queue[2]);
        assert_eq!(prev.map(|s| s.id.as_str()), Some("2"));
        assert!(next.is_none());
    }
}
