//! Settings → Appearance: theme swatches, System/Light/Dark, custom CSS.

use super::shared::{use_settings_ctx, Footnote, GroupTitle};
use crate::components::ui::env::{appearance_attr, stage_theme};
use crate::components::ui::{Button, ButtonVariant, Segmented};
use crate::components::Icon;
use dioxus::prelude::*;

const THEMES: [(&str, &str, &str); 3] = [
    ("stage", "Stage", "Cool grey lists, blue accent."),
    ("graphite", "Graphite", "Deeper neutrals, more contrast."),
    ("tape", "Tape", "Warm paper, signal orange."),
];

const APPEARANCES: [(&str, &str); 3] = [("system", "System"), ("light", "Light"), ("dark", "Dark")];

#[component]
pub(super) fn AppearanceSection() -> Element {
    let ctx = use_settings_ctx();
    let settings = ctx.settings;
    let (theme, appearance) = {
        let current = settings.read();
        (stage_theme(&current.theme), appearance_attr(&current))
    };
    let appearance_index = APPEARANCES
        .iter()
        .position(|(key, _)| *key == appearance)
        .unwrap_or(0);

    let mut draft = use_signal(|| settings.peek().custom_css.clone());
    let saved_css = settings.read().custom_css.clone();
    let dirty = draft() != saved_css;

    rsx! {
        GroupTitle { title: "Theme".to_string() }
        div {
            class: "rs-settings-themes",
            role: "radiogroup",
            aria_label: "Theme",
            for (key, label, blurb) in THEMES {
                ThemeSwatch {
                    key: "{key}",
                    theme: key.to_string(),
                    label: label.to_string(),
                    blurb: blurb.to_string(),
                    appearance: appearance.to_string(),
                    selected: key == theme,
                    on_pick: move |picked: String| ctx.update(|s| s.theme = picked),
                }
            }
        }

        GroupTitle { title: "Appearance".to_string() }
        Segmented {
            options: APPEARANCES.iter().map(|(_, label)| label.to_string()).collect::<Vec<_>>(),
            selected: appearance_index,
            label: "Appearance".to_string(),
            testid: "appearance-mode".to_string(),
            on_select: move |index: usize| {
                let key = APPEARANCES.get(index).map(|(k, _)| *k).unwrap_or("system");
                ctx.update(|s| s.appearance = key.to_string());
            },
        }
        Footnote { "System follows your device's light or dark setting. The stage behind artwork stays dark in every mode." }

        GroupTitle { title: "Custom CSS".to_string() }
        div { class: "rs-settings-css",
            label { class: "rs-settings-sr", r#for: "custom-css", "Custom CSS" }
            textarea {
                id: "custom-css",
                class: "rs-settings-css__input",
                spellcheck: "false",
                autocapitalize: "off",
                rows: "8",
                aria_describedby: "custom-css-hint",
                placeholder: "/* Loaded after the app's styles */\n.rs-large-title {{ letter-spacing: -0.02em; }}",
                value: "{draft}",
                oninput: move |evt| draft.set(evt.value()),
            }
        }
        Footnote {
            span { id: "custom-css-hint",
                "Applied live after every built-in style. Use the --page, --surface, --ink and --accent tokens to restyle a theme."
            }
        }
        div { class: "rs-settings-form__actions rs-settings-form__actions--start",
            Button {
                label: "Apply".to_string(),
                variant: ButtonVariant::Filled,
                disabled: !dirty,
                testid: "custom-css-apply".to_string(),
                onclick: move |_| {
                    let css = draft();
                    ctx.update(|s| s.custom_css = css);
                },
            }
            if !saved_css.is_empty() || !draft().is_empty() {
                Button {
                    label: "Clear".to_string(),
                    variant: ButtonVariant::Tinted,
                    onclick: move |_| {
                        draft.set(String::new());
                        ctx.update(|s| s.custom_css = String::new());
                    },
                }
            }
        }
    }
}

/// A live preview of a theme: a mini stage with covers and a list row, drawn
/// with that theme's own tokens (`data-theme` on the preview box).
#[component]
fn ThemeSwatch(
    theme: String,
    label: String,
    blurb: String,
    appearance: String,
    selected: bool,
    on_pick: EventHandler<String>,
) -> Element {
    let pick = theme.clone();
    rsx! {
        button {
            r#type: "button",
            class: "rs-settings-swatch",
            role: "radio",
            aria_checked: if selected { "true" } else { "false" },
            "data-testid": "theme-{theme}",
            onclick: move |_| on_pick.call(pick.clone()),
            div {
                class: "rs-settings-swatch__preview",
                "data-theme": "{theme}",
                "data-appearance": "{appearance}",
                aria_hidden: "true",
                div { class: "rs-settings-swatch__stage",
                    span { class: "rs-settings-swatch__cover rs-settings-swatch__cover--l" }
                    span { class: "rs-settings-swatch__cover rs-settings-swatch__cover--c" }
                    span { class: "rs-settings-swatch__cover rs-settings-swatch__cover--r" }
                }
                div { class: "rs-settings-swatch__page",
                    div { class: "rs-settings-swatch__row",
                        span { class: "rs-settings-swatch__thumb" }
                        span { class: "rs-settings-swatch__lines",
                            span {}
                            span {}
                        }
                        span { class: "rs-settings-swatch__play" }
                    }
                    div { class: "rs-settings-swatch__track",
                        span {}
                    }
                }
            }
            span { class: "rs-settings-swatch__meta",
                span { class: "rs-settings-swatch__name",
                    "{label}"
                    if selected {
                        span { class: "rs-check rs-check--on rs-settings-swatch__tick",
                            Icon { name: "check".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                        }
                    }
                }
                span { class: "rs-settings-swatch__blurb", "{blurb}" }
            }
        }
    }
}
