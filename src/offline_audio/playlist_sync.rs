//! Downloaded playlists kept in step with the server.
//!
//! A downloaded playlist is one the user downloaded from its page (the
//! collection index marks it `auto_download_tracked`). Each pass re-fetches
//! it, downloads the songs that aren't on the device yet (new ones, and any
//! that failed before) and removes the downloads of songs that left the
//! playlist, unless another downloaded playlist, a fully downloaded album or
//! the queue still needs them. `components::playlist_sync` runs the passes:
//! on launch, when the app comes to the foreground and every 30 minutes.
//!
//! `playlist_diff` is the one place that decides what changed.

// The web build keeps no downloads, so only the tests use the diff there.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::collections::HashSet;

/// What a playlist re-fetch changed, by song id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaylistDiff {
    /// New since the last sync, in playlist order.
    pub added: Vec<String>,
    /// Gone from the playlist.
    pub removed: Vec<String>,
    /// The removed songs whose downloads can go (nothing else needs them).
    pub delete: Vec<String>,
}

/// Compares the songs a playlist had (`previous`) with what it has now.
/// `needed_elsewhere`: songs another downloaded playlist or album still uses.
pub fn playlist_diff(previous: &[String], current: &[String], needed_elsewhere: &HashSet<String>) -> PlaylistDiff {
    let before: HashSet<&str> = previous.iter().map(String::as_str).collect();
    let now: HashSet<&str> = current.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let added: Vec<String> = current
        .iter()
        .filter(|id| !before.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    let mut seen = HashSet::new();
    let removed: Vec<String> = previous
        .iter()
        .filter(|id| !now.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    let delete = removed
        .iter()
        .filter(|id| !needed_elsewhere.contains(id.as_str()))
        .cloned()
        .collect();
    PlaylistDiff { added, removed, delete }
}

/// What one sync pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlaylistSyncReport {
    pub playlists: usize,
    pub downloaded: usize,
    pub removed: usize,
    pub failed: usize,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{playlist_diff, PlaylistSyncReport};
    use crate::api::{MusicClient, ServerConfig};
    use crate::db::AppSettings;
    use crate::offline_audio::{
        download_songs_batch, is_song_downloaded, list_downloaded_collections, load_collection_membership_index,
        mark_collection_downloaded, mark_playlist_auto_download_tracked, remove_downloaded_song,
        sync_downloaded_collection_members, DownloadCollectionEntry,
    };
    use std::collections::HashSet;

    /// Songs (ids on `server_id`) that something other than `playlist_id`
    /// keeps on the device: other downloaded playlists, complete albums.
    fn needed_elsewhere(
        server_id: &str,
        playlist_id: &str,
        collections: &[DownloadCollectionEntry],
    ) -> HashSet<String> {
        let keeps = |kind: &str, id: &str| {
            collections.iter().any(|c| {
                c.kind == kind
                    && c.server_id == server_id
                    && c.collection_id == id
                    && match kind {
                        "playlist" => c.auto_download_tracked && id != playlist_id,
                        // A whole album the user downloaded, not one song of it.
                        _ => {
                            let total = c.effective_total_song_count();
                            total > 0 && c.effective_downloaded_song_count() >= total
                        }
                    }
            })
        };
        load_collection_membership_index()
            .into_iter()
            .filter(|m| m.server_id == server_id && keeps(&m.kind, &m.collection_id))
            .flat_map(|m| m.song_ids)
            .collect()
    }

    /// Syncs every downloaded playlist on the active servers. `keep`:
    /// `server::song` keys never removed this pass (the queue).
    pub async fn sync_downloaded_playlists(
        servers: &[ServerConfig],
        settings: &AppSettings,
        keep: &HashSet<String>,
    ) -> PlaylistSyncReport {
        let mut report = PlaylistSyncReport::default();
        if settings.offline_mode || !settings.downloads_enabled {
            return report;
        }
        let tracked: Vec<_> = list_downloaded_collections()
            .into_iter()
            .filter(|c| c.kind == "playlist" && c.auto_download_tracked)
            .collect();
        for entry in tracked {
            // Jellyfin playlists sync too, so this goes through MusicClient.
            let Some(client) = ServerConfig::find(servers, &entry.server_id)
                .filter(|s| s.active)
                .cloned()
                .map(MusicClient::new)
            else {
                continue;
            };
            let Ok((playlist, songs)) = client.get_playlist(&entry.collection_id).await else {
                continue;
            };
            report.playlists += 1;
            let previous: Vec<String> = load_collection_membership_index()
                .into_iter()
                .find(|m| m.kind == "playlist" && m.server_id == entry.server_id && m.collection_id == entry.collection_id)
                .map(|m| m.song_ids)
                .unwrap_or_default();
            let current: Vec<String> = songs.iter().map(|s| s.id.clone()).collect();
            let mut needed = needed_elsewhere(&entry.server_id, &entry.collection_id, &list_downloaded_collections());
            needed.extend(
                keep.iter()
                    .filter_map(|key| key.split_once("::"))
                    .filter(|(server_id, _)| *server_id == entry.server_id)
                    .map(|(_, id)| id.to_string()),
            );
            let diff = playlist_diff(&previous, &current, &needed);
            for id in &diff.delete {
                report.removed += remove_downloaded_song(&entry.server_id, id).min(1);
            }

            let missing: Vec<_> = songs.iter().filter(|s| !is_song_downloaded(s)).cloned().collect();
            if !missing.is_empty() {
                let batch = download_songs_batch(&missing, servers, settings).await;
                report.downloaded += batch.downloaded;
                report.failed += batch.failed;
            }
            mark_collection_downloaded("playlist", &entry.server_id, &playlist.id, &playlist.name, songs.len());
            mark_playlist_auto_download_tracked(&entry.server_id, &playlist.id, &playlist.name, songs.len());
            sync_downloaded_collection_members("playlist", &entry.server_id, &playlist.id, &songs);
        }
        report
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::sync_downloaded_playlists;

/// The web build keeps no downloads.
#[cfg(target_arch = "wasm32")]
pub async fn sync_downloaded_playlists(
    _servers: &[crate::api::ServerConfig],
    _settings: &crate::db::AppSettings,
    _keep: &HashSet<String>,
) -> PlaylistSyncReport {
    PlaylistSyncReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_added_and_removed_songs() {
        let diff = playlist_diff(&ids(&["a", "b", "c"]), &ids(&["b", "c", "d", "e"]), &HashSet::new());
        assert_eq!(diff.added, ids(&["d", "e"]));
        assert_eq!(diff.removed, ids(&["a"]));
        assert_eq!(diff.delete, ids(&["a"]));
    }

    #[test]
    fn keeps_songs_another_download_needs() {
        let shared: HashSet<String> = ["a".to_string()].into_iter().collect();
        let diff = playlist_diff(&ids(&["a", "b"]), &ids(&[]), &shared);
        assert_eq!(diff.removed, ids(&["a", "b"]));
        assert_eq!(diff.delete, ids(&["b"]));
    }

    #[test]
    fn no_change_and_reorders_are_quiet() {
        let diff = playlist_diff(&ids(&["a", "b", "c"]), &ids(&["c", "a", "b"]), &HashSet::new());
        assert_eq!(diff, PlaylistDiff::default());
    }

    #[test]
    fn duplicates_count_once() {
        let diff = playlist_diff(&ids(&["a", "a"]), &ids(&["b", "b", "a"]), &HashSet::new());
        assert_eq!(diff.added, ids(&["b"]));
        assert!(diff.removed.is_empty());
        let diff = playlist_diff(&ids(&["x", "x"]), &ids(&[]), &HashSet::new());
        assert_eq!(diff.delete, ids(&["x"]));
    }

    #[test]
    fn first_sync_downloads_everything() {
        let diff = playlist_diff(&[], &ids(&["a", "b"]), &HashSet::new());
        assert_eq!(diff.added, ids(&["a", "b"]));
        assert!(diff.delete.is_empty());
    }
}
