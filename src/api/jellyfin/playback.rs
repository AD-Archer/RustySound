// Media URLs (streams, covers), playback reporting and lyrics.

use super::dto::{ms_to_ticks, LyricDto};
use super::{with_query, JellyfinClient};
use crate::api::PlaybackReportState;
#[cfg(not(target_arch = "wasm32"))]
use crate::cache_service::is_offline_mode;
#[cfg(not(target_arch = "wasm32"))]
use crate::offline_art::{cached_cover_art_data_url, maybe_prefetch_cover_art, smaller_cached_cover_art_data_url};

/// Largest cover edge asked for, as for Subsonic servers.
const COVER_ART_MAX_PX: u32 = 1200;
/// Above this, a file is transcoded instead of played as is. High enough
/// that lossless files play untouched on a home network.
const MAX_STREAMING_BITRATE: u32 = 140_000_000;
/// The bitrate of a transcode (MP3), when one is needed.
const TRANSCODE_BITRATE: u32 = 320_000;

/// The containers this build plays directly (`container|codec` where the
/// container alone isn't enough). Anything else is transcoded to MP3.
/// Browsers: what both Chromium and Safari play. Apple platforms add ALAC.
pub fn stream_containers() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "mp3,aac,m4a|aac,m4b|aac,flac,wav"
    } else if cfg!(any(target_os = "ios", target_os = "macos")) {
        "mp3,aac,m4a|aac,m4a|alac,m4b|aac,alac,flac,wav,aiff"
    } else {
        "mp3,aac,m4a|aac,m4b|aac,flac,wav,ogg,opus"
    }
}

/// `/Audio/{id}/universal`: plays the file directly when the device can,
/// else transcodes to MP3 over plain HTTP (which every `<audio>` and
/// AVPlayer streams). Pure; used by tests.
pub fn build_stream_url(base: &str, song_id: &str, user_id: &str, device_id: &str, token: &str, containers: &str) -> String {
    with_query(
        base,
        &format!("/Audio/{}/universal", urlencoding::encode(song_id)),
        &[
            ("userId", user_id.to_string()),
            ("deviceId", device_id.to_string()),
            ("container", containers.to_string()),
            ("transcodingContainer", "mp3".to_string()),
            ("transcodingProtocol", "http".to_string()),
            ("audioCodec", "mp3".to_string()),
            ("maxStreamingBitrate", MAX_STREAMING_BITRATE.to_string()),
            ("audioBitRate", TRANSCODE_BITRATE.to_string()),
            ("ApiKey", token.to_string()),
        ],
    )
}

/// The path and body of a playback report (`PlaybackStartInfo`,
/// `PlaybackProgressInfo` or `PlaybackStopInfo`). The play-session id, made
/// from the device and song, ties one song's reports together. Pure; used
/// by tests.
pub fn playback_report(device_id: &str, song_id: &str, state: PlaybackReportState, position_ms: u64) -> (&'static str, serde_json::Value) {
    let session = format!("{:x}", md5::compute(format!("{device_id}:{song_id}")));
    let ticks = ms_to_ticks(position_ms);
    match state {
        PlaybackReportState::Starting => (
            "/Sessions/Playing",
            serde_json::json!({ "ItemId": song_id, "PositionTicks": ticks, "IsPaused": false, "CanSeek": true, "PlaySessionId": session }),
        ),
        PlaybackReportState::Playing | PlaybackReportState::Paused => (
            "/Sessions/Playing/Progress",
            serde_json::json!({
                "ItemId": song_id,
                "PositionTicks": ticks,
                "IsPaused": state == PlaybackReportState::Paused,
                "CanSeek": true,
                "PlaySessionId": session,
            }),
        ),
        PlaybackReportState::Stopped => (
            "/Sessions/Playing/Stopped",
            serde_json::json!({ "ItemId": song_id, "PositionTicks": ticks, "PlaySessionId": session }),
        ),
    }
}

impl JellyfinClient {
    pub fn get_stream_url(&self, song_id: &str) -> String {
        build_stream_url(
            &self.server.url,
            song_id,
            &self.server.user_id,
            &self.device_id(),
            &self.server.access_token,
            stream_containers(),
        )
    }

    /// A square cover at `size` px. Images need no token.
    /// As `NavidromeClient::get_cover_art_network_url`.
    pub fn get_cover_art_network_url(&self, item_id: &str, size: u32) -> String {
        self.build_cover_art_network_url(item_id.trim(), size.min(COVER_ART_MAX_PX))
    }

    fn build_cover_art_network_url(&self, item_id: &str, size: u32) -> String {
        with_query(
            &self.server.url,
            &format!("/Items/{}/Images/Primary", urlencoding::encode(item_id)),
            &[
                ("fillWidth", size.to_string()),
                ("fillHeight", size.to_string()),
                ("quality", "90".to_string()),
            ],
        )
    }

