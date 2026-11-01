//! The pure half of the local library: stable ids, the tag record of one file
//! and how it maps onto the app's `Song`, which formats the player decodes,
//! the incremental-rescan decision, and de-duplication against server copies.
//! Compiled on every target, so the web build and the tests share it.

use crate::api::{Album, Artist, ReplayGain, SearchResult, Song};
use std::collections::HashSet;

/// `server_id` of everything on the device. Never a real server's id
/// (those are UUIDs), and never "Radio" (see `is_radio_song`).
pub const LOCAL_SOURCE_ID: &str = "local";
/// `server_name` of local songs, shown where the UI names a song's source.
pub const LOCAL_SOURCE_NAME: &str = "On This Device";

/// File extensions the scanner reads (every format lofty parses).
const INDEXED_EXTENSIONS: [&str; 17] = [
    "mp3", "m4a", "m4b", "mp4", "aac", "alac", "flac", "ogg", "oga", "opus", "spx", "wav",
    "aif", "aiff", "ape", "mpc", "wv",
];

/// Tags and file facts of one indexed file. `folder_id` + `rel_path` locate
/// it; everything else comes from lofty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackRecord {
    pub id: String,
    pub folder_id: String,
    pub rel_path: String,
    pub mtime: i64,
    pub size: u64,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub duration: u32,
    pub bitrate: Option<u32>,
    pub suffix: String,
    pub content_type: String,
    pub art_id: Option<String>,
    pub lyrics: Option<String>,
    pub mbid: Option<String>,
    /// ReplayGain tags, for volume levelling like a server's `replayGain`.
    pub replay_gain: Option<ReplayGain>,
}

/// What the user set on a local item: favourite, rating, plays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marks {
    pub starred: Option<String>,
    pub rating: Option<u32>,
    pub play_count: u32,
    pub played: Option<String>,
}

impl TrackRecord {
    /// The artist albums are grouped under: album artist, else track artist.
    pub fn grouping_artist(&self) -> &str {
        self.album_artist
            .as_deref()
            .or(self.artist.as_deref())
            .unwrap_or(UNKNOWN_ARTIST)
    }

    pub fn album_name(&self) -> &str {
        self.album.as_deref().unwrap_or(UNKNOWN_ALBUM)
    }

    pub fn album_id(&self) -> String {
        album_id(self.grouping_artist(), self.album_name())
    }

    pub fn album_artist_id(&self) -> String {
        artist_id(self.grouping_artist())
    }

    pub fn artist_id(&self) -> String {
        artist_id(self.artist.as_deref().unwrap_or(UNKNOWN_ARTIST))
    }

    /// The `Song` the rest of the app plays and lists. `stream_url` stays
    /// empty: the player resolves it per platform (`local_music::stream_url`).
    pub fn to_song(&self, marks: &Marks) -> Song {
        Song {
            id: self.id.clone(),
            title: self.title.clone(),
            album: Some(self.album_name().to_string()),
            album_id: Some(self.album_id()),
            artist: Some(self.artist.clone().unwrap_or_else(|| UNKNOWN_ARTIST.to_string())),
            artist_id: Some(self.artist_id()),
            duration: self.duration,
            track: self.track,
            cover_art: self.art_id.clone(),
            content_type: Some(self.content_type.clone()),
            stream_url: None,
            suffix: Some(self.suffix.clone()),
            bitrate: self.bitrate,
            starred: marks.starred.clone(),
            user_rating: marks.rating.filter(|r| *r > 0),
            play_count: Some(marks.play_count),
            played: marks.played.clone(),
            year: self.year,
            genre: self.genre.clone(),
            server_id: LOCAL_SOURCE_ID.to_string(),
            server_name: LOCAL_SOURCE_NAME.to_string(),
            replay_gain: self.replay_gain.clone(),
            queue_meta: None,
        }
    }
}

