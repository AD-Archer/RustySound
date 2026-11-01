//! 30-second previews ("Quick Listen"), played on their own page `Audio`
//! element so the queue is never touched. Discover plays off-server picks
//! from their preview URLs; the track menu plays library songs from the
//! server stream, starting around the chorus (`quick_listen_start`).
//!
//! A preview fades in over 0.4 s and out over 0.6 s (on stop, on replace and
//! at the end of the clip). Main playback fades down and pauses meanwhile,
//! then resumes and fades back up. Pressing the main play button during a
//! preview stops the preview.
//!
//! The page script also publishes the preview's state for the kit's
//! `PreviewRing`, with no re-render per frame: `data-rs-preview` on `<html>`
//! ("loading" while buffering, "playing") and `--rs-preview-p`, the clip's
//! progress from 0 to 1, updated on `timeupdate`.

use super::{ramp_output_volume, set_output_volume};
use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::ui::env::sleep_ms;
use crate::components::{IsPlayingSignal, VolumeSignal};
use dioxus::core::{current_scope_id, Runtime, Task};
use dioxus::prelude::*;

/// Length of a preview clip, in seconds.
pub const PREVIEW_SECONDS: f64 = 30.0;
/// Main playback fades down over this long when a preview starts...
const DUCK_MS: u64 = 400;
/// ...and back up over this long when it ends.
const RESUME_MS: u64 = 600;

/// Where a library song's Quick Listen starts: about 35% in, roughly where a
/// chorus tends to be, but early enough to leave a whole clip. Songs no
/// longer than a clip (or of unknown length) start at the top.
pub fn quick_listen_start(duration: f64) -> f64 {
    if !duration.is_finite() || duration <= PREVIEW_SECONDS {
        return 0.0;
    }
    (duration * 0.35).min(duration - PREVIEW_SECONDS).max(0.0)
}

/// Starts the preview inside the click's call stack (the first statements run
/// synchronously on web, so `play()` keeps the user gesture), then waits for
/// it to end, fail, be replaced or be stopped, and returns why. Each preview
/// gets a fresh element, so a replaced one fades out while the new one fades
/// in. The fade-out after the result runs on, detached.
const PLAY_JS: &str = r#"
const url = __URL__;
const start = __START__;
const clip = __CLIP__;
const root = document.documentElement;
const setState = (s) => { if (s) root.setAttribute("data-rs-preview", s); else root.removeAttribute("data-rs-preview"); };
const setProgress = (p) => root.style.setProperty("--rs-preview-p", String(Math.max(0, Math.min(1, p))));
const fade = (el, to, ms) => new Promise((resolve) => {
  if (el.__rsFade) clearInterval(el.__rsFade);
  const from = el.volume;
  const t0 = performance.now();
  el.__rsFade = setInterval(() => {
    const t = Math.min(1, (performance.now() - t0) / ms);
    try { el.volume = from + (to - from) * t * t * (3 - 2 * t); } catch (_) {}
    if (t >= 1) { clearInterval(el.__rsFade); el.__rsFade = null; resolve(); }
  }, 30);
});
const previous = window.__rsPreviewDone;
if (previous) previous("replaced");
const a = new Audio();
a.preload = "auto";
a.setAttribute("playsinline", "true");
a.volume = 0;
let done;
const finished = new Promise((resolve) => { done = resolve; });
window.__rsPreviewDone = done;
window.__rsPreview = a;
setState("loading");
setProgress(0);
let started = false;
let seeking = false;
let outFade = null;
let length = clip;
const begin = () => {
  if (started || seeking) return;
  started = true;
  if (Number.isFinite(a.duration) && a.duration > start) length = Math.min(clip, a.duration - start);
  setState("playing");
  fade(a, 1, 400);
};
a.addEventListener("loadedmetadata", () => {
  // Fallback for engines that ignore the #t fragment.
  if (start > 0 && Math.abs(a.currentTime - start) > 1) {
    seeking = true;
    try { a.currentTime = start; } catch (_) { seeking = false; }
  }
});
a.addEventListener("seeked", () => { if (seeking) { seeking = false; if (!a.paused) begin(); } });
a.addEventListener("playing", () => { if (started) setState("playing"); else begin(); });
a.addEventListener("waiting", () => { if (started && !outFade) setState("loading"); });
a.addEventListener("timeupdate", () => {
  if (!started || outFade) return;
  const elapsed = a.currentTime - start;
  setProgress(elapsed / length);
  if (elapsed >= length - 0.6) {
    outFade = fade(a, 0, 600);
    done("ended");
  }
});
a.onended = () => done("ended");
a.onerror = () => done("error");
const timer = setTimeout(() => done("ended"), (clip + 20) * 1000);
a.src = start > 0 ? `${url}#t=${start.toFixed(2)}` : url;
const p = a.play();
if (p && p.catch) p.catch(() => done("blocked"));
const result = await finished;
clearTimeout(timer);
if (window.__rsPreviewDone === done) {
  window.__rsPreviewDone = null;
  window.__rsPreview = null;
  setState(null);
  setProgress(0);
}
(async () => {
  if (result !== "error" && result !== "blocked" && !a.paused) await (outFade || fade(a, 0, 600));
  a.pause();
  a.removeAttribute("src");
  try { a.load(); } catch (_) {}
})();
return result;
"#;

