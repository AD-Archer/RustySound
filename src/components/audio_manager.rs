//! Audio Manager - Handles audio playback outside of the component render cycle.
//! Keeps audio side-effects isolated and defers signal writes to avoid borrow loops.

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

/// Global audio state that persists across renders.
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

/// Initialize the global audio element once.
#[cfg(target_arch = "wasm32")]
pub fn get_or_create_audio_element() -> Option<HtmlAudioElement> {
    let document = window()?.document()?;

    if let Some(existing) = document.get_element_by_id("rustysound-audio") {
        return existing.dyn_into::<HtmlAudioElement>().ok();
    }

    let audio: HtmlAudioElement = document.create_element("audio").ok()?.dyn_into().ok()?;
    audio.set_id("rustysound-audio");
    audio.set_attribute("preload", "metadata").ok()?;
    document.body()?.append_child(&audio).ok()?;

    Some(audio)
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn get_or_create_audio_element() -> Option<()> {
    None
}

#[cfg(target_arch = "wasm32")]
fn defer_signal_update<F>(f: F)
where
    F: FnOnce() + 'static,
{
    spawn(async move {
        gloo_timers::future::TimeoutFuture::new(0).await;
        f();
    });
}

/// Audio controller hook - manages playback imperatively.
#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn AudioController() -> Element {
    rsx! {}
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_shuffle_queue(
    servers: Vec<ServerConfig>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut now_playing: Signal<Option<Song>>,
    mut is_playing: Signal<bool>,
    seed_song: Option<Song>,
    play_state: Option<bool>,
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

        let first = songs.get(0).cloned();
        defer_signal_update(move || {
            queue.set(songs);
            queue_index.set(0);
            now_playing.set(first);
            if let Some(play_state) = play_state {
                is_playing.set(play_state);
            }
        });
    });
}

#[cfg(target_arch = "wasm32")]
fn resolve_stream_url(song: &Song, servers: &[ServerConfig]) -> Option<String> {
    if let Some(url) = song
        .stream_url
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(url);
    }

    servers
        .iter()
        .find(|s| s.id == song.server_id)
        .map(|server| {
            let client = NavidromeClient::new(server.clone());
            client.get_stream_url(&song.id)
        })
}

#[cfg(target_arch = "wasm32")]
fn scrobble_song(servers: &[ServerConfig], song: &Song, finished: bool) {
    let server = servers.iter().find(|s| s.id == song.server_id).cloned();
    if let Some(server) = server {
        let song_id = song.id.clone();
        spawn(async move {
            let client = NavidromeClient::new(server);
            let _ = client.scrobble(&song_id, finished).await;
        });
    }
}