/// A ReplayGain tag value: "-6.54 dB" or "0.988" → the number.
pub fn parse_gain(text: &str) -> Option<f64> {
    let number: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        .collect();
    number.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The record's ReplayGain from its four tags; `None` when it has none.
pub fn replay_gain_from_tags(
    track_gain: Option<&str>,
    album_gain: Option<&str>,
    track_peak: Option<&str>,
    album_peak: Option<&str>,
) -> Option<ReplayGain> {
    let gain = ReplayGain {
        track_gain: track_gain.and_then(parse_gain),
        album_gain: album_gain.and_then(parse_gain),
        track_peak: track_peak.and_then(parse_gain),
        album_peak: album_peak.and_then(parse_gain),
        ..Default::default()
    };
    (gain.track_gain.is_some() || gain.album_gain.is_some()).then_some(gain)
}

pub const UNKNOWN_ARTIST: &str = "Unknown Artist";
pub const UNKNOWN_ALBUM: &str = "Unknown Album";

pub fn is_local(server_id: &str) -> bool {
    server_id == LOCAL_SOURCE_ID
}

pub fn is_local_song(song: &Song) -> bool {
    is_local(&song.server_id)
}

// ---- Ids ----------------------------------------------------------------------
//
// Ids are short md5 digests, so they survive rescans and app updates (std's
// hasher makes no such promise) and use only `[a-z0-9-]`, which the cover
// cache and the audio route accept as file names.

fn short_hash(input: &[u8]) -> String {
    let digest = format!("{:x}", md5::compute(input));
    digest[..16].to_string()
}

/// Lower-case, trimmed, single-spaced: "The  Beatles " and "the beatles" group together.
pub fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn folder_id(path: &str) -> String {
    format!("lf-{}", short_hash(path.as_bytes()))
}

/// A song's id: its folder plus its path inside it, so moving the folder's
/// files around changes it but rescanning or retagging doesn't.
pub fn song_id(folder_id: &str, rel_path: &str) -> String {
    format!("lt-{}", short_hash(format!("{folder_id}/{rel_path}").as_bytes()))
}

pub fn album_id(artist: &str, album: &str) -> String {
    let key = format!("{}\u{1f}{}", normalize(artist), normalize(album));
    format!("la-{}", short_hash(key.as_bytes()))
}

pub fn artist_id(name: &str) -> String {
    format!("lr-{}", short_hash(normalize(name).as_bytes()))
}

/// Embedded pictures are stored once per distinct image.
pub fn art_id(bytes: &[u8]) -> String {
    format!("lc-{}", short_hash(bytes))
}

/// The cover cache's file name for a local picture, in `offline_art`'s
/// `<source>__<cover>__<size>.img` scheme. Size 0: the picture as embedded.
pub fn art_file_name(art_id: &str) -> String {
    format!("{LOCAL_SOURCE_ID}__{art_id}__0.img")
}

/// Ids the scanner makes are `l?-` plus hex; anything else from a URL is refused.
pub fn is_local_id(id: &str) -> bool {
    id.len() == 19
        && id.starts_with('l')
        && id.as_bytes()[2] == b'-'
        && id[3..].chars().all(|c| c.is_ascii_hexdigit())
}

/// A picture id (`lc-…`). Albums fall back to their own id for a cover,
/// which locally names no picture.
pub fn is_art_id(id: &str) -> bool {
    id.starts_with("lc-") && is_local_id(id)
}

// ---- Files ----------------------------------------------------------------------

pub fn is_indexed_extension(ext: &str) -> bool {
    let ext = ext.to_ascii_lowercase();
    INDEXED_EXTENSIONS.contains(&ext.as_str())
}

pub fn content_type_for(suffix: &str) -> &'static str {
    match suffix.to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "m4a" | "m4b" | "mp4" | "alac" => "audio/mp4",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" | "oga" | "spx" => "audio/ogg",
        "opus" => "audio/ogg; codecs=opus",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "ape" => "audio/x-ape",
        "mpc" => "audio/x-musepack",
        "wv" => "audio/x-wavpack",
        _ => "application/octet-stream",
    }
}

/// Whether a file must be read again: new, or its mtime or size changed.
/// Unchanged files cost one `stat` per rescan.
pub fn needs_rescan(known: Option<(i64, u64)>, mtime: i64, size: u64) -> bool {
    known != Some((mtime, size))
}

// ---- Formats the player decodes ----------------------------------------------

/// Which engine plays local files on this build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    /// The Mac app's webview `<audio>` (WebKit).
    WebKit,
    /// AVPlayer on iPhone.
    AvPlayer,
}

pub fn current_player() -> Player {
    if cfg!(target_os = "ios") {
        Player::AvPlayer
    } else {
        Player::WebKit
    }
}

