// Cross-device sync: the client name (`c=`), reportPlayback (extension
// `playbackReport`), the server play queue (savePlayQueue / getPlayQueue and
// their `indexBasedQueue` variants) and getNowPlaying.

/// Queues longer than this go as a form POST when the server allows it.
const FORM_POST_MIN_IDS: usize = 40;

/// The device part of the client name: "iPhone", "iPad", "Mac", "Web"…
pub fn device_label() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        "Web"
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
    {
        if ios_is_ipad() {
            "iPad"
        } else {
            "iPhone"
        }
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
    {
        "Mac"
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    {
        "Windows"
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
    {
        "Android"
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(target_os = "ios", target_os = "macos", target_os = "windows", target_os = "android"))
    ))]
    {
        "Linux"
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ios_is_ipad() -> bool {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    static IPAD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *IPAD.get_or_init(|| unsafe {
        let device: *mut Object = msg_send![class!(UIDevice), currentDevice];
        if device.is_null() {
            return false;
        }
        // UIUserInterfaceIdiomPad
        let idiom: isize = msg_send![device, userInterfaceIdiom];
        idiom == 1
    })
}

/// "RustySound (iPhone)". Pure; used by tests.
pub fn client_name_for(device: &str) -> String {
    format!("{CLIENT_NAME} ({device})")
}

/// The `c=` parameter on every Subsonic request. Servers show it as the
/// player name and as `changedBy` on the saved play queue, so it names the
/// device. Stable for the life of the install.
pub fn client_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| client_name_for(device_label()))
}

/// The device a `changedBy` / `playerName` names: "iPhone" for
/// "RustySound (iPhone)", the raw name for other clients ("NavidromeUI").
pub fn device_from_client_name(name: &str) -> String {
    let trimmed = name.trim();
    trimmed
        .strip_prefix(CLIENT_NAME)
        .map(str::trim)
        .and_then(|rest| rest.strip_prefix('(')?.strip_suffix(')'))
        .map(|device| device.trim().to_string())
        .filter(|device| !device.is_empty())
        .unwrap_or_else(|| trimmed.to_string())
}

/// Which of the sync extensions a server advertises.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncSupport {
    /// `playbackReport`: report plays with reportPlayback instead of scrobble.
    pub playback_report: bool,
    /// `indexBasedQueue`: savePlayQueueByIndex / getPlayQueueByIndex.
    pub index_queue: bool,
    /// `formPost`: parameters may go in a form body.
    pub form_post: bool,
}

/// Reads the sync extensions out of an extension list. Pure; used by tests.
pub fn sync_support(extensions: &[OpenSubsonicExtension]) -> SyncSupport {
    let has = |name: &str| extensions.iter().any(|e| e.name.eq_ignore_ascii_case(name));
    SyncSupport {
        playback_report: has("playbackReport"),
        index_queue: has("indexBasedQueue"),
        form_post: has("formPost"),
    }
}

/// reportPlayback's `state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackReportState {
    Starting,
    Playing,
    Paused,
    Stopped,
}

impl PlaybackReportState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Playing => "playing",
            Self::Paused => "paused",
            Self::Stopped => "stopped",
        }
    }
}

/// The play queue saved on the server, with entries ready to play.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ServerPlayQueue {
    pub entries: Vec<Song>,
    pub current_index: usize,
    pub position_ms: u64,
    /// When it was saved, in Unix milliseconds (server clock).
    pub changed_ms: Option<i64>,
    /// The `c=` of the client that saved it.
    pub changed_by: String,
}

impl ServerPlayQueue {
    pub fn current(&self) -> Option<&Song> {
        self.entries.get(self.current_index)
    }
}

/// One getNowPlaying entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NowPlayingEntry {
    pub song_id: String,
    pub title: String,
    pub username: String,
    /// The player's client name (Navidrome: `c=`). Absent on older servers.
    pub player_name: Option<String>,
    /// `starting`, `playing`, `paused`… (playbackReport servers only).
    pub state: Option<String>,
    pub position_ms: Option<u64>,
    pub minutes_ago: u32,
}

#[derive(Debug, Deserialize, Default)]
pub struct PlayQueuePayload {
    #[serde(default)]
    pub entry: Vec<serde_json::Value>,
    #[serde(default)]
    pub current: Option<serde_json::Value>,
    #[serde(default, alias = "currentIndex")]
    pub current_index: Option<serde_json::Value>,
    #[serde(default)]
    pub position: Option<serde_json::Value>,
    #[serde(default)]
    pub changed: Option<String>,
    #[serde(default, alias = "changedBy")]
    pub changed_by: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct NowPlayingPayload {
    #[serde(default)]
    pub entry: Vec<serde_json::Value>,
}

fn json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite() && *f >= 0.0).map(|f| f.round() as u64)),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok().filter(|f| *f >= 0.0).map(|f| f.round() as u64),
        _ => None,
    }
}

fn json_text(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// RFC 3339 → Unix milliseconds.
pub fn parse_server_time_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc).timestamp_millis())
}

