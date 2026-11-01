//! The LRC parser: plain LRC exactly as before, plus Enhanced LRC word tags.

use super::*;

fn plain(timestamp_seconds: f64, text: &str) -> LyricLine {
    LyricLine {
        timestamp_seconds,
        text: text.to_string(),
        words: Vec::new(),
    }
}

fn texts(words: &[LyricWord]) -> Vec<&str> {
    words.iter().map(|word| word.text.as_str()).collect()
}

#[test]
fn plain_lrc_parses_as_before() {
    let lines = parse_lrc_lines(
        "[ti:Song]\n[ar:Someone]\n[00:12.50] First line\n[00:05.00]Earlier\n\n[01:02.25]  Spaced   out  \n[00:20.00]\n[bad]Skipped",
    );
    assert_eq!(
        lines,
        vec![
            plain(5.0, "Earlier"),
            plain(12.5, "First line"),
            plain(62.25, "Spaced   out"),
        ]
    );
}

#[test]
fn repeated_stamps_and_hours_still_work() {
    let lines = parse_lrc_lines("[00:10.00][00:30.00]Chorus\n[1:00:00.5]Late");
    assert_eq!(
        lines,
        vec![plain(10.0, "Chorus"), plain(30.0, "Chorus"), plain(3600.5, "Late")]
    );
}

#[test]
fn enhanced_lrc_gives_word_timings() {
    let lines =
        parse_lrc_lines("[00:12.00]<00:12.00>Hello <00:12.50>bright <00:13.25>world<00:14.00>");
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line.timestamp_seconds, 12.0);
    assert_eq!(line.text, "Hello bright world");
    assert_eq!(texts(&line.words), ["Hello ", "bright ", "world"]);
    assert_eq!(line.words[0].start_seconds, 12.0);
    assert_eq!(line.words[1].start_seconds, 12.5);
    assert_eq!(line.words[2].start_seconds, 13.25);
    // Only the trailing tag gives an explicit end.
    assert_eq!(line.words[0].end_seconds, None);
    assert_eq!(line.words[2].end_seconds, Some(14.0));
    assert_eq!(
        line.word_bounds(Some(20.0)),
        vec![(12.0, 12.5), (12.5, 13.25), (13.25, 14.0)]
    );
}

#[test]
fn text_before_the_first_tag_starts_with_the_line() {
    let lines = parse_lrc_lines("[00:01.00]Oh <00:01.80>yeah");
    let line = &lines[0];
    assert_eq!(line.text, "Oh yeah");
    assert_eq!(texts(&line.words), ["Oh ", "yeah"]);
    assert_eq!(line.words[0].start_seconds, 1.0);
    // The last word without an end runs to the next line, capped.
    assert_eq!(line.word_bounds(Some(2.5)), vec![(1.0, 1.8), (1.8, 2.5)]);
    assert_eq!(line.word_bounds(None), vec![(1.0, 1.8), (1.8, 3.3)]);
}

#[test]
fn mixed_plain_and_enhanced_lines() {
    let lines = parse_lrc_lines(
        "[00:01.00]Plain words here\n[00:04.00]<00:04.00>Timed <00:04.60>words\n[00:08.00] <00:08.00> <00:08.40>Spaced<00:09.00> ",
    );
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], plain(1.0, "Plain words here"));
    assert_eq!(texts(&lines[1].words), ["Timed ", "words"]);
    assert_eq!(lines[2].text, "Spaced");
    assert_eq!(texts(&lines[2].words), ["Spaced"]);
    assert_eq!(lines[2].words[0].start_seconds, 8.4);
    assert_eq!(lines[2].words[0].end_seconds, Some(9.0));
}

#[test]
fn repeated_stamps_shift_word_timings() {
    let lines = parse_lrc_lines("[00:10.00][00:30.00]<00:10.00>La <00:10.50>la");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].timestamp_seconds, 30.0);
    assert_eq!(lines[1].words[0].start_seconds, 30.0);
    assert_eq!(lines[1].words[1].start_seconds, 30.5);
}

#[test]
fn malformed_word_tags_stay_literal_text() {
    let lines = parse_lrc_lines(
        "[00:02.00]a <b> c <00:xx.00>d <3>\n[00:03.00]less < than\n[00:04.00]<00:04.00",
    );
    assert_eq!(lines[0], plain(2.0, "a <b> c <00:xx.00>d <3>"));
    assert_eq!(lines[1], plain(3.0, "less < than"));
    // An unclosed tag is text too.
    assert_eq!(lines[2], plain(4.0, "<00:04.00"));
}

#[test]
fn tag_only_lines_are_dropped() {
    assert!(parse_lrc_lines("[00:05.00]<00:05.00><00:06.00>").is_empty());
}

#[test]
fn lrclib_records_keep_word_timing() {
    let result = build_lrclib_result(LrclibResponse {
        track_name: "Song".to_string(),
        artist_name: "Artist".to_string(),
        album_name: String::new(),
        duration: Some(100.0),
        synced_lyrics: Some("[00:01.00]<00:01.00>Hi <00:01.50>there".to_string()),
        plain_lyrics: None,
    })
    .unwrap();
    assert_eq!(result.plain_lyrics, "Hi there");
    assert!(result.has_word_timing());
}

#[test]
fn word_timings_survive_the_persistent_cache() {
    let result = LyricsResult {
        provider: LyricsProvider::Server,
        plain_lyrics: "Hi there".to_string(),
        synced_lines: parse_lrc_lines("[00:01.00]<00:01.00>Hi <00:01.50>there"),
    };
    let json = serde_json::to_string(&PersistentLyricsResult::from(&result)).unwrap();
    let back: PersistentLyricsResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.to_runtime().unwrap(), result);
    // Entries written before word timings existed still load.
    let old: PersistentLyricsResult = serde_json::from_str(
        r#"{"provider_key":"lrclib","plain_lyrics":"x","synced_lines":[{"timestamp_seconds":1.0,"text":"x"}]}"#,
    )
    .unwrap();
    assert_eq!(old.to_runtime().unwrap().synced_lines, vec![plain(1.0, "x")]);
}

#[test]
fn server_provider_leads_the_default_order() {
    assert_eq!(default_lyrics_provider_order()[0], "server");
    assert_eq!(LyricsProvider::from_key("Navidrome"), Some(LyricsProvider::Server));
    // A saved order without it gets it appended by normalisation; the
    // settings migration (db) moves it to the front, once.
    let normalized = normalize_lyrics_provider_order(&["genius".to_string()]);
    assert_eq!(normalized, ["genius", "server", "lrclib", "netease"]);
}

#[test]
fn query_finds_the_songs_server() {
    let server = ServerConfig {
        id: "s1".to_string(),
        name: "Home".to_string(),
        url: "http://example.invalid".to_string(),
        username: "u".to_string(),
        password: "p".to_string(),
        active: true,
    };
    let song = Song {
        id: "song-1".to_string(),
        server_id: "s1".to_string(),
        title: "Title".to_string(),
        ..Song::default()
    };
    let query = LyricsQuery::from_song_on(&song, std::slice::from_ref(&server));
    assert_eq!(query.server_song.unwrap().song_id, "song-1");
    assert!(LyricsQuery::from_song_on(&song, &[]).server_song.is_none());
    assert!(LyricsQuery::from_song(&song).server_song.is_none());
}
