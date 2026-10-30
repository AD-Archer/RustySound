//! Settings → Servers: the server list, enable/disable, edit, delete, add.

use super::server_form::{format_connection_error, ServerSheet};
use super::shared::{use_settings_ctx, ActionRow, Footnote};
use crate::api::{NavidromeClient, ServerConfig};
use crate::components::ui::{
    EmptyState, IconButton, IconButtonSize, Button, ButtonVariant, Sheet, Switch,
};
use crate::db::save_servers_now;
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, PartialEq)]
enum Reach {
    Checking,
    Ok,
    Failed(String),
}

#[derive(Clone, PartialEq)]
enum Overlay {
    None,
    Add,
    Edit(ServerConfig),
    ConfirmDelete(ServerConfig),
}

/// Enables `server_id` and, when it turns on, switches every other server off
/// (playlist editing needs exactly one active server).
fn toggle_server(list: &mut [ServerConfig], server_id: &str, on: bool) {
    if on {
        for server in list.iter_mut() {
            server.active = false;
        }
    }
    if let Some(server) = list.iter_mut().find(|s| s.id == server_id) {
        server.active = on;
    }
}

#[component]
pub(super) fn ServersSection() -> Element {
    let ctx = use_settings_ctx();
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut overlay = use_signal(|| Overlay::None);
    let mut reach = use_signal(HashMap::<String, Reach>::new);
    let mut check_generation = use_signal(|| 0u64);

    // Ping every active server when the section opens (and after edits), so
    // the status dot means "reachable", not just "switched on".
    use_effect(move || {
        let _ = check_generation();
        let targets: Vec<ServerConfig> = servers.peek().iter().filter(|s| s.active).cloned().collect();
        for server in targets {
            reach.with_mut(|map| {
                map.insert(server.id.clone(), Reach::Checking);
            });
            spawn(async move {
                let id = server.id.clone();
                let result = NavidromeClient::new(server).ping().await;
                reach.with_mut(|map| {
                    map.insert(
                        id,
                        match result {
                            Ok(_) => Reach::Ok,
                            Err(error) => Reach::Failed(format_connection_error(&error)),
                        },
                    );
                });
            });
        }
    });

    let list = servers();
    let active_count = list.iter().filter(|s| s.active).count();

    rsx! {
        if list.is_empty() {
            EmptyState {
                icon: "server".to_string(),
                title: "No server yet".to_string(),
                message: "Add your Navidrome (or any Subsonic) server to start listening.".to_string(),
                testid: "servers-empty".to_string(),
                Button {
                    label: "Add Server".to_string(),
                    icon: "plus".to_string(),
                    testid: "server-add".to_string(),
                    onclick: move |_| overlay.set(Overlay::Add),
                }
            }
        } else {
            ul { class: "rs-list rs-settings-servers", aria_label: "Servers",
                for server in list.iter().cloned() {
                    ServerRow {
                        key: "{server.id}",
                        reach: reach.read().get(&server.id).cloned(),
                        server: server.clone(),
                        on_toggle: {
                            let id = server.id.clone();
                            move |on: bool| {
                                servers.with_mut(|list| toggle_server(list, &id, on));
                                let _ = save_servers_now(&servers.peek());
                                check_generation.with_mut(|g| *g += 1);
                                ctx.notify(if on { "Server on" } else { "Server off" });
                            }
                        },
                        on_edit: {
                            let server = server.clone();
                            move |_| overlay.set(Overlay::Edit(server.clone()))
                        },
                        on_delete: {
                            let server = server.clone();
                            move |_| overlay.set(Overlay::ConfirmDelete(server.clone()))
                        },
                    }
                }
            }
            Footnote {
                "Playlists are edited on one server at a time, so turning a server on turns the others off."
            }
            if active_count > 1 {
                Footnote { "{active_count} servers are on. Search and Discover use all of them; creating and editing playlists waits until only one is on." }
            }
            ul { class: "rs-list rs-list--text",
                ActionRow {
                    label: "Add Server".to_string(),
                    icon: "plus".to_string(),
                    testid: "server-add".to_string(),
                    onclick: move |_| overlay.set(Overlay::Add),
                }
            }
        }

        match overlay() {
            Overlay::None => rsx! {},
            Overlay::Add => rsx! {
                ServerSheet {
                    on_close: move |_| overlay.set(Overlay::None),
                    on_saved: move |_| {
                        overlay.set(Overlay::None);
                        check_generation.with_mut(|g| *g += 1);
                        ctx.notify("Server added");
                    },
                }
            },
            Overlay::Edit(server) => rsx! {
                ServerSheet {
                    editing: Some(server),
                    on_close: move |_| overlay.set(Overlay::None),
                    on_saved: move |_| {
                        overlay.set(Overlay::None);
                        check_generation.with_mut(|g| *g += 1);
                        ctx.notify("Server updated");
                    },
                }
            },
            Overlay::ConfirmDelete(server) => rsx! {
                Sheet {
                    title: "Delete Server?".to_string(),
                    testid: "server-delete-confirm".to_string(),
                    on_close: move |_| overlay.set(Overlay::None),
                    p { class: "rs-settings-confirm",
                        "“{server.name}” will be removed from RustySound. Nothing on the server itself is deleted, and you can add it again later."
                    }
                    div { class: "rs-settings-form__actions",
                        Button {
                            label: "Cancel".to_string(),
                            variant: ButtonVariant::Tinted,
                            onclick: move |_| overlay.set(Overlay::None),
                        }
                        button {
                            r#type: "button",
                            class: "rs-btn rs-btn--filled rs-btn--danger",
                            "data-testid": "server-delete-yes",
                            onclick: {
                                let id = server.id.clone();
                                move |_| {
                                    servers.with_mut(|list| list.retain(|s| s.id != id));
                                    let _ = save_servers_now(&servers.peek());
                                    overlay.set(Overlay::None);
                                    ctx.notify("Server deleted");
                                }
                            },
                            "Delete Server"
                        }
                    }
                }
            },
        }
    }
}

