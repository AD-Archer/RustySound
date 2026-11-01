// Native Android backend: ExoPlayer inside a Media3 MediaSessionService
// (android/rustysound-audio), driven over JNI. Rust sends the same JSON
// commands the other native backends take to `AudioBridge.apply` and reads
// `AudioBridge.snapshot`; see AudioBridge.kt for what the player does with
// them. On top of that the controller sends the queue around the current song
// ("plan", `android_sync_playback_plan`), so ExoPlayer moves on by itself:
// gapless, and with the notification's next/previous working while this UI
// is suspended in the background.
//
// The service shares the app's process. Destroying the activity ends that
// process (tao exits when its window goes), so the activity never finishes
// on Back: it moves to the background instead (android/MainActivity.kt).

/// Makes dx bundle android/rustysound-audio into the APK as the Gradle module
/// `:plugins:rustysoundaudio` (dx 0.7's hook for native Android code). The
/// wrapper type it generates goes unused: it finds classes with FindClass,
/// which can't see app classes from Rust's threads, so the calls below go
/// through the activity's class loader instead.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
#[allow(dead_code, unused_imports)]
mod android_audio_plugin {
    #[manganis::ffi("android/rustysound-audio")]
    extern "Kotlin" {
        pub type RustySoundAudio;
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_AUDIO_BRIDGE_CLASS: &str = "app.adarcher.rustysound.audio.AudioBridge";

/// How much of the queue ExoPlayer holds around the current song.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_PLAN_BEHIND: usize = 50;
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
const ANDROID_PLAN_AHEAD: usize = 300;

/// The JVM and the AudioBridge class, found once.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
struct AndroidAudioJni {
    vm: jni::JavaVM,
    class: jni::objects::GlobalRef,
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_audio() -> Option<&'static AndroidAudioJni> {
    static BRIDGE: std::sync::OnceLock<AndroidAudioJni> = std::sync::OnceLock::new();
    if let Some(bridge) = BRIDGE.get() {
        return Some(bridge);
    }
    // ndk_context panics outside an activity; try again on the next call.
    let found = std::panic::catch_unwind(android_audio_connect).ok().flatten()?;
    Some(BRIDGE.get_or_init(|| found))
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_audio_connect() -> Option<AndroidAudioJni> {
    use jni::objects::{JObject, JValue};
    let ctx = ndk_context::android_context();
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let class = {
        let mut env = vm.attach_current_thread_permanently().ok()?;
        let found = env.with_local_frame(8, |env| -> jni::errors::Result<_> {
            let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
            let loader = env
                .call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
                .l()?;
            let name = env.new_string(ANDROID_AUDIO_BRIDGE_CLASS)?;
            let class = env
                .call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)],
                )?
                .l()?;
            env.new_global_ref(class)
        });
        android_clear_exception(&mut env);
        found.ok()?
    };
    Some(AndroidAudioJni { vm, class })
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_clear_exception(env: &mut jni::JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}

/// Runs `f` on this thread's JNI env with the AudioBridge class. Threads stay
/// attached; the local frame frees what `f` creates.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn with_android_audio<R>(
    f: impl FnOnce(&mut jni::JNIEnv, &jni::objects::JClass) -> jni::errors::Result<R>,
) -> Option<R> {
    let bridge = android_audio()?;
    let mut env = bridge.vm.attach_current_thread_permanently().ok()?;
    let class = <&jni::objects::JClass>::from(bridge.class.as_obj());
    let result = env.with_local_frame(8, |env| f(env, class));
    android_clear_exception(&mut env);
    result.ok()
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_audio_apply(value: &serde_json::Value) {
    let payload = value.to_string();
    let _ = with_android_audio(|env, class| {
        let arg = env.new_string(&payload)?;
        env.call_static_method(
            class,
            "apply",
            "(Ljava/lang/String;)V",
            &[jni::objects::JValue::Object(&arg)],
        )?;
        Ok(())
    });
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_audio_snapshot() -> Option<NativeAudioSnapshot> {
    let raw = with_android_audio(|env, class| {
        let out = env
            .call_static_method(class, "snapshot", "()Ljava/lang/String;", &[])?
            .l()?;
        let text: String = env.get_string(&jni::objects::JString::from(out))?.into();
        Ok(text)
    })?;
    serde_json::from_str(&raw).ok()
}

/// Sends ExoPlayer the queue around the current song. It plays the current
/// song alone for repeat-one (the controller replays it), radio, and a song
/// that isn't the queue's current entry (previews). Repeat-all wraps natively
/// only when the whole queue fits; otherwise the controller wraps at the end.
#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
fn android_sync_playback_plan(
    queue: &[Song],
    index: usize,
    now_playing: Option<&Song>,
    repeat_mode: RepeatMode,
    servers: &[ServerConfig],
    settings: &AppSettings,
) {
    let in_queue = now_playing.is_some_and(|song| {
        queue
            .get(index)
            .is_some_and(|entry| entry.id == song.id && entry.server_id == song.server_id)
    });
    let single = !in_queue
        || repeat_mode == RepeatMode::One
        || now_playing.is_some_and(|song| song.server_name == "Radio");
    let (items, plan_index, repeat_all) = if single {
        (Vec::new(), 0, false)
    } else {
        let start = index.saturating_sub(ANDROID_PLAN_BEHIND);
        let end = (index + ANDROID_PLAN_AHEAD + 1).min(queue.len());
        let mut plan_index = index - start;
        let mut items = Vec::with_capacity(end - start);
        // ExoPlayer can't skip a song it has no source for (offline and not
        // downloaded, or a station): leave those out.
        for (offset, item) in native_plan_items(queue, start..end, servers, settings)
            .into_iter()
            .enumerate()
        {
            if item.src.is_some() && !item.meta.is_live {
                items.push(item);
            } else if offset < index - start {
                plan_index -= 1;
            }
        }
        let whole_queue = start == 0 && end == queue.len();
        (items, plan_index, repeat_mode == RepeatMode::All && whole_queue)
    };
    android_audio_apply(&serde_json::json!({
        "type": "plan",
        "items": items,
        "index": plan_index,
        "repeat_all": repeat_all,
    }));
}
