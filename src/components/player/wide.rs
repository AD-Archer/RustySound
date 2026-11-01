//! Wide player bar (Mac mock): now playing, transport + progress, extras.
//! Shuffle, Repeat and the rating live in its playback menu ("…",
//! `playback_menu.rs`), so the bar keeps to what you reach for often.

use super::controls::{AddToMenuButton, ControlSize, NextButton, PlayPauseButton, PrevButton};
use super::playback_menu::PlaybackMenu;
use crate::api::Song;
use crate::components::remote_sync::{PlayOnChoices, RemoteSync};
use crate::components::song_details::actions::format_badge;
use crate::components::ui::env::is_radio_song;
use crate::components::ui::{
    Artwork, IconButton, IconButtonSize, Menu, PlaybackProgress, PlaybackTime, ProgressTrack,
    TrackVariant,
};
use crate::components::views::artist_links::ArtistNameLinks;
use crate::components::{
    use_player, AppView, Navigation, SongDetailsController, SongDetailsTab, VolumeSignal,
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

/// The full-width player bar for wide layouts. No props.
#[component]
pub(super) fn WidePlayer() -> Element {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let song_details = use_context::<SongDetailsController>();
    let navigation = use_context::<Navigation>();
    let view = use_route::<AppView>();
    let sync = use_context::<RemoteSync>();
    let mut devices_menu = use_signal(|| false);

    let song = now_playing();
    // While a network speaker plays, its name sits by the format badge.
    let playing_on = crate::components::cast::use_playing_on();
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
                                            if let Some(target) = album_target.clone() {
                                                button {
                                                    r#type: "button",
                                                    class: "rs-bar__album rs-bar__album-btn",
                                                    title: "{album}",
                                                    onclick: move |_| navigation.navigate_to(target.clone()),
                                                    "{album}"
                                                }
                                            } else {
                                                span { class: "rs-truncate rs-bar__album", title: "{album}", "{album}" }
                                            }
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
                    PrevButton {}
                    PlayPauseButton { size: ControlSize::Bar }
                    NextButton { size: ControlSize::Bar }
                }
                PlaybackTime { class: "rs-time--end".to_string() }
                PlaybackProgress { class: "rs-bar__progress".to_string() }
                PlaybackTime { remaining: true, class: "rs-time--start".to_string() }
            }
            div { class: "rs-bar__rt",
                if let Some(name) = playing_on {
                    span { class: "rs-bar__cast", title: "Playing on {name}", "data-testid": "player-cast",
                        crate::components::Icon { name: "speaker".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                        span { class: "rs-truncate", "{name}" }
                    }
                }
                if let Some(badge) = song.as_ref().and_then(format_badge) {
                    span { class: "rs-bar__fmt", "{badge}" }
                }
                if !sync.play_on().is_empty() || sync.active_elsewhere().is_some() {
                    IconButton {
                        icon: "airplay".to_string(),
                        label: "Playback devices".to_string(),
                        size: IconButtonSize::Small,
                        class: "rs-bar__wide-only".to_string(),
                        onclick: move |_| devices_menu.set(true),
                    }
                }
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
                PlaybackMenu {}
                VolumeControl {}
            }
            if devices_menu() {
                Menu {
                    label: "Playback devices".to_string(),
                    on_close: move |_| devices_menu.set(false),
                    PlayOnChoices { on_done: move |_| devices_menu.set(false) }
                }
            }
        }
    }
}

/// Favourite toggle for the current song (`PlayerCtx::toggle_favorite`, so
/// music on the device and every server kind work, and lists update).
#[component]
fn FavoriteButton() -> Element {
    let player = use_player();
    let starred = player
        .now_playing
        .read()
        .as_ref()
        .is_some_and(|song| song.starred.is_some());

    rsx! {
        IconButton {
            icon: if starred { "heart-filled".to_string() } else { "heart".to_string() },
            label: if starred { "Unfavorite".to_string() } else { "Favorite".to_string() },
            pressed: starred,
            size: IconButtonSize::Small,
            testid: "player-favorite".to_string(),
            onclick: move |_| {
                if let Some(song) = player.now_playing.peek().clone() {
                    player.toggle_favorite(song);
                }
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