#[cfg(target_arch = "wasm32")]
#[component]
pub fn AudioController() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut is_playing = use_context::<Signal<bool>>();
    let volume = use_context::<VolumeSignal>().0;
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let repeat_mode = use_context::<Signal<RepeatMode>>();
    let shuffle_enabled = use_context::<Signal<bool>>();
    let playback_position = use_context::<PlaybackPositionSignal>().0;
    let mut seek_request = use_context::<SeekRequestSignal>().0;
    let mut audio_state = use_context::<Signal<AudioState>>();

    let mut last_song_id = use_signal(|| None::<String>);
    let mut last_src = use_signal(|| None::<String>);
    let mut last_bookmark = use_signal(|| None::<(String, u64)>);
    let mut last_song_for_bookmark = use_signal(|| None::<Song>);

    thread_local! {
        static USER_INTERACTED: Cell<bool> = Cell::new(false);
    }
    let mark_user_interacted = || USER_INTERACTED.with(|c| c.set(true));
    let has_user_interacted = || USER_INTERACTED.with(|c| c.get());

    // One-time setup: create audio element and attach listeners.
    {
        let servers = servers.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let repeat_mode = repeat_mode.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        let playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();

        use_effect(move || {
            let Some(audio) = get_or_create_audio_element() else {
                return;
            };

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

            let mut current_time_signal = audio_state.peek().current_time;
            let mut duration_signal = audio_state.peek().duration;
            let mut playback_pos = playback_position.clone();
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let repeat_mode = repeat_mode.clone();
            let shuffle_enabled = shuffle_enabled.clone();
            let servers = servers.clone();

            audio_state.write().is_initialized.set(true);

            spawn(async move {
                let mut last_emit = 0.0f64;
                let mut last_duration = -1.0f64;
                let mut ended_for_song: Option<String> = None;

                loop {
                    gloo_timers::future::TimeoutFuture::new(200).await;

                    let Some(audio) = get_or_create_audio_element() else {
                        continue;
                    };

                    let time = audio.current_time();
                    if (time - last_emit).abs() >= 0.2 {
                        last_emit = time;
                        current_time_signal.set(time);
                        playback_pos.set(time);
                    }

                    let dur = audio.duration();
                    if !dur.is_nan() && (dur - last_duration).abs() > 0.5 {
                        last_duration = dur;
                        duration_signal.set(dur);
                    }

                    if audio.ended() {
                        let current_song = { now_playing.read().clone() };
                        let current_id = current_song.as_ref().map(|s| s.id.clone());
                        if ended_for_song == current_id {
                            continue;
                        }
                        ended_for_song = current_id.clone();

                        let queue_snapshot = { queue.read().clone() };
                        let idx = { *queue_index.read() };
                        let repeat = { *repeat_mode.read() };
                        let shuffle = { *shuffle_enabled.read() };
                        let servers_snapshot = { servers.read().clone() };

                        if let Some(song) = current_song.clone() {
                            scrobble_song(&servers_snapshot, &song, true);
                        }

                        if repeat == RepeatMode::One {
                            continue;
                        }

                        let len = queue_snapshot.len();
                        if len == 0 {
                            is_playing.set(false);
                            continue;
                        }

                        let should_shuffle = repeat == RepeatMode::Off && shuffle;
                        if should_shuffle {
                            if idx < len.saturating_sub(1) {
                                if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                    queue_index.set(idx + 1);
                                    now_playing.set(Some(song));
                                }
                            } else {
                                spawn_shuffle_queue(
                                    servers_snapshot,
                                    queue.clone(),
                                    queue_index.clone(),
                                    now_playing.clone(),
                                    is_playing.clone(),
                                    current_song,
                                    Some(true),
                                );
                            }
                            continue;
                        }

                        if idx < len.saturating_sub(1) {
                            if let Some(song) = queue_snapshot.get(idx + 1).cloned() {
                                queue_index.set(idx + 1);
                                now_playing.set(Some(song));
                            }
                        } else if repeat == RepeatMode::All {
                            if let Some(song) = queue_snapshot.get(0).cloned() {
                                queue_index.set(0);
                                now_playing.set(Some(song));
                            }
                        } else {
                            is_playing.set(false);
                        }
                    } else {
                        ended_for_song = None;
                    }
                }
            });
        });
    }

    // Keep queue_index aligned when now_playing changes and the song is in the queue.
    {
        let mut queue_index = queue_index.clone();
        let queue = queue.clone();
        let now_playing = now_playing.clone();
        use_effect(move || {
            let song = now_playing();
            let queue_list = queue();
            if let Some(song) = song {
                if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
                    if pos != queue_index() {
                        defer_signal_update(move || {
                            queue_index.set(pos);
                        });
                    }
                }
            }
        });
    }

    // Update audio source and track changes for bookmarks/scrobbles.
    {
        let servers = servers.clone();
        let volume = volume.clone();
        let mut now_playing = now_playing.clone();
        let mut is_playing = is_playing.clone();
        let mut playback_position = playback_position.clone();
        let mut audio_state = audio_state.clone();
        let mut seek_request = seek_request.clone();
        let mut last_song_id = last_song_id.clone();
        let mut last_src = last_src.clone();
        let mut last_bookmark = last_bookmark.clone();
        let mut last_song_for_bookmark = last_song_for_bookmark.clone();
        use_effect(move || {
            let song = now_playing();
            let song_id = song.as_ref().map(|s| s.id.clone());
            let previous_song = last_song_for_bookmark.peek().clone();

            if let Some(prev) = previous_song {
                if Some(prev.id.clone()) != song_id {
                    let position_ms = get_or_create_audio_element()
                        .map(|a| a.current_time())
                        .unwrap_or(0.0)
                        .mul_add(1000.0, 0.0)
                        .round()
                        .max(0.0) as u64;
                    if position_ms > 1500 {
                        let servers_snapshot = servers.peek().clone();
                        let song_id = prev.id.clone();
                        let server_id = prev.server_id.clone();
                        spawn(async move {
                            last_bookmark.set(Some((song_id.clone(), position_ms)));
                            if let Some(server) =
                                servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                            {
                                let client = NavidromeClient::new(server);
                                let _ = client.create_bookmark(&song_id, position_ms, None).await;
                            }
                        });
                    }
                }
            }

            let last_id = last_song_id.peek().clone();
            if song_id != last_id {
                last_song_id.set(song_id.clone());
                last_song_for_bookmark.set(song.clone());
            }

            let Some(song) = song else {
                if let Some(audio) = get_or_create_audio_element() {
                    audio.set_src("");
                }
                last_src.set(None);
                is_playing.set(false);
                return;
            };

            let servers_snapshot = servers.peek().clone();
            if let Some(url) = resolve_stream_url(&song, &servers_snapshot) {
                if Some(url.clone()) != *last_src.peek() {
                    last_src.set(Some(url.clone()));
                    if let Some(audio) = get_or_create_audio_element() {
                        audio.set_src(&url);
                        audio.set_volume(volume.peek().clamp(0.0, 1.0));

                        if let Some((target_id, target_pos)) = seek_request.peek().clone() {
                            if target_id == song.id {
                                audio.set_current_time(target_pos);
                                let mut playback_position = playback_position.clone();
                                let mut audio_state = audio_state.clone();
                                defer_signal_update(move || {
                                    playback_position.set(target_pos);
                                    audio_state.write().current_time.set(target_pos);
                                    seek_request.set(None);
                                });
                            }
                        }

                        let was_playing = *is_playing.peek();
                        if has_user_interacted() && was_playing {
                            let _ = audio.play();
                        } else {
                            let _ = audio.pause();
                            is_playing.set(false);
                        }
                    }
                }

                scrobble_song(&servers_snapshot, &song, false);
            } else if let Some(audio) = get_or_create_audio_element() {
                audio.set_src("");
                last_src.set(None);
                is_playing.set(false);
            }
        });
    }

    // Handle play/pause state changes.
    {
        let mut is_playing = is_playing.clone();
        use_effect(move || {
            let playing = is_playing();
            if let Some(audio) = get_or_create_audio_element() {
                if playing {
                    if has_user_interacted() {
                        if audio.paused() {
                            let _ = audio.play();
                        }
                    } else {
                        is_playing.set(false);
                    }
                } else if !audio.paused() {
                    let _ = audio.pause();
                }
            }
        });
    }

    // Handle repeat mode changes.
    {
        let repeat_mode = repeat_mode.clone();
        use_effect(move || {
            let mode = repeat_mode();
            if let Some(audio) = get_or_create_audio_element() {
                audio.set_loop(mode == RepeatMode::One);
            }
        });
    }

    // Handle volume changes.
    {
        let volume = volume.clone();
        use_effect(move || {
            let vol = volume().clamp(0.0, 1.0);
            if let Some(audio) = get_or_create_audio_element() {
                audio.set_volume(vol);
            }
        });
    }

    // Persist a server bookmark when playback stops/pauses.
    {
        let servers = servers.clone();
        let mut last_bookmark = last_bookmark.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        use_effect(move || {
            let playing = is_playing();
            if playing {
                return;
            }

            let Some(song) = now_playing() else {
                return;
            };

            let position_ms = get_or_create_audio_element()
                .map(|a| a.current_time())
                .unwrap_or(0.0)
                .mul_add(1000.0, 0.0)
                .round()
                .max(0.0) as u64;

            if position_ms <= 1500 {
                return;
            }

            let should_save = match last_bookmark.peek().clone() {
                Some((id, pos)) => id != song.id || position_ms.abs_diff(pos) >= 2000,
                None => true,
            };

            if should_save {
                let servers_snapshot = servers.peek().clone();
                let song_id = song.id.clone();
                let server_id = song.server_id.clone();
                spawn(async move {
                    last_bookmark.set(Some((song_id.clone(), position_ms)));
                    if let Some(server) =
                        servers_snapshot.iter().find(|s| s.id == server_id).cloned()
                    {
                        let client = NavidromeClient::new(server);
                        let _ = client.create_bookmark(&song_id, position_ms, None).await;
                    }
                });
            }
        });
    }

    rsx! {}
}

/// Seek to a specific position in the current track.
#[cfg(target_arch = "wasm32")]
pub fn seek_to(position: f64) {
    if let Some(audio) = get_or_create_audio_element() {
        audio.set_current_time(position);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn seek_to(_position: f64) {}

/// Get the current playback position.
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

/// Get the current track duration.
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

/// Helper function to play a song and keep queue/now_playing aligned.
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
pub fn play_song(
    song: Song,
    mut now_playing: Signal<Option<Song>>,
    mut queue: Signal<Vec<Song>>,
    mut queue_index: Signal<usize>,
    mut is_playing: Signal<bool>,
) {
    let queue_list = queue.read().clone();
    if let Some(pos) = queue_list.iter().position(|s| s.id == song.id) {
        queue_index.set(pos);
        now_playing.set(Some(song));
    } else {
        queue.set(vec![song.clone()]);
        queue_index.set(0);
        now_playing.set(Some(song));
    }
    is_playing.set(true);
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn play_song(
    _song: crate::api::Song,
    _now_playing: Signal<Option<crate::api::Song>>,
    _queue: Signal<Vec<crate::api::Song>>,
    _queue_index: Signal<usize>,
    _is_playing: Signal<bool>,
) {
}