/// Ogg Vorbis and Opus decode in WebKit only from macOS 15.4 (Safari 18.4).
/// The Mac app asks its webview once (`canPlayType`) and records the answer.
static WEBKIT_PLAYS_OGG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg_attr(target_os = "ios", allow(dead_code))]
pub fn set_webkit_plays_ogg(yes: bool) {
    WEBKIT_PLAYS_OGG.store(yes, std::sync::atomic::Ordering::Relaxed);
}

/// Whether `player` can decode files with this extension. WMA, APE, WavPack,
/// Musepack and DSD play nowhere; Ogg depends on the WebKit version.
pub fn playable_on(player: Player, suffix: &str, webkit_ogg: bool) -> bool {
    match suffix.to_ascii_lowercase().as_str() {
        "mp3" | "m4a" | "m4b" | "mp4" | "aac" | "alac" | "wav" | "aif" | "aiff" | "flac" => true,
        "ogg" | "oga" | "opus" => player == Player::WebKit && webkit_ogg,
        _ => false,
    }
}

/// False only for local songs this device can't decode; server songs are
/// transcoded by the server when needed.
pub fn is_playable(song: &Song) -> bool {
    if !is_local_song(song) {
        return true;
    }
    let suffix = song.suffix.as_deref().unwrap_or_default();
    playable_on(
        current_player(),
        suffix,
        WEBKIT_PLAYS_OGG.load(std::sync::atomic::Ordering::Relaxed),
    )
}

pub const NOT_PLAYABLE: &str = "Not playable on this device";

// ---- De-duplication ------------------------------------------------------------

/// What identifies a recording across sources: MusicBrainz id when both
/// sides have one, else title + artist + duration within two seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub mbid: Option<String>,
    pub title: String,
    pub artist: String,
    pub duration: u32,
}

impl Fingerprint {
    pub fn of(song: &Song) -> Self {
        Self {
            mbid: None,
            title: normalize(&song.title),
            artist: normalize(song.artist.as_deref().unwrap_or_default()),
            duration: song.duration,
        }
    }

    pub fn same_recording(&self, other: &Fingerprint) -> bool {
        if let (Some(a), Some(b)) = (self.mbid.as_deref(), other.mbid.as_deref()) {
            if !a.is_empty() && !b.is_empty() {
                return a.eq_ignore_ascii_case(b);
            }
        }
        !self.title.is_empty()
            && self.title == other.title
            && self.artist == other.artist
            && self.duration.abs_diff(other.duration) <= 2
    }
}

/// Drops local songs that a server in the same list also has, keeping the
/// server copy (it syncs plays, favourites and the queue).
pub fn dedupe_songs(songs: &mut Vec<Song>) {
    if !songs.iter().any(is_local_song) {
        return;
    }
    let server: Vec<Fingerprint> = songs
        .iter()
        .filter(|s| !is_local_song(s))
        .map(Fingerprint::of)
        .collect();
    if server.is_empty() {
        return;
    }
    songs.retain(|song| {
        if !is_local_song(song) {
            return true;
        }
        let local = Fingerprint::of(song);
        !server.iter().any(|other| other.same_recording(&local))
    });
}

/// Drops local albums a server also has (same name and artist).
pub fn dedupe_albums(albums: &mut Vec<Album>) {
    let server: HashSet<(String, String)> = albums
        .iter()
        .filter(|a| !is_local(&a.server_id))
        .map(|a| (normalize(&a.name), normalize(&a.artist)))
        .collect();
    if server.is_empty() {
        return;
    }
    albums.retain(|a| {
        !is_local(&a.server_id) || !server.contains(&(normalize(&a.name), normalize(&a.artist)))
    });
}

/// Drops local artists a server also has (same name).
pub fn dedupe_artists(artists: &mut Vec<Artist>) {
    let server: HashSet<String> = artists
        .iter()
        .filter(|a| !is_local(&a.server_id))
        .map(|a| normalize(&a.name))
        .collect();
    if server.is_empty() {
        return;
    }
    artists.retain(|a| !is_local(&a.server_id) || !server.contains(&normalize(&a.name)));
}

