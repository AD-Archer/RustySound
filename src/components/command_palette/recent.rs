//! What the palette ran or opened lately, newest first, kept on this device
//! like Search's recent searches: localStorage on the web, a small JSON file
//! in the app data folder on native builds. Shown before anything is typed,
//! and recently used actions rank first among equal matches.

use crate::api::{Album, Artist, Playlist, Song};
use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 8;
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "rustysound.palette_recent";
#[cfg(not(target_arch = "wasm32"))]
const FILE_NAME: &str = "palette_recent.json";

/// One remembered pick. Library items keep what their row shows, so the
/// list draws without a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Recent {
    Action { key: String },
    Album { album: Album },
    Artist { artist: Artist },
    Song { song: Song },
    Playlist { playlist: Playlist },
}

impl Recent {
    /// Identity for de-duplication.
    fn id(&self) -> String {
        match self {
            Recent::Action { key } => format!("action:{key}"),
            Recent::Album { album } => format!("album:{}:{}", album.server_id, album.id),
            Recent::Artist { artist } => format!("artist:{}:{}", artist.server_id, artist.id),
            Recent::Song { song } => format!("song:{}:{}", song.server_id, song.id),
            Recent::Playlist { playlist } => format!("playlist:{}:{}", playlist.server_id, playlist.id),
        }
    }
}

/// Puts `item` first, without duplicates, capped at 8.
pub fn remember(list: &[Recent], item: Recent) -> Vec<Recent> {
    let id = item.id();
    let mut next = vec![item];
    next.extend(list.iter().filter(|r| r.id() != id).take(MAX_RECENT - 1).cloned());
    next
}

/// Position of an action among the recents (0 = newest), for ranking.
pub fn action_rank(list: &[Recent], key: &str) -> Option<usize> {
    list.iter()
        .filter(|r| matches!(r, Recent::Action { .. }))
        .position(|r| matches!(r, Recent::Action { key: k } if k == key))
}

fn parse(raw: &str) -> Vec<Recent> {
    serde_json::from_str::<Vec<Recent>>(raw)
        .unwrap_or_default()
        .into_iter()
        .take(MAX_RECENT)
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub fn load() -> Vec<Recent> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .map(|raw| parse(&raw))
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub fn save(list: &[Recent]) {
    if let (Some(storage), Ok(raw)) = (
        web_sys::window().and_then(|w| w.local_storage().ok().flatten()),
        serde_json::to_string(list),
    ) {
        let _ = storage.set_item(STORAGE_KEY, &raw);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load() -> Vec<Recent> {
    crate::storage::app_data_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
        .map(|raw| parse(&raw))
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save(list: &[Recent]) {
    if let (Some(dir), Ok(raw)) = (crate::storage::app_data_dir(), serde_json::to_string(list)) {
        let _ = std::fs::write(dir.join(FILE_NAME), raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(key: &str) -> Recent {
        Recent::Action { key: key.to_string() }
    }

    #[test]
    fn newest_first_without_duplicates() {
        let list = remember(&[action("next"), action("lyrics")], action("lyrics"));
        assert_eq!(list, vec![action("lyrics"), action("next")]);
        let long = (0..12).fold(Vec::new(), |acc, i| remember(&acc, action(&format!("a{i}"))));
        assert_eq!(long.len(), MAX_RECENT);
        assert_eq!(action_rank(&list, "next"), Some(1));
        assert_eq!(action_rank(&list, "shuffle"), None);
    }

    #[test]
    fn round_trips_and_parses_garbage_as_empty() {
        let album = Recent::Album { album: Album { id: "a".into(), name: "Paper".into(), ..Default::default() } };
        let list = remember(&[action("next")], album.clone());
        let raw = serde_json::to_string(&list).unwrap();
        assert_eq!(parse(&raw), list);
        assert!(parse("not json").is_empty());
    }
}
