//! "Save as Playlist" for mixes. Re-saving a mix replaces its earlier copy
//! (owner decision), so we remember which playlist each mix went to in the
//! `discover_saved_mix_playlists` setting and fall back to an exact name match.

use crate::api::discovery::{plan_mix_save, save_mix_as_playlist, Mix, SaveMixTarget, SavedMix};
use crate::api::{NavidromeClient, ServerConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a mix was last saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedMixRef {
    pub server_id: String,
    pub playlist_id: String,
}

/// What [`save_mix`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveOutcome {
    pub result: SavedMix,
    /// Where the mix now lives (store it in the map), when the id is known.
    pub saved: Option<SavedMixRef>,
    /// True when an earlier copy was replaced rather than a new one made.
    pub replaced: bool,
}

/// Mix id → saved playlist. Unreadable JSON reads as empty.
pub type SavedMixMap = BTreeMap<String, SavedMixRef>;

pub fn parse_saved(raw: &str) -> SavedMixMap {
    serde_json::from_str(raw).unwrap_or_default()
}

pub fn serialize_saved(map: &SavedMixMap) -> String {
    serde_json::to_string(map).unwrap_or_else(|_| "{}".to_string())
}

/// The remembered playlist for `mix_id`, if its server is still active.
pub fn remembered(map: &SavedMixMap, mix_id: &str, servers: &[ServerConfig]) -> Option<SavedMixRef> {
    map.get(mix_id)
        .filter(|saved| servers.iter().any(|s| s.active && s.id == saved.server_id))
        .cloned()
}

/// Save `mix` as a playlist named after it. Order of attempts:
/// 1. replace the playlist remembered for this mix;
/// 2. replace a playlist of the same name owned by the user;
/// 3. create a new one.
///
pub async fn save_mix(
    servers: &[ServerConfig],
    mix: &Mix,
    remembered: Option<SavedMixRef>,
) -> Result<SaveOutcome, String> {
    if let Some(saved) = remembered {
        let target = SaveMixTarget::Replace {
            server_id: saved.server_id.clone(),
            playlist_id: saved.playlist_id.clone(),
        };
        // A failure usually means the playlist was deleted; fall through.
        if let Ok(result) = save_mix_as_playlist(servers, mix, target).await {
            return Ok(SaveOutcome {
                result,
                saved: Some(saved),
                replaced: true,
            });
        }
    }

    let (server_id, _, _) = plan_mix_save(mix, servers)?;
    let server = servers
        .iter()
        .find(|s| s.id == server_id)
        .cloned()
        .ok_or_else(|| "Server not found.".to_string())?;
    if let Some(playlist_id) = find_owned_playlist(&server, &mix.name).await {
        let target = SaveMixTarget::Replace {
            server_id: server_id.clone(),
            playlist_id: playlist_id.clone(),
        };
        if let Ok(result) = save_mix_as_playlist(servers, mix, target).await {
            return Ok(SaveOutcome {
                result,
                saved: Some(SavedMixRef {
                    server_id,
                    playlist_id,
                }),
                replaced: true,
            });
        }
    }

    let target = SaveMixTarget::New {
        name: Some(mix.name.clone()),
    };
    let mut result = save_mix_as_playlist(servers, mix, target).await?;
    if result.playlist_id.is_none() {
        // Some servers don't return the new id; look it up by name.
        result.playlist_id = find_owned_playlist(&server, &mix.name).await;
    }
    let saved = result.playlist_id.clone().map(|playlist_id| SavedMixRef {
        server_id: result.server_id.clone(),
        playlist_id,
    });
    Ok(SaveOutcome {
        result,
        saved,
        replaced: false,
    })
}

/// A playlist on `server` named exactly `name` and owned by the signed-in user.
async fn find_owned_playlist(server: &ServerConfig, name: &str) -> Option<String> {
    let client = NavidromeClient::new(server.clone());
    client.refresh_playlist_cache();
    let playlists = client.get_playlists().await.ok()?;
    playlists
        .into_iter()
        .find(|p| {
            p.name == name
                && p.owner
                    .as_deref()
                    .map(|owner| owner.eq_ignore_ascii_case(&server.username))
                    .unwrap_or(true)
        })
        .map(|p| p.id)
}

/// "Saved “Deep Cuts” (25 songs)", noting songs left out from other servers.
pub fn saved_message(mix: &Mix, outcome: &SaveOutcome) -> String {
    let result = &outcome.result;
    let verb = if outcome.replaced { "Updated" } else { "Saved" };
    let songs = if result.saved_songs == 1 {
        "1 song".to_string()
    } else {
        format!("{} songs", result.saved_songs)
    };
    if result.skipped_songs > 0 {
        format!(
            "{verb} “{}” ({songs}; {} from other servers left out)",
            mix.name, result.skipped_songs
        )
    } else {
        format!("{verb} “{}” ({songs})", mix.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool) -> ServerConfig {
        ServerConfig {
            id: id.into(),
            name: id.into(),
            url: format!("https://{id}.example"),
            username: "u".into(),
            password: "p".into(),
            active,
        }
    }

    #[test]
    fn map_round_trips_and_tolerates_junk() {
        let mut map = SavedMixMap::new();
        map.insert(
            "discovery".into(),
            SavedMixRef {
                server_id: "a".into(),
                playlist_id: "pl-1".into(),
            },
        );
        let raw = serialize_saved(&map);
        assert_eq!(parse_saved(&raw), map);
        assert!(parse_saved("").is_empty());
        assert!(parse_saved("not json").is_empty());
        assert!(parse_saved("{}").is_empty());
    }

    #[test]
    fn remembered_needs_an_active_server() {
        let mut map = SavedMixMap::new();
        map.insert(
            "deep-cuts".into(),
            SavedMixRef {
                server_id: "a".into(),
                playlist_id: "pl-9".into(),
            },
        );
        assert!(remembered(&map, "deep-cuts", &[server("a", true)]).is_some());
        assert!(remembered(&map, "deep-cuts", &[server("a", false)]).is_none());
        assert!(remembered(&map, "rewind", &[server("a", true)]).is_none());
    }
}
