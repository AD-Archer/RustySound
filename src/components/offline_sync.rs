//! Sending the changes made offline (`api::offline_queue`) once the servers
//! answer again, and the quiet line that says how many wait.
//!
//! * [`use_offline_sync_host`] (the app shell): loads the active profile's
//!   queue, and while changes wait and Offline Mode is off, replays them in
//!   the background: shortly after each new change, then every half minute
//!   while a server stays unreachable.
//! * [`PendingSyncLine`]: "3 changes waiting to sync" with Sync now, shown
//!   only while something waits (Settings › Servers).
//! * [`pending_sync_text`]: the same words, for the offline banner.

use crate::api::offline_queue::{pending_count, replay, STATUS};
use crate::api::ServerConfig;
use crate::components::ui::env::sleep_ms;
use crate::components::use_setting;
use dioxus::prelude::*;

/// A short pause before sending, so a burst of changes goes in one replay.
const FIRST_TRY_MS: u64 = 1_500;
/// How often an unreachable server is tried again.
const RETRY_MS: u64 = 30_000;

/// Runs the replay loop for the shell's servers once startup has loaded them.
pub fn use_offline_sync_host(servers: Signal<Vec<ServerConfig>>, ready: Signal<bool>) {
    let offline = use_setting(|s| s.offline_mode);
    let pending = use_memo(|| STATUS.read().pending);
    let mut generation = use_signal(|| 0u64);

    // The queue is per profile: load it (and publish its count) on start.
    use_effect(move || {
        if ready() {
            let _ = pending_count();
        }
    });

    use_effect(move || {
        let (count, offline, list) = (pending(), offline(), servers());
        generation.with_mut(|g| *g = g.wrapping_add(1));
        let current = *generation.peek();
        if !ready() || offline || count == 0 {
            return;
        }
        spawn(async move {
            let mut delay = FIRST_TRY_MS;
            loop {
                sleep_ms(delay).await;
                if *generation.peek() != current {
                    break;
                }
                let report = replay(&list).await;
                if *generation.peek() != current || report.waiting_servers == 0 {
                    break;
                }
                delay = RETRY_MS;
            }
        });
    });
}

/// "1 change waiting to sync", or `None` when nothing waits.
pub fn pending_sync_text(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 change waiting to sync".to_string()),
        n => Some(format!("{n} changes waiting to sync")),
    }
}

/// The waiting-changes line with Sync now. Renders nothing when the queue
/// is empty.
#[component]
pub fn PendingSyncLine() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let offline = use_setting(|s| s.offline_mode);
    let status = STATUS();
    let Some(text) = pending_sync_text(status.pending) else {
        return rsx! {};
    };
    rsx! {
        div { class: "rs-sync-line", role: "status", "data-testid": "sync-pending",
            span { class: "rs-led rs-led--warn", aria_hidden: "true" }
            span { class: "rs-sync-line__text",
                "{text}"
                if offline() {
                    " · They'll sync when you go online."
                }
            }
            if !offline() {
                button {
                    r#type: "button",
                    class: "rs-btn rs-btn--plain rs-btn--small rs-sync-line__btn",
                    "data-testid": "sync-now",
                    disabled: status.syncing,
                    onclick: move |_| {
                        let list = servers.peek().clone();
                        spawn(async move {
                            let _ = replay(&list).await;
                        });
                    },
                    if status.syncing { "Syncing…" } else { "Sync now" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_counts_changes() {
        assert_eq!(pending_sync_text(0), None);
        assert_eq!(pending_sync_text(1).as_deref(), Some("1 change waiting to sync"));
        assert_eq!(pending_sync_text(3).as_deref(), Some("3 changes waiting to sync"));
    }
}
