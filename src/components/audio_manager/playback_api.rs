// Public playback utility API consumed by UI components.
/// Seek to a specific position in the current track.
#[cfg(target_arch = "wasm32")]
pub fn seek_to(position: f64) {
    if let Some(audio) = get_or_create_audio_element() {
        audio.set_current_time(position);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn seek_to(position: f64) {
    native_audio_command(serde_json::json!({
        "type": "seek",
        "position": position.max(0.0),
    }));
}

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
