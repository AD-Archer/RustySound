use crate::api::models::format_duration;
use crate::api::*;
#[cfg(target_arch = "wasm32")]
use crate::components::audio_manager::spawn_shuffle_queue;
use crate::components::{
    seek_to, AppView, AudioState, Icon, Navigation, PlaybackPositionSignal, VolumeSignal,
};
use crate::db::RepeatMode;
use dioxus::prelude::*;

#[component]
pub fn Player() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let mut volume = use_context::<VolumeSignal>().0;
    let navigation = use_context::<Navigation>();
    let audio_state = use_context::<Signal<AudioState>>();

    let mut is_favorited = use_signal(|| false);
    let mut is_scrubbing = use_signal(|| false);
    let mut scrub_percent = use_signal(|| 0.0f64);

    let current_song = now_playing();
    let current_song_for_fav = current_song.clone();
    let current_song_for_album = current_song.clone();
    let current_song_for_artist = current_song.clone();

    // Get time from audio state (Signal fields need to be read with ())
    let current_time = (audio_state().current_time)();
    let duration = (audio_state().duration)();

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
        is_scrubbing.set(false);
        scrub_percent.set(0.0);
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
        let mut scrub_percent = scrub_percent.clone();
        let mut is_scrubbing = is_scrubbing.clone();
        move |e: Event<FormData>| {
            if is_radio {
                return;
            }
            if let Ok(percent) = e.value().parse::<f64>() {
                scrub_percent.set(percent.clamp(0.0, 100.0));
                if !is_scrubbing() {
                    is_scrubbing.set(true);
                }
            }
        }
    };

    let on_seek_commit = {
        let mut is_scrubbing = is_scrubbing.clone();
        move |e: Event<FormData>| {
            if is_radio {
                return;
            }
            if let Ok(percent) = e.value().parse::<f64>() {
                let dur = duration;
                if dur > 0.0 {
                    let new_time = (percent / 100.0) * dur;
                    seek_to(new_time);
                }
            }
            is_scrubbing.set(false);
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
                                // Clickable album art
                                button {
                                    class: "w-12 h-12 md:w-14 md:h-14 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden shadow-lg hover:ring-2 hover:ring-emerald-500/50 transition-all cursor-pointer",
                                    onclick: {
                                        let song = current_song_for_album.clone();
                                        let navigation = navigation.clone();
                                        move |_| {
                                            if let Some(ref s) = song {
                                                if let Some(album_id) = &s.album_id {
                                                    navigation
                                                        .navigate_to(
                                                            AppView::AlbumDetailView {
                                                                album_id: album_id.clone(),
                                                                server_id: s.server_id.clone(),
                                                            },
                                                        );
                                                }
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
                                div { class: "min-w-0 flex-1",
                                    button {
                                        class: "text-sm font-medium text-white truncate hover:text-emerald-400 transition-colors cursor-pointer block text-left w-full",
                                        onclick: {
                                            let song = current_song_for_album.clone();
                                            let navigation = navigation.clone();
                                            move |_| {
                                                if let Some(ref s) = song {
                                                    if let Some(album_id) = &s.album_id {
                                                        navigation.navigate_to(AppView::AlbumDetailView {
                                                            album_id: album_id.clone(),
                                                            server_id: s.server_id.clone(),
                                                        });
                                                    }
                                                }
                                            }
                                        },
                                        "{song.title}"
                                    }
                                    button {
                                        class: "text-xs text-zinc-400 truncate hover:text-white transition-colors cursor-pointer block text-left w-full",
                                        onclick: on_artist_click,
                                        "{song.artist.clone().unwrap_or_default()}"
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
                    div { class: "flex flex-wrap items-center gap-6 md:gap-4 justify-center",
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
                    }
                    // Progress bar
                    div { class: "flex items-center gap-2 md:gap-3 w-full",
                        span { class: "text-xs text-zinc-500 w-10 text-right",
                            "{format_duration(current_time as u32)}"
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            disabled: is_radio,
                            value: if is_scrubbing() { scrub_percent() as i32 } else if duration > 0.0 { (current_time / duration * 100.0) as i32 } else { 0 },
                            class: "flex-1 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-emerald-500",
                            oninput: on_seek_input,
                            onchange: on_seek_commit,
                        }
                        span { class: "text-xs text-zinc-500 w-10",
                            {
                                current_song
                                    .as_ref()
                                    .map(|s| format_duration(s.duration))
                                    .unwrap_or_else(|| "--:--".to_string())
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

/// Bookmark button - capture current playback position on the server
#[component]
fn BookmarkButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let saving = use_signal(|| false);
    let saved = use_signal(|| false);
    let base_class = "hidden sm:flex items-center justify-center";

    {
        let now_playing = now_playing.clone();
        let mut saved_signal = saved.clone();
        use_effect(move || {
            let _ = now_playing();
            saved_signal.set(false);
        });
    }

    let has_song = now_playing().is_some();

    let on_save = move |_| {
        if saving() {
            return;
        }
        if let Some(song) = now_playing.peek().clone() {
            if let Some(server) = servers().iter().find(|s| s.id == song.server_id).cloned() {
                let song_id = song.id.clone();
                let position_ms = (playback_position() * 1000.0).round().max(0.0) as u64;
                let mut saving = saving.clone();
                let mut saved = saved.clone();
                spawn(async move {
                    saving.set(true);
                    let client = NavidromeClient::new(server);
                    let res = client.create_bookmark(&song_id, position_ms, None).await;
                    saving.set(false);
                    saved.set(res.is_ok());
                });
            }
        }
    };

    rsx! {
        button {
            id: "bookmark-btn",
            r#type: "button",
            disabled: !has_song || saving(),
            class: if saved() { format!(
                "{base_class} p-3 md:p-2 text-emerald-400 hover:text-emerald-300 transition-colors",
            ) } else { format!("{base_class} p-3 md:p-2 text-zinc-400 hover:text-white transition-colors") },
            onclick: on_save,
            if saving() {
                Icon { name: "loader".to_string(), class: "w-5 h-5".to_string() }
            } else {
                Icon {
                    name: "bookmark".to_string(),
                    class: "w-5 h-5".to_string(),
                }
            }
        }
    }
}

/// Rating button - set a star rating for the current song
#[component]
fn RatingButton() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut rating_open = use_signal(|| false);

    let current = now_playing();
    let current_rating = current
        .as_ref()
        .and_then(|s| s.user_rating)
        .unwrap_or(0)
        .min(5);
    let has_song = current.is_some();

    let on_rate = {
        let servers = servers.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut rating_open = rating_open.clone();
        move |rating: u32| {
            if let Some(song) = now_playing() {
                let server = servers().iter().find(|s| s.id == song.server_id).cloned();
                if let Some(server) = server {
                    let song_id = song.id.clone();
                    let normalized = if rating > 5 { 5 } else { rating };
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        if client.set_rating(&song_id, normalized).await.is_ok() {
                            let updated = if normalized == 0 {
                                None
                            } else {
                                Some(normalized)
                            };
                            now_playing.with_mut(|current| {
                                if let Some(ref mut s) = current {
                                    if s.id == song_id {
                                        s.user_rating = updated;
                                    }
                                }
                            });
                            queue.with_mut(|items| {
                                for song in items.iter_mut() {
                                    if song.id == song_id {
                                        song.user_rating = updated;
                                    }
                                }
                            });
                        }
                        rating_open.set(false);
                    });
                }
            }
        }
    };

    rsx! {
        div { class: "relative",
            button {
                id: "rating-btn",
                r#type: "button",
                disabled: !has_song,
                class: if current_rating > 0 { "p-3 md:p-2 text-amber-400 hover:text-amber-300 transition-colors" } else { "p-3 md:p-2 text-zinc-400 hover:text-white transition-colors" },
                onclick: move |_| rating_open.set(!rating_open()),
                Icon {
                    name: if current_rating > 0 { "star-filled".to_string() } else { "star".to_string() },
                    class: "w-5 h-5".to_string(),
                }
            }
            if rating_open() && has_song {
                div { class: "absolute bottom-12 left-1/2 -translate-x-1/2 bg-zinc-900/95 border border-zinc-800 rounded-xl px-3 py-2 shadow-xl flex items-center gap-2",
                    for value in 1..=5 {
                        button {
                            r#type: "button",
                            class: if value <= current_rating { "text-amber-400 hover:text-amber-300 transition-colors" } else { "text-zinc-500 hover:text-zinc-300 transition-colors" },
                            onclick: {
                                let on_rate = on_rate.clone();
                                move |_| on_rate(value)
                            },
                            Icon {
                                name: if value <= current_rating { "star-filled".to_string() } else { "star".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    }
                    button {
                        r#type: "button",
                        class: "ml-1 text-xs text-zinc-400 hover:text-white transition-colors",
                        onclick: {
                            let on_rate = on_rate.clone();
                            move |_| on_rate(0)
                        },
                        "Clear"
                    }
                }
            }
        }
    }
}

/// Play/Pause button - completely isolated component
#[component]
fn PlayPauseButton() -> Element {
    let mut is_playing = use_context::<Signal<bool>>();
    let playing = is_playing();

    rsx! {
        button {
            id: "play-pause-btn",
            r#type: "button",
            class: "w-10 h-10 rounded-full bg-white flex items-center justify-center hover:scale-105 transition-transform shadow-lg",
            onclick: move |_| {
                let current = is_playing();
                is_playing.set(!current);
            },
            if playing {
                Icon {
                    name: "pause".to_string(),
                    class: "w-5 h-5 text-black".to_string(),
                }
            } else {
                Icon {
                    name: "play".to_string(),
                    class: "w-5 h-5 text-black ml-0.5".to_string(),
                }
            }
        }
    }
}

/// Previous button - completely isolated component
#[component]
fn PrevButton() -> Element {
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();

    rsx! {
        button {
            id: "prev-btn",
            r#type: "button",
            class: "p-3 md:p-2 text-zinc-300 hover:text-white transition-colors",
            onclick: move |_| {
                let idx = queue_index();
                let queue_list = queue();
                if idx > 0 && !queue_list.is_empty() {
                    let next_idx = idx - 1;
                    if let Some(song) = queue_list.get(next_idx).cloned() {
                        queue_index.set(next_idx);
                        now_playing.set(Some(song));
                    }
                }
            },
            Icon { name: "prev".to_string(), class: "w-5 h-5".to_string() }
        }
    }
}

/// Next button - completely isolated component
#[component]
fn NextButton() -> Element {
    #[cfg(target_arch = "wasm32")]
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    #[cfg(target_arch = "wasm32")]
    let is_playing = use_context::<Signal<bool>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();

    rsx! {
        button {
            id: "next-btn",
            r#type: "button",
            class: "p-3 md:p-2 text-zinc-300 hover:text-white transition-colors",
            onclick: move |_| {
                let repeat = *repeat_mode.peek();
                if repeat == RepeatMode::One {
                    seek_to(0.0);
                    return;
                }
                let idx = *queue_index.peek();
                let queue_list = queue.peek();
                let can_shuffle = repeat == RepeatMode::Off;
                if can_shuffle && *shuffle_enabled.peek() {
                    if queue_list.is_empty() {
                        #[cfg(target_arch = "wasm32")]
                        spawn_shuffle_queue(
                            servers.peek().clone(),
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            is_playing.clone(),
                            now_playing.peek().clone(),
                            None,
                        );
                    } else if idx < queue_list.len().saturating_sub(1) {
                        if let Some(song) = queue_list.get(idx + 1).cloned() {
                            queue_index.set(idx + 1);
                            now_playing.set(Some(song));
                        }
                    } else {
                        #[cfg(target_arch = "wasm32")]
                        spawn_shuffle_queue(
                            servers.peek().clone(),
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            is_playing.clone(),
                            now_playing.peek().clone(),
                            None,
                        );
                    }
                } else if idx < queue_list.len().saturating_sub(1) {
                    if let Some(song) = queue_list.get(idx + 1).cloned() {
                        queue_index.set(idx + 1);
                        now_playing.set(Some(song));
                    }
                } else if repeat == RepeatMode::All && !queue_list.is_empty() {
                    if let Some(song) = queue_list.get(0).cloned() {
                        queue_index.set(0);
                        now_playing.set(Some(song));
                    }
                }
            },
            Icon { name: "next".to_string(), class: "w-5 h-5".to_string() }
        }
    }
}

/// Repeat button - completely isolated component
#[component]
fn RepeatButton() -> Element {
    let mut repeat_mode = use_context::<Signal<RepeatMode>>();
    let mode = repeat_mode();

    rsx! {
        button {
            id: "repeat-btn",
            r#type: "button",
            class: match mode {
                RepeatMode::Off => "p-3 md:p-2 text-zinc-400 hover:text-white transition-colors",
                RepeatMode::All | RepeatMode::One => {
                    "p-3 md:p-2 text-emerald-400 hover:text-emerald-300 transition-colors"
                }
            },
            onclick: move |_| {
                let next = match repeat_mode() {
                    RepeatMode::Off => RepeatMode::All,
                    RepeatMode::All => RepeatMode::One,
                    RepeatMode::One => RepeatMode::Off,
                };
                repeat_mode.set(next);
            },
            Icon {
                name: match mode {
                    RepeatMode::One => "repeat-1".to_string(),
                    _ => "repeat".to_string(),
                },
                class: "w-5 h-5".to_string(),
            }
        }
    }
}

/// Shuffle button - toggle shuffle mode
#[component]
fn ShuffleButton() -> Element {
    let mut shuffle_enabled = use_context::<Signal<bool>>();
    let enabled = shuffle_enabled();

    rsx! {
        button {
            id: "shuffle-btn",
            r#type: "button",
            class: if enabled { "p-3 md:p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-3 md:p-2 text-zinc-400 hover:text-white transition-colors" },
            onclick: move |_| {
                let current = shuffle_enabled();
                shuffle_enabled.set(!current);
            },
            Icon { name: "shuffle".to_string(), class: "w-5 h-5".to_string() }
        }
    }
}
