//! Wrapped's words: plain and warm, and honest about estimates. Pure.

use super::super::stats::number;
use crate::api::listening::{DayPart, Personality, WrappedYear};
use chrono::NaiveDate;

pub(super) fn count(n: u64) -> String {
    number(Some(n))
}

pub(super) fn plays(n: u64) -> String {
    if n == 1 { "1 play".to_string() } else { format!("{} plays", count(n)) }
}

/// "this year" for the current year, "in 2025" otherwise.
pub(super) fn when(year: &WrappedYear) -> String {
    if year.so_far { "this year".to_string() } else { format!("in {}", year.year) }
}

/// The heading over the story: "2026 so far" or "2025".
pub(super) fn title(year: i32, so_far: bool) -> String {
    if so_far { format!("{year} so far") } else { year.to_string() }
}

/// What the songs of a year are, honestly: their last play falls in it.
pub(super) fn songs_of(year: &WrappedYear) -> String {
    if year.so_far {
        "the songs you played this year".to_string()
    } else {
        format!("the songs you last played in {}", year.year)
    }
}

/// The comparison under the minutes, when the year before has listening.
pub(super) fn comparison(year: &WrappedYear) -> Option<String> {
    let before = year.previous_minutes.filter(|m| *m > 0)?;
    let last = year.year - 1;
    let now = year.minutes as f64;
    let then = before as f64;
    Some(if now > then * 1.05 {
        format!("Up from about {} in {last}.", count(before))
    } else if now < then * 0.95 {
        format!("Down from about {} in {last}.", count(before))
    } else {
        format!("About the same as {last}.")
    })
}

/// Why the numbers are estimates. Shown small under the minutes and on the
/// clock, the streak and the top lists.
pub(super) const MINUTES_NOTE: &str = "An estimate. Your server keeps a total for each song, not a date for every play, so earlier plays of these songs count too.";
pub(super) const LAST_PLAY_NOTE: &str = "Based on the last time you played each song.";
pub(super) const RANK_NOTE: &str = "Ranked by each song's total plays.";

pub(super) fn part(part: DayPart) -> (&'static str, &'static str) {
    match part {
        DayPart::Morning => ("a morning listener", "between 5 am and noon"),
        DayPart::Afternoon => ("an afternoon listener", "between noon and 5 pm"),
        DayPart::Evening => ("an evening listener", "between 5 and 10 pm"),
        DayPart::Night => ("a night listener", "after 10 pm"),
    }
}

pub(super) fn weekday(day: u32) -> &'static str {
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
        .get(day as usize)
        .copied()
        .unwrap_or("")
}

/// "March 14".
pub(super) fn date(day: NaiveDate) -> String {
    day.format("%B %-d").to_string()
}

/// "March 3 to 14", "March 28 to April 2".
pub(super) fn date_range(start: NaiveDate, end: NaiveDate) -> String {
    if start.format("%m").to_string() == end.format("%m").to_string() {
        format!("{} to {}", date(start), end.format("%-d"))
    } else {
        format!("{} to {}", date(start), date(end))
    }
}

/// Clock labels for an hour: "6 am", "noon", "midnight".
pub(super) fn hour(h: usize) -> String {
    match h {
        0 => "midnight".to_string(),
        12 => "noon".to_string(),
        1..=11 => format!("{h} am"),
        _ => format!("{} pm", h - 12),
    }
}

pub(super) fn personality(kind: &Personality) -> (String, String) {
    let (title, line) = match kind {
        Personality::Devoted { artist } => {
            ("Loyal listener".to_string(), format!("{artist} was with you for a good part of the year."))
        }
        Personality::NightOwl => {
            ("Night listener".to_string(), "Your music came out after dark, once the day had gone quiet.".to_string())
        }
        Personality::EarlyBird => ("Early listener".to_string(), "You started a lot of days with music on.".to_string()),
        Personality::Explorer { new_artists } => (
            "Explorer".to_string(),
            format!("You went looking, and {} artists new to your library came along.", count(*new_artists as u64)),
        ),
        Personality::Eclectic { genres } => (
            "All over the map".to_string(),
            format!("{genres} genres, and none of them had you to itself."),
        ),
        Personality::AlbumListener => {
            ("Album listener".to_string(), "You like a record from the first song to the last.".to_string())
        }
        Personality::Steady => {
            ("Steady listener".to_string(), "Music was there most days, keeping you company.".to_string())
        }
    };
    (title, line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn year(minutes: u64, previous: Option<u64>) -> WrappedYear {
        WrappedYear {
            year: 2026,
            so_far: true,
            listens: 1,
            plays: 1,
            minutes,
            previous_minutes: previous,
            artist_count: 1,
            top_artists: Vec::new(),
            top_songs: Vec::new(),
            top_albums: Vec::new(),
            top_genres: Vec::new(),
            genre_count: 0,
            hours: [0; 24],
            busiest_day: None,
            busiest_weekday: None,
            streak: None,
            new_artists: Vec::new(),
            new_artist_count: 0,
            personality: Personality::Steady,
        }
    }

    #[test]
    fn comparisons_are_gentle_and_only_with_data() {
        assert_eq!(comparison(&year(12_000, Some(9_800))).as_deref(), Some("Up from about 9,800 in 2025."));
        assert_eq!(comparison(&year(5_000, Some(9_800))).as_deref(), Some("Down from about 9,800 in 2025."));
        assert_eq!(comparison(&year(10_000, Some(9_800))).as_deref(), Some("About the same as 2025."));
        assert_eq!(comparison(&year(10_000, None)), None);
        assert_eq!(comparison(&year(10_000, Some(0))), None);
    }

    #[test]
    fn past_years_say_last_played() {
        let mut past = year(1, None);
        past.so_far = false;
        past.year = 2025;
        assert_eq!(songs_of(&past), "the songs you last played in 2025");
        assert_eq!(when(&past), "in 2025");
        assert_eq!(songs_of(&year(1, None)), "the songs you played this year");
        assert_eq!(title(2026, true), "2026 so far");
    }

    #[test]
    fn small_words() {
        assert_eq!(plays(1), "1 play");
        assert_eq!(plays(1_204), "1,204 plays");
        assert_eq!(hour(0), "midnight");
        assert_eq!(hour(6), "6 am");
        assert_eq!(hour(12), "noon");
        assert_eq!(hour(18), "6 pm");
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert_eq!(date_range(d(3, 3), d(3, 14)), "March 3 to 14");
        assert_eq!(date_range(d(3, 28), d(4, 2)), "March 28 to April 2");
        assert_eq!(weekday(5), "Saturday");
    }
}
