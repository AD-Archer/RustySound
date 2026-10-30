//! Small pure helpers: hashing, a seeded RNG, time parsing and name matching.

use chrono::{DateTime, NaiveDateTime};

/// 64-bit FNV-1a hash. Stable across platforms and Rust versions (unlike
/// `std::hash`), so seeds derived from it are reproducible.
pub fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Seed for one generator on one day. `salt` changes when the user refreshes.
pub fn day_seed(date_key: &str, salt: u32, scope: &str) -> u64 {
    fnv1a(&format!("{date_key}#{salt}#{scope}"))
}

/// SplitMix64: tiny, deterministic, platform-independent PRNG.
#[derive(Debug, Clone)]
pub struct SeededRng(u64);

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform-ish integer in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    /// Pick `count` items from the first `pool` items, keeping their original order.
    pub fn pick_ordered<T: Clone>(&mut self, items: &[T], pool: usize, count: usize) -> Vec<T> {
        let pool = pool.min(items.len());
        let mut indexes: Vec<usize> = (0..pool).collect();
        self.shuffle(&mut indexes);
        indexes.truncate(count);
        indexes.sort_unstable();
        indexes.into_iter().map(|i| items[i].clone()).collect()
    }
}

/// Parse Subsonic/Navidrome timestamps (RFC 3339 or naive ISO forms) to epoch seconds.
pub fn parse_timestamp(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.timestamp());
    }
    for format in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed.and_utc().timestamp());
        }
    }
    None
}

/// Current time in epoch seconds.
pub fn now_epoch_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Today's local date as `YYYY-MM-DD`; the key that keeps mixes stable for a day.
pub fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Normalise an artist/album/track name for fuzzy equality: lowercase,
/// "&" → "and", drop a leading "the ", drop bracketed suffixes like
/// "(Remastered)", keep only letters and digits.
pub fn normalize_name(name: &str) -> String {
    let mut lowered = name.trim().to_lowercase().replace('&', " and ");
    // Drop bracketed qualifiers: "Album (Deluxe Edition)" → "Album".
    for (open, close) in [('(', ')'), ('[', ']')] {
        while let (Some(start), Some(end)) = (lowered.find(open), lowered.find(close)) {
            if end <= start {
                break;
            }
            let candidate = format!("{}{}", &lowered[..start], &lowered[end + close.len_utf8()..]);
            if candidate.trim().is_empty() {
                break;
            }
            lowered = candidate;
        }
    }
    let lowered = lowered.trim();
    let lowered = lowered.strip_prefix("the ").unwrap_or(lowered);
    lowered.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// True when two names refer to the same thing after [`normalize_name`].
pub fn same_name(left: &str, right: &str) -> bool {
    let l = normalize_name(left);
    !l.is_empty() && l == normalize_name(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_for_a_seed() {
        let mut a = SeededRng::new(day_seed("2026-10-29", 0, "mix"));
        let mut b = SeededRng::new(day_seed("2026-10-29", 0, "mix"));
        let seq_a: Vec<u64> = (0..5).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..5).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
        let mut c = SeededRng::new(day_seed("2026-10-30", 0, "mix"));
        assert_ne!(seq_a[0], c.next_u64());
        let mut d = SeededRng::new(day_seed("2026-10-29", 1, "mix"));
        assert_ne!(seq_a[0], d.next_u64());
    }

    #[test]
    fn fnv_is_stable() {
        // Pinned so seeds never silently change between builds.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn parses_common_timestamps() {
        assert_eq!(parse_timestamp("1970-01-02T00:00:00Z"), Some(86_400));
        assert_eq!(parse_timestamp("1970-01-02T00:00:00.123Z"), Some(86_400));
        assert_eq!(parse_timestamp("1970-01-02 00:00:00"), Some(86_400));
        assert_eq!(parse_timestamp(""), None);
        assert_eq!(parse_timestamp("yesterday"), None);
    }

    #[test]
    fn normalizes_names() {
        assert!(same_name("The Halvorsen Set", "halvorsen set"));
        assert!(same_name("Kestrel & Vine", "Kestrel and Vine"));
        assert!(same_name("Low Tide Radio (Deluxe Edition)", "Low Tide Radio"));
        assert!(!same_name("", ""));
        assert!(!same_name("Mira Okafor", "Junie Marlow"));
    }
}
