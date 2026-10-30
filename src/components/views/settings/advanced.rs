//! Settings → Advanced: Quick Scan per server (tap a row), and the iOS audio
//! log tools (iOS builds only).

use super::shared::{ActionRow, Footnote, GroupTitle};
use crate::api::{NavidromeClient, ScanStatus, ServerConfig};
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone)]
enum ScanState {
    Busy,
    Status(ScanStatus),
    Failed(String),
}

fn describe(status: &ScanStatus) -> String {
    let mut parts = vec![status.status.clone()];
    if let Some(task) = status.current_task.as_ref().filter(|t| !t.trim().is_empty()) {
        parts.push(task.clone());
    }
    if let Some(left) = status.seconds_remaining {
        parts.push(format!("{left}s left"));
    }
    if let Some(elapsed) = status.seconds_elapsed {
        parts.push(format!("{elapsed}s elapsed"));
    }
    parts.join(" · ")
}

/// A scan this session reported as still running.
fn scanning_now(state: Option<&ScanState>) -> bool {
    matches!(
        state,
        Some(ScanState::Status(s)) if s.scanning == Some(true) || s.status == "running" || s.status == "scanning"
    )
}

#[component]
pub(super) fn AdvancedSection() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut scans = use_signal(HashMap::<String, ScanState>::new);

    let mut run = move |server: ServerConfig, start: bool| {
        let id = server.id.clone();
        if matches!(scans.peek().get(&id), Some(ScanState::Busy)) {
            return;
        }
        scans.with_mut(|map| {
            map.insert(id.clone(), ScanState::Busy);
        });
        spawn(async move {
            let client = NavidromeClient::new(server);
            let result = if start {
                client.start_scan().await
            } else {
                client.get_scan_status().await
            };
            scans.with_mut(|map| {
                map.insert(
                    id,
                    match result {
                        Ok(status) => ScanState::Status(status),
                        Err(error) => ScanState::Failed(error),
                    },
                );
            });
        });
    };

    let list = servers();

    rsx! {
        GroupTitle { title: "Quick Scan".to_string() }
        if list.is_empty() {
            Footnote { "Add a server to scan its library." }
        } else {
            ul { class: "rs-list rs-list--text", aria_label: "Quick Scan",
                for server in list.iter().cloned() {
                    li { key: "{server.id}", "data-testid": "scan-row",
                        // The whole row starts a scan; tapping again while one
                        // runs just reports its progress.
                        button {
                            r#type: "button",
                            class: "rs-row rs-settings-row rs-settings-row--tap",
                            aria_label: "Scan {server.name}",
                            "data-testid": "scan-start",
                            disabled: matches!(scans.read().get(&server.id), Some(ScanState::Busy)),
                            onclick: {
                                let server = server.clone();
                                move |_| {
                                    let running = scanning_now(scans.peek().get(&server.id));
                                    run(server.clone(), !running)
                                }
                            },
                            span { class: "rs-row__meta",
                                span { class: "rs-settings-row__title", "{server.name}" }
                                span { class: "rs-settings-row__sub", role: "status",
                                    match scans.read().get(&server.id) {
                                        None => "No scan yet this session".to_string(),
                                        Some(ScanState::Busy) => "Asking the server…".to_string(),
                                        Some(ScanState::Status(status)) => describe(status),
                                        Some(ScanState::Failed(error)) => format!("Couldn't scan: {error}"),
                                    }
                                }
                            }
                            span { class: "rs-settings-row__action", aria_hidden: "true",
                                if scanning_now(scans.read().get(&server.id)) { "Refresh" } else { "Scan" }
                            }
                        }
                    }
                }
            }
            Footnote { "Tap a server to ask it to pick up new and changed files. Big libraries can take a few minutes; tap again to see progress." }
        }

        if cfg!(target_os = "ios") {
            IosAudioLogs {}
        }
    }
}

/// Lock-screen / Control Center playback diagnostics for a physical iPhone.
#[component]
fn IosAudioLogs() -> Element {
    use crate::components::{ios_audio_log_clear, ios_audio_log_export_txt, ios_audio_log_snapshot};

    let mut text = use_signal(String::new);
    let mut status = use_signal(|| None::<String>);
    let empty = "No audio logs yet. Reproduce the problem, then tap Refresh.";

    rsx! {
        GroupTitle { title: "iOS Audio Logs".to_string() }
        ul { class: "rs-list rs-list--text", aria_label: "iOS audio log actions",
            ActionRow {
                label: "Refresh".to_string(),
                icon: "refresh-cw".to_string(),
                onclick: move |_| {
                    let lines = ios_audio_log_snapshot(1000);
                    if lines.is_empty() {
                        text.set(String::new());
                        status.set(Some(empty.to_string()));
                    } else {
                        status.set(Some(format!("{} lines.", lines.len())));
                        text.set(lines.join("\n"));
                    }
                },
            }
            ActionRow {
                label: "Export .txt".to_string(),
                icon: "file-text".to_string(),
                onclick: move |_| {
                    let mut body = text();
                    if body.trim().is_empty() {
                        let lines = ios_audio_log_snapshot(1000);
                        if lines.is_empty() {
                            status.set(Some(empty.to_string()));
                            return;
                        }
                        body = lines.join("\n");
                        text.set(body.clone());
                    }
                    status.set(Some(match ios_audio_log_export_txt(&body) {
                        Ok(path) => format!("Opened the share sheet for {} lines ({path}).", body.lines().count()),
                        Err(error) => format!("Export failed: {error}"),
                    }));
                },
            }
            ActionRow {
                label: "Clear".to_string(),
                icon: "trash".to_string(),
                danger: true,
                onclick: move |_| {
                    ios_audio_log_clear();
                    text.set(String::new());
                    status.set(Some("Cleared the log buffer.".to_string()));
                },
            }
        }
        if let Some(message) = status() {
            p { class: "rs-list-foot rs-settings-foot", role: "status", "{message}" }
        }
        label { class: "rs-settings-sr", r#for: "ios-audio-log", "iOS audio log" }
        textarea {
            id: "ios-audio-log",
            class: "rs-settings-css__input rs-settings-log",
            readonly: true,
            rows: "12",
            value: "{text}",
        }
        Footnote { "Capture logs from a physical iPhone for lock-screen and Control Center playback problems: reproduce, Refresh, then Export." }
    }
}
