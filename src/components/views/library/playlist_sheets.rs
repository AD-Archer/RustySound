//! Create, rename and delete sheets for playlists (list and detail pages).

use crate::api::{NavidromeClient, Playlist, ServerConfig};
use crate::components::ui::{Button, ButtonVariant, Sheet};
use dioxus::prelude::*;

fn server_for(servers: &[ServerConfig], server_id: &str) -> Option<ServerConfig> {
    servers.iter().find(|s| s.id == server_id).cloned()
}

/// Name field plus the primary action, shared by Create and Rename.
#[component]
fn NameForm(
    value: Signal<String>,
    busy: bool,
    error: Option<String>,
    action: String,
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut value = value;
    rsx! {
        form {
            class: "rs-library-form",
            onsubmit: move |evt| {
                evt.prevent_default();
                on_submit.call(());
            },
            label { class: "rs-library-form__label", r#for: "rs-playlist-name", "Name" }
            div { class: "rs-field",
                input {
                    id: "rs-playlist-name",
                    r#type: "text",
                    value: "{value}",
                    placeholder: "Playlist name",
                    autocomplete: "off",
                    disabled: busy,
                    "data-testid": "playlist-name-input",
                    onmounted: move |evt| async move {
                        let _ = evt.set_focus(true).await;
                    },
                    oninput: move |evt| value.set(evt.value()),
                }
            }
            if let Some(err) = error {
                p { class: "rs-library-form__error", role: "alert", "{err}" }
            }
            div { class: "rs-library-form__actions",
                Button {
                    label: "Cancel".to_string(),
                    variant: ButtonVariant::Tinted,
                    onclick: move |_| on_cancel.call(()),
                }
                Button {
                    label: if busy { format!("{action}…") } else { action.clone() },
                    variant: ButtonVariant::Filled,
                    disabled: busy || value.read().trim().is_empty(),
                    testid: "playlist-name-submit".to_string(),
                    onclick: move |_| on_submit.call(()),
                }
            }
        }
    }
}

/// New empty playlist on the single active server.
#[component]
pub fn CreatePlaylistSheet(on_close: EventHandler<()>, on_created: EventHandler<()>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let name = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let submit = move |_| {
        if busy() {
            return;
        }
        let value = name.peek().trim().to_string();
        if value.is_empty() {
            error.set(Some("Give the playlist a name.".to_string()));
            return;
        }
        let active: Vec<ServerConfig> = servers.peek().iter().filter(|s| s.active).cloned().collect();
        if active.len() != 1 {
            error.set(Some("Turn on exactly one server in Settings to create playlists.".to_string()));
            return;
        }
        let server = active[0].clone();
        busy.set(true);
        error.set(None);
        spawn(async move {
            let client = NavidromeClient::new(server);
            match client.create_playlist(&value, None, &[]).await {
                Ok(_) => {
                    client.refresh_playlist_cache();
                    on_created.call(());
                }
                Err(err) => {
                    error.set(Some(err));
                    busy.set(false);
                }
            }
        });
    };

    rsx! {
        Sheet {
            title: "New Playlist".to_string(),
            on_close: move |_| on_close.call(()),
            testid: "playlist-create-sheet".to_string(),
            NameForm {
                value: name,
                busy: busy(),
                error: error(),
                action: "Create".to_string(),
                on_submit: submit,
                on_cancel: move |_| on_close.call(()),
            }
        }
    }
}

/// Rename a playlist. `on_renamed` gets the new name.
#[component]
pub fn RenamePlaylistSheet(
    playlist: Playlist,
    on_close: EventHandler<()>,
    on_renamed: EventHandler<String>,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let name = use_signal(|| playlist.name.clone());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let submit = {
        let playlist = playlist.clone();
        move |_| {
            if busy() {
                return;
            }
            let value = name.peek().trim().to_string();
            if value.is_empty() {
                error.set(Some("Playlist name can't be empty.".to_string()));
                return;
            }
            if value == playlist.name.trim() {
                on_close.call(());
                return;
            }
            let Some(server) = server_for(&servers.peek(), &playlist.server_id) else {
                error.set(Some("This playlist's server isn't configured any more.".to_string()));
                return;
            };
            let playlist_id = playlist.id.clone();
            busy.set(true);
            error.set(None);
            spawn(async move {
                let client = NavidromeClient::new(server);
                match client.rename_playlist(&playlist_id, &value).await {
                    Ok(_) => {
                        client.refresh_playlist_cache();
                        on_renamed.call(value);
                    }
                    Err(err) => {
                        error.set(Some(err));
                        busy.set(false);
                    }
                }
            });
        }
    };

    rsx! {
        Sheet {
            title: "Rename Playlist".to_string(),
            on_close: move |_| on_close.call(()),
            testid: "playlist-rename-sheet".to_string(),
            NameForm {
                value: name,
                busy: busy(),
                error: error(),
                action: "Rename".to_string(),
                on_submit: submit,
                on_cancel: move |_| on_close.call(()),
            }
        }
    }
}

/// Confirm deleting a playlist (replaces `window.confirm`).
#[component]
pub fn DeletePlaylistSheet(
    playlist: Playlist,
    on_close: EventHandler<()>,
    on_deleted: EventHandler<()>,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let confirm = {
        let playlist = playlist.clone();
        move |_| {
            if busy() {
                return;
            }
            let Some(server) = server_for(&servers.peek(), &playlist.server_id) else {
                error.set(Some("This playlist's server isn't configured any more.".to_string()));
                return;
            };
            let playlist_id = playlist.id.clone();
            busy.set(true);
            error.set(None);
            spawn(async move {
                let client = NavidromeClient::new(server);
                match client.delete_playlist(&playlist_id).await {
                    Ok(_) => {
                        client.refresh_playlist_cache();
                        on_deleted.call(());
                    }
                    Err(err) => {
                        error.set(Some(err));
                        busy.set(false);
                    }
                }
            });
        }
    };

    rsx! {
        Sheet {
            title: "Delete Playlist?".to_string(),
            on_close: move |_| on_close.call(()),
            testid: "playlist-delete-sheet".to_string(),
            p { class: "rs-library-form__text",
                "“{playlist.name}” will be removed from the server. Its songs stay in your library."
            }
            if let Some(err) = error() {
                p { class: "rs-library-form__error", role: "alert", "{err}" }
            }
            div { class: "rs-library-form__actions",
                Button {
                    label: "Cancel".to_string(),
                    variant: ButtonVariant::Tinted,
                    onclick: move |_| on_close.call(()),
                }
                button {
                    r#type: "button",
                    class: "rs-btn rs-btn--filled rs-btn--danger",
                    disabled: busy(),
                    "data-testid": "playlist-delete-confirm",
                    onclick: confirm,
                    if busy() { "Deleting…" } else { "Delete" }
                }
            }
        }
    }
}
