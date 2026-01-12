use dioxus::prelude::*;
use crate::api::*;
use crate::components::{Icon, AppView, AudioState, seek_to};
use crate::db::RepeatMode;
use crate::api::models::format_duration;

#[component]
pub fn Player() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let mut volume = use_context::<Signal<f64>>();
    let mut current_view = use_context::<Signal<AppView>>();
    let mut shuffle_enabled = use_context::<Signal<bool>>();
    let mut repeat_mode = use_context::<Signal<RepeatMode>>();
    let audio_state = use_context::<Signal<AudioState>>();
    
    let mut is_favorited = use_signal(|| false);
    let mut is_scrubbing = use_signal(|| false);
    let mut scrub_percent = use_signal(|| 0.0f64);
    
    let current_song = now_playing();
    let current_song_for_fav = current_song.clone();
    let current_song_for_album = current_song.clone();
    let current_song_for_artist = current_song.clone();
    let playing = is_playing();
    let vol = volume();
    let shuffle = shuffle_enabled();
    let repeat = repeat_mode();
    
    // Get time from audio state (Signal fields need to be read with ())
    let current_time = (audio_state().current_time)();
    let duration = (audio_state().duration)();
    
    // Get cover art URL if available
    let cover_url = current_song.as_ref().and_then(|song| {
        let server = servers().iter().find(|s| s.id == song.server_id)?.clone();
        let client = NavidromeClient::new(server);
        song.cover_art.as_ref().map(|ca| client.get_cover_art_url(ca, 100))
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
    
    let on_prev = move |_| {
        let idx = queue_index();
        let queue_list = queue();
        if shuffle {
            // Pick a random song from the queue
            if !queue_list.is_empty() {
                let mut bytes = [0u8; 4];
                getrandom::getrandom(&mut bytes).unwrap_or_default();
                let random_idx = u32::from_le_bytes(bytes) as usize % queue_list.len();
                queue_index.set(random_idx);
            }
        } else if idx > 0 && !queue_list.is_empty() {
            queue_index.set(idx - 1);
        }
    };
    
    let on_next = move |_| {
        let idx = queue_index();
        let queue_list = queue();
        if shuffle {
            // Pick a random song from the queue
            if !queue_list.is_empty() {
                let mut bytes = [0u8; 4];
                getrandom::getrandom(&mut bytes).unwrap_or_default();
                let random_idx = u32::from_le_bytes(bytes) as usize % queue_list.len();
                queue_index.set(random_idx);
            }
        } else if idx < queue_list.len().saturating_sub(1) {
            queue_index.set(idx + 1);
        } else if repeat == RepeatMode::All && !queue_list.is_empty() {
            // Wrap around to the beginning
            queue_index.set(0);
        }
    };
    
    let on_toggle = move |_| {
        is_playing.set(!playing);
    };
    
    let on_volume_change = move |e: Event<FormData>| {
        if let Ok(val) = e.value().parse::<f64>() {
            let new_vol = (val / 100.0).clamp(0.0, 1.0);
            volume.set(new_vol);
        }
    };
    
    let on_seek_input = move |e: Event<FormData>| {
        if let Ok(percent) = e.value().parse::<f64>() {
            scrub_percent.set(percent.clamp(0.0, 100.0));
            if !is_scrubbing() {
                is_scrubbing.set(true);
            }
        }
    };

    let on_seek_commit = move |e: Event<FormData>| {
        if let Ok(percent) = e.value().parse::<f64>() {
            let dur = duration;
            if dur > 0.0 {
                let new_time = (percent / 100.0) * dur;
                seek_to(new_time);
            }
        }
        is_scrubbing.set(false);
    };
    
    let on_open_queue = move |_| {
        current_view.set(AppView::Queue);
    };
    
    let on_shuffle_toggle = move |_| {
        shuffle_enabled.set(!shuffle);
    };
    
    let on_repeat_toggle = move |_| {
        let next = match repeat {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
        repeat_mode.set(next);
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
    
    // Navigation handlers
    let on_album_click = {
        let song = current_song_for_album.clone();
        move |_| {
            if let Some(ref s) = song {
                if let Some(album_id) = &s.album_id {
                    current_view.set(AppView::AlbumDetail(album_id.clone(), s.server_id.clone()));
                }
            }
        }
    };
    
    let on_artist_click = {
        let song = current_song_for_artist.clone();
        move |_| {
            if let Some(ref s) = song {
                if let Some(artist_id) = &s.artist_id {
                    current_view.set(AppView::ArtistDetail(artist_id.clone(), s.server_id.clone()));
                }
            }
        }
    };
    
    rsx! {
        div { class: "fixed bottom-0 left-0 right-0 h-24 bg-zinc-950/95 backdrop-blur-xl border-t border-zinc-800/50 z-50",
            div { class: "h-full flex items-center justify-between px-6 gap-8",
                // Now playing info
                div { class: "flex items-center gap-4 min-w-0 w-1/4",
                    {
                        // Album art
                        // Track info
                        // Favorite button
                        match &current_song {
                            Some(song) => rsx! {
                                // Clickable album art
                                button {
                                    class: "w-14 h-14 rounded-lg bg-zinc-800 flex-shrink-0 overflow-hidden shadow-lg hover:ring-2 hover:ring-emerald-500/50 transition-all cursor-pointer",
                                    onclick: on_album_click.clone(),
                                    {
                                        match &cover_url {
                                            Some(url) => rsx! {
                                                img { class: "w-full h-full object-cover", src: "{url}" }
                                            },
                                            None => rsx! {
                                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-emerald-600 to-teal-700",
                                                    Icon { name: "music".to_string(), class: "w-6 h-6 text-white/70".to_string() }
                                                }
                                            },
                                        }
                                    }
                                }
                                div { class: "min-w-0",
                                    // Clickable song title (goes to album)
                                    button {
                                        class: "text-sm font-medium text-white truncate hover:text-emerald-400 transition-colors cursor-pointer block text-left w-full",
                                        onclick: on_album_click,
                                        "{song.title}"
                                    }
                                    // Clickable artist name
                                    button {
                                        class: "text-xs text-zinc-400 truncate hover:text-white transition-colors cursor-pointer block text-left w-full",
                                        onclick: on_artist_click,
                                        "{song.artist.clone().unwrap_or_default()}"
                                    }
                                }
                                // Favorite button
                                button {
                                    class: if is_favorited() { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-400 hover:text-emerald-400 transition-colors" },
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
                                div {
                                    p { class: "text-sm text-zinc-500", "No track playing" }
                                    p { class: "text-xs text-zinc-600", "Select a song to start" }
                                }
                            },
                        }
                    }
                }

                // Player controls
                div { class: "flex flex-col items-center gap-2 flex-1 max-w-2xl",
                    // Control buttons
                    div { class: "flex items-center gap-4",
                        // Shuffle
                        button {
                            class: if shuffle { "p-2 text-emerald-400 hover:text-emerald-300 transition-colors" } else { "p-2 text-zinc-400 hover:text-white transition-colors" },
                            onclick: on_shuffle_toggle,
                            Icon {
                                name: "shuffle".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Previous
                        button {
                            class: "p-2 text-zinc-300 hover:text-white transition-colors",
                            onclick: on_prev,
                            Icon {
                                name: "prev".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Play/Pause
                        button {
                            class: "w-10 h-10 rounded-full bg-white flex items-center justify-center hover:scale-105 transition-transform shadow-lg",
                            onclick: on_toggle,
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
                        // Next
                        button {
                            class: "p-2 text-zinc-300 hover:text-white transition-colors",
                            onclick: on_next,
                            Icon {
                                name: "next".to_string(),
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        // Repeat
                        button {
                            class: match repeat {
                                RepeatMode::Off => "p-2 text-zinc-400 hover:text-white transition-colors",
                                RepeatMode::All => {
                                    "p-2 text-emerald-400 hover:text-emerald-300 transition-colors"
                                }
                                RepeatMode::One => {
                                    "p-2 text-emerald-400 hover:text-emerald-300 transition-colors"
                                }
                            },
                            onclick: on_repeat_toggle,
                            Icon {
                                name: match repeat {
                                    RepeatMode::One => "repeat-1".to_string(),
                                    _ => "repeat".to_string(),
                                },
                                class: "w-5 h-5".to_string(),
                            }
                        }
                    }
                    // Progress bar
                    div { class: "flex items-center gap-3 w-full",
                        span { class: "text-xs text-zinc-500 w-10 text-right",
                            "{format_duration(current_time as u32)}"
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: if is_scrubbing() {
                                scrub_percent() as i32
                            } else if duration > 0.0 {
                                (current_time / duration * 100.0) as i32
                            } else {
                                0
                            },
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
                div { class: "flex items-center gap-4 w-1/4 justify-end",
                    // Queue button
                    button {
                        class: "p-2 text-zinc-400 hover:text-white transition-colors",
                        onclick: on_open_queue,
                        Icon {
                            name: "queue".to_string(),
                            class: "w-5 h-5".to_string(),
                        }
                    }
                    // Volume
                    div { class: "flex items-center gap-2",
                        button { class: "p-2 text-zinc-400 hover:text-white transition-colors",
                            Icon {
                                name: if vol > 0.5 { "volume-2".to_string() } else if vol > 0.0 { "volume-1".to_string() } else { "volume-x".to_string() },
                                class: "w-5 h-5".to_string(),
                            }
                        }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "100",
                            value: (vol * 100.0) as i32,
                            class: "w-24 h-1.5 bg-zinc-800 rounded-full appearance-none cursor-pointer accent-zinc-400",
                            oninput: on_volume_change,
                            onchange: on_volume_change,
                        }
                    }
                }
            }
        }
    }
}
