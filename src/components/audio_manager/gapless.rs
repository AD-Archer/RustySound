//! Gapless playback and crossfades: which song to preload, when, and how two
//! songs meet. The decisions here are pure; the audio controllers act on them.
//!
//! Mac and web: near the end of a song (`PRELOAD_LEAD_SECS`, plus the
//! crossfade) the controller arms the audio deck's standby element with the
//! next song (`DeckArm`, one song ahead). The deck (`audio_deck.js`) starts it
//! the moment the current song ends, or overlaps the two for a crossfade, and
//! reports the handover ("from|to"); the controller then moves the queue on
//! without loading anything (`apply_gapless_handover`).
//!
//! iOS: AVQueuePlayer holds the next song's item behind the current one
//! (`IosAudioPlayer::sync_next_item`), so it starts with no gap even while the
//! web view is suspended. No crossfade there: one player has one volume.
//!
//! Android: ExoPlayer holds the queue around the current song as a playlist
//! (`android_sync_playback_plan`) and reports "gapless:from|to" the same way,
//! so it keeps going while the web view is suspended. No crossfade there
//! either.
//!
//! Crossfades are skipped (plain gapless) between songs of the same album, as
//! Plexamp does, and radio is never preloaded or crossfaded.
// iOS and Android queue natively and Windows has no gapless yet: most of
// this is the Mac and web path.
#![cfg_attr(
    all(
        not(target_arch = "wasm32"),
        any(target_os = "ios", target_os = "windows", target_os = "android")
    ),
    allow(dead_code)
)]

use super::levelling::{levelling_gain, same_album};
use crate::api::Song;
use crate::db::{AppSettings, RepeatMode};

/// Arm the next song this long before the current one ends (plus the
/// crossfade length). Songs shorter than this arm as soon as they play.
pub const PRELOAD_LEAD_SECS: f64 = 20.0;
/// Crossfade lengths the settings offer, in seconds.
pub const CROSSFADE_MIN_SECS: u32 = 1;
pub const CROSSFADE_MAX_SECS: u32 = 12;

pub fn is_live(song: &Song) -> bool {
    song.server_name == "Radio"
}

/// The queue position that plays after `idx` when a song ends by itself:
/// the next entry, or the first one under repeat-all. Repeat-one (replay) and
/// the end of the queue have none.
pub fn next_index(len: usize, idx: usize, repeat: RepeatMode) -> Option<usize> {
    if len == 0 || repeat == RepeatMode::One {
        return None;
    }
    if idx + 1 < len {
        Some(idx + 1)
    } else if repeat == RepeatMode::All {
        Some(0)
    } else {
        None
    }
}

/// Crossfade length between `current` and `next`, in seconds; 0 for a plain
/// gapless start (crossfade off, same album, radio, or songs too short to
/// overlap by that much).
pub fn crossfade_secs(settings: &AppSettings, current: &Song, next: &Song) -> f64 {
    if !settings.crossfade_enabled || is_live(current) || is_live(next) {
        return 0.0;
    }
    if same_album(current, next) {
        return 0.0;
    }
    let secs = settings
        .crossfade_duration
        .clamp(CROSSFADE_MIN_SECS, CROSSFADE_MAX_SECS) as f64;
    // Unknown (0) durations don't block it; short songs do.
    let too_short = |song: &Song| song.duration > 0 && (song.duration as f64) < secs * 3.0;
    if too_short(current) || too_short(next) {
        return 0.0;
    }
    secs
}

/// How long before the end to arm the next song.
pub fn preload_lead_secs(settings: &AppSettings) -> f64 {
    let fade = if settings.crossfade_enabled {
        settings
            .crossfade_duration
            .clamp(CROSSFADE_MIN_SECS, CROSSFADE_MAX_SECS) as f64
    } else {
        0.0
    };
    PRELOAD_LEAD_SECS + fade
}

/// True once the song is within `lead` seconds of its end. Unknown or
/// endless durations (radio) never are.
pub fn should_preload(position: f64, duration: f64, lead: f64) -> bool {
    if !duration.is_finite() || duration <= 0.0 || !position.is_finite() {
        return false;
    }
    duration - position <= lead
}

/// The song to preload after `current`, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct NextTrack {
    pub index: usize,
    pub song: Song,
    /// 0 for a gapless start.
    pub fade_secs: f64,
    /// The next song's levelling multiplier.
    pub gain: f64,
}

