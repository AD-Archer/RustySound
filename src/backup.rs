//! Backup and restore: the active profile's settings (with the equalizer,
//! saved mixes and lyrics preferences, which live in them) and its servers,
//! as one versioned JSON file.
//!
//! * [`export`] writes the file. Passwords, Jellyfin tokens and the Last.fm
//!   key stay out unless the person opts in ("Include sign-in details").
//! * [`parse`] reads one back, refusing files that aren't backups or come
//!   from a newer format; [`plan`] works out what importing it would change
//!   (shown as a preview) and the merged result. Servers merge by id, then
//!   by address and user, so importing twice never duplicates them.
//! * [`save_file`] hands the file over the platform's way: a download on
//!   the web, a save panel on the Mac, the share sheet on iPhone (the file is
//!   also saved to the app's folder in Files). Opening one is a file input
//!   (`settings/backup.rs`): the browser's picker, the Mac's open panel
//!   (Dioxus routes file inputs to it), and the iPhone's document picker.

use crate::api::{ServerConfig, ServerKind};
use crate::db::AppSettings;
use serde::{Deserialize, Serialize};

/// Says "this is a RustySound backup" at the top of the file.
pub const FORMAT: &str = "rustysound-backup";
/// The format version this build writes. Older versions are read (and
/// migrated in [`parse`] when they need it); newer ones are refused.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub exported_at: String,
    #[serde(default)]
    pub app_version: String,
    /// Whether passwords and tokens are in the file.
    #[serde(default)]
    pub includes_sign_in: bool,
    pub settings: AppSettings,
    #[serde(default)]
    pub servers: Vec<BackupServer>,
}

/// A server as backed up: everything but the sign-in, unless included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupServer {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    #[serde(default)]
    pub kind: ServerKind,
    #[serde(default)]
    pub active: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
}

impl BackupServer {
    fn from_config(server: &ServerConfig, include_sign_in: bool) -> Self {
        let secret = |value: &str| if include_sign_in { value.to_string() } else { String::new() };
        Self {
            id: server.id.clone(),
            name: server.name.clone(),
            url: server.url.clone(),
            username: server.username.clone(),
            kind: server.kind,
            active: server.active,
            user_id: server.user_id.clone(),
            password: secret(&server.password),
            access_token: secret(&server.access_token),
        }
    }

    fn has_sign_in(&self) -> bool {
        !self.password.is_empty() || !self.access_token.is_empty()
    }
}

/// The file name to suggest: `RustySound Backup 2026-11-01.json`.
pub fn file_name(date: &str) -> String {
    format!("RustySound Backup {date}.json")
}

/// The backup file for these settings and servers. `include_sign_in` adds
/// passwords, Jellyfin tokens and the Last.fm key.
pub fn export(settings: &AppSettings, servers: &[ServerConfig], include_sign_in: bool, exported_at: &str) -> String {
    let mut settings = settings.clone();
    if !include_sign_in {
        settings.lastfm_api_key.clear();
    }
    let backup = Backup {
        format: FORMAT.to_string(),
        version: VERSION,
        exported_at: exported_at.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        includes_sign_in: include_sign_in,
        settings,
        servers: servers.iter().map(|s| BackupServer::from_config(s, include_sign_in)).collect(),
    };
    serde_json::to_string_pretty(&backup).unwrap_or_default()
}

/// Reads a backup file. Errors are ready to show.
pub fn parse(json: &str) -> Result<Backup, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| "That file isn't a RustySound backup.".to_string())?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err("That file isn't a RustySound backup.".to_string());
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version == 0 {
        return Err("That backup has no format version, so it can't be read.".to_string());
    }
    if version > VERSION as u64 {
        return Err("That backup is from a newer RustySound. Update the app to import it.".to_string());
    }
    // Version 1 is the first; later versions migrate older files here.
    serde_json::from_value(value).map_err(|e| format!("That backup is damaged ({e})."))
}

/// What importing a backup would do, and the result.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    pub settings: AppSettings,
    pub servers: Vec<ServerConfig>,
    /// Settings areas that change ("Appearance", "Equalizer"…).
    pub settings_changed: Vec<&'static str>,
    pub servers_added: Vec<String>,
    pub servers_updated: Vec<String>,
    /// Added servers that need signing in before they're switched on.
    pub needs_sign_in: Vec<String>,
}