    /// As `NavidromeClient::get_cover_art_url`: the cached copy on native
    /// builds when there is one, and the cache warms in the background.
    pub fn get_cover_art_url(&self, item_id: &str, size: u32) -> String {
        let size = size.min(COVER_ART_MAX_PX);
        let item_id = item_id.trim();
        if item_id.is_empty() {
            return String::new();
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(data_url) = cached_cover_art_data_url(&self.server.id, item_id, size) {
                return data_url;
            }
            if is_offline_mode() {
                return smaller_cached_cover_art_data_url(&self.server.id, item_id, size, 0).unwrap_or_default();
            }
        }
        let remote_url = self.build_cover_art_network_url(item_id, size);
        #[cfg(not(target_arch = "wasm32"))]
        {
            maybe_prefetch_cover_art(self.server.id.clone(), item_id.to_string(), size, remote_url.clone());
            // As Navidrome's: a saved copy at least half the size meanwhile.
            if let Some(data_url) = smaller_cached_cover_art_data_url(&self.server.id, item_id, size, size / 2) {
                return data_url;
            }
        }
        remote_url
    }

    /// As `NavidromeClient::get_cover_art_display_url`: native webviews get
    /// the local cover route, the web build the server URL.
    pub fn get_cover_art_display_url(&self, item_id: &str, size: u32) -> String {
        #[cfg(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile")))]
        {
            let size = size.min(COVER_ART_MAX_PX);
            let item_id = item_id.trim();
            if item_id.is_empty() {
                return String::new();
            }
            let remote_url = self.build_cover_art_network_url(item_id, size);
            crate::offline_art::cover_art_display_url(&self.server.id, item_id, size, &remote_url)
        }
        #[cfg(not(all(not(target_arch = "wasm32"), any(feature = "desktop", feature = "mobile"))))]
        {
            self.get_cover_art_url(item_id, size)
        }
    }

    /// Reports playback to `/Sessions/Playing*`, which is how Jellyfin shows
    /// "now playing" and counts plays (on `Stopped`, once enough was heard).
    pub async fn report_playback(&self, song_id: &str, state: PlaybackReportState, position_ms: u64) -> Result<(), String> {
        let (path, body) = playback_report(&self.device_id(), song_id, state, position_ms);
        self.send(reqwest::Method::POST, path, &[], Some(body)).await.map(|_| ())
    }

    /// The song's lyrics, `None` when it has none (404).
    pub async fn get_lyrics(&self, song_id: &str) -> Result<Option<LyricDto>, String> {
        let path = format!("/Audio/{}/Lyrics", urlencoding::encode(song_id));
        match self.get::<LyricDto>(&path, &[]).await {
            Ok(lyrics) => Ok(Some(lyrics)),
            Err(error) if error.starts_with("404") => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stream_url_lists_containers_and_carries_the_token() {
        let url = build_stream_url("http://jf:8096/", "abc", "u1", "dev", "tok", "mp3,m4a|aac,flac");
        assert!(url.starts_with("http://jf:8096/Audio/abc/universal?userId=u1&deviceId=dev&"), "{url}");
        assert!(url.contains("container=mp3%2Cm4a%7Caac%2Cflac"), "{url}");
        assert!(url.contains("transcodingContainer=mp3&transcodingProtocol=http&audioCodec=mp3"), "{url}");
        assert!(url.ends_with("&ApiKey=tok"), "{url}");
        assert!(!url.contains("api_key"), "legacy auth is off on Jellyfin 12");
    }

    #[test]
    fn playback_reports_map_states_to_session_endpoints() {
        let (path, start) = playback_report("dev", "song", PlaybackReportState::Starting, 1_500);
        assert_eq!(path, "/Sessions/Playing");
        assert_eq!(start["ItemId"], "song");
        assert_eq!(start["PositionTicks"], 15_000_000);
        assert_eq!(start["IsPaused"], false);

        let (path, paused) = playback_report("dev", "song", PlaybackReportState::Paused, 60_000);
        assert_eq!(path, "/Sessions/Playing/Progress");
        assert_eq!(paused["IsPaused"], true);
        assert_eq!(paused["PositionTicks"], 600_000_000);
        let (path, playing) = playback_report("dev", "song", PlaybackReportState::Playing, 0);
        assert_eq!(path, "/Sessions/Playing/Progress");
        assert_eq!(playing["IsPaused"], false);

        let (path, stop) = playback_report("dev", "song", PlaybackReportState::Stopped, 200_000);
        assert_eq!(path, "/Sessions/Playing/Stopped");
        assert!(stop.get("IsPaused").is_none());
        // One song's reports share a play session; another song gets its own.
        assert_eq!(start["PlaySessionId"], stop["PlaySessionId"]);
        let (_, other) = playback_report("dev", "other", PlaybackReportState::Starting, 0);
        assert_ne!(start["PlaySessionId"], other["PlaySessionId"]);
    }

    #[test]
    fn covers_are_square_and_need_no_token() {
        let server = crate::api::ServerConfig::jellyfin("J".into(), "http://jf".into(), "u".into(), "uid".into(), "tok".into());
        let url = JellyfinClient::new(server).build_cover_art_network_url("al 1", 300);
        assert_eq!(url, "http://jf/Items/al%201/Images/Primary?fillWidth=300&fillHeight=300&quality=90");
    }

    #[test]
    fn every_build_plays_mp3_and_flac_directly() {
        let containers = stream_containers().split(',').collect::<Vec<_>>();
        assert!(containers.contains(&"mp3") && containers.contains(&"flac"));
    }
}
