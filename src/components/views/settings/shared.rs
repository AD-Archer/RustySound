//! Building blocks shared by the Settings sections: the save/toast context and
//! iOS-style setting rows (switch, number, select, action, value).

use crate::cache_service::apply_settings as apply_cache_settings;
use crate::components::ui::{env::sleep_ms, Switch};
use crate::components::Icon;
use crate::db::{save_settings, AppSettings};
use dioxus::prelude::*;

/// Shared Settings state, provided by `SettingsView`.
#[derive(Clone, Copy)]
pub(super) struct SettingsCtx {
    pub settings: Signal<AppSettings>,
    toast: Signal<Option<String>>,
    toast_nonce: Signal<u64>,
}

impl SettingsCtx {
    pub fn provide() -> Self {
        let settings = use_context::<Signal<AppSettings>>();
        let toast = use_signal(|| None::<String>);
        let toast_nonce = use_signal(|| 0u64);
        use_context_provider(|| SettingsCtx {
            settings,
            toast,
            toast_nonce,
        })
    }

    pub fn toast_text(&self) -> Option<String> {
        (self.toast)()
    }

    /// Change settings, persist them and flash "Saved". No-op when nothing changed.
    pub fn update(self, change: impl FnOnce(&mut AppSettings)) {
        self.apply(change, false);
    }

    /// Like `update`, then pushes the cache settings to the cache service.
    pub fn update_cache(self, change: impl FnOnce(&mut AppSettings)) {
        self.apply(change, true);
    }

    fn apply(self, change: impl FnOnce(&mut AppSettings), cache: bool) {
        let mut settings_signal = self.settings;
        let mut next = settings_signal.peek().clone();
        change(&mut next);
        if next == *settings_signal.peek() {
            return;
        }
        if cache {
            apply_cache_settings(&next);
        }
        settings_signal.set(next.clone());
        spawn(async move {
            let _ = save_settings(next).await;
        });
        self.notify("Saved");
    }

    /// Shows a short status message at the bottom of the page.
    pub fn notify(self, message: impl Into<String>) {
        let mut toast = self.toast;
        let mut nonce_signal = self.toast_nonce;
        let nonce = nonce_signal.peek().wrapping_add(1);
        nonce_signal.set(nonce);
        toast.set(Some(message.into()));
        spawn(async move {
            sleep_ms(1600).await;
            if *nonce_signal.peek() == nonce {
                toast.set(None);
            }
        });
    }
}

pub(super) fn use_settings_ctx() -> SettingsCtx {
    use_context::<SettingsCtx>()
}

/// Small muted heading above a grouped list.
#[component]
pub(super) fn GroupTitle(title: String) -> Element {
    rsx! {
        h3 { class: "rs-settings-group", "{title}" }
    }
}

/// Explanatory text under a grouped list.
#[component]
pub(super) fn Footnote(children: Element) -> Element {
    rsx! {
        p { class: "rs-list-foot rs-settings-foot", {children} }
    }
}

/// The title + optional subtitle block of a setting row. `id` labels the control.
#[component]
fn RowText(id: String, title: String, subtitle: Option<String>) -> Element {
    rsx! {
        span { class: "rs-row__meta",
            span { class: "rs-settings-row__title", id: "{id}-title", "{title}" }
            if let Some(sub) = subtitle {
                span { class: "rs-settings-row__sub", id: "{id}-desc", "{sub}" }
            }
        }
    }
}

/// A row with a trailing switch. `id` must be unique on the page.
#[component]
pub(super) fn SwitchRow(
    id: String,
    title: String,
    #[props(default)] subtitle: Option<String>,
    checked: bool,
    on_toggle: EventHandler<bool>,
    #[props(default)] disabled: bool,
) -> Element {
    rsx! {
        li {
            div { class: "rs-row rs-row--compact rs-settings-row",
                RowText { id: id.clone(), title: title.clone(), subtitle: subtitle.clone() }
                Switch {
                    checked,
                    on_toggle,
                    labelled_by: format!("{id}-title"),
                    disabled,
                    testid: id.clone(),
                }
            }
        }
    }
}