impl ImportPlan {
    pub fn changes_anything(&self) -> bool {
        !self.settings_changed.is_empty() || !self.servers_added.is_empty() || !self.servers_updated.is_empty()
    }
}

/// The settings area a field belongs to, for the preview.
fn settings_area(field: &str) -> &'static str {
    match field {
        "equalizer" => "Equalizer",
        "discover_saved_mix_playlists" => "Saved mixes",
        f if f.starts_with("lyrics_") => "Lyrics",
        "theme" | "appearance" | "custom_css" | "art_colors" => "Appearance",
        f if f.starts_with("ambient_") => "Appearance",
        f if f.starts_with("lastfm_") || f.starts_with("listenbrainz_") || f.starts_with("discovery_") => "Discovery",
        f if f.starts_with("cache_")
            || f.contains("download")
            || f.starts_with("artwork_")
            || f.starts_with("bookmark_") =>
        {
            "Storage"
        }
        _ => "Playback",
    }
}

/// `scheme://host:port/path` without a trailing slash, host in lowercase.
fn same_address(a: &str, b: &str) -> bool {
    let norm = |url: &str| {
        let url = url.trim().trim_end_matches('/');
        match url.split_once("://") {
            Some((scheme, rest)) => {
                let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
                format!("{}://{}/{}", scheme.to_ascii_lowercase(), host.to_ascii_lowercase(), path)
            }
            None => url.to_ascii_lowercase(),
        }
    };
    norm(a) == norm(b)
}

/// Works out the import: the backup's settings (keeping this device's
/// Offline Mode, and its Last.fm key when the backup has none), and the
/// servers merged into the current ones.
pub fn plan(backup: &Backup, current_settings: &AppSettings, current_servers: &[ServerConfig]) -> ImportPlan {
    let mut settings = backup.settings.clone();
    settings.offline_mode = current_settings.offline_mode;
    if settings.lastfm_api_key.is_empty() {
        settings.lastfm_api_key = current_settings.lastfm_api_key.clone();
    }

    let mut settings_changed: Vec<&'static str> = Vec::new();
    if let (Ok(serde_json::Value::Object(before)), Ok(serde_json::Value::Object(after))) =
        (serde_json::to_value(current_settings), serde_json::to_value(&settings))
    {
        for (field, value) in &after {
            if before.get(field) != Some(value) {
                let area = settings_area(field);
                if !settings_changed.contains(&area) {
                    settings_changed.push(area);
                }
            }
        }
    }

    let mut servers = current_servers.to_vec();
    let (mut added, mut updated, mut needs_sign_in) = (Vec::new(), Vec::new(), Vec::new());
    for incoming in &backup.servers {
        let existing = servers.iter().position(|s| s.id == incoming.id).or_else(|| {
            servers.iter().position(|s| {
                s.kind == incoming.kind
                    && same_address(&s.url, &incoming.url)
                    && s.username.eq_ignore_ascii_case(&incoming.username)
            })
        });
        match existing {
            Some(index) => {
                let server = &mut servers[index];
                let before = server.clone();
                server.name = incoming.name.clone();
                if !incoming.password.is_empty() {
                    server.password = incoming.password.clone();
                }
                if !incoming.access_token.is_empty() {
                    server.access_token = incoming.access_token.clone();
                    server.user_id = incoming.user_id.clone();
                }
                if *server != before {
                    updated.push(server.name.clone());
                }
            }
            None => {
                let taken = servers.iter().any(|s| s.id == incoming.id) || incoming.id.trim().is_empty();
                let signed_in = incoming.has_sign_in();
                if !signed_in {
                    needs_sign_in.push(incoming.name.clone());
                }
                servers.push(ServerConfig {
                    id: if taken { uuid::Uuid::new_v4().to_string() } else { incoming.id.clone() },
                    name: incoming.name.clone(),
                    url: incoming.url.trim_end_matches('/').to_string(),
                    username: incoming.username.clone(),
                    password: incoming.password.clone(),
                    // Without its sign-in it would only fail: it waits, off.
                    active: incoming.active && signed_in,
                    kind: incoming.kind,
                    user_id: incoming.user_id.clone(),
                    access_token: incoming.access_token.clone(),
                });
                added.push(incoming.name.clone());
            }
        }
    }

    ImportPlan {
        settings,
        servers,
        settings_changed,
        servers_added: added,
        servers_updated: updated,
        needs_sign_in,
    }
}

