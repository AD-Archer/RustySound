// Remote output: a player that isn't this device (a network speaker,
// `components::cast`) standing in for the local backend. While one is set,
// the native controller's commands go to it and its state comes back as
// the snapshot, so the controller, the queue and every player view work
// unchanged: ended songs move the queue on and the next song loads there.
// Remote controls (media keys, the lock screen, the menu bar) still arrive
// through the local backend's snapshot and are passed on.

/// A backend command, typed: what a remote output acts on. Commands it
/// can't use (rate, equalizer, gapless arming, metadata) aren't sent.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OutputCommand {
    /// Play `song_id` (the now-playing song) from `position`.
    Load { song_id: String, position: f64, play: bool },
    Play,
    Pause,
    Seek(f64),
    /// The user's volume, 0–1 (levelling isn't applied to a speaker).
    Volume(f64),
    Clear,
}

#[cfg(not(target_arch = "wasm32"))]
impl OutputCommand {
    fn from_backend(value: &serde_json::Value) -> Option<Self> {
        let number = |key: &str| value.get(key).and_then(|v| v.as_f64()).filter(|v| v.is_finite());
        match value.get("type")?.as_str()? {
            "load" => Some(OutputCommand::Load {
                song_id: value.get("song_id")?.as_str()?.to_string(),
                position: number("position").unwrap_or(0.0).max(0.0),
                play: value.get("play").and_then(|v| v.as_bool()).unwrap_or(false),
            }),
            "play" => Some(OutputCommand::Play),
            "pause" => Some(OutputCommand::Pause),
            "seek" => Some(OutputCommand::Seek(number("position")?.max(0.0))),
            "volume" => Some(OutputCommand::Volume(number("user").or_else(|| number("value"))?.clamp(0.0, 1.0))),
            "clear" => Some(OutputCommand::Clear),
            _ => None,
        }
    }
}

/// A remote output's state, read every controller tick (250 ms): keep it
/// cheap (a cached reading, not a network call).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OutputState {
    pub current_time: f64,
    pub duration: f64,
    pub paused: bool,
    pub ended: bool,
    pub song_id: Option<String>,
    /// `play` / `pause` when the output changed by itself.
    pub action: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait RemoteOutput: Send + Sync {
    /// Called on the UI thread; must not block.
    fn command(&self, command: OutputCommand);
    fn state(&self) -> OutputState;
}

#[cfg(not(target_arch = "wasm32"))]
static REMOTE_OUTPUT: std::sync::RwLock<Option<std::sync::Arc<dyn RemoteOutput>>> =
    std::sync::RwLock::new(None);

#[cfg(not(target_arch = "wasm32"))]
fn remote_output() -> Option<std::sync::Arc<dyn RemoteOutput>> {
    REMOTE_OUTPUT.read().ok()?.clone()
}

/// Puts a remote output in place of this device (`Some`), or takes it
/// away (`None`; follow with `resume_local_output`). Taking over clears the
/// local player, so nothing plays in two places and the lock screen's
/// play button can't wake it.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn set_remote_output(output: Option<std::sync::Arc<dyn RemoteOutput>>) {
    let taking_over = output.is_some();
    if let Ok(mut slot) = REMOTE_OUTPUT.write() {
        *slot = output;
    }
    if taking_over {
        local_audio_command(serde_json::json!({ "type": "clear" }));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_audio_command(value: serde_json::Value) {
    match remote_output() {
        Some(remote) => {
            if let Some(command) = OutputCommand::from_backend(&value) {
                remote.command(command);
            }
        }
        None => local_audio_command(value),
    }
}

/// Remote-control actions the local backend reports that a remote output
/// should get too.
#[cfg(not(target_arch = "wasm32"))]
fn is_remote_control_action(action: &str) -> bool {
    matches!(
        action,
        "play" | "pause" | "toggle_play" | "playpause" | "next" | "previous" | "previous_track"
    ) || action.starts_with("seek:")
}

#[cfg(not(target_arch = "wasm32"))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    let Some(remote) = remote_output() else {
        return local_audio_snapshot().await;
    };
    let local_action = local_audio_snapshot()
        .await
        .and_then(|snapshot| snapshot.action)
        .filter(|action| is_remote_control_action(action));
    // The local backend has already seeked itself; the remote hasn't.
    if let Some(target) = local_action
        .as_deref()
        .and_then(|action| action.strip_prefix("seek:"))
        .and_then(|raw| raw.parse::<f64>().ok())
    {
        remote.command(OutputCommand::Seek(target.max(0.0)));
    }
    let state = remote.state();
    Some(NativeAudioSnapshot {
        current_time: state.current_time,
        duration: state.duration,
        paused: state.paused,
        ended: state.ended,
        action: local_action.or(state.action),
        song_id: state.song_id,
    })
}

/// Hands playback back to this device after a remote output: loads `song`
/// here at `position` (the controller thinks it's already loaded).
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn resume_local_output(
    song: &Song,
    servers: &[ServerConfig],
    offline_mode: bool,
    position: f64,
    play: bool,
    volume: f64,
) {
    let Some(src) = resolve_stream_url(song, servers, offline_mode) else {
        return;
    };
    let user = volume.clamp(0.0, 1.0);
    local_audio_command(serde_json::json!({
        "type": "load",
        "src": src,
        "song_id": song.id,
        "position": position.max(0.0),
        "volume": levelled_volume(user, levelling_gain_now()),
        "user": user,
        "play": play,
        "meta": song_metadata(song, servers),
    }));
}

/// The title, artist and album a remote output shows: the lock screen's.
#[cfg(not(target_arch = "wasm32"))]
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn output_labels(song: &Song, servers: &[ServerConfig]) -> (String, String, String) {
    let labels = track_labels(song, servers);
    (labels.title, labels.artist, labels.album)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod remote_output_tests {
    use super::OutputCommand;
    use serde_json::json;

    #[test]
    fn backend_commands_become_output_commands() {
        let load = json!({ "type": "load", "src": "file:///x.flac", "song_id": "s1", "position": 12.5, "play": true });
        assert_eq!(
            OutputCommand::from_backend(&load),
            Some(OutputCommand::Load { song_id: "s1".into(), position: 12.5, play: true })
        );
        let volume = json!({ "type": "volume", "value": 0.2, "user": 0.4, "ramp_ms": 0 });
        assert_eq!(OutputCommand::from_backend(&volume), Some(OutputCommand::Volume(0.4)));
        assert_eq!(OutputCommand::from_backend(&json!({ "type": "seek", "position": -3.0 })), Some(OutputCommand::Seek(0.0)));
        assert_eq!(OutputCommand::from_backend(&json!({ "type": "clear" })), Some(OutputCommand::Clear));
        for ignored in ["rate", "arm", "disarm", "metadata", "loop", "eq"] {
            assert_eq!(OutputCommand::from_backend(&json!({ "type": ignored })), None);
        }
        assert_eq!(OutputCommand::from_backend(&json!({ "type": "load" })), None);
    }

    #[test]
    fn only_remote_controls_pass_through() {
        assert!(super::is_remote_control_action("next"));
        assert!(super::is_remote_control_action("seek:42"));
        assert!(!super::is_remote_control_action("ended"));
        assert!(!super::is_remote_control_action("gapless:a>b"));
    }
}
