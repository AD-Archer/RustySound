//! Editing a playlist's songs: drag to reorder, remove (swipe, full swipe or
//! the track menu) with Undo, and Add Songs. Changes show at once and go to
//! the server one call at a time, in order (each call's song indexes assume
//! the ones before it landed); a failed call rolls its change back.

use crate::api::{Capabilities, NavidromeClient, Playlist, ServerConfig, Song};
use crate::components::ui::env::sleep_ms;
use crate::components::ui::{use_row_gestures, RowGesture};
use crate::components::views::library::shared::{is_auto_imported, song_key};
use crate::db::AppSettings;
use crate::offline_audio::{
    is_playlist_auto_download_tracked, mark_collection_downloaded,
    mark_playlist_auto_download_tracked, prefetch_song_audio_with_origin,
    sync_downloaded_collection_members, DownloadOrigin,
};
use dioxus::prelude::*;

/// The list id the playlist's rows register under.
pub const PLAYLIST_LIST: &str = "playlist";

/// Label of the track menu's remove item on an editable playlist.
pub const REMOVE_FROM_PLAYLIST: &str = "Remove from Playlist";

/// The song a remove took out, and where it was.
#[derive(Clone, PartialEq)]
pub struct PlaylistUndo {
    pub song: Song,
    pub index: usize,
    /// Restarts the toast's timer for each new remove.
    pub generation: u64,
}

/// The user can change this playlist here: it is theirs (Navidrome refuses
/// edits to other users' public playlists), it wasn't auto-imported from a
/// playlist file (read-only on the server), and its server is configured,
/// active and can edit playlists (Jellyfin playlists are read-only here).
pub fn playlist_editable(playlist: &Playlist, server: Option<&ServerConfig>) -> bool {
    let Some(server) = server else {
        return false;
    };
    let own = playlist
        .owner
        .as_deref()
        .map(|owner| owner.trim().is_empty() || owner.eq_ignore_ascii_case(&server.username))
        .unwrap_or(true);
    server.active && own && !is_auto_imported(playlist) && Capabilities::of(server).playlist_edits
}

type PlaylistData = Resource<Result<(Playlist, Vec<Song>), String>>;

#[derive(Clone, Copy)]
pub struct PlaylistEdits {
    songs: Signal<Vec<Song>>,
    data: PlaylistData,
    meta_override: Signal<Option<Playlist>>,
    servers: Memo<Vec<ServerConfig>>,
    settings: Signal<AppSettings>,
    status: Signal<Option<String>>,
    retry: Signal<u32>,
    saving: Signal<bool>,
    generation: Signal<u64>,
    pub undo: Signal<Option<PlaylistUndo>>,
}

/// One server call at a time.
async fn take_turn(mut saving: Signal<bool>) {
    while *saving.peek() {
        sleep_ms(30).await;
    }
    saving.set(true);
}

impl PlaylistEdits {
    /// The server and playlist, when the playlist can be edited.
    fn target(&self) -> Option<(ServerConfig, Playlist)> {
        let playlist = self.meta_override.peek().clone().or_else(|| match &*self.data.peek() {
            Some(Ok((playlist, _))) => Some(playlist.clone()),
            _ => None,
        })?;
        let server = self
            .servers
            .peek()
            .iter()
            .find(|s| s.id == playlist.server_id)
            .cloned()?;
        playlist_editable(&playlist, Some(&server)).then_some((server, playlist))
    }

    /// The song at `index`, if it is still the one with `key`.
    fn entry(&self, index: usize, key: &str) -> Option<Song> {
        self.songs
            .peek()
            .get(index)
            .filter(|song| song_key(song) == key)
            .cloned()
    }

    /// Moves a song and saves the whole order.
    pub fn move_song(&self, from: usize, to: usize) {
        let Some((server, playlist)) = self.target() else {
            return;
        };
        let (mut songs, mut status, mut retry, mut saving) =
            (self.songs, self.status, self.retry, self.saving);
        let before = songs.peek().clone();
        if from >= before.len() || to >= before.len() || from == to {
            return;
        }
        let mut after = before.clone();
        let song = after.remove(from);
        after.insert(to, song);
        let ids: Vec<String> = after.iter().map(|s| s.id.clone()).collect();
        songs.set(after);
        spawn(async move {
            take_turn(saving).await;
            let client = NavidromeClient::new(server);
            let result = client.reorder_playlist(&playlist.id, &ids, ids.len()).await;
            saving.set(false);
            match result {
                Ok(()) => client.refresh_playlist_cache(),
                Err(err) => {
                    // Roll back, unless the list changed again since; then
                    // the server's copy is the truth.
                    let unchanged = songs.peek().iter().map(|s| &s.id).eq(ids.iter());
                    if unchanged {
                        songs.set(before);
                    } else {
                        retry += 1;
                    }
                    status.set(Some(format!("Couldn't save the new order: {err}")));
                }
            }
        });
    }

