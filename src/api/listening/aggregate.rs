//! Play-count aggregation shared by every listening view: ranking items by
//! their plays (Stats' Most Played, Discover's Rewind) and tallying plays per
//! artist, album or genre (Discover's seeds, Wrapped). Pure.

use std::collections::HashMap;

/// `items` ranked by plays, most first. Items with no plays are dropped,
/// and of several with the same `key` only the best ranked is kept. Ties
/// keep their input order. At most `limit` come back.
pub fn rank_by_plays<T>(
    mut items: Vec<T>,
    plays: impl Fn(&T) -> u32,
    key: impl Fn(&T) -> String,
    limit: usize,
) -> Vec<T> {
    items.retain(|item| plays(item) > 0);
    items.sort_by_key(|item| std::cmp::Reverse(plays(item)));
    dedupe_by(&mut items, key);
    items.truncate(limit);
    items
}

/// Keeps the first item of each `key`.
pub fn dedupe_by<T>(items: &mut Vec<T>, key: impl Fn(&T) -> String) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(key(item)));
}

/// Sums a weight per key. The first item added for a key represents it, so
/// add the most telling item first (the most played song of an artist, say);
/// `add` hands it back for any totals of its own.
#[derive(Debug, Clone)]
pub struct Tally<T> {
    entries: HashMap<String, (usize, T, u64)>,
}

impl<T> Default for Tally<T> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}

impl<T> Tally<T> {
    pub fn add(&mut self, key: String, weight: u64, first: impl FnOnce() -> T) -> &mut T {
        let order = self.entries.len();
        let entry = self.entries.entry(key).or_insert_with(|| (order, first(), 0));
        entry.2 += weight;
        &mut entry.1
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Every entry with a weight, heaviest first; ties in the order added.
    pub fn ranked(self) -> Vec<(T, u64)> {
        let mut entries: Vec<(usize, T, u64)> =
            self.entries.into_values().filter(|(_, _, weight)| *weight > 0).collect();
        entries.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        entries.into_iter().map(|(_, item, weight)| (item, weight)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_by_plays_without_zeros_or_duplicates() {
        let items = vec![("a", 3), ("b", 0), ("c", 9), ("a", 1), ("d", 3)];
        let ranked = rank_by_plays(items, |i| i.1, |i| i.0.to_string(), 10);
        assert_eq!(ranked, vec![("c", 9), ("a", 3), ("d", 3)]);
        let top = rank_by_plays(vec![("a", 1), ("b", 2), ("c", 3)], |i| i.1, |i| i.0.to_string(), 2);
        assert_eq!(top, vec![("c", 3), ("b", 2)]);
    }

    #[test]
    fn tally_sums_per_key_and_keeps_the_first_item() {
        let mut tally = Tally::default();
        tally.add("x".into(), 5, || "first x");
        tally.add("y".into(), 7, || "y");
        tally.add("x".into(), 4, || "second x");
        tally.add("z".into(), 0, || "z");
        assert_eq!(tally.len(), 3, "a key with no weight still counts as seen");
        assert_eq!(tally.ranked(), vec![("first x", 9), ("y", 7)]);
    }

    #[test]
    fn tally_ties_keep_insertion_order() {
        let mut tally = Tally::default();
        for name in ["b", "a", "c"] {
            tally.add(name.into(), 1, || name);
        }
        assert_eq!(tally.ranked(), vec![("b", 1), ("a", 1), ("c", 1)]);
    }
}
