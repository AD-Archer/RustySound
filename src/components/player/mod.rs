use crate::api::models::format_duration;
use crate::api::*;
use crate::components::{
    seek_to, AppView, AudioState, Icon, Navigation, PlaybackPositionSignal, SongDetailsController,
    VolumeSignal,
};
use dioxus::prelude::*;

mod controls;

use controls::{
    AddToMenuButton, BookmarkButton, NextButton, PlayPauseButton, PrevButton, RatingButton,
    RepeatButton,
};

#[component]
pub fn Player() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let mut volume = use_context::<VolumeSignal>().0;
    let navigation = use_context::<Navigation>();
    let song_details = use_context::<SongDetailsController>();
    let audio_state = use_context::<Signal<AudioState>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;

    let mut is_favorited = use_signal(|| false);

    let current_song = now_playing();
    let current_song_for_fav = current_song.clone();
    let current_song_for_album = current_song.clone();
    let current_song_for_artist = current_song.clone();

    // Get time from audio state (Signal fields need to be read with ())
    let current_time = (audio_state().current_time)();
    let duration = (audio_state().duration)();
    let playback_error = (audio_state().playback_error)();

    // Get cover art URL if available
    let cover_url = current_song.as_ref().and_then(|song| {
        let server = servers().iter().find(|s| s.id == song.server_id)?.clone();
        let client = NavidromeClient::new(server);
        song.cover_art
            .as_ref()
            .map(|ca| client.get_cover_art_url(ca, 100))
    });

    use_effect(move || {
        let starred = now_playing()
            .as_ref()
            .map(|s| s.starred.is_some())
            .unwrap_or(false);
        is_favorited.set(starred);
    });

    let on_volume_change = move |e: Event<FormData>| {
        if let Ok(val) = e.value().parse::<f64>() {
            volume.set((val / 100.0).clamp(0.0, 1.0));
        }
    };

    let is_radio = current_song
        .as_ref()
        .map(|s| s.server_name == "Radio")
        .unwrap_or(false);

    let on_seek_input = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        move |e: Event<FormData>| {
            if is_radio {
                return;
            }
            if let Ok(percent) = e.value().parse::<f64>() {
                let percent = percent.clamp(0.0, 100.0);
                if duration > 0.0 {
                    let new_time = (percent / 100.0) * duration;
                    playback_position.set(new_time);
                    audio_state.write().current_time.set(new_time);
                    seek_to(new_time);
                }
            }
        }
    };

    let on_seek_commit = {
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        move |e: Event<FormData>| {
            if is_radio {
                return;
            }
            if let Ok(percent) = e.value().parse::<f64>() {
                let dur = duration;
                if dur > 0.0 {
                    let new_time = (percent.clamp(0.0, 100.0) / 100.0) * dur;
                    playback_position.set(new_time);
                    audio_state.write().current_time.set(new_time);
                    seek_to(new_time);
                }
            }
        }
    };

    let on_open_queue = {
        let navigation = navigation.clone();
        move |_| navigation.navigate_to(AppView::QueueView {})
    };

    // Favorite toggle handler
    let on_favorite_toggle = move |_| {
        if let Some(song) = current_song_for_fav.clone() {
            let server_list = servers();
            if let Some(server) = server_list.iter().find(|s| s.id == song.server_id).cloned() {
                let song_id = song.id.clone();
                let should_star = !is_favorited();
                let mut now_playing = now_playing;
                let mut is_favorited = is_favorited;
                spawn(async move {
                    let client = NavidromeClient::new(server);
                    let result = if should_star {
                        client.star(&song_id, "song").await
                    } else {
                        client.unstar(&song_id, "song").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                        now_playing.with_mut(|current| {
                            if let Some(ref mut song) = current {
                                if song.id == song_id {
                                    song.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                    }
                });
            }
        }
    };

    let on_artist_click = {
        let song = current_song_for_artist.clone();
        let navigation = navigation.clone();
        move |_| {
            if let Some(ref s) = song {
                if let Some(artist_id) = &s.artist_id {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id: artist_id.clone(),
                        server_id: s.server_id.clone(),
                    });
                }
            }
        }
    };

    rsx! {
        if let Some(message) = playback_error.clone() {
            div { class: "fixed left-0 right-0 bottom-28 md:bottom-24 px-3 md:px-6 z-[60] pointer-events-none",
                div { class: "rounded-lg border border-rose-500/35 bg-rose-500/10 px-3 py-2 text-center text-xs text-rose-200 shadow-lg",
                    "{message}"
                }
            }
        }
        div { class: "player-shell fixed bottom-0 left-0 right-0 bg-zinc-950/90 backdrop-blur-xl border-t border-zinc-800/60 z-50 md:h-24",
            div { class: "h-full flex flex-col md:flex-row md:items-center md:justify-between px-4 md:px-6 gap-3 md:gap-8 py-2 md:py-0",
                // Now playing info
                div { class: "flex items-center gap-3 md:gap-4 min-w-0 w-full md:w-1/4",
                    {
                        // Album art
                        // Track info
                        // Favorite button
                        match &current_song {
                            Some(song) => rsx! {
                                div { class: "relative flex-shrink-0",
                                    div { class: "pointer-events-none absolute -top-4 -right-1 z-10 text-zinc-200 drop-shadow-md",
                                        Icon {
                                            name: "arrow-left".to_string(),
                                            class: "w-6 h-6 rotate-90".to_string(),
                                        }
                                    }
                                    // Clickable album art
                                    button {
                                        class: "w-14 h-14 md:w-16 md:h-16 rounded-lg bg-zinc-800 overflow-hidden shadow-lg hover:ring-2 hover:ring-emerald-500/50 transition-all cursor-pointer",
                                        onclick: {
                                            let song = current_song_for_album.clone();
                                            let mut song_details = song_details.clone();
                                            move |_| {
                                                if let Some(selected_song) = song.clone() {
                                                    song_details.open(selected_song);
                                                }
                                            }
                                        },
                                        {
                                            match &cover_url {
                                                Some(url) => rsx! {
                                                    img {
                                                        src: "{url}",
                                                        alt: "{song.title}",
                                                        class: "w-full h-full object-cover",
                                                        loading: "lazy",
                                                    }
                                                },
                                                None => rsx! {
                                                    div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-emerald-600 to-teal-700",
                                                        Icon { name: "music".to_string(), class: "w-6 h-6 text-white/70".to_string() }
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                                div { class: "min-w-0 flex-1 overflow-hidden",
                                    button {
                                        class: "text-sm font-medium text-white truncate max-w-full hover:text-emerald-400 transition-colors cursor-pointer block text-left w-full",
                                        onclick: {
                                            let song = current_song_for_album.clone();
                                            let navigation = navigation.clone();
                                            move |_| {
                                                if let Some(ref s) = song {
                                                    if let Some(album_id) = &s.album_id {
                                                        navigation
                                                            .navigate_to(AppView::AlbumDetailView {
                                                                album_id: album_id.clone(),
                                                                server_id: s.server_id.clone(),
                                                            });
                                                    }
                                                }
                                            }
                                        },
                                        {
                                            if song.server_name == "Radio" {
                                                if song.title.trim().is_empty()
                                                    || song.title.trim().eq_ignore_ascii_case("unknown song")
                                                {
                                                    "Unknown Song".to_string()
                                                } else {
                                                    song.title.clone()
                                                }
                                            } else {
                                                song.title.clone()
                                            }
                                        }
                                    }
                                    button {
                                        class: "text-xs text-zinc-400 truncate max-w-full hover:text-white transition-colors cursor-pointer block text-left w-full",
                                        onclick: on_artist_click,
                                        {
                                            if song.server_name == "Radio" {
                                                let station_name = song
                                                    .album
                                                    .clone()
                                                    .or_else(|| song.artist.clone())
                                                    .filter(|name| !name.trim().is_empty())
                                                    .unwrap_or_else(|| "Internet Radio".to_string());
                                                let song_artist = song
                                                    .artist
                                                    .clone()
                                                    .filter(|name| {
                                                        let trimmed = name.trim();
                                                        !trimmed.is_empty()
                                                            && !trimmed.eq_ignore_ascii_case("unknown artist")
                                                            && !trimmed.eq_ignore_ascii_case(&station_name)
                                                    });
                                                if song.title.trim().is_empty()
                                                    || song.title.trim().eq_ignore_ascii_case("unknown song")
                                                {
                                                    station_name
                                                } else if let Some(artist_name) = song_artist {
                                                    format!("{artist_name} • {station_name}")
                                                } else {
                                                    station_name
                                                }
                                            } else {
                                                song.artist.clone().unwrap_or_default()
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors flex-shrink-0" } else { "p-2 text-zinc-400 hover:text-emerald-400 transition-colors flex-shrink-0" },
                                    onclick: on_favorite_toggle,
                                    Icon {
                                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                        class: "w-5 h-5".to_string(),
                                    }
                                }
                            },
                            None => rsx! {
                                div { class: "w-14 h-14 rounded-lg bg-zinc-800/50 flex items-center justify-center",
                                    Icon { name: "music".to_string(), class: "w-6 h-6 text-zinc-600".to_string() }
                                }
                                div { class: "min-w-0 flex-1",
                                    p { class: "text-sm text-zinc-500", "No track playing" }
                                    p { class: "text-xs text-zinc-600", "Select a song to start" }
                                }
                            },
                        }
                    }
                    div { class: "md:hidden flex items-center flex-shrink-0",
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: (volume() * 100.0).round() as i32,
                            class: "vertical-range bg-zinc-800 rounded-full cursor-pointer accent-emerald-400",
                            oninput: on_volume_change,
                        }
                    }
                }

                // Player controls
                div { class: "flex flex-col items-center gap-3 w-full md:flex-1 md:max-w-2xl",
                    // Control buttons
                    div { class: "flex items-center gap-1.5 sm:gap-2 md:gap-4 justify-center w-full",
                        // Bookmark button
                        BookmarkButton {}
                        // Rating button
                        RatingButton {}
                        // Previous button
                        PrevButton {}
                        // Play/Pause button
                        PlayPauseButton {}
                        // Next button
                        NextButton {}
                        // Repeat button
                        RepeatButton {}
                        // Add menu button
                        AddToMenuButton {}
                    }
                    // Progress bar
                    div { class: "flex items-center gap-2 md:gap-3 w-full",
                        span { class: "text-xs text-zinc-500 w-10 text-right",
                            {if is_radio { "LIVE".to_string() } else { format_duration(current_time as u32) }}
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            disabled: is_radio,
                            value: if duration > 0.0 { (current_time / duration * 100.0).round() as i32 } else { 0 },
                            class: "flex-1 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-emerald-500",
                            oninput: on_seek_input,
                            onchange: on_seek_commit,
                        }
                        span { class: "text-xs text-zinc-500 w-10",
                            {
                                if is_radio {
                                    "LIVE".to_string()
                                } else {
                                    current_song
                                        .as_ref()
                                        .map(|s| format_duration(s.duration))
                                        .unwrap_or_else(|| "--:--".to_string())
                                }
                            }
                        }
                    }
                }

                // Volume and other controls
                div { class: "flex items-center w-full md:w-1/4 justify-end",
                    // Desktop queue + volume
                    div { class: "hidden md:flex items-center gap-3",
                        button {
                            class: "p-2 text-zinc-400 hover:text-white transition-colors",
                            onclick: on_open_queue,
                            Icon {
                                name: "queue".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: (volume() * 100.0).round() as i32,
                            class: "w-24 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-zinc-400",
                            oninput: on_volume_change,
                        }
                    }
                }
            }
        }
    }
}
