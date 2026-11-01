//! Playback shared by the Home shelves. Music starts through `PlayerCtx`;
//! messages go to the shell's status toast, whose scope also runs the tasks
//! that must outlive the card or menu that started them.

use crate::api::{Album, MusicClient, QueueSourceKind};
use crate::components::ui::StatusToast;
use crate::components::PlayerCtx;
use dioxus::prelude::*;

/// Loads `album`'s songs and plays them from the top. Errors go to the toast.
pub fn play_album(player: PlayerCtx, toast: StatusToast, album: Album) {
    let Some(client) = MusicClient::for_source(&player.servers.peek(), &album.server_id) else {
        toast.show("That album's server isn't connected.", true);
        return;
    };
    toast.spawn(async move {
        match client.get_album(&album.id).await {
            Ok((_, songs)) if !songs.is_empty() => {
                let source = Some((QueueSourceKind::Album, album.key()));
                if let Err(message) = player.play(songs, 0, source) {
                    toast.show(message, true);
                }
            }
            Ok(_) => toast.show(format!("“{}” has no playable songs.", album.name), true),
            Err(_) => toast.show(format!("Couldn't load “{}”.", album.name), true),
        }
    });
}
