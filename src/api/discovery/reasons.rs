//! Human-readable reason, length and freshness strings. Pure functions.

const DAY: i64 = 86_400;

fn plural(count: i64, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// "today", "yesterday", "3 days ago", "last week", "3 weeks ago",
/// "14 months ago", "2 years ago". Future or equal timestamps give "today".
pub fn relative_ago(now: i64, then: i64) -> String {
    let days = (now - then).max(0) / DAY;
    match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=6 => format!("{days} days ago"),
        7..=13 => "last week".to_string(),
        14..=59 => format!("{} weeks ago", days / 7),
        _ => {
            let months = days * 12 / 365;
            if months < 24 {
                format!("{} ago", plural(months.max(2), "month", "months"))
            } else {
                format!("{} ago", plural(months / 12, "year", "years"))
            }
        }
    }
}

/// "212 plays", "1 play".
pub fn plays_label(plays: u32) -> String {
    plural(i64::from(plays), "play", "plays")
}

/// "Last played 14 months ago · 212 plays".
pub fn rewind_reason(now: i64, last_played: i64, plays: u32) -> String {
    format!(
        "Last played {} · {}",
        relative_ago(now, last_played),
        plays_label(plays)
    )
}

/// "Added today", "Added 2 days ago", "Added last week", "Added 3 weeks ago".
/// Falls back to "New in your library" when the date is unknown.
pub fn added_reason(now: i64, created: Option<i64>) -> String {
    match created {
        Some(created) => format!("Added {}", relative_ago(now, created)),
        None => "New in your library".to_string(),
    }
}

/// "Because you play Mira Okafor".
pub fn because_reason(artist: &str) -> String {
    format!("Because you play {artist}")
}

/// "Similar to Mira Okafor".
pub fn similar_reason(artist: &str) -> String {
    format!("Similar to {artist}")
}

/// "1 hr 38 min", "52 min", "1 hr", "Under a minute" for tiny totals.
pub fn length_label(total_seconds: u64) -> String {
    let minutes = (total_seconds + 30) / 60;
    if minutes == 0 {
        return "Under a minute".to_string();
    }
    let hours = minutes / 60;
    let rest = minutes % 60;
    match (hours, rest) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} hr"),
        (h, m) => format!("{h} hr {m} min"),
    }
}

/// "Updated today" when generated for `today`, otherwise "Updated <date>".
pub fn freshness_label(generated_on: &str, today: &str) -> String {
    if generated_on == today {
        "Updated today".to_string()
    } else {
        format!("Updated {generated_on}")
    }
}

/// Join seed artist names for a blurb: "A", "A and B", "A, B and C".
pub fn join_names(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn relative_labels() {
        assert_eq!(relative_ago(NOW, NOW), "today");
        assert_eq!(relative_ago(NOW, NOW - DAY), "yesterday");
        assert_eq!(relative_ago(NOW, NOW - 2 * DAY), "2 days ago");
        assert_eq!(relative_ago(NOW, NOW - 8 * DAY), "last week");
        assert_eq!(relative_ago(NOW, NOW - 21 * DAY), "3 weeks ago");
        assert_eq!(relative_ago(NOW, NOW - 426 * DAY), "14 months ago");
        assert_eq!(relative_ago(NOW, NOW - 2 * 365 * DAY), "2 years ago");
        assert_eq!(relative_ago(NOW, NOW + DAY), "today");
    }

    #[test]
    fn reason_strings_match_the_mock() {
        assert_eq!(
            rewind_reason(NOW, NOW - 426 * DAY, 212),
            "Last played 14 months ago · 212 plays"
        );
        assert_eq!(added_reason(NOW, Some(NOW - 2 * DAY)), "Added 2 days ago");
        assert_eq!(added_reason(NOW, None), "New in your library");
        assert_eq!(plays_label(1), "1 play");
    }

    #[test]
    fn length_labels() {
        assert_eq!(length_label(98 * 60), "1 hr 38 min");
        assert_eq!(length_label(52 * 60 + 10), "52 min");
        assert_eq!(length_label(3600), "1 hr");
        assert_eq!(length_label(10), "Under a minute");
    }

    #[test]
    fn joins_names() {
        let names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(join_names(&names[..1]), "A");
        assert_eq!(join_names(&names[..2]), "A and B");
        assert_eq!(join_names(&names), "A, B and C");
    }
}
