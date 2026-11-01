//! Text for durations, sizes, counts and audio formats, shared by every view.

use crate::api::Song;

/// `m:ss`, or `h:mm:ss` from an hour up. Negative and non-finite values
/// show as `0:00`.
pub fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 {
        seconds.floor() as u64
    } else {
        0
    };
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A total length in words: "32 min", "1 hr", "1 hr 12 min". Rounded to the
/// nearest minute; anything above zero shows at least "1 min".
#[allow(dead_code)] // the views switch to it in refactor chunks B-D
pub fn total(seconds: u64) -> String {
    let minutes = (seconds + 30) / 60;
    if minutes >= 60 {
        let (h, m) = (minutes / 60, minutes % 60);
        if m == 0 {
            format!("{h} hr")
        } else {
            format!("{h} hr {m} min")
        }
    } else {
        format!("{} min", minutes.max(if seconds > 0 { 1 } else { 0 }))
    }
}

/// "8 songs, 32 min"; just "8 songs" when the length is unknown.
#[allow(dead_code)] // the views switch to it in refactor chunks B-D
pub fn songs_summary(count: usize, seconds: u64) -> String {
    let noun = if count == 1 { "song" } else { "songs" };
    if seconds == 0 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}, {}", total(seconds))
    }
}

/// Megabytes with one decimal ("12.4 MB"), switching to GB from 1024 MB.
#[allow(dead_code)] // the views switch to it in refactor chunks B-D
pub fn bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.1} MB")
    }
}

/// "1,204"-style thousands separators.
#[allow(dead_code)] // the views switch to it in refactor chunks B-D
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The player's format badge: "FLAC", or "MP3 320" for lossy files (the
/// song model has no sample rate or bit depth, so lossy files show their
/// bitrate instead). The container comes from the suffix, else the content
/// type; `None` when neither is known.
#[allow(dead_code)] // the views switch to it in refactor chunks B-D
pub fn audio_badge(song: &Song) -> Option<String> {
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
                .map(|s| s.trim().trim_start_matches("x-").to_ascii_uppercase())
                .filter(|s| !s.is_empty())
        })?;
    let lossless = matches!(
        suffix.as_str(),
        "FLAC" | "ALAC" | "WAV" | "AIFF" | "APE" | "WV" | "DSF"
    );
    match song.bitrate.filter(|kbps| *kbps > 0) {
        Some(kbps) if !lossless => Some(format!("{suffix} {kbps}")),
        _ => Some(suffix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_switches_to_hours_and_clamps_bad_input() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(5.9), "0:05");
        assert_eq!(clock(61.0), "1:01");
        assert_eq!(clock(3599.0), "59:59");
        assert_eq!(clock(3600.0), "1:00:00");
        assert_eq!(clock(4505.0), "1:15:05");
        assert_eq!(clock(-3.0), "0:00");
        assert_eq!(clock(f64::NAN), "0:00");
        assert_eq!(clock(f64::INFINITY), "0:00");
    }

    #[test]
    fn total_rounds_to_minutes_and_names_hours() {
        assert_eq!(total(0), "0 min");
        assert_eq!(total(1), "1 min");
        assert_eq!(total(29 * 60 + 29), "29 min");
        assert_eq!(total(29 * 60 + 30), "30 min");
        assert_eq!(total(3600), "1 hr");
        assert_eq!(total(3600 + 12 * 60), "1 hr 12 min");
        assert_eq!(total(2 * 3600 - 10), "2 hr");
    }

    #[test]
    fn songs_summary_counts_and_adds_the_length() {
        assert_eq!(songs_summary(1, 0), "1 song");
        assert_eq!(songs_summary(8, 0), "8 songs");
        assert_eq!(songs_summary(8, 32 * 60), "8 songs, 32 min");
        assert_eq!(songs_summary(1, 200), "1 song, 3 min");
    }

    #[test]
    fn bytes_uses_megabytes_then_gigabytes() {
        assert_eq!(bytes(0), "0.0 MB");
        assert_eq!(bytes(1024 * 1024 * 3 / 2), "1.5 MB");
        assert_eq!(bytes(1023 * 1024 * 1024), "1023.0 MB");
        assert_eq!(bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn thousands_groups_by_three() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1204), "1,204");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn audio_badge_prefers_suffix_and_shows_lossy_bitrates() {
        let song = |suffix: Option<&str>, content_type: Option<&str>, bitrate: Option<u32>| Song {
            suffix: suffix.map(str::to_string),
            content_type: content_type.map(str::to_string),
            bitrate,
            ..Default::default()
        };
        assert_eq!(audio_badge(&song(Some("flac"), None, Some(1024))).as_deref(), Some("FLAC"));
        assert_eq!(audio_badge(&song(Some("mp3"), None, Some(320))).as_deref(), Some("MP3 320"));
        assert_eq!(audio_badge(&song(Some("mp3"), None, Some(0))).as_deref(), Some("MP3"));
        assert_eq!(audio_badge(&song(Some(" "), Some("audio/x-flac"), None)).as_deref(), Some("FLAC"));
        assert_eq!(audio_badge(&song(None, Some("audio/mpeg"), Some(256))).as_deref(), Some("MPEG 256"));
        assert_eq!(audio_badge(&song(Some("dsf"), None, Some(5644))).as_deref(), Some("DSF"));
        assert_eq!(audio_badge(&song(None, None, Some(320))), None);
        assert_eq!(audio_badge(&song(None, Some("audio/"), None)), None);
    }
}