#[component]
fn ServerRow(
    server: ServerConfig,
    reach: Option<Reach>,
    on_toggle: EventHandler<bool>,
    on_edit: EventHandler<MouseEvent>,
    on_delete: EventHandler<MouseEvent>,
) -> Element {
    let (led, status) = if !server.active {
        ("rs-led rs-led--off", "Off".to_string())
    } else {
        match reach {
            Some(Reach::Ok) => ("rs-led", "Connected".to_string()),
            Some(Reach::Failed(message)) => ("rs-led rs-led--warn", message),
            _ => ("rs-led rs-led--off", "Checking…".to_string()),
        }
    };
    let title_id = format!("server-{}-name", server.id);
    rsx! {
        li { "data-testid": "server-row",
            div { class: "rs-row rs-settings-server",
                span { class: "rs-settings-server__dot",
                    span { class: "{led}", role: "img", aria_label: "{status}" }
                }
                span { class: "rs-row__meta",
                    span { class: "rs-row__title",
                        span { id: "{title_id}", "{server.name}" }
                    }
                    span { class: "rs-row__sub", style: "display:block", "{server.url} · {server.username}" }
                    span { class: "rs-row__why rs-settings-server__status", style: "display:block", "{status}" }
                }
                IconButton {
                    icon: "edit".to_string(),
                    label: format!("Edit {}", server.name),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "server-edit".to_string(),
                    onclick: move |evt| on_edit.call(evt),
                }
                IconButton {
                    icon: "trash".to_string(),
                    label: format!("Delete {}", server.name),
                    quiet: true,
                    size: IconButtonSize::Small,
                    testid: "server-delete".to_string(),
                    onclick: move |evt| on_delete.call(evt),
                }
                Switch {
                    checked: server.active,
                    label: format!("{} on", server.name),
                    testid: "server-toggle".to_string(),
                    on_toggle: move |on| on_toggle.call(on),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(id: &str, active: bool) -> ServerConfig {
        let mut s = ServerConfig::new(id.into(), "http://x".into(), "u".into(), "p".into());
        s.id = id.into();
        s.active = active;
        s
    }

    #[test]
    fn turning_one_on_turns_the_others_off() {
        let mut list = vec![server("a", true), server("b", true), server("c", false)];
        toggle_server(&mut list, "c", true);
        assert_eq!(list.iter().map(|s| s.active).collect::<Vec<_>>(), [false, false, true]);
        toggle_server(&mut list, "c", false);
        assert!(list.iter().all(|s| !s.active));
    }
}