/// A row with a trailing number field that commits on change (blur / Enter).
#[component]
pub(super) fn NumberRow(
    id: String,
    title: String,
    #[props(default)] subtitle: Option<String>,
    value: i64,
    min: i64,
    max: i64,
    #[props(default = 1)] step: i64,
    #[props(default)] unit: Option<String>,
    on_commit: EventHandler<i64>,
) -> Element {
    let described = subtitle.as_ref().map(|_| format!("{id}-desc"));
    rsx! {
        li {
            div { class: "rs-row rs-row--compact rs-settings-row",
                span { class: "rs-row__meta",
                    label { class: "rs-settings-row__title", r#for: "{id}", "{title}" }
                    if let Some(sub) = subtitle.clone() {
                        span { class: "rs-settings-row__sub", id: "{id}-desc", "{sub}" }
                    }
                }
                span { class: "rs-settings-num",
                    input {
                        id: "{id}",
                        r#type: "number",
                        inputmode: "numeric",
                        min: "{min}",
                        max: "{max}",
                        step: "{step}",
                        value: "{value}",
                        aria_describedby: described,
                        onchange: move |evt| {
                            if let Ok(parsed) = evt.value().trim().parse::<f64>() {
                                on_commit.call((parsed.round() as i64).clamp(min, max));
                            }
                        },
                    }
                    if let Some(unit) = unit.clone() {
                        span { class: "rs-settings-num__unit", aria_hidden: "true", "{unit}" }
                    }
                }
            }
        }
    }
}

/// A row with a trailing native select (menu-style).
#[component]
pub(super) fn SelectRow(
    id: String,
    title: String,
    #[props(default)] subtitle: Option<String>,
    value: String,
    options: Vec<(String, String)>,
    on_change: EventHandler<String>,
) -> Element {
    let described = subtitle.as_ref().map(|_| format!("{id}-desc"));
    rsx! {
        li {
            div { class: "rs-row rs-row--compact rs-settings-row",
                span { class: "rs-row__meta",
                    label { class: "rs-settings-row__title", r#for: "{id}", "{title}" }
                    if let Some(sub) = subtitle.clone() {
                        span { class: "rs-settings-row__sub", id: "{id}-desc", "{sub}" }
                    }
                }
                span { class: "rs-settings-select",
                    select {
                        id: "{id}",
                        aria_describedby: described,
                        onchange: move |evt| on_change.call(evt.value()),
                        for (key, label) in options.iter() {
                            option {
                                key: "{key}",
                                value: "{key}",
                                selected: *key == value,
                                "{label}"
                            }
                        }
                    }
                    Icon { name: "chevron-down".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                }
            }
        }
    }
}

/// A full-width button row ("Add Server", "Clear Cache"). `danger` paints it red.
#[component]
pub(super) fn ActionRow(
    label: String,
    #[props(default)] icon: Option<String>,
    #[props(default)] danger: bool,
    #[props(default)] disabled: bool,
    #[props(default)] busy: bool,
    #[props(default)] detail: Option<String>,
    #[props(default)] testid: Option<String>,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        li {
            button {
                r#type: "button",
                class: "rs-row rs-row--compact rs-settings-action",
                class: if danger { "rs-settings-action--danger" },
                "data-testid": testid.clone(),
                disabled: disabled || busy,
                aria_busy: if busy { "true" } else { "false" },
                onclick: move |evt| onclick.call(evt),
                if busy {
                    span { class: "rs-spinner rs-spinner--page", aria_hidden: "true" }
                } else if let Some(name) = icon.clone() {
                    Icon { name, class: "rs-ic rs-ic--sm".to_string() }
                }
                span { class: "rs-settings-action__label", "{label}" }
                if let Some(detail) = detail.clone() {
                    span { class: "rs-row__detail", "{detail}" }
                }
            }
        }
    }
}

/// A read-only "Title ...... value" row.
#[component]
pub(super) fn ValueRow(
    title: String,
    value: String,
    #[props(default)] subtitle: Option<String>,
) -> Element {
    rsx! {
        li {
            div { class: "rs-row rs-row--compact rs-settings-row",
                span { class: "rs-row__meta",
                    span { class: "rs-settings-row__title", "{title}" }
                    if let Some(sub) = subtitle {
                        span { class: "rs-settings-row__sub", "{sub}" }
                    }
                }
                span { class: "rs-row__detail", "{value}" }
            }
        }
    }
}

/// A flat usage meter (cache and downloads). Not the glossy progress track:
/// gloss is reserved for playback progress.
#[component]
pub(super) fn UsageMeter(label: String, percent: f64) -> Element {
    let percent = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    rsx! {
        span {
            class: "rs-settings-meter",
            role: "meter",
            aria_label: "{label}",
            aria_valuemin: "0",
            aria_valuemax: "100",
            aria_valuenow: "{percent:.0}",
            span { class: "rs-settings-meter__fill", style: "width: {percent:.1}%" }
        }
    }
}

/// Megabytes with one decimal ("12.4 MB"), switching to GB past 1024 MB.
pub(super) fn format_mb(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.1} MB")
    }
}

/// "1,204" style thousands separators.
pub(super) fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1204), "1,204");
        assert_eq!(group_thousands(1234567), "1,234,567");
    }

    #[test]
    fn formats_megabytes() {
        assert_eq!(format_mb(0), "0.0 MB");
        assert_eq!(format_mb(1024 * 1024 * 3 / 2), "1.5 MB");
        assert_eq!(format_mb(2 * 1024 * 1024 * 1024), "2.0 GB");
    }
}
