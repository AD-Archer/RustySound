//! The app side of the Now Playing widgets (WidgetKit, iPhone and Mac).
//!
//! The widget extension (`apple/widgets`, embedded only with `RS_WIDGETS=1`)
//! can't see the app, so the app writes a small JSON snapshot of the song and
//! play state into the shared App Group container and asks WidgetKit to
//! reload. The widget draws progress from `position` + `updated_at`, so a
//! playing song needs no more writes until something changes: a new song,
//! play/pause, a new cover, or a seek (position drifting from where the last
//! snapshot says it should be).
//!
//! Sources, so nothing is tracked twice: on iPhone, the lock-screen Now
//! Playing info (`set_ios_now_playing_info` and its artwork), which keeps
//! running while the web view is suspended; on the Mac, the shell signals the
//! menu bar item follows.
//!
//! Commands come back the other way: the widget's buttons write
//! `command.json` and post a Darwin notification; the app reads the file and
//! runs the lock screen's handlers (iPhone) or the menu bar's (Mac).
//!
//! Everything native goes through `RustySoundWidgetBridge.framework`, which
//! the bundle script embeds next to the widget. Without it (plain builds,
//! `dx serve`) or without the App Group entitlement, every call here is a
//! no-op after one failed lookup.

#![cfg_attr(test, allow(dead_code))]

use serde::{Deserialize, Serialize};

/// Snapshot format; the widget ignores versions it doesn't know.
const SNAPSHOT_VERSION: u32 = 1;
/// A position this far from where the last snapshot predicts is a seek.
const SEEK_DRIFT_SECS: f64 = 3.0;
/// Seeks rewrite the snapshot at most this often. Song and play-state changes
/// always write at once.
const SEEK_WRITE_INTERVAL_SECS: f64 = 5.0;
/// Older commands are dropped (pressed while the app was suspended; replaying
/// them minutes later would surprise).
const COMMAND_TTL_SECS: f64 = 15.0;

/// The song as the widget shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// [`cover_id`] of the artwork, if the song has one.
    pub cover: Option<String>,
    pub duration: f64,
    pub live: bool,
}

/// The song plus where playback is.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetPlayback {
    pub track: WidgetTrack,
    pub position: f64,
    pub playing: bool,
    /// Playback speed while playing (1.0 normally).
    pub rate: f64,
}

/// A widget button press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetAction {
    TogglePlayPause,
    Play,
    Pause,
    Next,
    Previous,
}

/// `now-playing.json` in the App Group container.
#[derive(Debug, Serialize)]
struct Snapshot<'a> {
    version: u32,
    title: &'a str,
    artist: &'a str,
    album: &'a str,
    /// File name of the cover in the container, once it has been written.
    cover: Option<String>,
    duration: f64,
    /// Seconds into the song at `updated_at`.
    position: f64,
    playing: bool,
    rate: f64,
    live: bool,
    /// Unix seconds when this was written.
    updated_at: f64,
}

