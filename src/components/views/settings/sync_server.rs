//! Settings → Servers → Sync Server: a rustysound-sync address per Subsonic
//! server (empty = off), checked with its `/health` when saved, with the
//! live connection status, and the name this device shows to the others.
//! Hidden in offline mode and when only Jellyfin servers are set up.

use super::shared::{use_settings_ctx, Footnote, GroupTitle};
use crate::api::ServerConfig;
use crate::components::remote_sync::logic::normalize_sync_url;
use crate::components::remote_sync::{display_name, media, room_server, RemoteSync, SyncStatus};
use crate::components::Icon;
use crate::db::save_servers_now;
use dioxus::prelude::*;

/// One server's `/health` check, after a save.
#[derive(Clone, PartialEq)]
enum Check {
    Idle,
    Running,
    Done(Result<String, String>),
}

#[component]
pub(super) fn SyncServerGroup() -> Element {
    let ctx = use_settings_ctx();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let sync = use_context::<RemoteSync>();
    let mut name = use_signal(|| sync.device_name.peek().clone());

    if ctx.settings.read().offline_mode {
        return rsx! {};
    }
    let subsonic: Vec<ServerConfig> = servers.read().iter().filter(|s| !s.is_jellyfin()).cloned().collect();
    if subsonic.is_empty() {
        return rsx! {};
    }
    let several = subsonic.len() > 1;
    let placeholder = display_name("");

    rsx! {
        GroupTitle { title: "Sync Server".to_string() }
        ul { class: "rs-list rs-list--text rs-settings-fields",
            for server in subsonic {
                SyncUrlField { key: "{server.id}", server, several }
            }
            li {
                div { class: "rs-settings-field",
                    label { r#for: "sync-device-name", "This device's name" }
                    input {
                        id: "sync-device-name",
                        autocomplete: "off",
                        spellcheck: "false",
                        maxlength: "64",
                        placeholder: "{placeholder}",
                        "data-testid": "sync-device-name",
                        value: "{name}",
                        oninput: move |evt| name.set(evt.value()),
                        onchange: move |evt| {
                            sync.set_device_name(&evt.value());
                            ctx.notify("Device name saved");
                        },
                    }
                }
            }
        }
        Footnote {
            "Play and pause your other devices, and move playback between them, from Output in Now Playing. Devices appear while RustySound is open or playing: a paused iPhone in the background can't be reached until you open RustySound on it."
        }
    }
}

/// The address field for one server, saved on Enter or leaving the field,
/// with the check result and, for the server whose room this device is in,
/// the connection status.
#[component]
fn SyncUrlField(server: ServerConfig, several: bool) -> Element {
    let ctx = use_settings_ctx();
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let sync = use_context::<RemoteSync>();
    let mut draft = use_signal(|| server.sync_url.clone());
    let mut check = use_signal(|| Check::Idle);
    let field_id = format!("sync-url-{}", server.id);
    let result_id = format!("{field_id}-result");
    let label = if several { format!("Sync server · {}", server.name) } else { "Sync server".to_string() };
    let in_room = room_server(&servers.read()).is_some_and(|s| s.id == server.id);
    let status = sync.status.read().clone();
    let devices = sync.devices.read().len();
    let has_url = !server.sync_url.trim().is_empty();

    let mut save = {
        let id = server.id.clone();
        move |raw: String| {
            let url = normalize_sync_url(&raw);
            draft.set(url.clone());
            let changed = servers.peek().iter().any(|s| s.id == id && s.sync_url != url);
            if changed {
                servers.with_mut(|list| {
                    if let Some(s) = list.iter_mut().find(|s| s.id == id) {
                        s.sync_url = url.clone();
                    }
                });
                let _ = save_servers_now(&servers.peek());
            }
            let Some(server) = servers.peek().iter().find(|s| s.id == id).cloned() else {
                return;
            };
            if url.is_empty() {
                check.set(Check::Idle);
                media::set_health(&server.id, "", None);
                if changed {
                    ctx.notify("Sync server off");
                }
                return;
            }
            // Saving an address that doesn't answer is allowed: the server
            // may be down for now.
            check.set(Check::Running);
            spawn(async move {
                let result = media::refresh(&server).await.map(|health| {
                    if health.version.is_empty() {
                        "Sync server found".to_string()
                    } else {
                        format!("Sync server {}", health.version)
                    }
                });
                check.set(Check::Done(result));
            });
        }
    };

    rsx! {
        li {
            div { class: "rs-settings-field rs-settings-field--test",
                label { r#for: "{field_id}", "{label}" }
                input {
                    id: "{field_id}",
                    r#type: "url",
                    inputmode: "url",
                    autocomplete: "off",
                    autocapitalize: "off",
                    spellcheck: "false",
                    placeholder: "https://music.example.com/rustysound-sync",
                    aria_describedby: "{result_id}",
                    "data-testid": "sync-url",
                    value: "{draft}",
                    oninput: move |evt| draft.set(evt.value()),
                    onchange: {
                        let mut save = save.clone();
                        move |evt: FormEvent| save(evt.value())
                    },
                }
                button {
                    r#type: "button",
                    class: "rs-btn rs-btn--plain rs-settings-field__test",
                    "data-testid": "sync-url-save",
                    onclick: move |_| save(draft()),
                    "Save"
                }
            }
            div { class: "rs-settings-test rs-settings-test--inline", id: "{result_id}", role: "status", aria_live: "polite",
                "data-testid": "sync-status",
                match check() {
                    Check::Running => rsx! {
                        span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                        span { "Checking…" }
                    },
                    Check::Done(Err(message)) => rsx! {
                        span { class: "rs-settings-test__err",
                            Icon { name: "x".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                            "{message}"
                        }
                    },
                    Check::Done(Ok(found)) if in_room => rsx! { StatusLine { prefix: Some(found), status, devices } },
                    Check::Done(Ok(found)) => rsx! {
                        span { class: "rs-settings-test__ok",
                            Icon { name: "check".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                            "{found}"
                        }
                    },
                    Check::Idle if in_room && has_url => rsx! { StatusLine { prefix: None, status, devices } },
                    Check::Idle => rsx! {},
                }
            }
        }
    }
}

/// "Sync server 0.1.0 · Connected · 3 devices", or the problem.
#[component]
fn StatusLine(prefix: Option<String>, status: SyncStatus, devices: usize) -> Element {
    let ok = matches!(status, SyncStatus::Connected | SyncStatus::Connecting);
    let line = match prefix {
        Some(prefix) => format!("{prefix} · {}", status.line(devices)),
        None => status.line(devices),
    };
    rsx! {
        span { class: if ok { "rs-settings-test__ok" } else { "rs-settings-test__err" },
            Icon { name: if ok { "check".to_string() } else { "alert".to_string() }, class: "rs-ic rs-ic--sm".to_string() }
            "{line}"
        }
    }
}
