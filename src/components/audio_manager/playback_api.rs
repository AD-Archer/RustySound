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
    if let Some(pos) = find_song_instance_index(&queue_list, &song) {
        queue_index.set(pos);
        now_playing.set(Some(song));
    } else {
        queue.set(normalize_manual_queue_songs(vec![song.clone()]));
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
    if let Some(pos) = find_song_instance_index(&queue_list, &song) {
        queue_index.set(pos);
        now_playing.set(Some(song));
    } else {
        queue.set(normalize_manual_queue_songs(vec![song.clone()]));
        queue_index.set(0);
        now_playing.set(Some(song));
    }
    is_playing.set(true);
}

/// Sets the output volume directly, without touching `VolumeSignal` (fades:
/// the sleep timer and the preview's duck). The next volume change or track
/// load puts the setting back.
#[cfg(target_arch = "wasm32")]
pub fn set_output_volume(value: f64) {
    if let Some(audio) = get_or_create_audio_element() {
        audio.set_volume(value.clamp(0.0, 1.0));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_output_volume(value: f64) {
    native_audio_command(serde_json::json!({
        "type": "volume",
        "value": value.clamp(0.0, 1.0),
    }));
}

/// Eased 0→1 progress for volume fades (smoothstep: no click at either end).
pub fn fade_curve(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Moves the output volume from `from` to `to` over `ms`, in small steps.
/// Cancelling the task stops the ramp where it is.
pub async fn ramp_output_volume(from: f64, to: f64, ms: u64) {
    let step_ms = if ms > 2000 { 100 } else { 40 };
    let steps = (ms / step_ms).max(1);
    for i in 1..=steps {
        crate::components::ui::env::sleep_ms(step_ms).await;
        let t = fade_curve(i as f64 / steps as f64);
        set_output_volume(from + (to - from) * t);
    }
}

/// Sets the playback rate, keeping the pitch (`preservesPitch`). The default
/// rate is set too, so the next track loads at the same speed.
#[cfg(target_arch = "wasm32")]
pub fn set_playback_rate(rate: f64) {
    let Some(audio) = get_or_create_audio_element() else {
        return;
    };
    let target = wasm_bindgen::JsValue::from(audio.clone());
    for key in ["preservesPitch", "webkitPreservesPitch", "mozPreservesPitch"] {
        let _ = js_sys::Reflect::set(&target, &key.into(), &true.into());
    }
    audio.set_default_playback_rate(rate);
    audio.set_playback_rate(rate);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn set_playback_rate(rate: f64) {
    native_audio_command(serde_json::json!({ "type": "rate", "value": rate }));
}

/// "End of Track" sleep timer on iOS: AVPlayer moves to the next song by
/// itself when one ends (so it works while the web view is suspended); this
/// makes it pause there instead. Elsewhere the controllers' ended handling
/// asks the sleep timer, so this does nothing.
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
pub fn set_stop_after_current(on: bool) {
    IOS_STOP_AFTER_CURRENT.store(on, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(not(all(not(target_arch = "wasm32"), target_os = "ios")))]
pub fn set_stop_after_current(_on: bool) {}

#[cfg(test)]
mod output_tests {
    use super::fade_curve;

    #[test]
    fn fade_curve_is_eased_and_clamped() {
        assert_eq!(fade_curve(0.0), 0.0);
        assert_eq!(fade_curve(1.0), 1.0);
        assert_eq!(fade_curve(0.5), 0.5);
        assert!(fade_curve(0.1) < 0.1);
        assert_eq!(fade_curve(-1.0), 0.0);
        assert_eq!(fade_curve(2.0), 1.0);
    }
}
