//! Wide player bar (Mac mock): now playing, transport + progress, extras.

use super::controls::{
    AddToMenuButton, ControlSize, NextButton, PlayPauseButton, PrevButton, RatingButton,
    RepeatButton, ShuffleButton,
};
use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::ui::env::is_radio_song;
use crate::components::ui::{
    Artwork, IconButton, IconButtonSize, PlaybackProgress, PlaybackTime, ProgressTrack,
    TrackVariant,
};
use crate::components::views::artist_links::ArtistNameLinks;
use crate::components::{
    AppView, Navigation, SongDetailsController, SongDetailsTab, VolumeSignal,
};
use dioxus::prelude::*;

fn radio_labels(song: &Song) -> (String, String) {
    let station = song
        .album
        .clone()
        .or_else(|| song.artist.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Internet Radio".to_string());
    let unknown_title = song.title.trim().is_empty()
        || song.title.trim().eq_ignore_ascii_case("unknown song");
    let artist = song.artist.clone().filter(|name| {
        let trimmed = name.trim();
        !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("unknown artist")
            && !trimmed.eq_ignore_ascii_case(&station)
    });
    if unknown_title {
        ("Unknown Song".to_string(), station)
    } else if let Some(artist) = artist {
        (song.title.clone(), format!("{artist} · {station}"))
    } else {
        (song.title.clone(), station)
    }
}

fn format_badge(song: &Song) -> Option<String> {
    let suffix = song.suffix.as_deref()?.trim().to_ascii_uppercase();
    if suffix.is_empty() {
        return None;
    }
    let lossless = matches!(suffix.as_str(), "FLAC" | "ALAC" | "WAV" | "AIFF" | "APE" | "WV");
    match song.bitrate {
        Some(rate) if rate > 0 && !lossless => Some(format!("{suffix} {rate}")),
        _ => Some(suffix),
    }
}

/// The full-width player bar for wide layouts. No props.
#[component]
pub(super) fn WidePlayer() -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let song_details = use_context::<SongDetailsController>();
    let navigation = use_context::<Navigation>();
    let view = use_route::<AppView>();

    let song = now_playing();
    let on_queue = matches!(view, AppView::QueueView {});

    let open_details = move |tab: Option<SongDetailsTab>| {
        let mut song_details = song_details.clone();
        move |_| {
            if let Some(song) = now_playing.peek().clone() {
                song_details.open(song);
                if let Some(tab) = tab {
                    song_details.set_tab(tab);
                }
            }
        }
    };

    rsx! {
        div { class: "rs-bar", role: "region", aria_label: "Player", "data-testid": "player-bar",
            div { class: "rs-bar__np",
                match song.clone() {
                    Some(song) => {
                        let radio = is_radio_song(&song);
                        let (title, radio_sub) = if radio {
                            radio_labels(&song)
                        } else {
                            (song.title.clone(), String::new())
                        };
                        let album_target = song.album_id.clone().map(|album_id| AppView::AlbumDetailView {
                            album_id,
                            server_id: song.server_id.clone(),
                        });
                        rsx! {
                            button {
                                r#type: "button",
                                class: "rs-bar__art",
                                aria_label: "Open Now Playing",
                                title: "Open Now Playing",
                                onclick: open_details(None),
                                Artwork {
                                    cover_art: song.cover_art.clone(),
                                    server_id: song.server_id.clone(),
                                    size_px: 44,
                                    eager: true,
                                    placeholder: if radio { "radio".to_string() } else { "music".to_string() },
                                }
                            }
                            div { class: "rs-bar__meta",
                                button {
                                    r#type: "button",
                                    class: "rs-bar__title",
                                    title: "{title}",
                                    disabled: album_target.is_none(),
                                    onclick: move |_| {
                                        if let Some(target) = album_target.clone() {
                                            navigation.navigate_to(target);
                                        }
                                    },
                                    "{title}"
                                }
                                div { class: "rs-bar__sub",
                                    if radio {
                                        span { class: "rs-truncate", "{radio_sub}" }
                                    } else {
                                        ArtistNameLinks {
                                            artist_text: song.artist.clone().unwrap_or_default(),
                                            server_id: song.server_id.clone(),
                                            fallback_artist_id: song.artist_id.clone(),
                                            container_class: "rs-bar__artists rs-truncate".to_string(),
                                            button_class: "rs-bar__artist".to_string(),
                                            separator_class: "rs-bar__sep".to_string(),
                                        }
                                        if let Some(album) = song.album.clone().filter(|a| !a.trim().is_empty()) {
                                            span { class: "rs-bar__sep rs-bar__album-sep", aria_hidden: "true", "·" }
                                            span { class: "rs-truncate rs-bar__album", title: "{album}", "{album}" }
                                        }
                                    }
                                }
                            }
                            if !radio {
                                span { class: "rs-bar__optional", FavoriteButton {} }
                            }
                        }
                    }
                    None => rsx! {
                        Artwork { size_px: 44 }
                        div { class: "rs-bar__meta",
                            div { class: "rs-bar__title", "Not Playing" }
                            div { class: "rs-bar__sub", "Pick something to play" }
                        }
                    },
                }
            }
            div { class: "rs-bar__mid",
                div { class: "rs-bar__transport",
                    span { class: "rs-bar__optional", ShuffleButton {} }
                    PrevButton {}
                    PlayPauseButton { size: ControlSize::Bar }
                    NextButton { size: ControlSize::Bar }
                    span { class: "rs-bar__optional", RepeatButton {} }
                }
                PlaybackTime { class: "rs-time--end".to_string() }
                PlaybackProgress { class: "rs-bar__progress".to_string() }
                PlaybackTime { remaining: true, class: "rs-time--start".to_string() }
            }
            div { class: "rs-bar__rt",
                if let Some(badge) = song.as_ref().and_then(format_badge) {
                    span { class: "rs-bar__fmt", "{badge}" }
                }
                span { class: "rs-bar__optional", RatingButton {} }
                AddToMenuButton {}
                IconButton {
                    icon: "lyrics".to_string(),
                    label: "Lyrics".to_string(),
                    size: IconButtonSize::Small,
                    class: "rs-bar__wide-only".to_string(),
                    disabled: song.is_none(),
                    onclick: open_details(Some(SongDetailsTab::Lyrics)),
                }
                IconButton {
                    icon: "queue".to_string(),
                    label: "Up Next".to_string(),
                    size: IconButtonSize::Small,
                    pressed: on_queue,
                    class: "rs-bar__wide-only".to_string(),
                    testid: "player-queue".to_string(),
                    onclick: move |_| navigation.navigate_to(AppView::QueueView {}),
                }
                VolumeControl {}
            }
        }
    }
}