pub fn dedupe_search(result: &mut SearchResult) {
    dedupe_artists(&mut result.artists);
    dedupe_albums(&mut result.albums);
    dedupe_songs(&mut result.songs);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> TrackRecord {
        TrackRecord {
            id: song_id("lf-1", "Abbey Road/01 Come Together.flac"),
            folder_id: "lf-1".into(),
            rel_path: "Abbey Road/01 Come Together.flac".into(),
            mtime: 10,
            size: 20,
            title: "Come Together".into(),
            artist: Some("The Beatles".into()),
            album_artist: None,
            album: Some("Abbey Road".into()),
            track: Some(1),
            disc: Some(1),
            year: Some(1969),
            genre: Some("Rock".into()),
            duration: 259,
            bitrate: Some(900),
            suffix: "flac".into(),
            content_type: content_type_for("flac").into(),
            art_id: Some("lc-abc".into()),
            lyrics: None,
            mbid: None,
            replay_gain: None,
        }
    }

    fn server_song(title: &str, artist: &str, duration: u32) -> Song {
        Song {
            id: "s1".into(),
            title: title.into(),
            artist: Some(artist.into()),
            duration,
            server_id: "srv".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_record_maps_onto_a_local_song() {
        let marks = Marks { starred: Some("2026-10-31T00:00:00Z".into()), rating: Some(4), play_count: 3, played: None };
        let song = record().to_song(&marks);
        assert_eq!(song.server_id, LOCAL_SOURCE_ID);
        assert_eq!(song.server_name, LOCAL_SOURCE_NAME);
        assert_ne!(song.server_name, "Radio");
        assert_eq!(song.title, "Come Together");
        assert_eq!(song.album.as_deref(), Some("Abbey Road"));
        assert_eq!(song.album_id, Some(album_id("The Beatles", "Abbey Road")));
        assert_eq!(song.artist_id, Some(artist_id("The Beatles")));
        assert_eq!(song.track, Some(1));
        assert_eq!(song.year, Some(1969));
        assert_eq!(song.duration, 259);
        assert_eq!(song.suffix.as_deref(), Some("flac"));
        assert_eq!(song.content_type.as_deref(), Some("audio/flac"));
        assert_eq!(song.cover_art.as_deref(), Some("lc-abc"));
        assert_eq!(song.user_rating, Some(4));
        assert_eq!(song.play_count, Some(3));
        assert!(song.starred.is_some());
        assert!(song.stream_url.is_none());
    }

    #[test]
    fn replay_gain_tags_are_read() {
        assert_eq!(parse_gain("-6.54 dB"), Some(-6.54));
        assert_eq!(parse_gain("+1.20 dB"), Some(1.2));
        assert_eq!(parse_gain("0.988547"), Some(0.988547));
        assert_eq!(parse_gain("loud"), None);
        let gain = replay_gain_from_tags(Some("-7.1 dB"), None, Some("0.99"), None).unwrap();
        assert_eq!(gain.track_gain, Some(-7.1));
        assert_eq!(gain.track_peak, Some(0.99));
        assert_eq!(gain.album_gain, None);
        // Peaks alone don't level anything.
        assert!(replay_gain_from_tags(None, None, Some("0.9"), None).is_none());
        let mut rec = record();
        rec.replay_gain = Some(gain.clone());
        assert_eq!(rec.to_song(&Marks::default()).replay_gain, Some(gain));
    }

    #[test]
    fn missing_tags_fall_back_to_unknowns() {
        let mut rec = record();
        rec.artist = None;
        rec.album = None;
        let song = rec.to_song(&Marks::default());
        assert_eq!(song.artist.as_deref(), Some(UNKNOWN_ARTIST));
        assert_eq!(song.album.as_deref(), Some(UNKNOWN_ALBUM));
        assert_eq!(song.user_rating, None);
    }

    #[test]
    fn album_artist_groups_compilations() {
        let mut a = record();
        a.artist = Some("Artist A".into());
        a.album_artist = Some("Various Artists".into());
        let mut b = record();
        b.artist = Some("Artist B".into());
        b.album_artist = Some("various  artists".into());
        assert_eq!(a.album_id(), b.album_id());
        assert_ne!(a.artist_id(), b.artist_id());
        assert_eq!(a.album_artist_id(), artist_id("Various Artists"));
    }

    #[test]
    fn ids_are_stable_and_safe() {
        let a = song_id("lf-1", "x/y.mp3");
        assert_eq!(a, song_id("lf-1", "x/y.mp3"));
        assert_ne!(a, song_id("lf-2", "x/y.mp3"));
        assert_ne!(a, song_id("lf-1", "x/z.mp3"));
        // Known value: changing the hash would orphan favourites and ratings.
        assert_eq!(folder_id("/Music"), format!("lf-{}", &format!("{:x}", md5::compute(b"/Music"))[..16]));
        for id in [a, folder_id("/Users/me/Music"), album_id("A", "B"), artist_id("A"), art_id(b"img")] {
            assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{id}");
            assert_eq!(id.len(), 19);
        }
        assert_eq!(album_id("The Beatles", "Abbey Road"), album_id(" the beatles", "ABBEY  road"));
        assert!(is_local_id(&song_id("lf-1", "a.mp3")));
        assert!(is_local_id(&art_id(b"x")));
        assert!(!is_local_id("lt-../../etc/pass"));
        assert!(!is_local_id("al-0123456789abcdef0"));
        assert_eq!(art_file_name("lc-0123456789abcdef"), "local__lc-0123456789abcdef__0.img");
    }

    #[test]
    fn rescan_only_new_or_changed_files() {
        assert!(needs_rescan(None, 1, 2));
        assert!(!needs_rescan(Some((1, 2)), 1, 2));
        assert!(needs_rescan(Some((1, 2)), 5, 2));
        assert!(needs_rescan(Some((1, 2)), 1, 3));
    }

    #[test]
    fn formats_per_player() {
        for ext in ["mp3", "M4A", "flac", "wav", "aiff"] {
            assert!(playable_on(Player::WebKit, ext, false), "{ext}");
            assert!(playable_on(Player::AvPlayer, ext, false), "{ext}");
        }
        assert!(!playable_on(Player::WebKit, "ogg", false));
        assert!(playable_on(Player::WebKit, "ogg", true));
        assert!(playable_on(Player::WebKit, "opus", true));
        assert!(!playable_on(Player::AvPlayer, "ogg", true));
        for ext in ["wma", "ape", "wv", "mpc", "dsf"] {
            assert!(!playable_on(Player::WebKit, ext, true), "{ext}");
        }
        // Server songs are always playable (the server transcodes).
        let mut song = server_song("x", "y", 1);
        song.suffix = Some("wma".into());
        assert!(is_playable(&song));
        let mut local = record().to_song(&Marks::default());
        local.suffix = Some("wv".into());
        assert!(!is_playable(&local));
    }

    #[test]
    fn local_copies_of_server_songs_are_dropped() {
        let local = record().to_song(&Marks::default());
        let mut other_local = local.clone();
        other_local.id = "lt-other".into();
        other_local.title = "Something".into();
        let mut list = vec![
            local.clone(),
            server_song("come together", "the beatles", 261),
            other_local.clone(),
        ];
        dedupe_songs(&mut list);
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|s| s.id != local.id));
        // Beyond two seconds it's another recording.
        let mut list = vec![local.clone(), server_song("Come Together", "The Beatles", 262)];
        dedupe_songs(&mut list);
        assert_eq!(list.len(), 2);
        // Only local songs: nothing to compare against.
        let mut list = vec![local.clone(), other_local];
        dedupe_songs(&mut list);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn mbids_win_when_both_sides_have_them() {
        let mut a = Fingerprint::of(&server_song("Live", "X", 100));
        let mut b = Fingerprint::of(&server_song("Live", "X", 100));
        a.mbid = Some("AAA".into());
        b.mbid = Some("bbb".into());
        assert!(!a.same_recording(&b));
        b.mbid = Some("aaa".into());
        assert!(a.same_recording(&b));
        b.mbid = None;
        assert!(a.same_recording(&b));
    }

    #[test]
    fn albums_and_artists_dedupe_by_name() {
        let mut albums = vec![
            Album { id: "la-1".into(), name: "Abbey Road".into(), artist: "The Beatles".into(), server_id: LOCAL_SOURCE_ID.into(), ..Default::default() },
            Album { id: "al-1".into(), name: "abbey road".into(), artist: "the beatles".into(), server_id: "srv".into(), ..Default::default() },
            Album { id: "la-2".into(), name: "Demos".into(), artist: "The Beatles".into(), server_id: LOCAL_SOURCE_ID.into(), ..Default::default() },
        ];
        dedupe_albums(&mut albums);
        assert_eq!(albums.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), vec!["al-1", "la-2"]);
        let mut artists = vec![
            Artist { id: "lr-1".into(), name: "The Beatles".into(), server_id: LOCAL_SOURCE_ID.into(), ..Default::default() },
            Artist { id: "ar-1".into(), name: "The Beatles".into(), server_id: "srv".into(), ..Default::default() },
        ];
        dedupe_artists(&mut artists);
        assert_eq!(artists.len(), 1);
        assert_eq!(artists[0].server_id, "srv");
    }
}