/// Parses a getNowPlaying entry list. Pure; used by tests.
pub fn parse_now_playing(entries: &[serde_json::Value]) -> Vec<NowPlayingEntry> {
    entries
        .iter()
        .filter_map(|entry| {
            let song_id = json_text(entry.get("id"))?;
            Some(NowPlayingEntry {
                song_id,
                title: json_text(entry.get("title")).unwrap_or_default(),
                username: json_text(entry.get("username")).unwrap_or_default(),
                player_name: json_text(entry.get("playerName")),
                state: json_text(entry.get("state")).map(|s| s.to_ascii_lowercase()),
                position_ms: json_u64(entry.get("positionMs")),
                minutes_ago: json_u64(entry.get("minutesAgo")).unwrap_or(0) as u32,
            })
        })
        .collect()
}

impl NavidromeClient {
    /// The sync extensions this server advertises (cached with the
    /// extension list; none on plain Subsonic servers or when unreachable).
    pub async fn sync_support(&self) -> SyncSupport {
        self.get_open_subsonic_extensions(5)
            .await
            .map(|extensions| sync_support(&extensions))
            .unwrap_or_default()
    }

    async fn send_sync_request(&self, endpoint: &str, params: Vec<(String, String)>, form: bool) -> Result<(), String> {
        let request = if form {
            let body = params
                .iter()
                .map(|(k, v)| format!("{}={}", urlencoding_simple(k), urlencoding_simple(v)))
                .collect::<Vec<_>>()
                .join("&");
            HTTP_CLIENT
                .post(self.build_url(endpoint, &[]))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
        } else {
            HTTP_CLIENT.get(self.build_url_owned(endpoint, params))
        };
        subsonic_send(request.timeout(std::time::Duration::from_secs(10)))
            .await
            .map(|_| ())
    }

    /// `reportPlayback` (extension `playbackReport`). The server scrobbles
    /// from it on `stopped` once enough was played, so callers must not
    /// also call `scrobble`.
    pub async fn report_playback(
        &self,
        media_id: &str,
        state: PlaybackReportState,
        position_ms: u64,
        playback_rate: f64,
    ) -> Result<(), String> {
        let rate = if playback_rate.is_finite() && playback_rate > 0.0 { playback_rate } else { 1.0 };
        let params = vec![
            ("mediaId".to_string(), media_id.to_string()),
            ("mediaType".to_string(), "song".to_string()),
            ("positionMs".to_string(), position_ms.to_string()),
            ("state".to_string(), state.as_str().to_string()),
            ("playbackRate".to_string(), format!("{rate}")),
        ];
        self.send_sync_request("reportPlayback", params, false).await
    }

