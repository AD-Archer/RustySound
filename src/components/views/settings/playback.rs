//! Settings → Playback: default volume, bookmarks, Offline Mode.
//!
//! Crossfade and Replay Gain are hidden on purpose: no audio backend ever used
//! them. Their `AppSettings` fields stay so old settings files still load.

use super::shared::{use_settings_ctx, Footnote, GroupTitle, NumberRow, SwitchRow};
use crate::components::ui::{ProgressTrack, TrackVariant};
use crate::components::{Icon, VolumeSignal};
use dioxus::prelude::*;

#[component]
pub(super) fn PlaybackSection() -> Element {
    let ctx = use_settings_ctx();
    let mut volume = use_context::<VolumeSignal>().0;
    let percent = (volume() * 100.0).round();
    let icon = if percent > 50.0 {
        "volume-2"
    } else if percent > 0.0 {
        "volume-1"
    } else {
        "volume-x"
    };

    let settings = ctx.settings.read();
    let auto_save = settings.bookmark_auto_save;
    let resume = settings.bookmark_autoplay_on_launch;
    let limit = settings.bookmark_limit as i64;
    let offline = settings.offline_mode;
    drop(settings);

    let mut set_volume = move |value: f64| volume.set((value / 100.0).clamp(0.0, 1.0));

    rsx! {
        GroupTitle { title: "Volume".to_string() }
        ul { class: "rs-list rs-list--text",
            li {
                div { class: "rs-row rs-row--compact rs-settings-volume",
                    Icon { name: icon.to_string(), class: "rs-ic rs-settings-volume__icon".to_string() }
                    ProgressTrack {
                        value: percent,
                        max: 100.0,
                        step: 1.0,
                        variant: TrackVariant::Page,
                        label: "Default volume".to_string(),
                        value_text: format!("{percent:.0}%"),
                        testid: "settings-volume".to_string(),
                        class: "rs-settings-volume__track".to_string(),
                        on_input: move |v| set_volume(v),
                        on_commit: move |v| set_volume(v),
                    }
                    span { class: "rs-row__detail rs-settings-volume__value", "{percent:.0}%" }
                }
            }
        }

        GroupTitle { title: "Bookmarks".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "bookmark-auto-save".to_string(),
                title: "Auto-save bookmarks".to_string(),
                subtitle: "Remember your place while you listen and when the song changes.".to_string(),
                checked: auto_save,
                on_toggle: move |on| ctx.update(|s| s.bookmark_auto_save = on),
            }
            SwitchRow {
                id: "bookmark-resume".to_string(),
                title: "Resume on launch".to_string(),
                subtitle: "Queue your latest bookmark at its position, paused, when the app starts.".to_string(),
                checked: resume,
                on_toggle: move |on| ctx.update(|s| s.bookmark_autoplay_on_launch = on),
            }
            NumberRow {
                id: "bookmark-limit".to_string(),
                title: "Keep".to_string(),
                subtitle: "Newest bookmarks per server user; older ones are deleted.".to_string(),
                value: limit,
                min: 1,
                max: 5000,
                unit: "bookmarks".to_string(),
                on_commit: move |v: i64| ctx.update(|s| s.bookmark_limit = v as u32),
            }
        }

        GroupTitle { title: "Offline".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "offline-mode".to_string(),
                title: "Offline Mode".to_string(),
                subtitle: "Use downloaded and cached music only. No network requests until you turn it off.".to_string(),
                checked: offline,
                on_toggle: move |on| ctx.update_cache(|s| s.offline_mode = on),
            }
        }
        Footnote { "Download music in Storage & Downloads so it's ready before you go offline." }
    }
}
