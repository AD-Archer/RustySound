//! Title and artist under the cover, the star toggle and the "…" menu, plus
//! the rating and volume controls.

use super::actions::{is_live_song, use_player};
use super::types::SongDetailsController;
use crate::api::{ServerConfig, Song};
use crate::components::ui::{IconButton, ProgressTrack, TrackMenuController, TrackVariant};
use crate::components::views::artist_links::{parse_artist_names, resolve_artist_id_for_name};
use crate::components::{AppView, Icon, Navigation};
use dioxus::prelude::*;

#[component]
pub(super) fn NowPlayingMeta(song: Song) -> Element {
    let player = use_player();
    let mut menu = use_context::<TrackMenuController>();
    let radio = is_live_song(&song);
    let starred = song.starred.is_some();
    let title = if song.title.trim().is_empty() {
        if radio { "Internet Radio".to_string() } else { "Unknown Song".to_string() }
    } else {
        song.title.clone()
    };

    rsx! {
        div { class: "rs-player-meta",
            div { class: "rs-player-meta__text",
                h2 { class: "rs-player-meta__title", title: "{title}", "{title}" }
                if radio {
                    p { class: "rs-player-meta__artist",
                        "{song.album.clone().or(song.artist.clone()).unwrap_or_default()}"
                    }
                } else {
                    ArtistLinks { song: song.clone() }
                }
            }
            if !radio {
                IconButton {
                    icon: if starred { "heart-filled".to_string() } else { "heart".to_string() },
                    label: if starred { "Favorited".to_string() } else { "Favorite".to_string() },
                    pressed: starred,
                    class: "rs-player-meta__star".to_string(),
                    testid: "now-playing-favorite".to_string(),
                    onclick: {
                        let song = song.clone();
                        move |_| player.toggle_favorite(song.clone())
                    },
                }
            }
            IconButton {
                icon: "more".to_string(),
                label: "More for this song".to_string(),
                class: "rs-player-meta__more".to_string(),
                testid: "now-playing-more".to_string(),
                onclick: {
                    let song = song.clone();
                    move |evt: MouseEvent| {
                        let point = evt.client_coordinates();
                        menu.open(song.clone(), Some((point.x - 250.0, point.y + 16.0)));
                    }
                },
            }
        }
    }
}

/// Artist names as links; multi-artist strings ("A feat. B") link each name.
#[component]
fn ArtistLinks(song: Song) -> Element {
    let navigation = use_context::<Navigation>();
    let controller = use_context::<SongDetailsController>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let names = parse_artist_names(song.artist.as_deref().unwrap_or_default());
    if names.is_empty() {
        return rsx! {
            p { class: "rs-player-meta__artist", "Unknown Artist" }
        };
    }
    let direct_id = if names.len() == 1 { song.artist_id.clone() } else { None };
    let count = names.len();

    rsx! {
        p { class: "rs-player-meta__artist",
            for (i, name) in names.into_iter().enumerate() {
                button {
                    key: "{i}",
                    r#type: "button",
                    class: "rs-player-meta__link",
                    onclick: {
                        let name = name.clone();
                        let direct_id = direct_id.clone();
                        let server_id = song.server_id.clone();
                        let controller = controller.clone();
                        move |_| {
                            let mut controller = controller.clone();
                            if let Some(artist_id) = direct_id.clone() {
                                controller.close();
                                navigation.navigate_to(AppView::ArtistDetailView {
                                    artist_id,
                                    server_id: server_id.clone(),
                                });
                                return;
                            }
                            let server = servers.peek().iter().find(|s| s.id == server_id).cloned();
                            let Some(server) = server else {
                                return;
                            };
                            let (name, server_id) = (name.clone(), server_id.clone());
                            spawn(async move {
                                if let Some(artist_id) = resolve_artist_id_for_name(server, name).await {
                                    controller.close();
                                    navigation.navigate_to(AppView::ArtistDetailView {
                                        artist_id,
                                        server_id,
                                    });
                                }
                            });
                        }
                    },
                    "{name}"
                }
                if i + 1 < count {
                    span { class: "rs-player-meta__sep", aria_hidden: "true", ", " }
                }
            }
        }
    }
}

/// 1–5 stars plus clear, for the current song (`rating-btn` lives in the
/// player bar; these are the sheet's own controls).
#[component]
pub(super) fn RatingStars(song: Song) -> Element {
    let player = use_player();
    let rating = song.user_rating.unwrap_or(0).min(5);
    rsx! {
        div {
            class: "rs-player-rating",
            role: "group",
            aria_label: if rating > 0 { format!("Rating: {rating} of 5") } else { "Rating".to_string() },
            "data-testid": "now-playing-rating",
            for value in 1..=5u32 {
                button {
                    key: "{value}",
                    r#type: "button",
                    class: "rs-player-rating__star",
                    class: if value <= rating { "rs-player-rating__star--on" },
                    aria_label: "{value} star",
                    aria_pressed: if value == rating { "true" } else { "false" },
                    onclick: {
                        let song = song.clone();
                        move |_| {
                            let next = if value == rating { 0 } else { value };
                            player.set_rating(song.clone(), next);
                        }
                    },
                    Icon {
                        name: if value <= rating { "star-filled".to_string() } else { "star".to_string() },
                        class: "rs-ic rs-ic--sm".to_string(),
                    }
                }
            }
        }
    }
}

/// App volume as a thin track. Not shown on iOS, where the hardware buttons
/// set the volume.
#[component]
pub(super) fn VolumeSlider() -> Element {
    let mut player = use_player();
    let volume = (player.volume)();
    rsx! {
        div { class: "rs-player-volume",
            Icon { name: "volume-1".to_string(), class: "rs-ic rs-ic--sm".to_string() }
            ProgressTrack {
                value: volume * 100.0,
                max: 100.0,
                step: 1.0,
                variant: TrackVariant::Thin,
                label: "Volume".to_string(),
                value_text: format!("{:.0}%", volume * 100.0),
                testid: "now-playing-volume".to_string(),
                class: "rs-player-volume__track".to_string(),
                on_input: move |value: f64| player.set_volume(value / 100.0),
                on_commit: move |value: f64| player.set_volume(value / 100.0),
            }
            Icon { name: "volume".to_string(), class: "rs-ic rs-ic--sm".to_string() }
        }
    }
}

pub(super) fn shows_volume() -> bool {
    !cfg!(target_os = "ios")
}

/// Wide layout: rating and volume under the transport.
#[component]
pub(super) fn NowPlayingExtras(song: Song) -> Element {
    if is_live_song(&song) {
        return rsx! {
            div { class: "rs-player-extras",
                if shows_volume() {
                    VolumeSlider {}
                }
            }
        };
    }
    rsx! {
        div { class: "rs-player-extras",
            RatingStars { song }
            if shows_volume() {
                VolumeSlider {}
            }
        }
    }
}
