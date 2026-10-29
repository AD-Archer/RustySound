//! 30-second previews for off-server picks, played on a separate `Audio`
//! element so the main queue is never touched. Main playback is paused while a
//! preview plays and resumed when it ends or is stopped. Pressing the main
//! play button during a preview stops the preview.

use crate::components::IsPlayingSignal;
use dioxus::core::{current_scope_id, Runtime};
use dioxus::prelude::*;

/// Starts the preview inside the click's call stack (the first statements run
/// synchronously on web, so `play()` keeps the user gesture), then waits for
/// it to end, fail, be replaced or be stopped. Returns why it finished.
const PLAY_JS: &str = r#"
const url = __URL__;
let a = window.__rsDiscoverPreview;
if (!a) { a = new Audio(); a.preload = "auto"; window.__rsDiscoverPreview = a; }
const previous = window.__rsDiscoverPreviewDone;
if (previous) previous("replaced");
let done;
const finished = new Promise((resolve) => { done = resolve; });
window.__rsDiscoverPreviewDone = done;
a.onended = () => done("ended");
a.onerror = () => done("error");
const timer = setTimeout(() => done("ended"), 31000);
a.src = url;
a.currentTime = 0;
a.play().catch(() => done("blocked"));
const result = await finished;
clearTimeout(timer);
if (window.__rsDiscoverPreviewDone === done) {
  window.__rsDiscoverPreviewDone = null;
  a.pause();
}
return result;
"#;

const STOP_JS: &str = r#"
const done = window.__rsDiscoverPreviewDone;
if (done) done("stopped");
if (window.__rsDiscoverPreview) window.__rsDiscoverPreview.pause();
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

/// Preview state, provided by the Discover view.
#[derive(Clone, Copy)]
pub struct Preview {
    /// Id of the item previewing now (rows read this for their button state).
    pub current: Signal<Option<String>>,
    token: Signal<u64>,
    resume_main: Signal<bool>,
    is_playing: Signal<bool>,
    /// The Discover view's scope: the wait-for-end task lives there, not in
    /// the row that started it.
    scope: ScopeId,
}

/// Create the preview controller, provide it as context and wire the
/// "main playback started" and unmount stops.
pub fn use_preview_provider() -> Preview {
    let is_playing = use_context::<IsPlayingSignal>().0;
    let preview = use_context_provider(|| Preview {
        current: Signal::new(None),
        token: Signal::new(0),
        resume_main: Signal::new(false),
        is_playing,
        scope: current_scope_id(),
    });
    use_effect(move || {
        // The user pressed play on the main player mid-preview: yield to it.
        if is_playing() && preview.current.peek().is_some() {
            preview.stop(false);
        }
    });
    use_drop(move || preview.stop(true));
    preview
}

impl Preview {
    /// Start previewing `url` for item `id`, or stop when it is already playing.
    /// `on_end` runs when the clip finishes on its own or fails.
    pub fn toggle(&self, id: String, url: String, on_end: impl FnOnce(PreviewEnd) + 'static) {
        if self.current.peek().as_deref() == Some(id.as_str()) {
            self.stop(true);
            return;
        }
        let (mut current, mut token, mut resume_main, mut is_playing) =
            (self.current, self.token, self.resume_main, self.is_playing);
        if current.peek().is_none() {
            let was_playing = *is_playing.peek();
            resume_main.set(was_playing);
            if was_playing {
                is_playing.set(false);
            }
        }
        let mine = *token.peek() + 1;
        token.set(mine);
        current.set(Some(id));

        let url_js = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".to_string());
        let eval = document::eval(&PLAY_JS.replace("__URL__", &url_js));
        Runtime::current().spawn(self.scope, async move {
            let end = eval
                .join::<String>()
                .await
                .map(|raw| parse_end(&raw))
                .unwrap_or(PreviewEnd::Failed);
            if *token.peek() != mine {
                return;
            }
            current.set(None);
            if *resume_main.peek() {
                resume_main.set(false);
                is_playing.set(true);
            }
            if end != PreviewEnd::Replaced {
                on_end(end);
            }
        });
    }

    /// Stop any preview. `resume` restarts main playback if it was playing.
    pub fn stop(&self, resume: bool) {
        let (mut current, mut token, mut resume_main, mut is_playing) =
            (self.current, self.token, self.resume_main, self.is_playing);
        if current.peek().is_none() {
            return;
        }
        let next = *token.peek() + 1;
        token.set(next);
        current.set(None);
        let _ = document::eval(STOP_JS);
        if *resume_main.peek() {
            resume_main.set(false);
            if resume {
                is_playing.set(true);
            }
        }
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
    fn url_is_embedded_as_a_js_string() {
        let url = serde_json::to_string("https://a.example/x.m4a?q=\"1\"").unwrap();
        let js = PLAY_JS.replace("__URL__", &url);
        assert!(js.contains(r#"const url = "https://a.example/x.m4a?q=\"1\"";"#));
    }
}
