//! Lift and carry (desktop): drag a song row with the mouse onto a playlist
//! or the queue.
//!
//! `row_gestures.js` does the dragging: past a few pixels of mouse travel on
//! a `TrackRow` (and while the page has a drop target) it lifts a small copy
//! of the row, highlights the target under the pointer (`.rs-drop-over`) and
//! posts the drop. The row learns it is carried from a synthetic
//! `dragstart` (`TrackRow`'s `ondragstart` calls [`carry_song`]), so the drop
//! message only needs the song's key.
//!
//! Drop targets are plain attributes on any element, no Rust handler needed:
//! * `data-drop-queue`: the song goes to the end of the queue (the player
//!   bar, Up Next, a Queue item in the sidebar).
//! * `data-drop-playlist="<playlist id>"`, plus `data-drop-server="<server
//!   id>"` when the playlist may live on another server than the song: the
//!   song is added to that playlist.
//!
//! For chunk D's sidebar playlists:
//!
//! ```ignore
//! button {
//!     class: "rs-nav",
//!     "data-drop-playlist": "{playlist.id}",
//!     "data-drop-server": "{playlist.server_id}",
//!     …
//! }
//! ```
//!
//! [`use_song_drop_host`] (called once by `TrackMenuHost`) installs the
//! gesture script for the app and carries out the drops, with a status
//! toast either way.

use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::ui::row_gestures::row_gestures_script;
use crate::components::ui::StatusToast;
use crate::components::{PlayerCtx, QueuePosition};
use dioxus::prelude::*;
use serde::Deserialize;

/// The song being carried, set by the row when the carry starts.
static CARRIED: GlobalSignal<Option<Song>> = Signal::global(|| None);

/// Marks `song` as the one being carried (`TrackRow`'s `ondragstart`).
pub fn carry_song(song: &Song) {
    *CARRIED.write() = Some(song.clone());
}

#[derive(Deserialize)]
struct DropMessage {
    kind: String,
    key: String,
    #[serde(default)]
    queue: bool,
    #[serde(default)]
    playlist: Option<String>,
    #[serde(default)]
    server: Option<String>,
}

/// Installs `row_gestures.js` for the whole app (long-press, browse swipes,
/// carry) and handles carry drops. Call once, from the shell's
/// `TrackMenuHost`.
pub fn use_song_drop_host() {
    let toast = use_context::<StatusToast>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let player = PlayerCtx::from_context();
    use_hook(move || {
        let mut channel = document::eval(&row_gestures_script(
            "rowGestures.setHost((message) => dioxus.send(message));",
        ));
        spawn(async move {
            while let Ok(message) = channel.recv::<DropMessage>().await {
                if message.kind != "drop" {
                    continue;
                }
                let carried = CARRIED.write().take();
                let Some(song) =
                    carried.filter(|s| format!("{}::{}", s.server_id, s.id) == message.key)
                else {
                    continue;
                };
                if message.queue {
                    player.enqueue(vec![song.clone()], QueuePosition::End);
                    toast.show(format!("Added \"{}\" to the queue.", song.title), false);
                } else if let Some(playlist) = message.playlist {
                    let server_id = message.server.unwrap_or_else(|| song.server_id.clone());
                    add_to_playlist(toast, &servers.peek(), song, playlist, server_id);
                }
            }
        });
    });
}

/// Adds the carried song to a playlist on its own server.
fn add_to_playlist(
    toast: StatusToast,
    servers: &[ServerConfig],
    song: Song,
    playlist: String,
    server_id: String,
) {
    if server_id != song.server_id {
        toast.show("Songs can only go to playlists on their own server.", true);
        return;
    }
    let Some(server) = servers.iter().find(|s| s.id == server_id).cloned() else {
        toast.show("That playlist's server isn't available.", true);
        return;
    };
    toast.spawn(async move {
        let result = NavidromeClient::new(server)
            .add_songs_to_playlist(&playlist, &[song.id.clone()])
            .await;
        match result {
            Ok(_) => toast.show(format!("Added \"{}\" to the playlist.", song.title), false),
            Err(err) => toast.show(format!("Couldn't add the song: {err}"), true),
        }
    });
}