    /// Removes the song at `index` (if it is still `key`) and offers Undo.
    pub fn remove(&self, index: usize, key: &str) {
        let Some((server, playlist)) = self.target() else {
            return;
        };
        let Some(song) = self.entry(index, key) else {
            return;
        };
        let (mut songs, mut status, mut saving, mut undo, mut generation) =
            (self.songs, self.status, self.saving, self.undo, self.generation);
        songs.write().remove(index);
        let next = *generation.peek() + 1;
        generation.set(next);
        undo.set(Some(PlaylistUndo {
            song: song.clone(),
            index,
            generation: next,
        }));
        spawn(async move {
            take_turn(saving).await;
            let client = NavidromeClient::new(server);
            let result = client.remove_songs_from_playlist(&playlist.id, &[index]).await;
            saving.set(false);
            match result {
                Ok(()) => client.refresh_playlist_cache(),
                Err(err) => {
                    songs.with_mut(|list| list.insert(index.min(list.len()), song));
                    if undo.peek().as_ref().map(|u| u.generation) == Some(next) {
                        undo.set(None);
                    }
                    status.set(Some(format!("Couldn't remove the song: {err}")));
                }
            }
        });
    }

    /// Puts the last removed song back where it was.
    pub fn undo_remove(&self) {
        let mut undo = self.undo;
        let Some(entry) = undo.take() else {
            return;
        };
        let Some((server, playlist)) = self.target() else {
            return;
        };
        let (mut songs, mut status, mut retry, mut saving) =
            (self.songs, self.status, self.retry, self.saving);
        let mut list = songs.peek().clone();
        list.insert(entry.index.min(list.len()), entry.song);
        let ids: Vec<String> = list.iter().map(|s| s.id.clone()).collect();
        // The server still has the list without it.
        let on_server = list.len() - 1;
        songs.set(list);
        spawn(async move {
            take_turn(saving).await;
            let client = NavidromeClient::new(server);
            let result = client.reorder_playlist(&playlist.id, &ids, on_server).await;
            saving.set(false);
            match result {
                Ok(()) => client.refresh_playlist_cache(),
                Err(err) => {
                    retry += 1;
                    status.set(Some(format!("Couldn't put the song back: {err}")));
                }
            }
        });
    }

    /// Appends a song (the Add Songs sheet), keeping a downloaded playlist
    /// complete.
    pub fn add(&self, song: Song) {
        let Some((server, playlist)) = self.target() else {
            return;
        };
        let (mut songs, mut status, mut saving) = (self.songs, self.status, self.saving);
        let settings = self.settings.peek().clone();
        let servers_now = self.servers.peek().clone();
        spawn(async move {
            take_turn(saving).await;
            let client = NavidromeClient::new(server);
            let result = client.add_songs_to_playlist(&playlist.id, &[song.id.clone()]).await;
            saving.set(false);
            if let Err(err) = result {
                status.set(Some(format!("Couldn't add “{}”: {err}", song.title)));
                return;
            }
            client.refresh_playlist_cache();
            songs.write().push(song.clone());
            if is_playlist_auto_download_tracked(&playlist.server_id, &playlist.id) {
                let total = songs.peek().len();
                mark_collection_downloaded("playlist", &playlist.server_id, &playlist.id, &playlist.name, total);
                mark_playlist_auto_download_tracked(&playlist.server_id, &playlist.id, &playlist.name, total);
                if settings.downloads_enabled && settings.auto_downloads_enabled {
                    let mut settings = settings.clone();
                    settings.downloads_enabled = true;
                    let _ = prefetch_song_audio_with_origin(&song, &servers_now, &settings, DownloadOrigin::Auto).await;
                }
                let list = songs.peek().clone();
                sync_downloaded_collection_members("playlist", &playlist.server_id, &playlist.id, &list);
            }
        });
    }
}

/// Creates the playlist's edit state and routes its row gestures: handle
/// drags and removes (editable playlists only). Add to… and Play Next are
/// the rows' own (TrackRow).
#[allow(clippy::too_many_arguments)]
pub fn use_playlist_edits(
    songs: Signal<Vec<Song>>,
    data: PlaylistData,
    meta_override: Signal<Option<Playlist>>,
    servers: Memo<Vec<ServerConfig>>,
    status: Signal<Option<String>>,
    retry: Signal<u32>,
) -> PlaylistEdits {
    let settings = use_context::<Signal<AppSettings>>();
    let edits = PlaylistEdits {
        songs,
        data,
        meta_override,
        servers,
        settings,
        status,
        retry,
        saving: use_signal(|| false),
        generation: use_signal(|| 0),
        undo: use_signal(|| None),
    };
    let on_gesture = use_callback(move |gesture: RowGesture| match gesture {
        RowGesture::Move { from, to, key } => {
            if edits.entry(from, &key).is_some() {
                edits.move_song(from, to);
            }
        }
        RowGesture::Remove { index, key } => edits.remove(index, &key),
    });

    use_row_gestures(PLAYLIST_LIST, on_gesture);
    edits
}