const STOP_JS: &str = r#"
const done = window.__rsPreviewDone;
if (done) done("stopped");
else if (window.__rsPreview) window.__rsPreview.pause();
return true;
"#;

/// Why a preview finished, as reported by the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEnd {
    Ended,
    Failed,
    Replaced,
}

fn parse_end(raw: &str) -> PreviewEnd {
    match raw {
        "error" | "blocked" => PreviewEnd::Failed,
        "replaced" => PreviewEnd::Replaced,
        _ => PreviewEnd::Ended,
    }
}

fn play_script(url: &str, start: f64, clip: f64) -> String {
    let url_js = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".to_string());
    PLAY_JS
        .replace("__URL__", &url_js)
        .replace("__START__", &format!("{:.3}", start.max(0.0)))
        .replace("__CLIP__", &format!("{:.3}", clip.max(1.0)))
}

/// Preview id for a library song's Quick Listen.
pub fn quick_listen_id(song: &Song) -> String {
    format!("song:{}:{}", song.server_id, song.id)
}

/// The shell's preview player (context).
#[derive(Clone, Copy)]
pub struct Preview {
    /// Id of the item previewing now (buttons read this for their state).
    pub current: Signal<Option<String>>,
    /// The library song a Quick Listen is playing (the shell's capsule shows
    /// it); None for Discover's off-server previews.
    pub song: Signal<Option<Song>>,
    token: Signal<u64>,
    resume_main: Signal<bool>,
    /// The main-playback fade in flight (down when starting, up when ending).
    duck: Signal<Option<Task>>,
    is_playing: Signal<bool>,
    volume: Signal<f64>,
    /// The shell's scope: preview tasks live there, not in the button or
    /// menu that started them.
    scope: ScopeId,
}

/// Create the preview player, provide it as context and wire the "main
/// playback started" stop. Call once, in the shell.
pub fn use_preview_provider() -> Preview {
    let is_playing = use_context::<IsPlayingSignal>().0;
    let volume = use_context::<VolumeSignal>().0;
    let preview = use_context_provider(|| Preview {
        current: Signal::new(None),
        song: Signal::new(None),
        token: Signal::new(0),
        resume_main: Signal::new(false),
        duck: Signal::new(None),
        is_playing,
        volume,
        scope: current_scope_id(),
    });
    use_effect(move || {
        // The user pressed play on the main player mid-preview: yield to it.
        if is_playing() && preview.current.peek().is_some() {
            preview.stop(false);
        }
    });
    preview
}

impl Preview {
    /// Start previewing `url` from `start` seconds for item `id`, or stop when
    /// it is already playing. `on_end` runs when the clip finishes on its own
    /// or fails.
    pub fn toggle(
        &self,
        id: String,
        url: String,
        start: f64,
        on_end: impl FnOnce(PreviewEnd) + 'static,
    ) {
        if self.current.peek().as_deref() == Some(id.as_str()) {
            self.stop(true);
            return;
        }
        self.play(id, url, start, None, on_end);
    }

    /// Quick Listen for a library song: 30 seconds of the server stream from
    /// around the chorus, without touching the queue. Toggles like `toggle`.
    pub fn quick_listen(&self, song: Song, servers: &[ServerConfig]) {
        let id = quick_listen_id(&song);
        if self.current.peek().as_deref() == Some(id.as_str()) {
            self.stop(true);
            return;
        }
        let Some(server) = servers.iter().find(|s| s.id == song.server_id).cloned() else {
            return;
        };
        let url = NavidromeClient::new(server).get_stream_url(&song.id);
        let start = quick_listen_start(f64::from(song.duration));
        self.play(id, url, start, Some(song), |_| {});
    }

