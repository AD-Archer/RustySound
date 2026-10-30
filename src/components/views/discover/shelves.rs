//! Album and song shelves for the "New" and "Rewind" segments.

use super::actions::{use_player, Toast};
use super::DiscoverData;
use crate::api::discovery::{DiscoverAlbum, DiscoverSong, REWIND_MONTH_CHOICES};
use crate::api::Song;
use crate::components::ui::{
    Artwork, InsetList, ListInset, ListRow, SectionHeader, Segmented, Skeleton, TrackRow,
};
use crate::components::{AppView, Navigation};
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

/// Rows shown before "See All".
const COLLAPSED_ROWS: usize = 6;

/// Label for a Rewind window.
pub fn window_label(months: u32) -> String {
    match months {
        12 => "1 year".to_string(),
        1 => "1 month".to_string(),
        m => format!("{m} months"),
    }
}

/// "New in your library" (from the stage section).
#[component]
pub fn NewShelf() -> Element {
    let data = use_context::<DiscoverData>();
    let albums = data
        .stage
        .read()
        .as_ref()
        .map(|section| section.new_in_library.clone());
    rsx! {
        AlbumShelf {
            title: "New in your library".to_string(),
            albums,
            empty: "Nothing new on your server lately.".to_string(),
            testid: "new-albums".to_string(),
        }
    }
}

/// A titled list of albums with reasons; rows open the album.
/// `albums: None` shows a skeleton.
#[component]
pub fn AlbumShelf(
    title: String,
    albums: Option<Vec<DiscoverAlbum>>,
    empty: String,
    testid: String,
) -> Element {
    let navigation = use_context::<Navigation>();
    let mut expanded = use_signal(|| false);
    let total = albums.as_ref().map(Vec::len).unwrap_or(0);
    let can_expand = total > COLLAPSED_ROWS;
    let shown = if expanded() { total } else { total.min(COLLAPSED_ROWS) };
    rsx! {
        SectionHeader {
            title: title.clone(),
            action: can_expand.then(|| if expanded() { "Show Less" } else { "See All" }.to_string()),
            on_action: if can_expand { Some(EventHandler::new(move |_: MouseEvent| expanded.toggle())) } else { None },
        }
        match albums {
            None => rsx! {
                Skeleton { rows: 4 }
            },
            Some(list) if list.is_empty() => rsx! {
                InsetList { inset: ListInset::Text, label: title.clone(),
                    li { class: "rs-discover__quiet", "{empty}" }
                }
            },
            Some(list) => rsx! {
                InsetList { label: title.clone(), testid: testid.clone(),
                    for pick in list.into_iter().take(shown) {
                        ListRow {
                            key: "{pick.album.server_id}::{pick.album.id}",
                            title: pick.album.name.clone(),
                            subtitle: pick.album.artist.clone(),
                            why: pick.reason.clone(),
                            testid: "discover-album".to_string(),
                            leading: rsx! {
                                Artwork {
                                    cover_art: pick.album.cover_art.clone(),
                                    server_id: pick.album.server_id.clone(),
                                    size_px: 44,
                                    placeholder: "album".to_string(),
                                }
                            },
                            onclick: move |_| {
                                navigation.navigate_to(AppView::AlbumDetailView {
                                    album_id: pick.album.id.clone(),
                                    server_id: pick.album.server_id.clone(),
                                })
                            },
                        }
                    }
                }
            },
        }
    }
}

/// The Rewind segment: window picker (3 / 6 / 12 months), albums and songs.
#[component]
pub fn RewindPanel(wide: bool) -> Element {
    let data = use_context::<DiscoverData>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let months = (data.rewind_months)();
    let selected = REWIND_MONTH_CHOICES
        .iter()
        .position(|m| *m == months)
        .unwrap_or(1);
    let options: Vec<String> = REWIND_MONTH_CHOICES.iter().map(|m| window_label(*m)).collect();

    // Show a skeleton while the chosen window is (re)loading.
    let section = data
        .rewind
        .read()
        .clone()
        .flatten()
        .filter(|section| section.months == months);
    let albums = section.as_ref().map(|s| s.albums.clone());
    let songs = section.map(|s| s.songs);

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

    let window = window_label(months);
    let shelves = rsx! {
        div { class: "rs-discover__col",
            AlbumShelf {
                title: "Albums".to_string(),
                albums,
                empty: format!("No albums you played a lot but not in the last {window}."),
                testid: "rewind-albums".to_string(),
            }
        }
        div { class: "rs-discover__col",
            SongShelf { songs, window: window.clone() }
        }
    };

    rsx! {
        div { class: "rs-discover__window",
            Segmented {
                options,
                selected,
                inline: true,
                label: "Rewind window".to_string(),
                testid: "rewind-window".to_string(),
                on_select: pick,
            }
            p { class: "rs-discover__window-note", "Played a lot, not in the last {window}" }
        }
        if wide {
            div { class: "rs-discover__cols", {shelves} }
        } else {
            {shelves}
        }
    }
}

/// Rewind songs as track rows (tap plays from that song; long-press for the menu).
#[component]
fn SongShelf(songs: Option<Vec<DiscoverSong>>, window: String) -> Element {
    let player = use_player();
    let toast = use_context::<Toast>();
    let mut expanded = use_signal(|| false);
    let total = songs.as_ref().map(Vec::len).unwrap_or(0);
    let can_expand = total > COLLAPSED_ROWS;
    let shown = if expanded() { total } else { total.min(COLLAPSED_ROWS) };
    let queue: Vec<Song> = songs
        .as_ref()
        .map(|list| list.iter().map(|s| s.song.clone()).collect())
        .unwrap_or_default();
    rsx! {
        SectionHeader {
            title: "Songs".to_string(),
            action: can_expand.then(|| if expanded() { "Show Less" } else { "See All" }.to_string()),
            on_action: if can_expand { Some(EventHandler::new(move |_: MouseEvent| expanded.toggle())) } else { None },
        }
        match songs {
            None => rsx! {
                Skeleton { rows: 4 }
            },
            Some(list) if list.is_empty() => rsx! {
                InsetList { inset: ListInset::Text, label: "Songs".to_string(),
                    li { class: "rs-discover__quiet", "No songs you played a lot but not in the last {window}." }
                }
            },
            Some(list) => rsx! {
                InsetList { label: "Rewind songs".to_string(), testid: "rewind-songs".to_string(),
                    for (index, pick) in list.into_iter().take(shown).enumerate() {
                        TrackRow {
                            key: "{pick.song.server_id}::{pick.song.id}",
                            song: pick.song.clone(),
                            subtitle: format!(
                                "{} · {}",
                                pick.song.artist.clone().unwrap_or_default(),
                                pick.reason
                            ),
                            show_duration: false,
                            index,
                            on_play: {
                                let queue = queue.clone();
                                move |_| {
                                    if let Err(message) = player.play_at(queue.clone(), index) {
                                        toast.show(message, true);
                                    }
                                }
                            },
                        }
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::window_label;

    #[test]
    fn window_labels() {
        assert_eq!(window_label(3), "3 months");
        assert_eq!(window_label(6), "6 months");
        assert_eq!(window_label(12), "1 year");
    }
}
