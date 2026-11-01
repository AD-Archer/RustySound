//! The sync server's `/lyrics` (its HTTP.md): the music server's lyrics,
//! else LRCLIB's, looked up and cached once on the sync server, in one
//! shape. Tried before the provider chain when the song's server has a sync
//! server with the `lyrics` feature (`remote_sync::media::lyrics_url`).

use super::{LyricLine, LyricsProvider, LyricsQuery, LyricsResult};
use crate::api::{server_lyrics_support, NavidromeClient, ServerLyricsSupport};
use serde::Deserialize;
use std::time::Duration;

/// What the sync server said.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SyncLyrics {
    Found(LyricsResult),
    /// Neither the music server nor LRCLIB has lyrics (or it's
    /// instrumental): those two providers can be skipped.
    NotFound,
    /// No sync server, or it failed: run the chain as usual.
    Unavailable,
}

#[derive(Debug, Deserialize)]
struct Response {
    found: bool,
    source: Option<String>,
    #[serde(default)]
    instrumental: bool,
    #[serde(default)]
    offset_ms: f64,
    #[serde(default)]
    lines: Vec<Line>,
}

#[derive(Debug, Deserialize)]
struct Line {
    start_ms: Option<f64>,
    text: String,
}

/// Turns a `/lyrics` body into a result. The music server's `offset_ms`
/// (OpenSubsonic: positive shows lyrics sooner) is applied as the server
/// provider applies it.
fn parse(body: &str) -> SyncLyrics {
    let Ok(response) = serde_json::from_str::<Response>(body) else {
        return SyncLyrics::Unavailable;
    };
    if !response.found || response.instrumental || response.lines.is_empty() {
        return SyncLyrics::NotFound;
    }
    let provider = match response.source.as_deref() {
        Some("server") => LyricsProvider::Server,
        Some("lrclib") => LyricsProvider::Lrclib,
        _ => return SyncLyrics::Unavailable,
    };
    let offset = response.offset_ms / 1000.0;
    let synced = response.lines.iter().all(|line| line.start_ms.is_some());
    let synced_lines = if synced {
        response
            .lines
            .iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| LyricLine {
                timestamp_seconds: (line.start_ms.unwrap_or(0.0) / 1000.0 - offset).max(0.0),
                text: line.text.trim().to_string(),
                words: Vec::new(),
            })
            .collect()
    } else {
        Vec::new()
    };
    let plain_lyrics = response
        .lines
        .iter()
        .map(|line| line.text.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if plain_lyrics.is_empty() {
        return SyncLyrics::NotFound;
    }
    SyncLyrics::Found(LyricsResult { provider, plain_lyrics, synced_lines })
}

/// Asks the song's sync server. Lyrics it found on the music server are
/// passed over when that server has word timings (`songLyrics` v2): the
/// chain's server provider asks for them, and karaoke needs them.
pub(super) async fn fetch_from_sync(query: &LyricsQuery, timeout_seconds: u32) -> SyncLyrics {
    let Some(target) = query.server_song.as_ref() else {
        return SyncLyrics::Unavailable;
    };
    let Some(url) = crate::components::remote_sync::media::lyrics_url(
        &target.server,
        &target.song_id,
        &query.artist,
        &query.title,
        &query.album,
        query.duration_seconds,
    ) else {
        return SyncLyrics::Unavailable;
    };
    let response = super::LYRICS_HTTP_CLIENT
        .get(&url)
        .timeout(Duration::from_secs(timeout_seconds as u64))
        .send()
        .await;
    let Ok(response) = response else {
        return SyncLyrics::Unavailable;
    };
    if !response.status().is_success() {
        return SyncLyrics::Unavailable;
    }
    let Ok(body) = response.text().await else {
        return SyncLyrics::Unavailable;
    };
    let result = parse(&body);
    if let SyncLyrics::Found(found) = &result {
        if found.provider == LyricsProvider::Server && !found.synced_lines.is_empty() {
            let client = NavidromeClient::new(target.server.clone());
            let enhanced = client
                .get_open_subsonic_extensions(timeout_seconds)
                .await
                .map(|extensions| server_lyrics_support(&extensions) == ServerLyricsSupport::Enhanced)
                .unwrap_or(false);
            if enhanced {
                return SyncLyrics::Unavailable;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_lyrics_keep_their_times_and_gaps_drop_out() {
        let body = r#"{"found":true,"source":"lrclib","synced":true,"instrumental":false,"lang":null,"offset_ms":0,
            "lines":[{"start_ms":1000,"text":"First line"},{"start_ms":3500,"text":"Second line"},{"start_ms":7200,"text":""}]}"#;
        let SyncLyrics::Found(result) = parse(body) else { panic!() };
        assert_eq!(result.provider, LyricsProvider::Lrclib);
        assert_eq!(result.synced_lines.len(), 2);
        assert_eq!(result.synced_lines[1].timestamp_seconds, 3.5);
        assert_eq!(result.plain_lyrics, "First line\nSecond line");
    }

    #[test]
    fn the_servers_offset_moves_lines_sooner() {
        let body = r#"{"found":true,"source":"server","synced":true,"offset_ms":500,"lines":[{"start_ms":2000,"text":"a"}]}"#;
        let SyncLyrics::Found(result) = parse(body) else { panic!() };
        assert_eq!(result.provider, LyricsProvider::Server);
        assert_eq!(result.synced_lines[0].timestamp_seconds, 1.5);
    }

    #[test]
    fn unsynced_lyrics_are_plain() {
        let body = r#"{"found":true,"source":"server","synced":false,"lines":[{"text":"one"},{"text":"two"}]}"#;
        let SyncLyrics::Found(result) = parse(body) else { panic!() };
        assert!(result.synced_lines.is_empty());
        assert_eq!(result.plain_lyrics, "one\ntwo");
    }

    #[test]
    fn nothing_found_and_instrumental_skip_those_providers() {
        assert_eq!(parse(r#"{"found":false,"source":null,"lines":[]}"#), SyncLyrics::NotFound);
        assert_eq!(parse(r#"{"found":true,"source":"lrclib","instrumental":true,"lines":[]}"#), SyncLyrics::NotFound);
        assert_eq!(parse("not json"), SyncLyrics::Unavailable);
    }
}
