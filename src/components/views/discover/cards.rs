//! Library cards for the Home shelves: albums (open; hover play on wide),
//! server playlists (open) and songs (play; "…" opens the track menu).

use super::actions::{play_album, use_player, Toast};
use super::shelf::Card;
use crate::api::discovery::{DiscoverAlbum, DiscoverSong};
use crate::api::{Playlist, ServerConfig, Song};
use crate::components::ui::{Artwork, TrackMenuController};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

/// Artwork request size for a card (the card is ~150–220 CSS px wide).
pub const CARD_ART_PX: u32 = 200;

/// An album card. Tapping opens the album; on wide layouts a round play
/// button appears over the art on hover or keyboard focus.
/// * `reason`: the quiet third line ("Because you play …", "Added …").
#[component]
pub fn AlbumCard(pick: DiscoverAlbum, #[props(default)] show_reason: bool) -> Element {
    let navigation = use_context::<Navigation>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let toast = use_context::<Toast>();
    let player = use_player();
    let album = pick.album.clone();
    let target = AppView::AlbumDetailView {
        album_id: album.id.clone(),
        server_id: album.server_id.clone(),
    };
    let to_play = album.clone();
    rsx! {
        Card {
            title: album.name.clone(),
            subtitle: album.artist.clone(),
            reason: show_reason.then(|| pick.reason.clone()),
            testid: "album-card".to_string(),
            art: rsx! {
                Artwork {
                    cover_art: album.cover_art.clone(),
                    server_id: album.server_id.clone(),
                    size_px: CARD_ART_PX,
                    fill: true,
                    placeholder: "album".to_string(),
                }
            },
            overlay: rsx! {
                button {
                    r#type: "button",
                    class: "rs-hcard__play rs-hcard__play--hover",
                    aria_label: "Play {album.name}",
                    title: "Play",
                    onclick: move |_| play_album(player, toast, servers, to_play.clone()),
                    Icon { name: "play".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            },
            onclick: move |_| navigation.navigate_to(target.clone()),
        }
    }
}

/// "12 songs" / "1 song".
pub fn songs_label(count: usize) -> String {
    if count == 1 {
        "1 song".to_string()
    } else {
        format!("{count} songs")
    }
}

/// A server playlist card: tapping opens the playlist.
#[component]
pub fn PlaylistCard(playlist: Playlist) -> Element {
    let navigation = use_context::<Navigation>();
    let target = AppView::PlaylistDetailView {
        playlist_id: playlist.id.clone(),
        server_id: playlist.server_id.clone(),
    };
    rsx! {
        Card {
            title: playlist.name.clone(),
            subtitle: songs_label(playlist.song_count as usize),
            testid: "playlist-card".to_string(),
            art: rsx! {
                Artwork {
                    cover_art: playlist.cover_art.clone(),
                    server_id: playlist.server_id.clone(),
                    size_px: CARD_ART_PX,
                    fill: true,
                    placeholder: "playlist".to_string(),
                }
            },
            onclick: move |_| navigation.navigate_to(target.clone()),
        }
    }
}

/// A song card. Tapping plays the shelf from this song; "…", right-click or
/// a long-press opens the track menu.
/// * `queue`: the whole shelf's songs (shared, not cloned per card).
#[component]
pub fn SongCard(pick: DiscoverSong, index: usize, queue: Memo<Vec<Song>>) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let menu = use_context::<TrackMenuController>();
    let song = pick.song.clone();
    let menu_song = song.clone();
    rsx! {
        Card {
            title: song.title.clone(),
            subtitle: song.artist.clone().unwrap_or_default(),
            reason: Some(pick.reason.clone()),
            testid: "song-card".to_string(),
            art: rsx! {
                Artwork {
                    cover_art: song.cover_art.clone(),
                    server_id: song.server_id.clone(),
                    size_px: CARD_ART_PX,
                    fill: true,
                    placeholder: "music".to_string(),
                }
            },
            onclick: move |_| {
                if let Err(message) = player.play_at(queue.peek().clone(), index) {
                    toast.show(message, true);
                }
            },
            on_menu: move |anchor| {
                let mut menu = menu;
                menu.open(menu_song.clone(), anchor);
            },
        }
    }
}
