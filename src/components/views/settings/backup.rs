//! Settings → Backup & Restore: export this profile's settings and servers
//! to a file, and import one after a preview of what it changes
//! (`crate::backup`).
//!
//! Importing opens a file input: the browser's picker on the web, the Mac's
//! open panel (Dioxus routes file inputs there) and, on iPhone, the document
//! picker. Dioxus can't read files on iOS, so there the input is plain HTML
//! (no Dioxus id, so the webview opens its own picker) and a small script
//! hands the text back.

use super::shared::{use_settings_ctx, ActionRow, Footnote, SwitchRow};
use crate::backup::{self, ImportPlan, Saved};
use crate::api::ServerConfig;
use crate::components::ui::{Button, ButtonVariant, Sheet};
use crate::components::{Icon, ShuffleEnabledSignal, VolumeSignal};
use crate::db::{save_servers_now, RepeatMode};
use dioxus::prelude::*;

const FILE_INPUT_ID: &str = "backup-file";

#[component]
pub(super) fn BackupSection() -> Element {
    let ctx = use_settings_ctx();
    let mut servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut include_sign_in = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut preview = use_signal(|| None::<ImportPlan>);

    let export = move |_| {
        if saving() {
            return;
        }
        let now = chrono::Local::now();
        let json = backup::export(
            &ctx.settings.peek(),
            &servers.peek(),
            include_sign_in(),
            &chrono::Utc::now().to_rfc3339(),
        );
        let name = backup::file_name(&now.format("%Y-%m-%d").to_string());
        saving.set(true);
        spawn(async move {
            let result = backup::save_file(&name, json).await;
            saving.set(false);
            message.set(match result {
                Ok(Saved::To(Some(place))) => Some(format!("Backup saved to {place}.")),
                Ok(Saved::To(None)) => Some("Backup downloaded.".to_string()),
                Ok(Saved::Shared) | Ok(Saved::Cancelled) => None,
                Err(error) => Some(error),
            });
        });
    };

    let mut open = move |text: String| match backup::parse(&text) {
        Ok(file) => {
            message.set(None);
            preview.set(Some(backup::plan(&file, &ctx.settings.peek(), &servers.peek())));
        }
        Err(error) => message.set(Some(error)),
    };

    // iPhone: the plain input's script sends each picked file's text.
    #[cfg(target_os = "ios")]
    use_effect(move || {
        spawn(async move {
            let mut picked = document::eval(&format!(
                r#"
const input = document.getElementById("{FILE_INPUT_ID}");
if (input) {{
  input.addEventListener("change", async () => {{
    const file = input.files && input.files[0];
    input.value = "";
    if (file) dioxus.send(await file.text());
  }});
}}
await new Promise(() => {{}});
"#
            ));
            while let Ok(text) = picked.recv::<String>().await {
                open(text);
            }
        });
    });

    let volume = use_context::<VolumeSignal>().0;
    let shuffle = use_context::<ShuffleEnabledSignal>().0;
    let repeat = use_context::<Signal<RepeatMode>>();
    let apply = move |_| {
        let Some(plan) = preview.take() else {
            return;
        };
        servers.set(plan.servers.clone());
        let _ = save_servers_now(&plan.servers);
        // The shell keeps these three in their own signals.
        let (mut volume, mut shuffle, mut repeat) = (volume, shuffle, repeat);
        volume.set(plan.settings.volume);
        shuffle.set(plan.settings.shuffle_enabled);
        repeat.set(plan.settings.repeat_mode);
        ctx.update_cache(|settings| *settings = plan.settings.clone());
        ctx.notify("Backup imported");
    };

    rsx! {
        ul { class: "rs-list rs-list--text", aria_label: "Backup",
            ActionRow {
                label: "Export Backup…".to_string(),
                icon: "download".to_string(),
                busy: saving(),
                testid: "backup-export".to_string(),
                onclick: export,
            }
            li {
                label {
                    class: "rs-row rs-row--compact rs-settings-action",
                    r#for: FILE_INPUT_ID,
                    "data-testid": "backup-import",
                    Icon { name: "file-text".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    span { class: "rs-settings-action__label", "Import Backup…" }
                }
                if cfg!(target_os = "ios") {
                    div {
                        dangerous_inner_html: format!(
                            r#"<input type="file" id="{FILE_INPUT_ID}" class="rs-sr-only" accept=".json,application/json" tabindex="-1">"#
                        ),
                    }
                } else {
                    input {
                        r#type: "file",
                        id: FILE_INPUT_ID,
                        class: "rs-sr-only",
                        accept: ".json,application/json",
                        tabindex: "-1",
                        "data-testid": "backup-import-input",
                        onchange: move |evt| {
                            let Some(file) = evt.files().into_iter().next() else {
                                return;
                            };
                            spawn(async move {
                                match file.read_string().await {
                                    Ok(text) => open(text),
                                    Err(_) => message.set(Some("Couldn't read that file.".to_string())),
                                }
                            });
                        },
                    }
                }
            }
        }
        if let Some(text) = message() {
            p { class: "rs-list-foot rs-settings-foot", role: "status", "data-testid": "backup-status", "{text}" }
        }
        Footnote {
            "A backup holds this profile's settings, servers, equalizer, saved mixes and lyrics preferences. Importing one merges its servers with yours."
        }

        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "backup-include-sign-in".to_string(),
                title: "Include sign-in details".to_string(),
                subtitle: Some("Server passwords, Jellyfin tokens and your Last.fm key.".to_string()),
                checked: include_sign_in(),
                on_toggle: move |on| include_sign_in.set(on),
            }
        }
        if include_sign_in() {
            p { class: "rs-list-foot rs-settings-foot rs-backup-warn", role: "note", "data-testid": "backup-sign-in-warning",
                Icon { name: "alert".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                " Anyone who gets this file can sign in to your servers as you. Keep it private, and don't share it or put it somewhere public."
            }
        }

        if let Some(plan) = preview() {
            Sheet {
                title: "Import Backup?".to_string(),
                testid: "backup-preview".to_string(),
                on_close: move |_| preview.set(None),
                ImportSummary { plan: plan.clone() }
                div { class: "rs-settings-form__actions",
                    Button {
                        label: "Cancel".to_string(),
                        variant: ButtonVariant::Tinted,
                        onclick: move |_| preview.set(None),
                    }
                    Button {
                        label: if plan.changes_anything() { "Import".to_string() } else { "Done".to_string() },
                        testid: "backup-import-confirm".to_string(),
                        onclick: apply,
                    }
                }
            }
        }
    }
}

/// What an import changes, as a short list.
#[component]
fn ImportSummary(plan: ImportPlan) -> Element {
    let names = |list: &[String]| list.join(", ");
    rsx! {
        ul { class: "rs-backup-changes", "data-testid": "backup-changes",
            if plan.settings_changed.is_empty() {
                li { "Settings stay as they are." }
            } else {
                li { "Changes {plan.settings_changed.join(\", \")} settings." }
            }
            if !plan.servers_added.is_empty() {
                li { "Adds {names(&plan.servers_added)}." }
            }
            if !plan.servers_updated.is_empty() {
                li { "Updates {names(&plan.servers_updated)}." }
            }
            if plan.servers_added.is_empty() && plan.servers_updated.is_empty() {
                li { "Servers stay as they are." }
            }
            if !plan.needs_sign_in.is_empty() {
                li { class: "rs-backup-changes__note",
                    "Sign in to {names(&plan.needs_sign_in)} in Servers to turn it on; the backup has no password."
                }
            }
        }
    }
}
