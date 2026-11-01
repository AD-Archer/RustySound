//! Playlists kept up to date. A pass runs on launch, when the app comes back
//! to the foreground (the page's `visibilitychange`, which the Mac and iPhone
//! web views fire too), every 30 minutes while it's open, and when the quiet
//! "Updated 2 min ago" line is tapped.
//!
//! Each pass drops the cached playlists, so open playlist pages re-fetch
//! (they read `revision`), and on Mac and iPhone brings downloaded playlists
//! in step with the server (`offline_audio::sync_downloaded_playlists`:
//! new songs download, removed ones' downloads go). It runs in the
//! background and never touches playback. Offline Mode skips it.

use crate::api::{NavidromeClient, ServerConfig, Song};
use crate::components::ui::env::sleep_ms;
use crate::db::AppSettings;
use crate::offline_audio::sync_downloaded_playlists;
use dioxus::prelude::*;
use std::collections::HashSet;

/// Between timed passes.
const PERIOD_MS: u64 = 30 * 60 * 1000;
/// Automatic passes (foreground, timer) closer together than this are skipped.
const MIN_GAP_MS: f64 = 15_000.0;

/// Shared sync state (`use_playlist_sync_provider`).
#[derive(Clone, Copy)]
pub struct PlaylistSync {
    /// Bumps after each pass; playlist pages re-fetch on it.
    pub revision: Signal<u64>,
    /// When the last pass finished (ms since the epoch).
    pub last_synced: Signal<Option<f64>>,
    pub busy: Signal<bool>,
    /// Ticks every 30 s, so "2 min ago" moves on.
    tick: Signal<u64>,
    /// A pass request: (count, forced).
    request: Signal<(u64, bool)>,
}

impl PlaylistSync {
    /// Asks for a pass now (a tap: not throttled).
    pub fn sync_now(self) {
        let mut request = self.request;
        let count = request.peek().0 + 1;
        request.set((count, true));
    }

    fn sync_soon(self) {
        let mut request = self.request;
        let count = request.peek().0 + 1;
        request.set((count, false));
    }

    /// "Updating…", "Updated just now", "Updated 2 min ago".
    pub fn label(&self) -> String {
        let _ = (self.tick)();
        let age = (self.last_synced)().map(|at| now_ms() - at);
        updated_label(age, (self.busy)())
    }
}

pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

/// The status line's text for a pass `age_ms` ago.
pub fn updated_label(age_ms: Option<f64>, busy: bool) -> String {
    if busy {
        return "Updating…".to_string();
    }
    let Some(age) = age_ms else {
        return "Not updated yet".to_string();
    };
    let minutes = (age.max(0.0) / 60_000.0).floor() as u64;
    match minutes {
        0 => "Updated just now".to_string(),
        1..=59 => format!("Updated {minutes} min ago"),
        60..=1439 => format!("Updated {} hr ago", minutes / 60),
        _ => {
            let days = minutes / 1440;
            format!("Updated {days} day{} ago", if days == 1 { "" } else { "s" })
        }
    }
}

const VISIBILITY_JS: &str = r#"
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") dioxus.send(true);
});
await new Promise(() => {});
"#;

/// Provides `PlaylistSync` and runs the passes (in the shell's scope, so a
/// pass outlives the page that asked for it).
pub fn use_playlist_sync_provider(
    servers: Signal<Vec<ServerConfig>>,
    settings: Signal<AppSettings>,
    queue: Signal<Vec<Song>>,
    settings_loaded: Signal<bool>,
    servers_loaded: Signal<bool>,
) -> PlaylistSync {
    let sync = use_context_provider(|| PlaylistSync {
        revision: Signal::new(0),
        last_synced: Signal::new(None),
        busy: Signal::new(false),
        tick: Signal::new(0),
        request: Signal::new((0, false)),
    });
    let mut started_at = use_signal(|| None::<f64>);

    // Launch: once settings and servers are in.
    let mut launched = use_signal(|| false);
    use_effect(move || {
        if settings_loaded() && servers_loaded() && !*launched.peek() {
            launched.set(true);
            sync.sync_soon();
        }
    });

    // Timer, the "ago" tick and foreground.
    use_future(move || async move {
        let mut waited = 0u64;
        loop {
            sleep_ms(30_000).await;
            let mut tick = sync.tick;
            tick += 1;
            waited += 30_000;
            if waited >= PERIOD_MS {
                waited = 0;
                sync.sync_soon();
            }
        }
    });
    use_future(move || async move {
        let mut eval = document::eval(VISIBILITY_JS);
        while eval.recv::<bool>().await.is_ok() {
            sync.sync_soon();
        }
    });

    use_effect(move || {
        let (count, forced) = (sync.request)();
        if count == 0 || *sync.busy.peek() {
            return;
        }
        let snapshot = settings.peek().clone();
        if snapshot.offline_mode {
            return;
        }
        let now = now_ms();
        if !forced && started_at.peek().is_some_and(|at| now - at < MIN_GAP_MS) {
            return;
        }
        started_at.set(Some(now));
        let mut busy = sync.busy;
        busy.set(true);
        let servers_now: Vec<ServerConfig> = servers.peek().iter().filter(|s| s.active).cloned().collect();
        let keep: HashSet<String> = queue.peek().iter().map(|s| format!("{}::{}", s.server_id, s.id)).collect();
        spawn(async move {
            for server in &servers_now {
                NavidromeClient::new(server.clone()).refresh_playlist_cache();
            }
            let _ = sync_downloaded_playlists(&servers_now, &snapshot, &keep).await;
            let (mut revision, mut last) = (sync.revision, sync.last_synced);
            revision += 1;
            last.set(Some(now_ms()));
            busy.set(false);
        });
    });

    sync
}

/// The quiet "Updated 2 min ago" line; a tap checks for changes now.
#[component]
pub fn SyncStatus(#[props(default)] testid: Option<String>) -> Element {
    let sync = use_context::<PlaylistSync>();
    let label = sync.label();
    let busy = (sync.busy)();
    rsx! {
        button {
            r#type: "button",
            class: "rs-sync-status",
            class: if busy { "rs-sync-status--busy" },
            "data-testid": testid.unwrap_or_else(|| "sync-status".to_string()),
            title: "Check for changes",
            aria_label: if busy { label.clone() } else { format!("{label}. Check for changes now") },
            disabled: busy,
            onclick: move |_| sync.sync_now(),
            span { class: "rs-sync-status__dot", aria_hidden: "true" }
            "{label}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::updated_label;

    #[test]
    fn labels_read_naturally() {
        assert_eq!(updated_label(None, true), "Updating…");
        assert_eq!(updated_label(None, false), "Not updated yet");
        assert_eq!(updated_label(Some(5_000.0), false), "Updated just now");
        assert_eq!(updated_label(Some(2.5 * 60_000.0), false), "Updated 2 min ago");
        assert_eq!(updated_label(Some(3.0 * 3_600_000.0), false), "Updated 3 hr ago");
        assert_eq!(updated_label(Some(26.0 * 3_600_000.0), false), "Updated 1 day ago");
        assert_eq!(updated_label(Some(-10.0), false), "Updated just now");
    }
}
