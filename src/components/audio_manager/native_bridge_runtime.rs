// Native bridge dispatch and metadata mapping helpers used by native controllers.
#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "windows")
))]
fn ensure_native_audio_bridge() {
    let _ = document::eval(NATIVE_AUDIO_BOOTSTRAP_JS);
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn ensure_native_audio_bridge() {
    let _ = with_windows_player(|_| ());
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn ensure_native_audio_bridge() {
    let _ = with_ios_player(|_| ());
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "windows")
))]
fn native_audio_command(value: serde_json::Value) {
    ensure_native_audio_bridge();
    let payload = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    let script = format!(
        r#"(function () {{
            const bridge = window.__rustysoundAudioBridge;
            if (!bridge) return false;
            bridge.apply({payload});
            return true;
        }})();"#
    );
    let _ = document::eval(&script);
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
fn native_audio_command(value: serde_json::Value) {
    let _ = with_windows_player(|player| player.apply(value));
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn native_audio_command(value: serde_json::Value) {
    let _ = with_ios_player(|player| player.apply(value));
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "windows")
))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    ensure_native_audio_bridge();
    let eval = document::eval(
        r#"return (function () {
            const bridge = window.__rustysoundAudioBridge;
            const raw = (bridge && typeof bridge.snapshot === "function")
              ? (bridge.snapshot() || {})
              : {};
            const currentTime = Number.isFinite(raw.current_time) ? raw.current_time : 0;
            const duration = Number.isFinite(raw.duration) ? raw.duration : 0;
            const paused = !!raw.paused;
            const ended = !!raw.ended;
            const action = typeof raw.action === "string" ? raw.action : null;
            return {
              current_time: currentTime,
              duration,
              paused,
              ended,
              action,
            };
        })();"#,
    );
    eval.join::<NativeAudioSnapshot>().await.ok()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    with_windows_player(|player| player.snapshot())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
async fn native_audio_snapshot() -> Option<NativeAudioSnapshot> {
    with_ios_player(|player| player.snapshot())
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "ios"),
    not(target_os = "windows")
))]
async fn native_delay_ms(ms: u64) {
    let script = format!(
        r#"return (async function () {{
            await new Promise(resolve => setTimeout(resolve, {ms}));
            return true;
        }})();"#
    );
    let _ = document::eval(&script).await;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
async fn native_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
async fn native_delay_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn song_metadata(song: &Song, servers: &[ServerConfig]) -> NativeTrackMetadata {
    let is_live = song.server_name == "Radio";
    let title = if is_live && song.title.trim().eq_ignore_ascii_case("unknown song") {
        song.artist
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown Song".to_string())
    } else {
        song.title.clone()
    };
    let artist = song
        .artist
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if is_live {
                "Internet Radio".to_string()
            } else {
                "Unknown Artist".to_string()
            }
        });

    let mut album = if is_live {
        "LIVE".to_string()
    } else {
        song.album
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown Album".to_string())
    };
    if !is_live {
        if let Some(year) = song.year {
            album = format!("{album} ({year})");
        }
    }

    let artwork = servers
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            song.cover_art
                .as_ref()
                .map(|cover| NavidromeClient::new(server.clone()).get_cover_art_url(cover, 512))
        });

    NativeTrackMetadata {
        title,
        artist,
        album,
        artwork,
        duration: song.duration as f64,
        is_live,
    }
}

