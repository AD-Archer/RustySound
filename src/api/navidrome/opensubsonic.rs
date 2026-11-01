// OpenSubsonic extensions (getOpenSubsonicExtensions) and server lyrics
// (getLyricsBySongId, extension `songLyrics`).

static OPEN_SUBSONIC_EXTENSIONS: Lazy<Mutex<HashMap<String, Vec<OpenSubsonicExtension>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// One extension a server advertises, with the versions it speaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OpenSubsonicExtension {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub versions: Vec<u32>,
}

/// What the server's `songLyrics` extension allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerLyricsSupport {
    /// No `songLyrics`: the server can't give lyrics by song id.
    None,
    /// v1: line-level structured lyrics.
    Lines,
    /// v2: `enhanced=true` adds word cues, agents and translations.
    Enhanced,
}

/// Reads `songLyrics` out of an extension list. Pure; used by tests.
pub fn server_lyrics_support(extensions: &[OpenSubsonicExtension]) -> ServerLyricsSupport {
    match extensions
        .iter()
        .find(|extension| extension.name.eq_ignore_ascii_case("songLyrics"))
    {
        None => ServerLyricsSupport::None,
        Some(extension) if extension.versions.iter().any(|version| *version >= 2) => {
            ServerLyricsSupport::Enhanced
        }
        Some(_) => ServerLyricsSupport::Lines,
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct LyricsListPayload {
    #[serde(default, alias = "structuredLyrics")]
    pub structured_lyrics: Vec<StructuredLyricsPayload>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct StructuredLyricsPayload {
    /// `main` (also when absent), `translation` or `pronunciation`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub synced: bool,
    /// Milliseconds; positive shows the lyrics sooner.
    #[serde(default)]
    pub offset: Option<f64>,
    #[serde(default)]
    pub line: Vec<StructuredLinePayload>,
    #[serde(default)]
    pub agents: Vec<LyricsAgentPayload>,
    #[serde(default, alias = "cueLine")]
    pub cue_line: Vec<CueLinePayload>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct StructuredLinePayload {
    /// Milliseconds from the track start (synced only).
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct LyricsAgentPayload {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CueLinePayload {
    /// Index into the entry's `line` array.
    #[serde(default)]
    pub index: usize,
    #[serde(default)]
    pub value: String,
    #[serde(default, alias = "agentId")]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub cue: Vec<CuePayload>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CuePayload {
    /// Milliseconds.
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub value: String,
    /// UTF-8 byte offset into the cue line's `value` where the cue starts.
    /// (`byteEnd` isn't needed: a segment runs to the next cue's start.)
    #[serde(default, alias = "byteStart")]
    pub byte_start: Option<usize>,
}

fn subsonic_ok(json: &SubsonicResponse) -> Result<(), String> {
    if json.subsonic_response.status == "ok" {
        return Ok(());
    }
    Err(json
        .subsonic_response
        .error
        .as_ref()
        .map(|e| e.message.clone())
        .unwrap_or_else(|| "Unknown error".to_string()))
}

impl NavidromeClient {
    /// `getOpenSubsonicExtensions`. Cached per server for the session and
    /// 24 h on disk. A plain Subsonic server (an error or no list) has none.
    pub async fn get_open_subsonic_extensions(
        &self,
        timeout_seconds: u32,
    ) -> Result<Vec<OpenSubsonicExtension>, String> {
        let cache_key = format!("api:openSubsonicExtensions:v1:{}", self.server.id);
        if let Ok(cache) = OPEN_SUBSONIC_EXTENSIONS.lock() {
            if let Some(cached) = cache.get(&cache_key) {
                return Ok(cached.clone());
            }
        }
        let remember = |extensions: &Vec<OpenSubsonicExtension>| {
            if let Ok(mut cache) = OPEN_SUBSONIC_EXTENSIONS.lock() {
                cache.insert(cache_key.clone(), extensions.clone());
            }
        };
        if let Some(cached) = cache_get_json::<Vec<OpenSubsonicExtension>>(&cache_key) {
            remember(&cached);
            return Ok(cached);
        }

        let url = self.build_url("getOpenSubsonicExtensions", &[]);
        let response = HTTP_CLIENT
            .get(&url)
            .timeout(std::time::Duration::from_secs(timeout_seconds.max(1) as u64))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        // Servers without the endpoint answer 404 or a Subsonic error: both
        // mean "no extensions", not a failure.
        let extensions = match response.json::<SubsonicResponse>().await {
            Ok(json) if subsonic_ok(&json).is_ok() => json
                .subsonic_response
                .open_subsonic_extensions
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        remember(&extensions);
        let _ = cache_put_json(cache_key.clone(), &extensions, Some(24));
        Ok(extensions)
    }

    /// `getLyricsBySongId`. `enhanced` asks for word cues (songLyrics v2).
    /// Not cached here: the lyrics layer caches the converted result.
    pub async fn get_lyrics_by_song_id(
        &self,
        song_id: &str,
        enhanced: bool,
        timeout_seconds: u32,
    ) -> Result<Vec<StructuredLyricsPayload>, String> {
        let mut params = vec![("id", song_id)];
        if enhanced {
            params.push(("enhanced", "true"));
        }
        let url = self.build_url("getLyricsBySongId", &params);
        let response = HTTP_CLIENT
            .get(&url)
            .timeout(std::time::Duration::from_secs(timeout_seconds.max(1) as u64))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("status {}", response.status().as_u16()));
        }
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;
        subsonic_ok(&json)?;
        Ok(json
            .subsonic_response
            .lyrics_list
            .map(|list| list.structured_lyrics)
            .unwrap_or_default())
    }
}
