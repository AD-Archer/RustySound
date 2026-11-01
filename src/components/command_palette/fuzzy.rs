//! Fuzzy matching for palette rows: every query token (split the way Search
//! splits queries, `search::tokenize`) must match the text as a substring or
//! an in-order subsequence. Returns a score and the matched characters, so a
//! row can highlight them.
//!
//! Scores on one scale for actions and library titles: a prefix of the text
//! beats a word start, which beats a substring, which beats a scattered
//! subsequence ("np" → "Now Playing" sits between). Library results keep the
//! order Search's scoring gave them; this only orders sections and highlights.

use crate::components::views::tokenize;

/// A match: higher `score` is better; `hits` are char indices into the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    pub hits: Vec<usize>,
}

/// Score from which a match counts as "meant" (a prefix or word start of
/// every token): strong action matches list above library results.
pub const STRONG: i32 = 80;

/// The text's letters and digits, lowercased, with their char index and
/// whether a word starts there (after a space, punctuation or a lower→upper
/// change). Punctuation is skipped as Search's `normalize_text` skips it, so
/// "dont" matches "Don't" and "playpause" matches "Play/Pause".
fn fold(text: &str) -> Vec<(usize, char, bool)> {
    let mut out = Vec::with_capacity(text.len());
    let mut prev: Option<char> = None;
    for (index, ch) in text.chars().enumerate() {
        if ch.is_alphanumeric() {
            let start = match prev {
                None => true,
                Some(p) => !p.is_alphanumeric() || (p.is_lowercase() && ch.is_uppercase()),
            };
            for lower in ch.to_lowercase() {
                out.push((index, lower, start));
            }
        }
        prev = Some(ch);
    }
    out
}

/// One token against the folded text.
fn match_token(token: &[char], text: &[(usize, char, bool)]) -> Option<FuzzyMatch> {
    if token.is_empty() || token.len() > text.len() {
        return None;
    }
    // Contiguous: the best placed occurrence wins.
    let mut best: Option<(i32, usize)> = None;
    for start in 0..=text.len() - token.len() {
        if text[start..start + token.len()].iter().zip(token).all(|(t, q)| t.1 == *q) {
            let score = if start == 0 {
                100
            } else if text[start].2 {
                80
            } else {
                50
            };
            if best.map_or(true, |(s, _)| score > s) {
                best = Some((score, start));
            }
            if score == 100 {
                break;
            }
        }
    }
    if let Some((score, start)) = best {
        let hits = text[start..start + token.len()].iter().map(|t| t.0).collect();
        // Longer runs of the text matched read as more deliberate.
        return Some(FuzzyMatch { score: score + token.len() as i32, hits });
    }
    // Scattered: two greedy passes, word starts first and plain first; the
    // better one wins. Capped below a substring match.
    [true, false]
        .into_iter()
        .filter_map(|prefer_starts| subsequence(token, text, prefer_starts))
        .max_by_key(|m| m.score)
}

fn subsequence(token: &[char], text: &[(usize, char, bool)], prefer_starts: bool) -> Option<FuzzyMatch> {
    let mut hits = Vec::with_capacity(token.len());
    let mut score = 0i32;
    let mut cursor = 0usize;
    let mut last: Option<usize> = None;
    for &q in token {
        let plain = (cursor..text.len()).find(|&i| text[i].1 == q)?;
        let pick = if prefer_starts {
            (cursor..text.len())
                .find(|&i| text[i].1 == q && text[i].2)
                .unwrap_or(plain)
        } else {
            plain
        };
        score += 4;
        if text[pick].2 {
            score += 8;
        }
        match last {
            Some(prev) if pick == prev + 1 => score += 5,
            Some(prev) => score -= ((pick - prev - 1) as i32).min(6),
            None => score -= (pick as i32).min(6),
        }
        hits.push(text[pick].0);
        last = Some(pick);
        cursor = pick + 1;
    }
    (score > 0).then(|| FuzzyMatch { score: score.min(45), hits })
}

/// Matches `query` against `text`: every token must match. The score adds up
/// per token, plus a bonus when the whole text is the query.
pub fn fuzzy(query: &str, text: &str) -> Option<FuzzyMatch> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return None;
    }
    let folded = fold(text);
    let mut total = FuzzyMatch { score: 0, hits: Vec::new() };
    for token in &tokens {
        let chars: Vec<char> = token.chars().collect();
        let found = match_token(&chars, &folded)?;
        total.score += found.score;
        total.hits.extend(found.hits);
    }
    total.score /= tokens.len() as i32;
    let whole: String = folded.iter().map(|t| t.1).collect();
    if whole == tokens.concat() {
        total.score += 50;
    }
    total.hits.sort_unstable();
    total.hits.dedup();
    Some(total)
}

/// `text` split into runs of (highlighted, run), for rendering `<mark>`s.
pub fn segments(text: &str, hits: &[usize]) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        let on = hits.binary_search(&index).is_ok();
        match out.last_mut() {
            Some((state, run)) if *state == on => run.push(ch),
            _ => out.push((on, ch.to_string())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(query: &str, text: &str) -> i32 {
        fuzzy(query, text).map(|m| m.score).unwrap_or(0)
    }

    #[test]
    fn prefix_beats_word_start_beats_substring_beats_scattered() {
        let prefix = score("rep", "Repeat All");
        let word = score("all", "Repeat All");
        let inner = score("pea", "Repeat All");
        let scattered = score("rpt", "Repeat All");
        assert!(prefix > word, "{prefix} > {word}");
        assert!(word > inner, "{word} > {inner}");
        assert!(inner > scattered, "{inner} > {scattered}");
        assert!(scattered > 0);
    }

    #[test]
    fn exact_title_wins_and_every_token_must_match() {
        assert!(score("next", "Next") > score("next", "Next Album"));
        assert_eq!(fuzzy("next zebra", "Next"), None);
        assert_eq!(fuzzy("", "Next"), None);
        assert_eq!(fuzzy("xyz", "Next"), None);
    }

    #[test]
    fn acronyms_and_punctuation() {
        // Word starts: "np" → Now Playing.
        let np = fuzzy("np", "Open Now Playing").unwrap();
        assert_eq!(np.hits, vec![5, 9]);
        assert!(fuzzy("playpause", "Play/Pause").is_some());
        assert!(fuzzy("dont", "Don't Stop").is_some());
        // Camel-case boundaries count as word starts.
        assert_eq!(fuzzy("gb", "GoBack").unwrap().hits, vec![0, 2]);
    }

    #[test]
    fn hits_index_chars_not_bytes() {
        let m = fuzzy("café", "Café Society").unwrap();
        assert_eq!(m.hits, vec![0, 1, 2, 3]);
        let m = fuzzy("soc", "Café Society").unwrap();
        assert_eq!(m.hits, vec![5, 6, 7]);
    }

    #[test]
    fn segments_group_runs() {
        assert_eq!(
            segments("Next", &[0, 1]),
            vec![(true, "Ne".to_string()), (false, "xt".to_string())]
        );
        assert_eq!(segments("ab", &[]), vec![(false, "ab".to_string())]);
    }
}
