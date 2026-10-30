//! Home's library shelves, top to bottom: Playlists (with mixes), Songs you
//! might like, Albums you might like, New in your library and Rewind.
//! Each reads only its own signal; `None` shows a skeleton and an empty list
//! hides the shelf.

use super::cards::{AlbumCard, PlaylistCard, SongCard};
use super::data::HomeData;
use super::mixes::MixCard;
use super::shelf::{Shelf, ShelfSkeleton};
use crate::api::discovery::{DiscoverAlbum, DiscoverSong, HomePlaylist};
use crate::api::Song;
use dioxus::prelude::*;

/// Playlists: generated mixes (tagged "Mix") then the user's playlists.
#[component]
pub fn PlaylistsShelf() -> Element {
    let data = use_context::<HomeData>();
    let title = "Playlists".to_string();
    let testid = "shelf-playlists".to_string();
    let items = data.playlists.read();
    match items.as_ref() {
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        Some(list) if list.is_empty() => rsx! {},
        Some(list) => rsx! {
            Shelf { title, testid,
                for item in list.iter() {
                    match item {
                        HomePlaylist::Mix(mix) => rsx! {
                            MixCard { key: "{item.key()}", mix: mix.clone() }
                        },
                        HomePlaylist::Playlist(playlist) => rsx! {
                            PlaylistCard { key: "{item.key()}", playlist: playlist.clone() }
                        },
                    }
                }
            }
        },
    }
}

/// Songs you might like: tap plays the shelf from that song.
#[component]
pub fn SongsShelf() -> Element {
    let data = use_context::<HomeData>();
    // One shared queue for every card (no per-card list clones).
    let queue = use_memo(move || {
        data.songs
            .read()
            .as_ref()
            .map(|list| list.iter().map(|p| p.song.clone()).collect::<Vec<Song>>())
            .unwrap_or_default()
    });
    let title = "Songs You Might Like".to_string();
    let testid = "shelf-songs".to_string();
    let songs = data.songs.read();
    match songs.as_ref() {
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        Some(list) if list.is_empty() => rsx! {},
        Some(list) => rsx! {
            Shelf { title, testid,
                for (index, pick) in list.iter().enumerate() {
                    SongCard {
                        key: "{pick.song.server_id}::{pick.song.id}",
                        pick: pick.clone(),
                        index,
                        queue,
                    }
                }
            }
        },
    }
}

/// A shelf of album cards from one of Home's album signals.
#[component]
fn AlbumShelf(
    title: String,
    testid: String,
    albums: Option<Vec<DiscoverAlbum>>,
    #[props(default)] show_reason: bool,
) -> Element {
    match albums {
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        Some(list) if list.is_empty() => rsx! {},
        Some(list) => rsx! {
            Shelf { title, testid,
                for pick in list {
                    AlbumCard {
                        key: "{pick.album.server_id}::{pick.album.id}",
                        pick: pick.clone(),
                        show_reason,
                    }
                }
            }
        },
    }
}

/// Albums you might like ("Because you play X" as the quiet third line).
#[component]
pub fn AlbumsLikeShelf() -> Element {
    let data = use_context::<HomeData>();
    let albums = data.albums_like.read().clone();
    rsx! {
        AlbumShelf {
            title: "Albums You Might Like".to_string(),
            testid: "shelf-albums".to_string(),
            albums,
            show_reason: true,
        }
    }
}

/// New in your library ("Added 2 days ago").
#[component]
pub fn NewShelf() -> Element {
    let data = use_context::<HomeData>();
    let albums = data.new_albums.read().clone();
    rsx! {
        AlbumShelf {
            title: "New in Your Library".to_string(),
            testid: "shelf-new".to_string(),
            albums,
            show_reason: true,
        }
    }
}

/// Haven't heard in a while: a random pick of albums played a lot but not in
/// the last six months, fresh on every load. Falls back to songs when no album
/// qualifies, and is hidden when nothing does.
#[component]
pub fn RewindShelf() -> Element {
    let data = use_context::<HomeData>();
    let section = data.rewind.read().clone();
    let title = "Haven't Heard in a While".to_string();
    let testid = "shelf-rewind".to_string();

    match section {
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        Some(section) if section.albums.is_empty() && section.songs.is_empty() => rsx! {},
        Some(section) if section.albums.is_empty() => rsx! {
            RewindSongs { title, testid, picks: section.songs }
        },
        Some(section) => rsx! {
            AlbumShelf {
                title,
                testid,
                albums: Some(section.albums),
                show_reason: true,
            }
        },
    }
}

/// Rewind as song cards (when no album qualifies but songs do).
#[component]
fn RewindSongs(title: String, testid: String, picks: Vec<DiscoverSong>) -> Element {
    let queue = use_memo(use_reactive!(|picks| picks
        .iter()
        .map(|p| p.song.clone())
        .collect::<Vec<Song>>()));
    rsx! {
        Shelf { title, testid,
            for (index, pick) in picks.iter().enumerate() {
                SongCard {
                    key: "{pick.song.server_id}::{pick.song.id}",
                    pick: pick.clone(),
                    index,
                    queue,
                }
            }
        }
    }
}
