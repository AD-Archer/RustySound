// Share links (OpenSubsonic createShare / getShares). Navidrome serves them
// when `EnableSharing` is on (the default since 0.63); with it off the share
// endpoints answer 501, so support is detected per server with getShares.

static SHARE_SUPPORT: Lazy<Mutex<HashMap<String, bool>>> = Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Deserialize, Default, Clone)]
pub struct SharePayload {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct SharesPayload {
    #[serde(default)]
    pub share: Vec<SharePayload>,
}

/// What a server is known to say about sharing this session: Some(false)
/// once getShares failed, None before anyone asked.
pub fn known_share_support(server_id: &str) -> Option<bool> {
    SHARE_SUPPORT
        .lock()
        .ok()
        .and_then(|cache| cache.get(server_id).copied())
}

/// The link to hand out for a share. Servers normally give an absolute URL;
/// a path is joined to the server's origin, and no URL at all falls back to
/// Navidrome's `/share/<id>` under the server address. Pure; used by tests.
pub fn share_link_url(url: &str, server_url: &str, id: &str) -> Option<String> {
    let url = url.trim();
    let server = server_url.trim().trim_end_matches('/');
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(url.to_string());
    }
    if let Some(path) = url.strip_prefix('/') {
        // scheme://host[:port], without the server's own base path.
        let origin = match server.find("://") {
            Some(at) => {
                let rest = &server[at + 3..];
                let host_end = rest.find('/').map(|i| at + 3 + i).unwrap_or(server.len());
                &server[..host_end]
            }
            None => server,
        };
        return (!origin.is_empty()).then(|| format!("{origin}/{path}"));
    }
    if !url.is_empty() {
        return (!server.is_empty()).then(|| format!("{server}/{url}"));
    }
    let id = id.trim();
    (!id.is_empty() && !server.is_empty()).then(|| format!("{server}/share/{id}"))
}

impl NavidromeClient {
    fn remember_share_support(&self, supported: bool) {
        if let Ok(mut cache) = SHARE_SUPPORT.lock() {
            cache.insert(self.server.id.clone(), supported);
        }
    }

    /// Whether the server shares at all (`getShares` answers). Asked once
    /// per server per session.
    pub async fn shares_supported(&self) -> bool {
        if let Some(known) = known_share_support(&self.server.id) {
            return known;
        }
        let url = self.build_url("getShares", &[]);
        let supported = match HTTP_CLIENT.get(&url).send().await {
            Ok(response) if response.status().is_success() => response
                .json::<SubsonicResponse>()
                .await
                .map(|json| json.subsonic_response.status == "ok")
                .unwrap_or(false),
            // 501 (sharing off), 404 (a plain Subsonic server), offline.
            _ => false,
        };
        self.remember_share_support(supported);
        supported
    }

    /// `createShare` for one song, album or playlist id. Returns the link.
    pub async fn create_share(&self, id: &str, description: Option<&str>) -> Result<String, String> {
        let mut params = vec![("id", id)];
        if let Some(description) = description.map(str::trim).filter(|d| !d.is_empty()) {
            params.push(("description", description));
        }
        let url = self.build_url("createShare", &params);
        let response = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            if response.status().as_u16() == 501 {
                self.remember_share_support(false);
            }
            return Err(format!("status {}", response.status().as_u16()));
        }
        let json: SubsonicResponse = response.json().await.map_err(|e| e.to_string())?;
        subsonic_ok(&json)?;
        let share = json
            .subsonic_response
            .shares
            .and_then(|shares| shares.share.into_iter().next())
            .ok_or_else(|| "The server returned no share.".to_string())?;
        share_link_url(&share.url, &self.server.url, &share.id)
            .ok_or_else(|| "The server returned no link.".to_string())
    }
}

#[cfg(test)]
mod share_tests {
    use super::share_link_url;

    #[test]
    fn absolute_links_are_kept() {
        assert_eq!(
            share_link_url(" https://music.example.com/share/Ab3 ", "http://10.0.0.2:4533", "Ab3").as_deref(),
            Some("https://music.example.com/share/Ab3")
        );
    }

    #[test]
    fn paths_join_the_server_origin() {
        assert_eq!(
            share_link_url("/share/Ab3", "https://example.com/navidrome/", "Ab3").as_deref(),
            Some("https://example.com/share/Ab3")
        );
        assert_eq!(
            share_link_url("share/Ab3", "https://example.com/navidrome/", "Ab3").as_deref(),
            Some("https://example.com/navidrome/share/Ab3")
        );
    }

    #[test]
    fn a_missing_url_falls_back_to_the_id() {
        assert_eq!(
            share_link_url("", "http://127.0.0.1:4533/", "Ab3").as_deref(),
            Some("http://127.0.0.1:4533/share/Ab3")
        );
        assert_eq!(share_link_url("", "http://127.0.0.1:4533", " "), None);
    }
}
