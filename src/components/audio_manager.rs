//! Audio Manager - Handles audio playback outside of the component render cycle
//! This prevents audio from restarting when unrelated state changes.

use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::api::*;
#[cfg(target_arch = "wasm32")]
use crate::components::{PlaybackPositionSignal, SeekRequestSignal, VolumeSignal};
#[cfg(target_arch = "wasm32")]
use crate::db::RepeatMode;

#[cfg(target_arch = "wasm32")]
use js_sys;
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};
#[cfg(target_arch = "wasm32")]
use web_sys::{window, HtmlAudioElement};

/// Global audio state that persists across renders
#[derive(Clone)]
pub struct AudioState {
    pub current_time: Signal<f64>,
    pub duration: Signal<f64>,
    #[allow(dead_code)]
    pub is_initialized: Signal<bool>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self {
            current_time: Signal::new(0.0),
            duration: Signal::new(0.0),
            is_initialized: Signal::new(false),
        }
    }
}

/// Initialize the global audio element once
#[cfg(target_arch = "wasm32")]
pub fn get_or_create_audio_element() -> Option<HtmlAudioElement> {
    let document = window()?.document()?;

    // Check if audio element already exists
    if let Some(existing) = document.get_element_by_id("rustysound-audio") {
        return existing.dyn_into::<HtmlAudioElement>().ok();
    }

    // Create new audio element
    let audio: HtmlAudioElement = document.create_element("audio").ok()?.dyn_into().ok()?;
    audio.set_id("rustysound-audio");
    // Keep preload light so we stream instead of buffering entire files
    audio.set_attribute("preload", "metadata").ok()?;

    // Append to body (hidden)
    document.body()?.append_child(&audio).ok()?;

    Some(audio)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_or_create_audio_element() -> Option<()> {
    None
}

/// Audio controller hook - manages playback imperatively
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn AudioController() -> Element {
    rsx! {}
}

/// Audio controller hook - manages playback imperatively
#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    seed_song: Option<Song>,
) {
    let active_servers: Vec<ServerConfig> = servers.into_iter().filter(|s| s.active).collect();
    if active_servers.is_empty() {
        return;
    }

    spawn(async move {
        let mut songs = Vec::new();
        if let Some(seed) = seed_song {
            if let Some(server) = active_servers
                .iter()
                .find(|s| s.id == seed.server_id)
                .cloned()
            {
                let client = NavidromeClient::new(server);
                if let Ok(similar) = client.get_similar_songs(&seed.id, 50).await {
                    songs.extend(similar);
                }
            }
        }

        if songs.is_empty() {
            for server in active_servers.iter().cloned() {
                let client = NavidromeClient::new(server.clone());
                if let Ok(server_songs) = client.get_random_songs(25).await {
                    songs.extend(server_songs);
                }
            }
        }

        if songs.is_empty() {
            return;
        }

        let len = songs.len();
        for i in (1..len).rev() {
            let j = (js_sys::Math::random() * ((i + 1) as f64)) as usize;
            songs.swap(i, j);
        }
        songs.truncate(50);
        queue.set(songs);
        queue_index.set(0);
    });
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let mut seek_request = use_context::<SeekRequestSignal>().0;
    let mut last_bookmark = use_signal(|| None::<(String, u64)>);
    let mut last_song_for_bookmark = use_signal(|| None::<Song>);
    let audio_state = use_context::<Signal<AudioState>>();
    // Keep user interaction flag in a simple thread-local Cell (non-reactive) to avoid cross-scope Signal issues
    thread_local! {
        static USER_INTERACTED: Cell<bool> = Cell::new(false);
    }
    let mark_user_interacted = || USER_INTERACTED.with(|c| c.set(true));
    let has_user_interacted = || USER_INTERACTED.with(|c| c.get());

    // Track the current song ID to detect changes
    let mut last_song_id = use_signal(|| Option::<String>::None);
    let mut last_src = use_signal(|| Option::<String>::None);

    // Initialize audio element and set up event listeners
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let Some(audio) = get_or_create_audio_element() else {
            return;
        };

        // Mark user interaction on first click/keydown/touch
        if !has_user_interacted() {
            if let Some(doc) = window().and_then(|w| w.document()) {
                let click_cb =
                    Closure::wrap(Box::new(move || mark_user_interacted()) as Box<dyn FnMut()>);
                let key_cb =
                    Closure::wrap(Box::new(move || mark_user_interacted()) as Box<dyn FnMut()>);
                let touch_cb =
                    Closure::wrap(Box::new(move || mark_user_interacted()) as Box<dyn FnMut()>);
                let _ = doc
                    .add_event_listener_with_callback("click", click_cb.as_ref().unchecked_ref());
                let _ = doc
                    .add_event_listener_with_callback("keydown", key_cb.as_ref().unchecked_ref());
                let _ = doc.add_event_listener_with_callback(
                    "touchstart",
                    touch_cb.as_ref().unchecked_ref(),
                );
                click_cb.forget();
                key_cb.forget();
                touch_cb.forget();
            }
        }

        // Set up time update listener
        let mut current_time_signal = audio_state().current_time;
        let mut playback_pos = playback_position;
        // Throttle updates to ~5fps to avoid excessive re-renders
        let mut last_emit = 0.0f64;
        let time_closure = Closure::wrap(Box::new(move || {
            if let Some(audio) = get_or_create_audio_element() {
                let time = audio.current_time();
                if (time - last_emit).abs() >= 0.2 {
                    // 200ms cadence
                    last_emit = time;
                    current_time_signal.set(time);
                    playback_pos.set(time);
                }
            }
        }) as Box<dyn FnMut()>);
        audio.set_ontimeupdate(Some(time_closure.as_ref().unchecked_ref()));
        time_closure.forget();

        // Set up duration change listener
        let mut duration_signal = audio_state().duration;
        let dur_closure = Closure::wrap(Box::new(move || {
            if let Some(audio) = get_or_create_audio_element() {
                let dur = audio.duration();
                if !dur.is_nan() {
                    duration_signal.set(dur);
                }
            }
        }) as Box<dyn FnMut()>);
        audio.set_onloadedmetadata(Some(dur_closure.as_ref().unchecked_ref()));
        dur_closure.forget();

        // Set up ended listener for auto-next
        let end_closure = Closure::wrap(Box::new(move || {
            let idx = *queue_index.peek();
            let queue_list = queue();
            let current_repeat = *repeat_mode.peek();
            let current_shuffle = *shuffle_enabled.peek();

            // Mark current track as finished for Navidrome (server "Now Playing"/history)
            if let Some(current) = now_playing.peek().clone() {
                if let Some(server) = servers.peek().iter().find(|s| s.id == current.server_id) {
                    let song_id = current.id.clone();
                    let server = server.clone();
                    spawn(async move {
                        let client = NavidromeClient::new(server);
                        let _ = client.scrobble(&song_id, true).await;
                    });
                }
            }

            match current_repeat {
                RepeatMode::One => {
                    // Repeat-one is handled by audio.set_loop(true)
                }
                RepeatMode::All => {
                    if idx < queue_list.len().saturating_sub(1) {
                        queue_index.set(idx + 1);
                    } else if !queue_list.is_empty() {
                        queue_index.set(0);
                    } else {
                        is_playing.set(false);
                    }
                }
                RepeatMode::Off => {
                    if current_shuffle {
                        if queue_list.is_empty() {
                            spawn_shuffle_queue(
                                servers.peek().clone(),
                                queue.clone(),
                                queue_index.clone(),
                                now_playing.peek().clone(),
                            );
                        } else if idx < queue_list.len().saturating_sub(1) {
                            queue_index.set(idx + 1);
                        } else {
                            spawn_shuffle_queue(
                                servers.peek().clone(),
                                queue.clone(),
                                queue_index.clone(),
                                now_playing.peek().clone(),
                            );
                        }
                    } else if idx < queue_list.len().saturating_sub(1) {
                        queue_index.set(idx + 1);
                    } else {
                        is_playing.set(false);
                    }
                }
            }
        }) as Box<dyn FnMut()>);
        audio.set_onended(Some(end_closure.as_ref().unchecked_ref()));
        end_closure.forget();

        audio_state().is_initialized.set(true);
    });

    // Update song source when song changes
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let song = now_playing();
        let song_id = song.as_ref().map(|s| s.id.clone());
        let previous_song = last_song_for_bookmark();
        let mut last_bookmark = last_bookmark.clone();

        // Save a bookmark for the previous song when switching tracks (keeps position even if user never hit pause)
        if let Some(prev) = previous_song {
            if Some(prev.id.clone()) != song_id {
                let position_ms = (playback_position() * 1000.0).round().max(0.0) as u64;
                if position_ms > 1500 {
                    let servers = servers.peek().clone();
                    let song_id = prev.id.clone();
                    let server_id = prev.server_id.clone();
                    last_bookmark.set(Some((song_id.clone(), position_ms)));
                    spawn(async move {
                        if let Some(server) = servers.iter().find(|s| s.id == server_id).cloned() {
                            let client = NavidromeClient::new(server);
                            let _ = client.create_bookmark(&song_id, position_ms, None).await;
                        }
                    });
                }
            }
        }

        // Only update if song actually changed
        if song_id != last_song_id() {
            last_song_id.set(song_id.clone());
            last_song_for_bookmark.set(song.clone());

            if let Some(song) = song {
                let server_list = servers();
                let direct_url = song.stream_url.clone().filter(|url| !url.trim().is_empty());
                let resolved_url = if let Some(url) = direct_url {
                    Some(url)
                } else {
                    server_list
                        .iter()
                        .find(|s| s.id == song.server_id)
                        .map(|server| {
                            let client = NavidromeClient::new(server.clone());
                            client.get_stream_url(&song.id)
                        })
                };

                if let Some(url) = resolved_url {
                    if Some(url.clone()) != last_src() {
                        last_src.set(Some(url.clone()));

                        if let Some(audio) = get_or_create_audio_element() {
                            audio.set_src(&url);
                            audio.set_volume(volume().clamp(0.0, 1.0));
                            let pending_seek = seek_request.peek().clone();
                            if let Some((target_id, target_pos)) = pending_seek {
                                if target_id == song.id {
                                    audio.set_current_time(target_pos);
                                    seek_request.set(None);
                                }
                            }
                            // Only autoplay if user already interacted
                            if has_user_interacted() && is_playing() {
                                let _ = audio.play();
                            } else {
                                let _ = audio.pause();
                                is_playing.set(false);
                            }
                        }
                    }
                    // Report "now playing" to Navidrome for server-side visibility
                    if let Some(server) =
                        server_list.iter().find(|s| s.id == song.server_id).cloned()
                    {
                        let song_id = song.id.clone();
                        spawn(async move {
                            let client = NavidromeClient::new(server);
                            let _ = client.scrobble(&song_id, false).await;
                        });
                    }
                } else if let Some(audio) = get_or_create_audio_element() {
                    audio.set_src("");
                    is_playing.set(false);
                }
            }
        }
    });

    // Handle play/pause state changes
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let playing = is_playing();
        if let Some(audio) = get_or_create_audio_element() {
            if playing {
                if has_user_interacted() {
                    if audio.paused() {
                        let _ = audio.play();
                    }
                } else {
                    // Ensure we request playback once the user interacts to keep streaming behavior
                    if let Some(doc) = window().and_then(|w| w.document()) {
                        let play_cb = Closure::wrap(Box::new(move || {
                            mark_user_interacted();
                        }) as Box<dyn FnMut()>);
                        let _ = doc.add_event_listener_with_callback(
                            "click",
                            play_cb.as_ref().unchecked_ref(),
                        );
                        play_cb.forget();
                    }
                }
            } else if !audio.paused() {
                let _ = audio.pause();
            }
        }
    });

    // Handle repeat mode changes (RepeatMode::One should loop natively)
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let mode = repeat_mode();
        if let Some(audio) = get_or_create_audio_element() {
            audio.set_loop(mode == RepeatMode::One);
        }
    });

    // Automatically persist a server bookmark when playback stops/pauses
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let playing = is_playing();
        let song = now_playing();
        // Use the audio_state clock for the freshest position at pause time
        let position_ms = ((audio_state().current_time)() * 1000.0).round().max(0.0) as u64;
        let mut last_bookmark = last_bookmark.clone();

        if !playing {
            if let Some(song) = song {
                let should_save = match last_bookmark.peek().clone() {
                    Some((id, pos)) => id != song.id || position_ms.abs_diff(pos) >= 2000,
                    None => true,
                };

                if should_save && position_ms > 1500 {
                    let servers = servers.peek().clone();
                    let song_id = song.id.clone();
                    let server_id = song.server_id.clone();
                    last_bookmark.set(Some((song_id.clone(), position_ms)));

                    spawn(async move {
                        if let Some(server) = servers.iter().find(|s| s.id == server_id).cloned() {
                            let client = NavidromeClient::new(server);
                            let _ = client.create_bookmark(&song_id, position_ms, None).await;
                        }
                    });
                }
            }
        }
    });

    // Handle volume changes
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let vol = volume().clamp(0.0, 1.0);
        if let Some(audio) = get_or_create_audio_element() {
            audio.set_volume(vol);
        }
    });

    // Handle queue index changes (switching songs)
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let idx = queue_index();
        let queue_list = queue();
        let mut now_playing_mut = now_playing;

        if let Some(song) = queue_list.get(idx) {
            let is_same = {
                let current = now_playing_mut.peek();
                current.as_ref().map(|s| s.id.as_str()) == Some(song.id.as_str())
            };
            if !is_same {
                now_playing_mut.set(Some(song.clone()));
            }
        }
    });

    // Ensure the active song always exists in the queue
    #[cfg(target_arch = "wasm32")]
    use_effect(move || {
        let song = now_playing();
        if let Some(current) = song {
            let queue_list = queue();
            if let Some(pos) = queue_list.iter().position(|s| s.id == current.id) {
                if *queue_index.peek() != pos {
                    queue_index.set(pos);
                }
            } else {
                queue.set(vec![current]);
                queue_index.set(0);
            }
        }
    });

    // Return empty element - this component just manages state
    rsx! {}
}

/// Seek to a specific position in the current track
#[cfg(target_arch = "wasm32")]
pub fn seek_to(position: f64) {
    if let Some(audio) = get_or_create_audio_element() {
        audio.set_current_time(position);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn seek_to(_position: f64) {}

/// Get the current playback position
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn get_current_time() -> f64 {
    get_or_create_audio_element()
        .map(|a| a.current_time())
        .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_current_time() -> f64 {
    0.0
}

/// Get the current track duration
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn get_duration() -> f64 {
    get_or_create_audio_element()
        .map(|a| {
            let d = a.duration();
            if d.is_nan() {
                0.0
            } else {
                d
            }
        })
        .unwrap_or(0.0)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_duration() -> f64 {
    0.0
}