/// What follows `current` (at queue position `idx`) when it ends by itself,
/// or `None` when nothing should be preloaded: gapless and crossfade both
/// off, radio on either side, repeat-one, the end of the queue, or a queue
/// that no longer has `current` at `idx`.
pub fn plan_next_track(
    settings: &AppSettings,
    queue: &[Song],
    idx: usize,
    repeat: RepeatMode,
    current: &Song,
) -> Option<NextTrack> {
    if is_live(current) || queue.get(idx).map(|s| s.id.as_str()) != Some(current.id.as_str()) {
        return None;
    }
    let index = next_index(queue.len(), idx, repeat)?;
    let song = queue.get(index)?;
    if is_live(song) {
        return None;
    }
    let fade_secs = crossfade_secs(settings, current, song);
    if !settings.gapless_playback && fade_secs <= 0.0 {
        return None;
    }
    let after = queue.get(index + 1);
    Some(NextTrack {
        index,
        song: song.clone(),
        fade_secs,
        gain: levelling_gain(settings, Some(current), song, after),
    })
}

/// The standby element's next song, as the controllers send it to the deck.
#[derive(Clone, Debug, PartialEq)]
pub struct DeckArm {
    pub src: String,
    pub from_id: String,
    pub song_id: String,
    pub song: Song,
    /// Output volume for the next song (user volume times its levelling).
    pub volume: f64,
    pub fade_ms: u64,
}

impl DeckArm {
    pub fn new(next: NextTrack, current: &Song, src: String, user_volume: f64) -> Self {
        let volume = super::levelled_volume(user_volume, next.gain);
        Self {
            src,
            from_id: current.id.clone(),
            song_id: next.song.id.clone(),
            song: next.song,
            // Rounded, so tiny float noise doesn't re-arm every tick.
            volume: (volume * 1000.0).round() / 1000.0,
            fade_ms: (next.fade_secs * 1000.0).round() as u64,
        }
    }
}

/// Remembers what the deck was last armed with, so the poll loops send a
/// command only when it changes.
#[derive(Default)]
pub struct DeckArmer {
    last: Option<DeckArm>,
}

impl DeckArmer {
    /// Sends `wanted` through `send` if it differs from the last one sent.
    pub fn sync(&mut self, wanted: Option<DeckArm>, send: impl FnOnce(Option<&DeckArm>)) {
        if self.last != wanted {
            send(wanted.as_ref());
            self.last = wanted;
        }
    }

    /// The deck dropped its arm by itself (a handover, or a new song loaded):
    /// forget it, unless it is still for `current_id`.
    pub fn forget_unless_for(&mut self, current_id: Option<&str>) {
        if self.last.as_ref().map(|arm| arm.from_id.as_str()) != current_id {
            self.last = None;
        }
    }
}