    /// Saves the play queue: by index when the server has `indexBasedQueue`
    /// (a song queued twice resumes at the right copy), else by song id. Long
    /// queues go as a form POST when `formPost` is advertised. Callers trim
    /// the queue first (`trim_queue_window`).
    pub async fn save_play_queue(
        &self,
        ids: &[String],
        current_index: usize,
        position_ms: u64,
        support: SyncSupport,
    ) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        let current_index = current_index.min(ids.len() - 1);
        let mut params: Vec<(String, String)> =
            ids.iter().map(|id| ("id".to_string(), id.clone())).collect();
        let endpoint = if support.index_queue {
            params.push(("currentIndex".to_string(), current_index.to_string()));
            "savePlayQueueByIndex"
        } else {
            params.push(("current".to_string(), ids[current_index].clone()));
            "savePlayQueue"
        };
        params.push(("position".to_string(), position_ms.to_string()));
        let form = support.form_post && ids.len() >= FORM_POST_MIN_IDS;
        self.send_sync_request(endpoint, params, form).await
    }

    /// Body for a `navigator.sendBeacon` save when the page goes away: the
    /// URL (auth in the query) and the form fields without `position`, which
    /// the page adds from the audio element. Web only; needs `formPost`.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn play_queue_beacon(&self, ids: &[String], current_index: usize, support: SyncSupport) -> Option<(String, String)> {
        if ids.is_empty() || !support.form_post {
            return None;
        }
        let current_index = current_index.min(ids.len() - 1);
        let mut body: Vec<String> = ids.iter().map(|id| format!("id={}", urlencoding_simple(id))).collect();
        let endpoint = if support.index_queue {
            body.push(format!("currentIndex={current_index}"));
            "savePlayQueueByIndex"
        } else {
            body.push(format!("current={}", urlencoding_simple(&ids[current_index])));
            "savePlayQueue"
        };
        Some((self.build_url(endpoint, &[]), body.join("&")))
    }

    /// The server's saved play queue, or `None` when it has none. Entries
    /// the payload carries in full are used as they are; bare ids are
    /// fetched with getSong, so a queue saved elsewhere plays even when this
    /// device has never seen those songs.
    pub async fn get_play_queue(&self, support: SyncSupport) -> Result<Option<ServerPlayQueue>, String> {
        let endpoint = if support.index_queue { "getPlayQueueByIndex" } else { "getPlayQueue" };
        let json = subsonic_send(
            HTTP_CLIENT
                .get(self.build_url(endpoint, &[]))
                .timeout(std::time::Duration::from_secs(10)),
        )
        .await?;
        let inner = json.subsonic_response;
        let Some(payload) = inner.play_queue_by_index.or(inner.play_queue) else {
            return Ok(None);
        };

        let mut entries = Vec::with_capacity(payload.entry.len());
        for value in &payload.entry {
            let parsed = serde_json::from_value::<Song>(value.clone())
                .ok()
                .filter(|song| !song.id.trim().is_empty() && !song.title.trim().is_empty());
            let song = match parsed {
                Some(mut song) => {
                    song.server_id = self.server.id.clone();
                    song.server_name = self.server.name.clone();
                    normalize_song_cover_art(&mut song);
                    song
                }
                None => {
                    let Some(id) = json_text(value.get("id")) else { continue };
                    match self.get_song(&id).await {
                        Ok(song) => song,
                        Err(_) => continue,
                    }
                }
            };
            entries.push(song);
        }
        if entries.is_empty() {
            return Ok(None);
        }

        let current_index = json_u64(payload.current_index.as_ref())
            .map(|i| i as usize)
            .or_else(|| {
                let current = json_text(payload.current.as_ref())?;
                entries.iter().position(|song| song.id == current)
            })
            .unwrap_or(0)
            .min(entries.len() - 1);
        Ok(Some(ServerPlayQueue {
            entries,
            current_index,
            position_ms: json_u64(payload.position.as_ref()).unwrap_or(0),
            changed_ms: payload.changed.as_deref().and_then(parse_server_time_ms),
            changed_by: payload.changed_by.unwrap_or_default(),
        }))
    }

    /// `getNowPlaying`: what every player on the server is playing.
    pub async fn get_now_playing(&self) -> Result<Vec<NowPlayingEntry>, String> {
        let json = subsonic_send(
            HTTP_CLIENT
                .get(self.build_url("getNowPlaying", &[]))
                .timeout(std::time::Duration::from_secs(8)),
        )
        .await?;
        Ok(json
            .subsonic_response
            .now_playing
            .map(|payload| parse_now_playing(&payload.entry))
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod play_queue_tests {
    use super::*;

    fn ext(name: &str) -> OpenSubsonicExtension {
        OpenSubsonicExtension { name: name.to_string(), versions: vec![1] }
    }

    #[test]
    fn client_name_names_the_device() {
        assert_eq!(client_name_for("iPhone"), "RustySound (iPhone)");
        assert_eq!(client_name_for("Mac"), "RustySound (Mac)");
        assert!(client_name().starts_with("RustySound ("));
        assert!(client_name().ends_with(')'));
        assert!(client_name().len() <= 24, "short enough for a player list");
    }

    #[test]
    fn device_comes_back_out_of_a_client_name() {
        assert_eq!(device_from_client_name("RustySound (iPhone)"), "iPhone");
        assert_eq!(device_from_client_name(" RustySound (Web) "), "Web");
        assert_eq!(device_from_client_name("NavidromeUI"), "NavidromeUI");
        assert_eq!(device_from_client_name("RustySound"), "RustySound");
        assert_eq!(device_from_client_name("RustySound ()"), "RustySound ()");
    }

    #[test]
    fn the_client_name_is_url_safe_once_encoded() {
        assert_eq!(urlencoding_simple(&client_name_for("Mac")), "RustySound%20%28Mac%29");
    }

    #[test]
    fn sync_support_reads_the_extension_list() {
        assert_eq!(sync_support(&[]), SyncSupport::default());
        let all = sync_support(&[ext("playbackReport"), ext("indexBasedQueue"), ext("formPost"), ext("songLyrics")]);
        assert_eq!(all, SyncSupport { playback_report: true, index_queue: true, form_post: true });
        assert!(sync_support(&[ext("PLAYBACKREPORT")]).playback_report);
    }

    #[test]
    fn server_times_parse_to_millis() {
        assert_eq!(parse_server_time_ms("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(parse_server_time_ms("2023-03-10T02:19:35.784818075Z"), Some(1_678_414_775_784));
        assert_eq!(parse_server_time_ms("yesterday"), None);
    }

    #[test]
    fn now_playing_entries_parse_with_or_without_playback_report_fields() {
        let entries = vec![
            serde_json::json!({"id": "s1", "title": "Song", "username": "demo", "minutesAgo": 0, "playerId": 1,
                "playerName": "RustySound (Mac)", "state": "playing", "positionMs": 120000, "playbackRate": 1.0}),
            serde_json::json!({"id": "s2", "title": "Old", "username": "demo", "minutesAgo": 3, "playerId": 2}),
            serde_json::json!({"title": "no id"}),
        ];
        let parsed = parse_now_playing(&entries);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].player_name.as_deref(), Some("RustySound (Mac)"));
        assert_eq!(parsed[0].state.as_deref(), Some("playing"));
        assert_eq!(parsed[0].position_ms, Some(120_000));
        assert_eq!(parsed[1].player_name, None);
        assert_eq!(parsed[1].minutes_ago, 3);
    }
}
