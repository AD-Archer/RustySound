//! Multi-server result scoring: tokenised fuzzy matching that re-ranks what
//! each server's `search3` returned. Unchanged from the old Search view, plus
//! `top_hit`.

use crate::api::{Album, Artist, SearchResult, Song};
use std::collections::HashSet;

/// Strip punctuation (apostrophes, hyphens, etc.) from text for comparison.
/// e.g. "Don't" -> "dont", "re-enter" -> "reenter"
pub(super) fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

/// Tokenize a search query into meaningful words.
/// Strips punctuation before splitting so "Don't" becomes "dont" (not "don" + "t").
pub(super) fn tokenize(text: &str) -> Vec<String> {
    normalize_text(text)
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

pub(super) fn dedupe_search_results(mut results: SearchResult) -> SearchResult {
    let mut artist_seen = HashSet::new();
    results
        .artists
        .retain(|artist| artist_seen.insert(format!("{}::{}", artist.server_id, artist.id)));

    let mut album_seen = HashSet::new();
    results
        .albums
        .retain(|album| album_seen.insert(format!("{}::{}", album.server_id, album.id)));

    let mut song_seen = HashSet::new();
    results
        .songs
        .retain(|song| song_seen.insert(format!("{}::{}", song.server_id, song.id)));

    results
}

/// Calculate fuzzy match score for a field against query tokens.
/// Normalizes the field (strips punctuation) so "Don't" matches "dont".
fn calculate_score(field: &str, tokens: &[String]) -> i32 {
    let field_normalized = normalize_text(field);
    let field_words: Vec<&str> = field_normalized.split_whitespace().collect();
    let mut score = 0;

    for token in tokens {
        // Exact full match (highest score)
        if field_normalized == *token {
            score += 100;
        }
        // Starts with token (high score)
        else if field_normalized.starts_with(token.as_str()) {
            score += 80;
        }
        // A word in the field matches the token exactly
        else if field_words.iter().any(|word| *word == token) {
            score += 60;
        }
        // A word in the field starts with the token (prefix match)
        else if field_words
            .iter()
            .any(|word| word.starts_with(token.as_str()))
        {
            score += 50;
        }
        // Contains token anywhere in the normalized field
        else if field_normalized.contains(token.as_str()) {
            score += 40;
        }
    }

    score
}

/// Check if a single token matches a normalized field, returning true if found.
fn token_matches_field(field_normalized: &str, token: &str) -> bool {
    field_normalized == token || field_normalized.contains(token)
}

/// Score songs based on fuzzy matching across title, artist, and album.
/// Requires ALL tokens to appear somewhere across the combined fields.
fn score_song(song: &Song, tokens: &[String]) -> i32 {
    let title_norm = normalize_text(&song.title);
    let artist_norm = song
        .artist
        .as_deref()
        .map(normalize_text)
        .unwrap_or_default();
    let album_norm = song
        .album
        .as_deref()
        .map(normalize_text)
        .unwrap_or_default();

    // Every token must appear in at least one field
    for token in tokens {
        if !token_matches_field(&title_norm, token)
            && !token_matches_field(&artist_norm, token)
            && !token_matches_field(&album_norm, token)
        {
            return 0;
        }
    }

    let title_score = calculate_score(&song.title, tokens) * 3;
    let artist_score = song
        .artist
        .as_deref()
        .map_or(0, |artist| calculate_score(artist, tokens))
        * 2;
    let album_score = song
        .album
        .as_deref()
        .map_or(0, |album| calculate_score(album, tokens));

    title_score + artist_score + album_score
}

/// Score albums based on fuzzy matching across name and artist.
/// Requires ALL tokens to appear somewhere across name or artist.
fn score_album(album: &Album, tokens: &[String]) -> i32 {
    let name_norm = normalize_text(&album.name);
    let artist_norm = normalize_text(&album.artist);

    for token in tokens {
        if !token_matches_field(&name_norm, token) && !token_matches_field(&artist_norm, token) {
            return 0;
        }
    }

    let name_score = calculate_score(&album.name, tokens) * 3;
    let artist_score = calculate_score(&album.artist, tokens) * 2;

    name_score + artist_score
}

/// Score artists based on fuzzy matching on name.
/// For single-field entities, at least half the tokens must match.
fn score_artist(artist: &Artist, tokens: &[String]) -> i32 {
    let name_norm = normalize_text(&artist.name);
    let matched = tokens
        .iter()
        .filter(|t| token_matches_field(&name_norm, t))
        .count();
    if matched == 0 {
        return 0;
    }
    calculate_score(&artist.name, tokens) * 4
}

/// Filter and score all search results with fuzzy matching
pub(super) fn filter_and_score_results(mut results: SearchResult, query: &str) -> SearchResult {
    let tokens = tokenize(query);

    if tokens.is_empty() {
        return results;
    }

    // Score and filter songs
    let mut song_scores: Vec<(Song, i32)> = results
        .songs
        .into_iter()
        .map(|song| {
            let score = score_song(&song, &tokens);
            (song, score)
        })
        .filter(|(_, score)| *score > 0) // Only keep songs with positive scores
        .collect();

    song_scores.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by score descending
    results.songs = song_scores.into_iter().map(|(song, _)| song).collect();

    // Score and filter albums
    let mut album_scores: Vec<(Album, i32)> = results
        .albums
        .into_iter()
        .map(|album| {
            let score = score_album(&album, &tokens);
            (album, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    album_scores.sort_by(|a, b| b.1.cmp(&a.1));
    results.albums = album_scores.into_iter().map(|(album, _)| album).collect();

    // Score and filter artists
    let mut artist_scores: Vec<(Artist, i32)> = results
        .artists
        .into_iter()
        .map(|artist| {
            let score = score_artist(&artist, &tokens);
            (artist, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();

    artist_scores.sort_by(|a, b| b.1.cmp(&a.1));
    results.artists = artist_scores
        .into_iter()
        .map(|(artist, _)| artist)
        .collect();

    results
}

/// What the "Top hit" card shows.
#[derive(Clone, PartialEq)]
pub(super) enum TopHit {
    Artist(Artist),
    Album(Album),
    Song(Song),
}

/// The best of the three result kinds' leaders. Ties go to the artist, then
/// the album: a name match on an artist is usually what was meant.
pub(super) fn top_hit(results: &SearchResult, query: &str) -> Option<TopHit> {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return None;
    }
    let artist = results
        .artists
        .first()
        .map(|a| (score_artist(a, &tokens), TopHit::Artist(a.clone())));
    let album = results
        .albums
        .first()
        .map(|a| (score_album(a, &tokens), TopHit::Album(a.clone())));
    let song = results
        .songs
        .first()
        .map(|s| (score_song(s, &tokens), TopHit::Song(s.clone())));
    let mut best: Option<(i32, TopHit)> = None;
    for candidate in [artist, album, song].into_iter().flatten() {
        if best.as_ref().map_or(true, |(score, _)| candidate.0 > *score) {
            best = Some(candidate);
        }
    }
    best.map(|(_, hit)| hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenises_without_punctuation() {
        assert_eq!(tokenize("Don't Stop-Me"), vec!["dont", "stopme"]);
    }

    #[test]
    fn exact_artist_beats_partial_song() {
        let results = SearchResult {
            artists: vec![Artist { id: "a".into(), name: "Paper Comets".into(), ..Default::default() }],
            albums: vec![],
            songs: vec![Song { id: "s".into(), title: "Paper Planes".into(), ..Default::default() }],
        };
        assert!(matches!(top_hit(&results, "paper comets"), Some(TopHit::Artist(_))));
    }
}
