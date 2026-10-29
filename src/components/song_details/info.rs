//! Info: the song's details as a list, rating and volume on compact, and
//! Related (similar songs, each with an add button).

use super::actions::{is_live_song, same_song, use_player};
use super::meta::{shows_volume, RatingStars, VolumeSlider};
use super::types::SongDetailsController;
use crate::api::{format_duration, NavidromeClient, ServerConfig, Song};
use crate::components::ui::{
    Artwork, IconButton, IconButtonSize, InsetList, ListInset, ListRow, SectionHeader, Skeleton,
};
use crate::components::{AddIntent, AddMenuController, AppView, Navigation};
use dioxus::prelude::*;

const RELATED_COUNT: u32 = 30;

/// A song compared by identity only.
#[derive(Clone)]
struct Seed(Song);

impl PartialEq for Seed {
    fn eq(&self, other: &Self) -> bool {
        same_song(&self.0, &other.0)
    }
}

async fn fetch_related_candidates(client: &NavidromeClient, song: &Song, count: u32) -> Vec<Song> {
    let mut related = client.get_similar_songs2(&song.id, count).await.unwrap_or_default();
    if let Some(artist_id) = song.artist_id.as_deref() {
        related.extend(client.get_similar_songs(artist_id, count).await.unwrap_or_default());
    }
    if related.len() < count as usize {
        if let Some(artist) = song.artist.clone().filter(|name| !name.trim().is_empty()) {
            related.extend(client.get_top_songs(&artist, count).await.unwrap_or_default());
        }
    }
    if related.len() < count as usize {
        let query = format!("{} {}", song.artist.clone().unwrap_or_default(), song.title);
        if let Ok(search) = client.search(&query, 0, 0, count).await {
            related.extend(search.songs);
        }
    }
    related
}

async fn load_related_songs(song: Song, servers: Vec<ServerConfig>) -> Vec<Song> {
    let Some(server) = servers.into_iter().find(|server| server.id == song.server_id) else {
        return Vec::new();
    };
    let client = NavidromeClient::new(server);
    let mut unique = Vec::<Song>::new();
    for candidate in fetch_related_candidates(&client, &song, RELATED_COUNT).await {
        if same_song(&candidate, &song) || unique.iter().any(|seen| same_song(seen, &candidate)) {
            continue;
        }
        unique.push(candidate);
        if unique.len() >= RELATED_COUNT as usize {
            break;
        }
    }
    unique
}

fn detail_row(title: &str, value: Option<String>) -> Option<(String, String)> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| (title.to_string(), v))
}

#[component]
pub(super) fn InfoPanel(song: Song, wide: bool) -> Element {
    let navigation = use_context::<Navigation>();
    let mut controller = use_context::<SongDetailsController>();
    let live = is_live_song(&song);

    let format = song
        .suffix
        .clone()
        .map(|s| s.to_ascii_uppercase())
        .or_else(|| song.content_type.clone());
    let rows: Vec<(String, String)> = [
        detail_row("Artist", song.artist.clone()),
        detail_row("Track", song.track.filter(|t| *t > 0).map(|t| t.to_string())),
        detail_row("Year", song.year.filter(|y| *y > 0).map(|y| y.to_string())),
        detail_row("Genre", song.genre.clone()),
        detail_row("Duration", (song.duration > 0).then(|| format_duration(song.duration))),
        detail_row("Format", format),
        detail_row("Bitrate", song.bitrate.filter(|b| *b > 0).map(|b| format!("{b} kbps"))),
        detail_row("Plays", song.play_count.map(|p| p.to_string())),
        detail_row("Server", Some(song.server_name.clone())),
    ]
    .into_iter()
    .flatten()
    .collect();
    let album = song.album.clone().filter(|a| !a.trim().is_empty());
    let album_target = song.album_id.clone().map(|album_id| AppView::AlbumDetailView {
        album_id,
        server_id: song.server_id.clone(),
    });

    rsx! {
        section { class: "rs-player-info", aria_label: "Song info", "data-testid": "now-playing-info",
            InsetList { inset: ListInset::Text, label: "Details".to_string(),
                if let Some(album) = album {
                    if let Some(target) = album_target {
                        ListRow {
                            title: "Album".to_string(),
                            detail: album,
                            chevron: true,
                            compact: true,
                            onclick: move |_| {
                                controller.close();
                                navigation.navigate_to(target.clone());
                            },
                        }
                    } else {
                        ListRow { title: "Album".to_string(), detail: album, compact: true }
                    }
                }
                for (title, value) in rows {
                    ListRow { key: "{title}", title: title.clone(), detail: value, compact: true }
                }
            }
            if !wide {
                InsetList { inset: ListInset::Text, class: "rs-player-sheet-gap".to_string(),
                    if !live {
                        ListRow {
                            title: "Rating".to_string(),
                            compact: true,
                            trailing: rsx! { RatingStars { song: song.clone() } },
                        }
                    }
                    if shows_volume() {
                        li { class: "rs-player-info__volume", VolumeSlider {} }
                    }
                }
            }
            if !live {
                RelatedSongs { song: song.clone() }
            }
        }
    }
}

#[component]
fn RelatedSongs(song: Song) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let add_menu = use_context::<AddMenuController>();
    let mut player = use_player();
    // Keyed by identity: starring or rating the song doesn't refetch.
    let seed = use_memo(use_reactive!(|song| Seed(song.clone())));
    let related = use_resource(move || {
        let Seed(song) = seed();
        let servers = servers.peek().clone();
        async move { load_related_songs(song, servers).await }
    });

    rsx! {
        SectionHeader { title: "Related".to_string() }
        match related() {
            None => rsx! { Skeleton { rows: 4 } },
            Some(list) if list.is_empty() => rsx! {
                p { class: "rs-list-foot", "No related songs found for this track." }
            },
            Some(list) => rsx! {
                InsetList { label: "Related songs".to_string(), testid: "related-list".to_string(),
                    for entry in list {
                        li { key: "{entry.server_id}:{entry.id}",
                            div { class: "rs-trow rs-trow--art rs-player-related",
                                button {
                                    r#type: "button",
                                    class: "rs-player-related__play",
                                    aria_label: "Play {entry.title}",
                                    onclick: {
                                        let entry = entry.clone();
                                        move |_| {
                                            let found = player.queue.peek().iter().position(|s| same_song(s, &entry));
                                            let index = match found {
                                                Some(index) => index,
                                                None => {
                                                    player.queue.with_mut(|q| q.push(entry.clone()));
                                                    player.queue.peek().len() - 1
                                                }
                                            };
                                            player.play_index(index);
                                        }
                                    },
                                    Artwork { cover_art: entry.cover_art.clone(), server_id: entry.server_id.clone(), size_px: 44 }
                                    span { class: "rs-trow__meta",
                                        span { class: "rs-trow__title", style: "display:block", "{entry.title}" }
                                        span { class: "rs-trow__sub", style: "display:block",
                                            "{entry.artist.clone().unwrap_or_default()}"
                                        }
                                    }
                                    span { class: "rs-trow__dur", "{format_duration(entry.duration)}" }
                                }
                                IconButton {
                                    icon: "plus".to_string(),
                                    label: "Add {entry.title} to queue or playlist".to_string(),
                                    quiet: true,
                                    size: IconButtonSize::Small,
                                    onclick: {
                                        let entry = entry.clone();
                                        let mut add_menu = add_menu.clone();
                                        move |_| add_menu.open(AddIntent::from_song(entry.clone()))
                                    },
                                }
                            }
                        }
                    }
                }
            },
        }
    }
}
