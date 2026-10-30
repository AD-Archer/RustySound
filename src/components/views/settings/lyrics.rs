//! Settings → Lyrics: sync, timeout, offset, screenshot defaults, providers.

use super::shared::{use_settings_ctx, Footnote, GroupTitle, NumberRow, SwitchRow};
use crate::api::{normalize_lyrics_provider_order, LyricsProvider};
use crate::components::ui::{IconButton, IconButtonSize, PillButton, PillVariant};
use dioxus::prelude::*;

fn provider_label(key: &str) -> &'static str {
    LyricsProvider::from_key(key)
        .map(|provider| provider.label())
        .unwrap_or("Unknown")
}

/// Moves the provider at `index` by `delta` (-1 up, +1 down).
fn moved(order: &[String], index: usize, delta: isize) -> Vec<String> {
    let mut next = normalize_lyrics_provider_order(order);
    let target = index as isize + delta;
    if index < next.len() && target >= 0 && (target as usize) < next.len() {
        next.swap(index, target as usize);
    }
    next
}

#[component]
pub(super) fn LyricsSection() -> Element {
    let ctx = use_settings_ctx();
    let settings = ctx.settings.read().clone();
    let order = normalize_lyrics_provider_order(&settings.lyrics_provider_order);
    let count = order.len();

    rsx! {
        GroupTitle { title: "Sync".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "lyrics-sync".to_string(),
                title: "Synced lyrics".to_string(),
                subtitle: "Follow along line by line and tap a line to seek.".to_string(),
                checked: !settings.lyrics_unsynced_mode,
                on_toggle: move |on: bool| ctx.update(|s| s.lyrics_unsynced_mode = !on),
            }
            NumberRow {
                id: "lyrics-offset".to_string(),
                title: "Offset".to_string(),
                subtitle: "Positive shows lines later, negative earlier.".to_string(),
                value: settings.lyrics_offset_ms as i64,
                min: -5000,
                max: 5000,
                step: 50,
                unit: "ms".to_string(),
                on_commit: move |v: i64| ctx.update(|s| s.lyrics_offset_ms = v as i32),
            }
            NumberRow {
                id: "lyrics-timeout".to_string(),
                title: "Provider timeout".to_string(),
                value: settings.lyrics_request_timeout_secs as i64,
                min: 1,
                max: 20,
                unit: "s".to_string(),
                on_commit: move |v: i64| ctx.update(|s| s.lyrics_request_timeout_secs = v as u32),
            }
        }
        if settings.lyrics_offset_ms != 0 {
            div { class: "rs-settings-form__actions rs-settings-form__actions--start",
                PillButton {
                    label: "Reset Offset".to_string(),
                    variant: PillVariant::Mini,
                    onclick: move |_| ctx.update(|s| s.lyrics_offset_ms = 0),
                }
            }
        }

        GroupTitle { title: "Screenshot".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "lyrics-screenshot".to_string(),
                title: "Screenshot view".to_string(),
                subtitle: "A lyrics canvas with the artwork behind it and larger type, for clean screenshots.".to_string(),
                checked: settings.lyrics_screenshot_mode,
                on_toggle: move |on| ctx.update(|s| s.lyrics_screenshot_mode = on),
            }
            if settings.lyrics_screenshot_mode {
                SwitchRow {
                    id: "lyrics-screenshot-timestamps".to_string(),
                    title: "Include timestamps".to_string(),
                    subtitle: "When the lyrics are timed.".to_string(),
                    checked: settings.lyrics_screenshot_timestamps,
                    on_toggle: move |on| ctx.update(|s| s.lyrics_screenshot_timestamps = on),
                }
            }
        }

        GroupTitle { title: "Provider Order".to_string() }
        ol { class: "rs-list rs-list--text rs-settings-providers", aria_label: "Lyrics provider order",
            for (index, key) in order.iter().enumerate() {
                li { key: "{key}", "data-testid": "lyrics-provider",
                    div { class: "rs-row rs-row--compact rs-settings-row",
                        span { class: "rs-settings-providers__n", "{index + 1}" }
                        span { class: "rs-row__meta",
                            span { class: "rs-settings-row__title", "{provider_label(key)}" }
                        }
                        IconButton {
                            icon: "chevron-up".to_string(),
                            label: format!("Move {} up", provider_label(key)),
                            quiet: true,
                            size: IconButtonSize::Small,
                            disabled: index == 0,
                            onclick: move |_| ctx.update(|s| s.lyrics_provider_order = moved(&s.lyrics_provider_order, index, -1)),
                        }
                        IconButton {
                            icon: "chevron-down".to_string(),
                            label: format!("Move {} down", provider_label(key)),
                            quiet: true,
                            size: IconButtonSize::Small,
                            disabled: index + 1 >= count,
                            onclick: move |_| ctx.update(|s| s.lyrics_provider_order = moved(&s.lyrics_provider_order, index, 1)),
                        }
                    }
                }
            }
        }
        Footnote {
            "Tried top to bottom until one has the song. "
            if cfg!(target_arch = "wasm32") {
                "In the browser only LRCLIB answers (Netease and Genius block web requests), so keep it first here."
            } else {
                "The web version can only reach LRCLIB, so keep it first if you also use that."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_within_bounds() {
        let order = normalize_lyrics_provider_order(&[]);
        let first = order[0].clone();
        let down = moved(&order, 0, 1);
        assert_eq!(down[1], first);
        assert_eq!(moved(&order, 0, -1), order);
        assert_eq!(moved(&order, order.len() - 1, 1), order);
    }
}