/// Star toggle for the current song (Subsonic "starred").
#[component]
fn FavoriteButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let starred = now_playing
        .read()
        .as_ref()
        .map(|song| song.starred.is_some())
        .unwrap_or(false);

    rsx! {
        IconButton {
            icon: if starred { "star-filled".to_string() } else { "star".to_string() },
            label: if starred { "Unfavorite".to_string() } else { "Favorite".to_string() },
            pressed: starred,
            size: IconButtonSize::Small,
            testid: "player-favorite".to_string(),
            onclick: move |_| {
                let Some(song) = now_playing.peek().clone() else {
                    return;
                };
                let Some(server) = servers.peek().iter().find(|s| s.id == song.server_id).cloned() else {
                    return;
                };
                let should_star = song.starred.is_none();
                let (mut now_playing, mut queue) = (now_playing, queue);
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = if should_star {
                        client.star(&song.id, "song").await
                    } else {
                        client.unstar(&song.id, "song").await
                    };
                    if result.is_ok() {
                        let marker = should_star.then(|| "local".to_string());
                        now_playing.with_mut(|current| {
                            if let Some(current) = current.as_mut() {
                                if current.id == song.id {
                                    current.starred = marker.clone();
                                }
                            }
                        });
                        queue.with_mut(|items| {
                            for item in items.iter_mut().filter(|s| s.id == song.id) {
                                item.starred = marker.clone();
                            }
                        });
                    }
                });
            },
        }
    }
}

/// Volume slider (live while dragging).
#[component]
fn VolumeControl() -> Element {
    let mut volume = use_context::<VolumeSignal>().0;
    let level = (volume() * 100.0).round();
    rsx! {
        ProgressTrack {
            value: level,
            max: 100.0,
            step: 1.0,
            variant: TrackVariant::Thin,
            class: "rs-bar__vol".to_string(),
            label: "Volume".to_string(),
            value_text: format!("{level:.0}%"),
            testid: "player-volume".to_string(),
            on_input: move |value: f64| volume.set((value / 100.0).clamp(0.0, 1.0)),
            on_commit: move |value: f64| volume.set((value / 100.0).clamp(0.0, 1.0)),
        }
    }
}