/// Parses a handover report, "from|to", into the finished and next song ids.
pub fn parse_handover(raw: &str) -> Option<(String, String)> {
    let (from, to) = raw.split_once('|')?;
    if to.is_empty() {
        return None;
    }
    Some((from.to_string(), to.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: &str, album: &str, duration: u32) -> Song {
        Song {
            id: id.to_string(),
            album_id: Some(album.to_string()),
            server_id: "s".to_string(),
            duration,
            ..Default::default()
        }
    }

    fn radio(id: &str) -> Song {
        Song {
            id: id.to_string(),
            server_name: "Radio".to_string(),
            ..Default::default()
        }
    }

    fn crossfading(secs: u32) -> AppSettings {
        AppSettings {
            crossfade_enabled: true,
            crossfade_duration: secs,
            ..AppSettings::default()
        }
    }

    #[test]
    fn next_index_follows_repeat() {
        assert_eq!(next_index(3, 0, RepeatMode::Off), Some(1));
        assert_eq!(next_index(3, 2, RepeatMode::Off), None);
        assert_eq!(next_index(3, 2, RepeatMode::All), Some(0));
        assert_eq!(next_index(3, 0, RepeatMode::One), None);
        assert_eq!(next_index(0, 0, RepeatMode::All), None);
    }

    #[test]
    fn preload_starts_within_the_lead() {
        assert!(!should_preload(10.0, 200.0, 20.0));
        assert!(should_preload(180.0, 200.0, 20.0));
        assert!(should_preload(0.0, 15.0, 20.0), "short songs arm at once");
        assert!(!should_preload(5.0, f64::NAN, 20.0));
        assert!(
            !should_preload(5.0, f64::INFINITY, 20.0),
            "radio never arms"
        );
        assert!(!should_preload(5.0, 0.0, 20.0));
        let settings = crossfading(6);
        assert_eq!(preload_lead_secs(&settings), 26.0);
        assert_eq!(preload_lead_secs(&AppSettings::default()), 20.0);
    }

    #[test]
    fn crossfade_is_skipped_inside_an_album_and_for_radio() {
        let settings = crossfading(5);
        let a1 = song("1", "a", 200);
        let a2 = song("2", "a", 200);
        let b1 = song("3", "b", 200);
        assert_eq!(crossfade_secs(&settings, &a1, &b1), 5.0);
        assert_eq!(crossfade_secs(&settings, &a1, &a2), 0.0);
        assert_eq!(crossfade_secs(&settings, &a1, &radio("r")), 0.0);
        assert_eq!(crossfade_secs(&AppSettings::default(), &a1, &b1), 0.0);
        // Too short to overlap by 5 s; unknown durations are fine.
        assert_eq!(crossfade_secs(&settings, &song("4", "c", 12), &b1), 0.0);
        assert_eq!(crossfade_secs(&settings, &song("5", "c", 0), &b1), 5.0);
        // Out-of-range lengths are clamped.
        assert_eq!(crossfade_secs(&crossfading(40), &a1, &b1), 12.0);
    }

    #[test]
    fn plan_picks_the_next_song_or_nothing() {
        let queue = vec![
            song("1", "a", 200),
            song("2", "a", 200),
            song("3", "b", 200),
        ];
        let settings = crossfading(4);
        let plan = plan_next_track(&settings, &queue, 0, RepeatMode::Off, &queue[0]).unwrap();
        assert_eq!(
            (plan.index, plan.song.id.as_str(), plan.fade_secs),
            (1, "2", 0.0)
        );
        let plan = plan_next_track(&settings, &queue, 1, RepeatMode::Off, &queue[1]).unwrap();
        assert_eq!((plan.song.id.as_str(), plan.fade_secs), ("3", 4.0));
        assert!(plan_next_track(&settings, &queue, 2, RepeatMode::Off, &queue[2]).is_none());
        let wrap = plan_next_track(&settings, &queue, 2, RepeatMode::All, &queue[2]).unwrap();
        assert_eq!(wrap.index, 0);
        assert!(plan_next_track(&settings, &queue, 0, RepeatMode::One, &queue[0]).is_none());
        // A stale index (the queue changed under us) arms nothing.
        assert!(plan_next_track(&settings, &queue, 1, RepeatMode::Off, &queue[0]).is_none());
    }

    #[test]
    fn plan_respects_the_gapless_switch_and_radio() {
        let queue = vec![
            song("1", "a", 200),
            song("2", "a", 200),
            song("3", "b", 200),
        ];
        let off = AppSettings {
            gapless_playback: false,
            ..AppSettings::default()
        };
        assert!(plan_next_track(&off, &queue, 0, RepeatMode::Off, &queue[0]).is_none());
        // Crossfade still preloads with gapless off, but not inside an album.
        let fade_only = AppSettings {
            gapless_playback: false,
            ..crossfading(4)
        };
        assert!(plan_next_track(&fade_only, &queue, 0, RepeatMode::Off, &queue[0]).is_none());
        assert!(plan_next_track(&fade_only, &queue, 1, RepeatMode::Off, &queue[1]).is_some());
        let with_radio = vec![song("1", "a", 200), radio("r")];
        let settings = AppSettings::default();
        assert!(
            plan_next_track(&settings, &with_radio, 0, RepeatMode::Off, &with_radio[0]).is_none()
        );
        let from_radio = vec![radio("r"), song("1", "a", 200)];
        assert!(
            plan_next_track(&settings, &from_radio, 0, RepeatMode::Off, &from_radio[0]).is_none()
        );
    }

    #[test]
    fn arms_are_sent_once_and_forgotten_on_song_change() {
        let queue = vec![song("1", "a", 200), song("2", "b", 200)];
        let plan = plan_next_track(
            &AppSettings::default(),
            &queue,
            0,
            RepeatMode::Off,
            &queue[0],
        )
        .unwrap();
        let arm = DeckArm::new(plan, &queue[0], "u2".to_string(), 0.8);
        assert_eq!(arm.volume, 0.8);
        let mut armer = DeckArmer::default();
        let mut sent = 0;
        armer.sync(Some(arm.clone()), |_| sent += 1);
        armer.sync(Some(arm.clone()), |_| sent += 1);
        assert_eq!(sent, 1);
        armer.forget_unless_for(Some("1"));
        armer.sync(Some(arm.clone()), |_| sent += 1);
        assert_eq!(sent, 1);
        armer.forget_unless_for(Some("2"));
        armer.sync(Some(arm), |_| sent += 1);
        assert_eq!(sent, 2);
    }

    #[test]
    fn handover_reports_parse() {
        assert_eq!(
            parse_handover("a|b"),
            Some(("a".to_string(), "b".to_string()))
        );
        assert_eq!(parse_handover("|b"), Some((String::new(), "b".to_string())));
        assert_eq!(parse_handover("a|"), None);
        assert_eq!(parse_handover("ab"), None);
    }
}