    fn play(
        &self,
        id: String,
        url: String,
        start: f64,
        song: Option<Song>,
        on_end: impl FnOnce(PreviewEnd) + 'static,
    ) {
        let mut this = *self;
        // The script runs first, inside the click.
        let eval = document::eval(&play_script(&url, start, PREVIEW_SECONDS));
        if this.current.peek().is_none() {
            let was_playing = *this.is_playing.peek();
            this.resume_main.set(was_playing);
            if was_playing {
                this.fade_main_out();
            }
        }
        let mine = *this.token.peek() + 1;
        this.token.set(mine);
        this.current.set(Some(id));
        this.song.set(song);

        Runtime::current().spawn(this.scope, async move {
            let end = eval
                .join::<String>()
                .await
                .map(|raw| parse_end(&raw))
                .unwrap_or(PreviewEnd::Failed);
            if *this.token.peek() != mine {
                return;
            }
            this.finish(true);
            if end != PreviewEnd::Replaced {
                on_end(end);
            }
        });
    }

    /// Stop any preview. `resume` restarts main playback if it was playing.
    pub fn stop(&self, resume: bool) {
        let mut this = *self;
        if this.current.peek().is_none() {
            return;
        }
        let next = *this.token.peek() + 1;
        this.token.set(next);
        let _ = document::eval(STOP_JS);
        this.finish(resume);
    }

    /// Stops a Discover preview (a Quick Listen keeps playing). For views
    /// that own their previews' buttons and are going away.
    pub fn stop_off_server(&self) {
        if self.song.peek().is_none() {
            self.stop(true);
        }
    }

    /// Clears the preview state and hands playback back to the main player.
    fn finish(&self, resume: bool) {
        let mut this = *self;
        this.current.set(None);
        if this.song.peek().is_some() {
            this.song.set(None);
        }
        if let Some(task) = this.duck.write().take() {
            task.cancel();
        }
        let level = *this.volume.peek();
        if !*this.resume_main.peek() {
            return;
        }
        this.resume_main.set(false);
        if !resume {
            set_output_volume(level);
            return;
        }
        // Back up from silence while the preview fades out.
        set_output_volume(0.0);
        this.is_playing.set(true);
        let task = Runtime::current().spawn(this.scope, async move {
            ramp_output_volume(0.0, level, RESUME_MS).await;
            this.duck.set(None);
        });
        this.duck.set(Some(task));
    }

    /// Fades main playback down while the preview fades in, then pauses it
    /// and puts the volume setting back (paused, so nothing is heard).
    fn fade_main_out(&self) {
        let mut this = *self;
        let task = Runtime::current().spawn(this.scope, async move {
            let level = *this.volume.peek();
            ramp_output_volume(level, 0.0, DUCK_MS).await;
            this.is_playing.set(false);
            sleep_ms(150).await;
            set_output_volume(*this.volume.peek());
            this.duck.set(None);
        });
        this.duck.set(Some(task));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_results_map_to_ends() {
        assert_eq!(parse_end("ended"), PreviewEnd::Ended);
        assert_eq!(parse_end("stopped"), PreviewEnd::Ended);
        assert_eq!(parse_end("blocked"), PreviewEnd::Failed);
        assert_eq!(parse_end("error"), PreviewEnd::Failed);
        assert_eq!(parse_end("replaced"), PreviewEnd::Replaced);
    }

    #[test]
    fn url_and_timing_are_embedded_in_the_script() {
        let js = play_script("https://a.example/x.m4a?q=\"1\"", 63.25, 30.0);
        assert!(js.contains(r#"const url = "https://a.example/x.m4a?q=\"1\"";"#));
        assert!(js.contains("const start = 63.250;"));
        assert!(js.contains("const clip = 30.000;"));
        assert!(!js.contains("__URL__") && !js.contains("__START__") && !js.contains("__CLIP__"));
    }

    #[test]
    fn quick_listen_starts_near_the_chorus_and_leaves_a_full_clip() {
        assert_eq!(quick_listen_start(200.0), 70.0);
        // Long songs: 35% in.
        assert!((quick_listen_start(600.0) - 210.0).abs() < 1e-9);
        // Short songs: as late as still leaves 30 seconds.
        assert_eq!(quick_listen_start(40.0), 10.0);
        assert_eq!(quick_listen_start(45.0), 15.0);
        // No longer than a clip, or unknown: from the top.
        assert_eq!(quick_listen_start(30.0), 0.0);
        assert_eq!(quick_listen_start(12.0), 0.0);
        assert_eq!(quick_listen_start(0.0), 0.0);
        assert_eq!(quick_listen_start(f64::NAN), 0.0);
    }

    #[test]
    fn quick_listen_ids_name_the_server_and_song() {
        let song = Song {
            id: "s1".to_string(),
            server_id: "srv".to_string(),
            ..Song::default()
        };
        assert_eq!(quick_listen_id(&song), "song:srv:s1");
    }
}
