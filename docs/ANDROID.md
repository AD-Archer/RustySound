# Android

The Android app renders the same Dioxus UI in a WebView, but plays audio natively: ExoPlayer in a Media3 `MediaSessionService`. Playback keeps going in the background, with lock-screen, notification, headset and Bluetooth controls.

## How playback works

```
Rust audio controller ──JSON commands──▶ AudioBridge.apply ──▶ ExoPlayer (main thread)
(audio_controller_native)  ◀──snapshot──  AudioBridge.snapshot ◀── player events
                                                   │
                              PlaybackService (MediaSession: notification, lock screen,
                              media keys, audio focus, becoming-noisy, foreground service)
```

- **One seam for every native backend.** `native_audio_command` / `native_audio_snapshot` (`src/components/audio_manager/native_bridge_runtime.rs`) go to the iOS AVPlayer, the Windows MediaPlayer, the desktop webview's audio deck, or, on Android, `native_android_backend.rs`, which calls the Kotlin `AudioBridge` over JNI. It finds the class through the activity's class loader once, and keeps the JVM and class for the life of the process.
- **The queue lives in ExoPlayer.** The controller sends the queue around the current song (50 behind, 300 ahead) as a `plan`. ExoPlayer plays it as a playlist, so songs follow each other with no gap, and the notification's next/previous work without Rust. It reports `gapless:<from>|<to>` when a song ends into the next, and the controller moves the queue the way it does for iOS and the desktop deck. Repeat-one, radio and previews play one song at a time, and the controller decides what comes next.
- **Who changed what.** `AudioBridge` remembers whether Rust asked to play. When something else changes it (the notification, a headset button, an incoming call, unplugged headphones), it reports `play` or `pause`, and the UI follows. The sleep timer's "End of Track" uses ExoPlayer's pause at the end of the item and reports `ended`.
- **Per-song levelling** travels with each playlist item, so ExoPlayer applies the next song's gain when it moves on by itself. Speed and volume are plain commands.
- **No polling in Kotlin.** `snapshot()` returns a position extrapolated from the last player event. The Rust controller polls it every 250 ms, as it does for every native backend.

### The process and the activity

The service runs in the app's process. tao exits that process when its window, the activity, is destroyed, so:

- **Back** never finishes the activity. It closes the open sheet, menu or Now Playing, then goes back a page, then moves the app to the background as Home does (Android 12+ does the same for launcher activities). Playback and the UI stay as they were.
- **Swiping the app away from Recents** ends the process and stops playback, which is the system convention. The service is `START_NOT_STICKY`, so nothing restarts empty.
- `configChanges` in the manifest covers rotation, dark mode, keyboards, density and locale, so none of these recreate the activity.

## Feeling native

`android/MainActivity.kt` replaces dx's one-line activity:

- **Edge to edge.** The UI draws under the status and navigation bars. Their sizes reach CSS as `--safe-top/right/bottom/left`, the same variables the iPhone build fills from `env(safe-area-inset-*)`. The on-screen keyboard shrinks the WebView instead of covering it.
- **Bar icons** follow whatever is under the status bar: light icons over the Cover Flow stage and dark themes, dark icons over light pages (`src/components/ui/android_shell.js`).
- **System Back and the predictive back gesture** (`enableOnBackInvokedCallback`), as described above.
- **Haptics.** A tick when a row swipe arms Remove, and when a row lifts for reordering (`row_gestures.js`).
- **Remembering the server.** The database and caches live in the app's private folders (`Context.getFilesDir()` / `getCacheDir()`, `src/storage.rs`). Android apps get no `$HOME`, so before this every launch started signed out.
- **Self-hosted servers.** Plain HTTP and user-installed CA certificates are allowed (`res/xml/rustysound_network_security.xml`), for servers on the home network.

## Android TV

The manifest adds the leanback launcher entry and a banner, and doesn't require a touch screen. On a TV, `RustySoundAndroid.isTv()` makes `android_shell.js` set `data-tv` on `<html>`. It moves focus with the D-pad to the nearest control in that direction (inside the open sheet when there is one), Enter activates it, and Back works as on phones. Focus rings get thicker (`base.css`). Cover Flow counts as one stop, and left/right turn it. The UI uses its wide layout. A 10-foot layout (a larger Cover Flow and a big Now Playing) is not done yet.

The app builds for 64-bit Android only (arm64, x86_64), because manganis, the crate behind dx's plugin hook, refuses 32-bit targets. The stock Intel Android TV emulator images are 32-bit, so the D-pad code has been tested on a phone emulator with `isTv()` forced on. It still needs a real TV or an arm64 TV image.

## How it gets into the build (dx 0.7.10)

dx generates the Gradle project itself. RustySound hooks into it in three places:

| What | Where | Hook |
| --- | --- | --- |
| Playback (Kotlin + Media3) | `android/rustysound-audio/` | A Gradle library module. The `#[manganis::ffi("android/rustysound-audio")] extern "Kotlin"` block in `native_android_backend.rs` leaves a marker in the binary, and dx copies the folder in as `:plugins:rustysoundaudio`. Its manifest (service and permissions), resources and dependencies merge into the app. The generated Rust wrapper is unused: it uses `FindClass`, which can't see app classes from Rust threads. |
| Activity | `android/MainActivity.kt` | `[application] android_main_activity` in `Dioxus.toml` |
| Manifest | `android/AndroidManifest.xml` | `[application] android_manifest`. This replaces dx's template, so permissions added through dx's own config don't apply. Add them here or in the module's manifest. |

`[android]` in `Dioxus.toml` sets SDK 35 (compile and target, needed by Media3 1.5) and min SDK 24.

## Building and testing without Android Studio

`android/docker/Dockerfile` holds everything: JDK 17, SDK 35, NDK 26.3, Rust with both Android targets, dx 0.7.10 and an x86_64 emulator.

```bash
docker build -t rustysound-android-dev android/docker
docker run --rm -v "$PWD":/work -v rs-android-target:/work/target \
  -v rs-android-cargo:/usr/local/cargo/registry -v rs-android-gradle:/root/.gradle \
  -v rs-android-keystore:/root/.android \
  rustysound-android-dev bash -c 'cd /work && dx build --platform android --target x86_64-linux-android'
```

Keep `/root/.android` in a volume so the debug signing key survives, or every build needs an uninstall first. The APK lands in `target/dx/rustysound/debug/android/app/app/build/outputs/apk/debug/`.

To test with the mock server, run the emulator headless (`emulator -avd rs35 -no-window -no-audio -gpu swiftshader_indirect`, with `--device /dev/kvm`). Then run `bun tests/mock-server/server.ts` in the same network namespace (`--network container:<emulator>`), and `adb reverse tcp:4533 tcp:4533`, so the app signs in to `http://127.0.0.1:4533` as demo/demo. Typing through `adb shell input` drops characters in the WebView. Drive the page through DevTools instead: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`.

Checks:

```bash
adb shell dumpsys media_session | grep -A12 app.adarcher.rustysound   # state=PLAYING, position moving
adb shell cmd media_session dispatch pause|play|next|previous         # what the lock screen sends
adb shell dumpsys activity services app.adarcher.rustysound           # PlaybackService isForeground=true
```