fn snapshot_json(now: &WidgetPlayback, cover_file: Option<String>, at: f64) -> String {
    let duration = finite_or_zero(now.track.duration);
    let mut position = finite_or_zero(now.position);
    if duration > 0.0 {
        position = position.min(duration);
    }
    let snapshot = Snapshot {
        version: SNAPSHOT_VERSION,
        title: &now.track.title,
        artist: &now.track.artist,
        album: &now.track.album,
        cover: cover_file,
        duration,
        position,
        playing: now.playing,
        rate: if now.playing { finite_or_zero(now.rate).max(0.0) } else { 0.0 },
        live: now.track.live,
        updated_at: at,
    };
    serde_json::to_string(&snapshot).unwrap_or_default()
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// A short, stable id for a cover (FNV-1a of its cache key), used for the
/// cover's file name so the widget can tell covers apart.
pub fn cover_id(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn cover_file_name(id: &str) -> String {
    format!("cover-{id}.img")
}

/// What was last written, to decide whether the next update is worth a write.
#[derive(Clone, Debug)]
struct Written {
    track: WidgetTrack,
    cover_file: Option<String>,
    playing: bool,
    rate: f64,
    position: f64,
    at: f64,
}

/// Decides when a playback update rewrites the snapshot (and reloads the
/// widget): at once for a new song, cover or play state; for a seek, at most
/// every [`SEEK_WRITE_INTERVAL_SECS`]; never for plain progress, which the
/// widget extrapolates.
#[derive(Debug, Default)]
struct Throttle {
    last: Option<Written>,
}

impl Throttle {
    fn should_write(&self, now: &WidgetPlayback, cover_file: Option<&str>, at: f64) -> bool {
        let Some(last) = self.last.as_ref() else {
            return true;
        };
        if last.track != now.track || last.cover_file.as_deref() != cover_file || last.playing != now.playing {
            return true;
        }
        if now.playing && (last.rate - now.rate).abs() > 0.01 {
            return true;
        }
        let mut expected = if last.playing {
            last.position + (at - last.at).max(0.0) * last.rate
        } else {
            last.position
        };
        if now.track.duration > 0.0 {
            expected = expected.min(now.track.duration);
        }
        (now.position - expected).abs() > SEEK_DRIFT_SECS && at - last.at >= SEEK_WRITE_INTERVAL_SECS
    }

    fn record(&mut self, now: &WidgetPlayback, cover_file: Option<String>, at: f64) {
        self.last = Some(Written {
            track: now.track.clone(),
            cover_file,
            playing: now.playing,
            rate: now.rate,
            position: now.position,
            at,
        });
    }
}

/// `command.json`, written by the widget's intents.
#[derive(Debug, Deserialize)]
struct Command {
    action: String,
    /// Unix seconds when the button was pressed.
    at: f64,
    id: String,
}

/// Parses a command, dropping stale ones and the one already handled
/// (`last_id`: the notification and the app's foreground check can both see
/// the same file).
fn parse_command(json: &str, now: f64, last_id: Option<&str>) -> Option<(String, WidgetAction)> {
    let command: Command = serde_json::from_str(json).ok()?;
    if Some(command.id.as_str()) == last_id || (now - command.at).abs() >= COMMAND_TTL_SECS {
        return None;
    }
    let action = match command.action.as_str() {
        "toggle" => WidgetAction::TogglePlayPause,
        "play" => WidgetAction::Play,
        "pause" => WidgetAction::Pause,
        "next" => WidgetAction::Next,
        "previous" => WidgetAction::Previous,
        _ => return None,
    };
    Some((command.id, action))
}

#[cfg(any(target_os = "ios", all(feature = "desktop", target_os = "macos")))]
pub use native::{available, listen, publish, store_cover};
// The iPhone also checks for a command when it comes to the foreground.
#[cfg(target_os = "ios")]
pub use native::take_command;

#[cfg(all(feature = "desktop", target_os = "macos"))]
pub use mac::use_now_playing_widget;

/// The container, the bridge and the publisher state.
#[cfg(any(target_os = "ios", all(feature = "desktop", target_os = "macos")))]
mod native {
    use super::*;
    use once_cell::sync::Lazy;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, Once, OnceLock};

    const SNAPSHOT_FILE: &str = "now-playing.json";
    const COMMAND_FILE: &str = "command.json";

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn free(ptr: *mut c_void);
    }
    const RTLD_NOW: c_int = 0x2;
    const RTLD_LOCAL: c_int = 0x4;

    /// `RustySoundWidgetBridge.framework` (apple/widgets/Bridge): WidgetKit
    /// is Swift-only, so the reload and the Darwin notification observer
    /// live there behind C entry points.
    struct Bridge {
        /// The App Group container; None without the entitlement.
        container: Option<PathBuf>,
        reload: extern "C" fn(),
        observe: extern "C" fn(extern "C" fn()),
    }

    static BRIDGE: Lazy<Option<Bridge>> = Lazy::new(load_bridge);

    fn load_bridge() -> Option<Bridge> {
        let exe = std::env::current_exe().ok()?;
        let app = exe.parent()?;
        // iPhone: App.app/Frameworks; Mac: App.app/Contents/MacOS/../Frameworks.
        let frameworks = if cfg!(target_os = "ios") { app.join("Frameworks") } else { app.parent()?.join("Frameworks") };
        let path = frameworks.join("RustySoundWidgetBridge.framework/RustySoundWidgetBridge");
        if !path.exists() {
            return None;
        }
        let path = CString::new(path.to_str()?).ok()?;
        unsafe {
            let handle = dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL);
            if handle.is_null() {
                return None;
            }
            let symbol = |name: &CStr| {
                let ptr = dlsym(handle, name.as_ptr());
                (!ptr.is_null()).then_some(ptr)
            };
            let container_fn: extern "C" fn() -> *mut c_char =
                std::mem::transmute(symbol(c"rs_widget_container_path")?);
            let reload = std::mem::transmute::<*mut c_void, extern "C" fn()>(symbol(c"rs_widget_reload")?);
            let observe = std::mem::transmute::<*mut c_void, extern "C" fn(extern "C" fn())>(
                symbol(c"rs_widget_observe_commands")?,
            );
            let raw = container_fn();
            let container = (!raw.is_null()).then(|| {
                let path = PathBuf::from(CStr::from_ptr(raw).to_string_lossy().into_owned());
                free(raw.cast());
                path
            });
            // One line per launch, for device and simulator logs.
            eprintln!("[widgets] bridge loaded, container: {container:?}");
            Some(Bridge { container, reload, observe })
        }
    }

    /// The bridge and the container, when both are there.
    fn widgets() -> Option<(&'static Bridge, &'static Path)> {
        let bridge = BRIDGE.as_ref()?;
        Some((bridge, bridge.container.as_deref()?))
    }

    /// Whether this build has the widget (and the App Group), so callers can
    /// skip building updates nobody will read.
    pub fn available() -> bool {
        widgets().is_some()
    }

    #[derive(Default)]
    struct Publisher {
        throttle: Throttle,
        /// The latest update, written or not, so a cover that arrives later
        /// can be published with it.
        latest: Option<WidgetPlayback>,
        /// The cover whose file is in the container.
        stored_cover: Option<String>,
    }

    static PUBLISHER: Lazy<Mutex<Publisher>> = Lazy::new(|| Mutex::new(Publisher::default()));

    fn unix_now() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or_default()
    }

    /// Writes `contents` to `dir/name` through a rename, so the widget never
    /// reads half a file.
    fn write_atomically(dir: &Path, name: &str, contents: &[u8]) -> bool {
        let temp = dir.join(format!(".{name}.tmp"));
        fs::write(&temp, contents).is_ok() && fs::rename(&temp, dir.join(name)).is_ok()
    }

    /// Publishes the current playback (None: nothing playing). Cheap to call
    /// often: it writes and reloads only when [`Throttle`] says so.
    pub fn publish(playback: Option<WidgetPlayback>) {
        let Some((bridge, dir)) = widgets() else {
            return;
        };
        let mut publisher = PUBLISHER.lock().unwrap_or_else(|e| e.into_inner());
        let Some(now) = playback else {
            publisher.latest = None;
            if publisher.throttle.last.take().is_some() {
                drop(publisher);
                let _ = fs::remove_file(dir.join(SNAPSHOT_FILE));
                (bridge.reload)();
            }
            return;
        };
        let cover_file = now
            .track
            .cover
            .as_deref()
            .filter(|id| publisher.stored_cover.as_deref() == Some(*id))
            .map(cover_file_name);
        let at = unix_now();
        let write = publisher.throttle.should_write(&now, cover_file.as_deref(), at);
        publisher.latest = Some(now.clone());
        if !write {
            return;
        }
        let json = snapshot_json(&now, cover_file.clone(), at);
        if write_atomically(dir, SNAPSHOT_FILE, json.as_bytes()) {
            publisher.throttle.record(&now, cover_file, at);
            drop(publisher);
            (bridge.reload)();
        }
    }

    /// Stores the cover `id` (see [`cover_id`]) for the widget, replacing the
    /// previous one, and republishes if it belongs to the current song.
    /// Blocking file work: call it off the main thread.
    pub fn store_cover(id: &str, bytes: &[u8]) {
        let Some((_, dir)) = widgets() else {
            return;
        };
        if bytes.is_empty() {
            return;
        }
        {
            let publisher = PUBLISHER.lock().unwrap_or_else(|e| e.into_inner());
            if publisher.stored_cover.as_deref() == Some(id) {
                return;
            }
        }
        let name = cover_file_name(id);
        if !write_atomically(dir, &name, bytes) {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let file = entry.file_name();
                let file = file.to_string_lossy();
                if file.starts_with("cover-") && file != name {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        let latest = {
            let mut publisher = PUBLISHER.lock().unwrap_or_else(|e| e.into_inner());
            publisher.stored_cover = Some(id.to_string());
            publisher.latest.clone()
        };
        if latest.as_ref().is_some_and(|now| now.track.cover.as_deref() == Some(id)) {
            publish(latest);
        }
    }

    static LAST_COMMAND: Mutex<Option<String>> = Mutex::new(None);

    /// Takes a pending widget command, if a fresh one is waiting.
    pub fn take_command() -> Option<WidgetAction> {
        let (_, dir) = widgets()?;
        let path = dir.join(COMMAND_FILE);
        let json = fs::read_to_string(&path).ok()?;
        let _ = fs::remove_file(&path);
        let mut last = LAST_COMMAND.lock().unwrap_or_else(|e| e.into_inner());
        let (id, action) = parse_command(&json, unix_now(), last.as_deref())?;
        *last = Some(id);
        Some(action)
    }

    static HANDLER: OnceLock<fn(WidgetAction)> = OnceLock::new();

    extern "C" fn on_command_notification() {
        if let (Some(handler), Some(action)) = (HANDLER.get(), take_command()) {
            handler(action);
        }
    }

    /// Runs `handler` (on the main thread) for every widget button press.
    /// The first call wins; later calls do nothing.
    pub fn listen(handler: fn(WidgetAction)) {
        static LISTEN: Once = Once::new();
        let Some((bridge, _)) = widgets() else {
            return;
        };
        LISTEN.call_once(|| {
            let _ = HANDLER.set(handler);
            (bridge.observe)(on_command_notification);
        });
    }
}

/// The Mac: publishes from the shell's signals and routes widget buttons to
/// the player bar, like the menu bar item.
#[cfg(all(feature = "desktop", target_os = "macos"))]
mod mac {
    use super::*;
    use crate::api::{ServerConfig, Song};
    use crate::components::{effective_rate, widget_track_for_song};
    use crate::components::press_player_button;
    use crate::db::AppSettings;
    use dioxus::prelude::*;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

    static ACTIONS: Lazy<Mutex<Option<UnboundedSender<WidgetAction>>>> = Lazy::new(|| Mutex::new(None));

    fn forward(action: WidgetAction) {
        if let Some(sender) = ACTIONS.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = sender.send(action);
        }
    }

    /// Keeps the widget snapshot in step with playback and handles the
    /// widget's buttons. Call once, from the shell, with the signals the menu
    /// bar item follows.
    pub fn use_now_playing_widget(
        now_playing: Signal<Option<Song>>,
        is_playing: Signal<bool>,
        position: Signal<f64>,
        servers: Signal<Vec<ServerConfig>>,
        settings: Signal<AppSettings>,
    ) {
        // The song's widget fields, rebuilt only when the song changes (not
        // on every position tick), plus the cover's URL for the download.
        let track = use_memo(move || {
            if !available() {
                return None;
            }
            now_playing
                .read()
                .as_ref()
                .map(|song| widget_track_for_song(song, &servers.peek()))
        });

        // The cover: the app's cover cache or one download per song.
        use_effect(move || {
            let Some((track, Some(url))) = track() else {
                return;
            };
            let Some(id) = track.cover.clone() else {
                return;
            };
            tokio::spawn(async move {
                if let Some(bytes) = crate::offline_art::cover_bytes(&url).await {
                    let _ = tokio::task::spawn_blocking(move || store_cover(&id, &bytes)).await;
                }
            });
        });

        // Progress ticks rerun this, but `publish` only writes on changes.
        use_effect(move || {
            let playback = track.read().as_ref().map(|(track, _)| WidgetPlayback {
                track: track.clone(),
                position: position(),
                playing: is_playing(),
                rate: effective_rate(settings.read().playback_rate, now_playing.peek().as_ref()),
            });
            publish(playback);
        });

        use_hook(move || {
            if !available() {
                return;
            }
            let (sender, mut receiver) = unbounded_channel();
            *ACTIONS.lock().unwrap_or_else(|e| e.into_inner()) = Some(sender);
            listen(forward);
            spawn(async move {
                while let Some(action) = receiver.recv().await {
                    let playing = *is_playing.peek();
                    match action {
                        WidgetAction::TogglePlayPause => press_player_button("play-pause-btn"),
                        WidgetAction::Play if !playing => press_player_button("play-pause-btn"),
                        WidgetAction::Pause if playing => press_player_button("play-pause-btn"),
                        WidgetAction::Next => press_player_button("next-btn"),
                        WidgetAction::Previous => press_player_button("prev-btn"),
                        WidgetAction::Play | WidgetAction::Pause => {}
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str) -> WidgetTrack {
        WidgetTrack {
            title: title.to_string(),
            artist: "Joni Mitchell".to_string(),
            album: "Blue (1971)".to_string(),
            cover: Some(cover_id("https://music.example/rest/getCoverArt?id=al-1&size=512")),
            duration: 200.0,
            live: false,
        }
    }

    fn playing(title: &str, position: f64) -> WidgetPlayback {
        WidgetPlayback { track: track(title), position, playing: true, rate: 1.0 }
    }

    #[test]
    fn the_snapshot_has_the_widgets_shape() {
        let json = snapshot_json(&playing("Blue", 12.5), Some("cover-abc.img".to_string()), 1_000.0);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["title"], "Blue");
        assert_eq!(value["artist"], "Joni Mitchell");
        assert_eq!(value["album"], "Blue (1971)");
        assert_eq!(value["cover"], "cover-abc.img");
        assert_eq!(value["duration"], 200.0);
        assert_eq!(value["position"], 12.5);
        assert_eq!(value["playing"], true);
        assert_eq!(value["rate"], 1.0);
        assert_eq!(value["live"], false);
        assert_eq!(value["updated_at"], 1_000.0);
        assert_eq!(value.as_object().unwrap().len(), 11);
    }

    #[test]
    fn the_snapshot_cleans_up_odd_numbers() {
        let mut now = playing("Blue", 250.0);
        now.playing = false;
        let value: serde_json::Value = serde_json::from_str(&snapshot_json(&now, None, 1.0)).unwrap();
        // Past the end is clamped, a paused song has no rate, no cover is null.
        assert_eq!(value["position"], 200.0);
        assert_eq!(value["rate"], 0.0);
        assert!(value["cover"].is_null());

        now.track.duration = f64::NAN;
        now.position = f64::INFINITY;
        let value: serde_json::Value = serde_json::from_str(&snapshot_json(&now, None, 1.0)).unwrap();
        assert_eq!(value["duration"], 0.0);
        assert_eq!(value["position"], 0.0);
    }

    #[test]
    fn cover_ids_are_stable_and_distinct() {
        assert_eq!(cover_id("a"), cover_id("a"));
        assert_ne!(cover_id("a"), cover_id("b"));
        assert_eq!(cover_id("").len(), 16);
        assert_eq!(cover_file_name("00ff"), "cover-00ff.img");
    }

    #[test]
    fn the_first_update_writes() {
        assert!(Throttle::default().should_write(&playing("Blue", 0.0), None, 0.0));
    }

    #[test]
    fn plain_progress_never_writes() {
        let mut throttle = Throttle::default();
        throttle.record(&playing("Blue", 10.0), None, 100.0);
        // 30 s later, 30 s further in: just what the widget extrapolates.
        assert!(!throttle.should_write(&playing("Blue", 40.0), None, 130.0));
        assert!(!throttle.should_write(&playing("Blue", 41.5), None, 130.0));
        // Paused stays put.
        let mut paused = playing("Blue", 40.0);
        paused.playing = false;
        throttle.record(&paused, None, 130.0);
        assert!(!throttle.should_write(&paused, None, 500.0));
    }

    #[test]
    fn state_changes_write_at_once() {
        let mut throttle = Throttle::default();
        let now = playing("Blue", 10.0);
        throttle.record(&now, None, 100.0);
        let mut paused = now.clone();
        paused.playing = false;
        assert!(throttle.should_write(&paused, None, 100.1));
        assert!(throttle.should_write(&playing("River", 0.0), None, 100.1));
        // The cover file arriving is a change too.
        assert!(throttle.should_write(&now, Some("cover-1.img"), 100.1));
        let mut faster = now.clone();
        faster.rate = 1.5;
        assert!(throttle.should_write(&faster, None, 100.1));
    }

    #[test]
    fn seeks_write_but_not_too_often() {
        let mut throttle = Throttle::default();
        throttle.record(&playing("Blue", 10.0), None, 100.0);
        // A jump to 120 s two seconds later: too soon after the last write.
        assert!(!throttle.should_write(&playing("Blue", 120.0), None, 102.0));
        // Still off by much after the interval: written.
        assert!(throttle.should_write(&playing("Blue", 125.0), None, 105.0));
        // Small jitter is not a seek.
        assert!(!throttle.should_write(&playing("Blue", 17.0), None, 105.0));
    }

    #[test]
    fn the_end_of_a_song_is_not_a_seek() {
        let mut throttle = Throttle::default();
        throttle.record(&playing("Blue", 190.0), None, 100.0);
        // Extrapolation stops at the duration, where the player sits.
        assert!(!throttle.should_write(&playing("Blue", 200.0), None, 160.0));
    }

    #[test]
    fn commands_are_fresh_known_and_handled_once() {
        let json = r#"{"action":"toggle","at":100.0,"id":"a"}"#;
        assert_eq!(parse_command(json, 101.0, None), Some(("a".to_string(), WidgetAction::TogglePlayPause)));
        assert_eq!(parse_command(json, 101.0, Some("a")), None);
        assert_eq!(parse_command(json, 100.0 + COMMAND_TTL_SECS + 1.0, None), None);
        let next = r#"{"action":"next","at":100.0,"id":"b"}"#;
        assert_eq!(parse_command(next, 100.0, Some("a")), Some(("b".to_string(), WidgetAction::Next)));
        assert_eq!(parse_command(r#"{"action":"dance","at":100.0,"id":"c"}"#, 100.0, None), None);
        assert_eq!(parse_command("not json", 100.0, None), None);
    }
}