// ---- Handing the file over ---------------------------------------------------

/// How a saved backup left the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    /// Saved; where, when there's a place to name.
    To(Option<String>),
    /// The share sheet took it.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    Shared,
    /// The save panel was closed (Mac).
    #[cfg_attr(not(all(feature = "desktop", target_os = "macos")), allow(dead_code))]
    Cancelled,
}

/// Saves `contents` as `file_name`: a download (web), the save panel (Mac),
/// the share sheet plus the app's folder in Files (iPhone), the Downloads
/// folder elsewhere.
pub async fn save_file(file_name: &str, contents: String) -> Result<Saved, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let script = format!(
            r#"
const blob = new Blob([{body}], {{ type: "application/json" }});
const url = URL.createObjectURL(blob);
const link = document.createElement("a");
link.href = url;
link.download = {name};
document.body.appendChild(link);
link.click();
link.remove();
setTimeout(() => URL.revokeObjectURL(url), 2000);
return true;
"#,
            body = serde_json::to_string(&contents).unwrap_or_default(),
            name = serde_json::to_string(file_name).unwrap_or_default(),
        );
        dioxus::document::eval(&script).await.map_err(|e| e.to_string())?;
        return Ok(Saved::To(None));
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "desktop", target_os = "macos"))]
    {
        let Some(handle) = rfd::AsyncFileDialog::new()
            .set_title("Save Backup")
            .set_file_name(file_name)
            .add_filter("JSON", &["json"])
            .save_file()
            .await
        else {
            return Ok(Saved::Cancelled);
        };
        std::fs::write(handle.path(), contents).map_err(|e| format!("Couldn't save the backup: {e}"))?;
        return Ok(Saved::To(Some(handle.path().display().to_string())));
    }

    #[cfg(target_os = "ios")]
    {
        // Documents shows in Files › On My iPhone › RustySound.
        let dir = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .map(|home| home.join("Documents"))
            .ok_or_else(|| "Couldn't find the app's folder.".to_string())?;
        std::fs::write(dir.join(file_name), &contents).map_err(|e| format!("Couldn't save the backup: {e}"))?;
        let script = format!(
            r#"
const file = new File([{body}], {name}, {{ type: "application/json" }});
if (!navigator.canShare || !navigator.canShare({{ files: [file] }})) return "none";
try {{
  await navigator.share({{ files: [file] }});
  return "shared";
}} catch (err) {{
  return err && err.name === "AbortError" ? "shared" : "none";
}}
"#,
            body = serde_json::to_string(&contents).unwrap_or_default(),
            name = serde_json::to_string(file_name).unwrap_or_default(),
        );
        let outcome = dioxus::document::eval(&script).join::<String>().await.unwrap_or_default();
        return Ok(if outcome == "shared" {
            Saved::Shared
        } else {
            Saved::To(Some("Files › On My iPhone › RustySound".to_string()))
        });
    }

    #[allow(unreachable_code)]
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = dirs::download_dir()
            .or_else(crate::storage::app_data_dir)
            .ok_or_else(|| "Couldn't find a folder to save to.".to_string())?;
        let path = dir.join(file_name);
        std::fs::write(&path, contents).map_err(|e| format!("Couldn't save the backup: {e}"))?;
        Ok(Saved::To(Some(path.display().to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, url: &str, user: &str) -> ServerConfig {
        ServerConfig {
            id: id.into(),
            name: format!("Server {id}"),
            url: url.into(),
            username: user.into(),
            password: "secret-pass".into(),
            active: true,
            ..Default::default()
        }
    }

    fn sample_settings() -> AppSettings {
        let mut settings = AppSettings::default();
        settings.theme = "midnight".into();
        settings.lyrics_offset_ms = 250;
        settings.lastfm_api_key = "lastfm-key".into();
        settings.discover_saved_mix_playlists = r#"{"mix-1":{"server_id":"a","playlist_id":"p1"}}"#.into();
        settings.equalizer.set_preset(crate::equalizer::EqPreset::Custom);
        settings
    }

    #[test]
    fn round_trip_keeps_settings_and_servers() {
        let settings = sample_settings();
        let servers = vec![server("a", "http://music.local:4533", "sam")];
        let json = export(&settings, &servers, true, "2026-11-01T10:00:00Z");
        let backup = parse(&json).unwrap();
        assert_eq!(backup.version, VERSION);
        assert_eq!(backup.settings, settings, "equalizer, saved mixes and lyrics come back too");
        assert_eq!(backup.servers[0].password, "secret-pass");

        let plan = plan(&backup, &AppSettings::default(), &[]);
        assert_eq!(plan.settings, settings);
        assert_eq!(plan.servers, servers);
    }

    #[test]
    fn sign_in_details_stay_out_unless_asked_for() {
        let mut servers = vec![server("a", "http://music.local", "sam")];
        servers.push(ServerConfig {
            id: "j".into(),
            ..ServerConfig::jellyfin("Jelly".into(), "http://jf".into(), "sam".into(), "uid".into(), "jelly-token".into())
        });
        let json = export(&sample_settings(), &servers, false, "now");
        for secret in ["secret-pass", "jelly-token", "lastfm-key"] {
            assert!(!json.contains(secret), "{secret} leaked");
        }
        let backup = parse(&json).unwrap();
        assert!(!backup.includes_sign_in);
        assert!(json.contains("music.local") && json.contains("\"uid\""), "addresses and user ids stay");
    }

    #[test]
    fn versions_and_foreign_files() {
        let json = export(&AppSettings::default(), &[], false, "now");
        let newer = json.replace(&format!("\"version\": {VERSION}"), "\"version\": 99");
        assert!(parse(&newer).unwrap_err().contains("newer"));
        let missing = json.replace(&format!("\"version\": {VERSION},"), "");
        assert!(parse(&missing).unwrap_err().contains("version"));
        assert!(parse(r#"{"hello": 1}"#).unwrap_err().contains("isn't a RustySound backup"));
        assert!(parse("not json").is_err());
    }

    #[test]
    fn importing_merges_servers_without_duplicates() {
        let current = vec![
            ServerConfig { password: "kept".into(), ..server("a", "http://Music.local:4533/", "Sam") },
            server("b", "http://other", "sam"),
        ];
        // Exported elsewhere: "a" has a new id and name, "c" is new.
        let mut elsewhere = vec![server("a-2", "http://music.local:4533", "sam"), server("c", "http://new", "kim")];
        elsewhere[0].name = "Living Room".into();
        let backup = parse(&export(&AppSettings::default(), &elsewhere, false, "now")).unwrap();

        let plan = plan(&backup, &AppSettings::default(), &current);
        assert_eq!(plan.servers.len(), 3, "a matched by address and user, c added");
        assert_eq!(plan.servers[0].name, "Living Room");
        assert_eq!(plan.servers[0].password, "kept", "a backup without sign-in keeps the saved one");
        assert_eq!(plan.servers_updated, ["Living Room"]);
        assert_eq!(plan.servers_added, ["Server c"]);
        assert_eq!(plan.needs_sign_in, ["Server c"]);
        assert!(!plan.servers[2].active, "no sign-in: added switched off");

        // Importing the same file again changes nothing.
        let again = super::plan(&backup, &plan.settings, &plan.servers);
        assert_eq!(again.servers, plan.servers);
        assert!(!again.changes_anything());
    }

    #[test]
    fn the_preview_names_what_changes_and_keeps_device_state() {
        let mut current = AppSettings::default();
        current.offline_mode = true;
        current.lastfm_api_key = "mine".into();
        let backup = parse(&export(&sample_settings(), &[], false, "now")).unwrap();
        let plan = plan(&backup, &current, &[]);
        for area in ["Appearance", "Lyrics", "Saved mixes", "Equalizer"] {
            assert!(plan.settings_changed.contains(&area), "{area}: {:?}", plan.settings_changed);
        }
        assert!(plan.settings.offline_mode, "Offline Mode belongs to this device");
        assert_eq!(plan.settings.lastfm_api_key, "mine");
    }
}
