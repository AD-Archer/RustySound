//! What a speaker is sent for a song. Speakers fetch the audio themselves,
//! so the URL has to work from the speaker:
//!
//! * Server songs: the server's stream URL, token and all (the same URL
//!   `<audio>` gets, now seen by the speaker too). A format the speaker
//!   doesn't list (`GetProtocolInfo`; Cast's known formats) is transcoded
//!   by the server to 320 kbps MP3. Downloads cast from the server as well:
//!   the speaker can't read this device's cache.
//! * Radio: the station's URL.
//! * Music "On This Device": the file, served by `lan_server.rs`.

use crate::api::{MusicClient, ServerConfig, Song};
use std::path::PathBuf;

/// Where the speaker gets the audio.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaSource {
    Url(String),
    /// A local file, served over the LAN while it plays.
    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CastMedia {
    pub source: MediaSource,
    pub mime: String,
    /// The server converts it (the original format wasn't playable).
    pub transcoded: bool,
}

/// The bitrate asked for when the server transcodes.
pub const TRANSCODE_KBPS: u32 = 320;

/// The MIME type for a file extension or Subsonic `suffix`.
pub fn mime_for_suffix(suffix: &str) -> &'static str {
    match suffix.trim().trim_start_matches('.').to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "m4a" | "m4b" | "mp4" | "alac" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "wma" => "audio/x-ms-wma",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}

/// One spelling per format, so `audio/x-flac` matches `audio/flac`.
pub fn normalize_mime(mime: &str) -> String {
    let mime = mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    match mime.as_str() {
        "audio/mp3" | "audio/x-mp3" | "audio/mpeg3" | "audio/x-mpeg" => "audio/mpeg".to_string(),
        "audio/x-flac" => "audio/flac".to_string(),
        "audio/x-m4a" | "audio/m4a" | "audio/x-mp4" | "audio/mp4a-latm" => "audio/mp4".to_string(),
        "audio/x-aac" | "audio/aacp" => "audio/aac".to_string(),
        "audio/x-wav" | "audio/wave" | "audio/vnd.wave" | "audio/l16" => "audio/wav".to_string(),
        "audio/x-aiff" => "audio/aiff".to_string(),
        "application/ogg" | "audio/x-ogg" | "audio/vorbis" => "audio/ogg".to_string(),
        _ => mime,
    }
}

/// A song's MIME type: its `contentType`, else from its suffix.
pub fn song_mime(song: &Song) -> String {
    song.content_type
        .as_deref()
        .filter(|mime| mime.contains('/'))
        .map(normalize_mime)
        .or_else(|| song.suffix.as_deref().map(|suffix| mime_for_suffix(suffix).to_string()))
        .unwrap_or_else(|| "audio/mpeg".to_string())
}

/// Whether a UPnP speaker plays `mime`, from its `GetProtocolInfo` sinks.
/// A speaker that lists nothing gets MP3 only, which every renderer plays.
pub fn sinks_play(sinks: &[String], mime: &str) -> bool {
    let mime = normalize_mime(mime);
    if sinks.is_empty() {
        return mime == "audio/mpeg";
    }
    sinks.iter().any(|sink| sink == "*" || sink == "audio/*" || normalize_mime(sink) == mime)
}

/// The Default Media Receiver's audio formats.
pub fn cast_plays(mime: &str) -> bool {
    matches!(
        normalize_mime(mime).as_str(),
        "audio/mpeg" | "audio/mp4" | "audio/aac" | "audio/flac" | "audio/ogg" | "audio/opus" | "audio/wav" | "audio/webm"
    )
}

/// A stream URL asking the server for MP3: Subsonic's `format` and
/// `maxBitRate`; Jellyfin's universal endpoint offered only `mp3`, so it
/// transcodes to its `transcodingContainer` (MP3).
pub fn transcoded_url(url: &str, jellyfin: bool) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return url.to_string();
    };
    let replaced: Vec<(&str, String)> = if jellyfin {
        vec![("container", "mp3".to_string())]
    } else {
        vec![("format", "mp3".to_string()), ("maxBitRate", TRANSCODE_KBPS.to_string())]
    };
    let kept: Vec<(String, String)> = parsed
        .query_pairs()
        .filter(|(key, _)| !replaced.iter().any(|(name, _)| key.eq_ignore_ascii_case(name)))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    {
        let mut query = parsed.query_pairs_mut();
        query.clear();
        for (key, value) in &kept {
            query.append_pair(key, value);
        }
        for (key, value) in &replaced {
            query.append_pair(key, value);
        }
    }
    parsed.to_string()
}

/// The media for `song` on a speaker that plays what `plays` says, or
/// `None` when there's nothing it can reach.
pub fn cast_media(song: &Song, servers: &[ServerConfig], plays: impl Fn(&str) -> bool) -> Option<CastMedia> {
    if song.server_name == "Radio" {
        let url = song.stream_url.clone().filter(|url| !url.trim().is_empty())?;
        let mime = song.content_type.as_deref().map(normalize_mime).unwrap_or_else(|| "audio/mpeg".to_string());
        return Some(CastMedia { source: MediaSource::Url(url), mime, transcoded: false });
    }
    if crate::local_music::is_local_song(song) {
        let path = crate::local_music::song_file(&song.id)?;
        let mime = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(mime_for_suffix)
            .unwrap_or("audio/mpeg")
            .to_string();
        return Some(CastMedia { source: MediaSource::File(path), mime, transcoded: false });
    }
    let server = servers.iter().find(|server| server.id == song.server_id)?;
    let url = MusicClient::new(server.clone()).get_stream_url(song.id.trim());
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let mime = song_mime(song);
    if plays(&mime) {
        return Some(CastMedia { source: MediaSource::Url(url), mime, transcoded: false });
    }
    Some(CastMedia {
        source: MediaSource::Url(transcoded_url(&url, server.is_jellyfin())),
        mime: "audio/mpeg".to_string(),
        transcoded: true,
    })
}
