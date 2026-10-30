//! Home's library shelves, top to bottom: Playlists (with mixes), Songs you
//! might like, Albums you might like, New in your library and Rewind.
//! Each reads only its own signal; `None` shows a skeleton and an empty list
//! hides the shelf.

use super::cards::{AlbumCard, PlaylistCard, SongCard};
use super::data::HomeData;
use super::mixes::MixCard;
use super::shelf::{Shelf, ShelfSkeleton};
use crate::api::discovery::{DiscoverAlbum, DiscoverSong, HomePlaylist, REWIND_MONTH_CHOICES};
use crate::api::Song;
use crate::components::ui::Segmented;
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

/// Short label for a Rewind window in the shelf header.
pub fn window_label(months: u32) -> String {
    match months {
        12 => "1 yr".to_string(),
        m => format!("{m} mo"),
    }
}

/// Spoken label for a Rewind window.
pub fn window_name(months: u32) -> String {
    match months {
        12 => "1 year".to_string(),
        1 => "1 month".to_string(),
        m => format!("{m} months"),
    }
}

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
    #[props(default)] control: Option<Element>,
) -> Element {
    match albums {
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        Some(list) if list.is_empty() => rsx! {},
        Some(list) => rsx! {
            Shelf { title, testid, control,
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

/// Haven't heard in a while: albums played a lot but not within the chosen
/// window, with a small 3 / 6 / 12 month control in the header (remembered
/// per device). Falls back to songs when no album qualifies.
#[component]
pub fn RewindShelf() -> Element {
    let data = use_context::<HomeData>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let months = (data.rewind_months)();
    // Skeleton while the chosen window (re)loads.
    let section = data
        .rewind
        .read()
        .clone()
        .filter(|section| section.months == months);
    let title = "Haven't Heard in a While".to_string();
    let testid = "shelf-rewind".to_string();

    let selected = REWIND_MONTH_CHOICES
        .iter()
        .position(|m| *m == months)
        .unwrap_or(1);
    let pick = move |index: usize| {
        let Some(months) = REWIND_MONTH_CHOICES.get(index).copied() else {
            return;
        };
        let mut next = settings.peek().clone();
        if next.discover_rewind_months == months {
            return;
        }
        next.discover_rewind_months = months;
        settings.set(next.clone());
        spawn(async move {
            let _ = save_settings(next).await;
        });
    };
    let control = rsx! {
        Segmented {
            options: REWIND_MONTH_CHOICES.iter().map(|m| window_label(*m)).collect::<Vec<_>>(),
            selected,
            inline: true,
            label: format!("Not played in the last {}", window_name(months)),
            testid: "rewind-window".to_string(),
            on_select: pick,
        }
    };

    match section {
        // Switching windows reads the cached snapshot, so this is brief.
        None => rsx! {
            ShelfSkeleton { title, testid }
        },
        // 3 months is the widest window: empty there means empty everywhere.
        Some(section) if section.albums.is_empty() && section.songs.is_empty() && months == 3 => rsx! {},
        Some(section) if section.albums.is_empty() && section.songs.is_empty() => rsx! {
            // Nothing for this window: keep the control so a shorter one can be tried.
            section { class: "rs-shelf", "data-testid": "{testid}",
                div { class: "rs-shelf__hd",
                    h2 { class: "rs-shelf__title", "{title}" }
                    div { class: "rs-shelf__control", {control} }
                }
                p { class: "rs-home__note",
                    "Nothing you played a lot has been quiet for {window_name(months)}."
                }
            }
        },
        Some(section) if section.albums.is_empty() => rsx! {
            RewindSongs { title, testid, control, picks: section.songs }
        },
        Some(section) => rsx! {
            AlbumShelf {
                title,
                testid,
                albums: Some(section.albums),
                show_reason: true,
                control,
            }
        },
    }
}

/// Rewind as song cards (when no album qualifies but songs do).
#[component]
fn RewindSongs(title: String, testid: String, control: Element, picks: Vec<DiscoverSong>) -> Element {
    let queue = use_memo(use_reactive!(|picks| picks
        .iter()
        .map(|p| p.song.clone())
        .collect::<Vec<Song>>()));
    rsx! {
        Shelf { title, testid, control,
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

#[cfg(test)]
mod tests {
    use super::{window_label, window_name};

    #[test]
    fn window_labels() {
        assert_eq!(window_label(3), "3 mo");
        assert_eq!(window_label(6), "6 mo");
        assert_eq!(window_label(12), "1 yr");
        assert_eq!(window_name(6), "6 months");
        assert_eq!(window_name(12), "1 year");
    }
}
