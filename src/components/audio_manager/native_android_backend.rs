// Android MediaPlayer/MediaSession bridge. The Kotlin implementation is copied
// into the generated Android project from build.rs.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_bridge_class_name(
    env: &mut dioxus::mobile::wry::prelude::JNIEnv,
    activity: &dioxus::mobile::wry::prelude::JObject,
) -> Option<String> {
    let class = env
        .call_method(activity, "getClass", "()Ljava/lang/Class;", &[])
        .ok()?
        .l()
        .ok()?;
    let package = env
        .call_method(&class, "getPackage", "()Ljava/lang/Package;", &[])
        .ok()?
        .l()
        .ok()?;
    let name = env
        .call_method(&package, "getName", "()Ljava/lang/String;", &[])
        .ok()?
        .l()
        .ok()?;
    let name = dioxus::mobile::wry::prelude::JString::from(name);
    let package_name = env.get_string(&name).ok()?.to_string_lossy().to_string();
    Some(format!("{package_name}.RustySoundAudioBridge"))
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_call_apply(
    env: &mut dioxus::mobile::wry::prelude::JNIEnv,
    activity: &dioxus::mobile::wry::prelude::JObject,
    payload: &str,
) -> Result<(), String> {
    let Some(class_name) = android_bridge_class_name(env, activity) else {
        return Err("unable to resolve Android audio bridge package".to_string());
    };
    let class = dioxus::mobile::wry::prelude::find_class(env, activity, class_name)
        .map_err(|err| format!("bridge class unavailable: {err}"))?;
    let payload = env
        .new_string(payload)
        .map_err(|err| format!("failed to allocate command string: {err}"))?;
    env.call_static_method(
        &class,
        "apply",
        "(Landroid/content/Context;Ljava/lang/String;)V",
        &[activity.into(), (&payload).into()],
    )
    .map_err(|err| format!("bridge apply failed: {err}"))?;
    Ok(())
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_call_snapshot(
    env: &mut dioxus::mobile::wry::prelude::JNIEnv,
    activity: &dioxus::mobile::wry::prelude::JObject,
) -> Result<String, String> {
    let Some(class_name) = android_bridge_class_name(env, activity) else {
        return Err("unable to resolve Android audio bridge package".to_string());
    };
    let class = dioxus::mobile::wry::prelude::find_class(env, activity, class_name)
        .map_err(|err| format!("bridge class unavailable: {err}"))?;
    let raw = env
        .call_static_method(
            &class,
            "snapshot",
            "(Landroid/content/Context;)Ljava/lang/String;",
            &[activity.into()],
        )
        .and_then(|value| value.l())
        .map_err(|err| format!("bridge snapshot failed: {err}"))?;
    let raw = dioxus::mobile::wry::prelude::JString::from(raw);
    env.get_string(&raw)
        .map(|value| value.to_string_lossy().to_string())
        .map_err(|err| format!("bridge snapshot decode failed: {err}"))
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_audio_command_payload(payload: String) {
    dioxus::mobile::wry::prelude::dispatch(move |env, activity, _webview| {
        if let Err(err) = android_call_apply(env, activity, &payload) {
            eprintln!("[android-audio] {err}");
        }
    });
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_update_playback_plan(
    queue: &[Song],
    index: usize,
    repeat_mode: RepeatMode,
    shuffle: bool,
    servers: &[ServerConfig],
    offline_mode: bool,
) {
    let items = queue
        .iter()
        .map(|song| {
            serde_json::json!({
                "song_id": song.id.clone(),
                "src": resolve_stream_url(song, servers, offline_mode),
                "meta": song_metadata(song, servers),
            })
        })
        .collect::<Vec<_>>();
    let repeat = match repeat_mode {
        RepeatMode::Off => "off",
        RepeatMode::All => "all",
        RepeatMode::One => "one",
    };
    native_audio_command(serde_json::json!({
        "type": "plan",
        "items": items,
        "index": index,
        "repeat": repeat,
        "shuffle": shuffle,
    }));
}
