//! Settings → Playback: default volume, gapless, crossfade, Autoplay, the
//! Equalizer (its own page), volume levelling, bookmarks, queue sync,
//! network speakers (`speakers.rs`), Offline Mode.
//!
//! Crossfade needs two players, so the iPhone app (one AVQueuePlayer) hides
//! it and plays gapless instead. The Windows player has neither yet. The old
//! `replay_gain` switch is unused since `volume_levelling`; the field stays so
//! old settings files still load.

use super::shared::{
    use_settings_ctx, Footnote, GroupTitle, LinkRow, NumberRow, SelectRow, SwitchRow,
};
use crate::components::ui::{ProgressTrack, TrackVariant};
use crate::components::{
    AppView, Icon, Navigation, VolumeSignal, CROSSFADE_MAX_SECS, CROSSFADE_MIN_SECS, EQ_AVAILABLE,
};
use crate::db::VolumeLevelling;
use dioxus::prelude::*;

/// Gapless needs the audio deck (Mac, web) or AVQueuePlayer (iPhone).
const GAPLESS_AVAILABLE: bool = !cfg!(all(not(target_arch = "wasm32"), target_os = "windows"));
/// The iPhone app has one native player: no crossfade there.
const CROSSFADE_AVAILABLE: bool =
    GAPLESS_AVAILABLE && !cfg!(all(not(target_arch = "wasm32"), target_os = "ios"));

const LEVELLING_CHOICES: [(VolumeLevelling, &str, &str); 4] = [
    (VolumeLevelling::Auto, "auto", "Auto"),
    (VolumeLevelling::Album, "album", "Album"),
    (VolumeLevelling::Track, "track", "Track"),
    (VolumeLevelling::Off, "off", "Off"),
];

const PREAMP_CHOICES: [f64; 5] = [-6.0, -3.0, 0.0, 3.0, 6.0];

fn levelling_key(mode: VolumeLevelling) -> &'static str {
    LEVELLING_CHOICES
        .iter()
        .find(|(m, _, _)| *m == mode)
        .map(|(_, key, _)| *key)
        .unwrap_or("auto")
}

fn parse_levelling(key: &str) -> VolumeLevelling {
    LEVELLING_CHOICES
        .iter()
        .find(|(_, k, _)| *k == key)
        .map(|(mode, _, _)| *mode)
        .unwrap_or_default()
}

/// "+3 dB", "0 dB", "-6 dB".
fn preamp_label(db: f64) -> String {
    if db > 0.0 {
        format!("+{db:.0} dB")
    } else {
        format!("{db:.0} dB")
    }
}

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
    let queue_sync = settings.queue_sync_enabled;
    let gapless = settings.gapless_playback;
    let crossfade = settings.crossfade_enabled && CROSSFADE_AVAILABLE;
    let crossfade_secs = settings
        .crossfade_duration
        .clamp(CROSSFADE_MIN_SECS, CROSSFADE_MAX_SECS) as i64;
    let levelling = settings.volume_levelling;
    let preamp = settings.replay_gain_preamp_db;
    let autoplay = settings.autoplay_enabled;
    let eq_summary = settings.equalizer.summary();
    drop(settings);
    let navigation = use_context::<Navigation>();

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

        if GAPLESS_AVAILABLE {
            GroupTitle { title: "Transitions".to_string() }
            ul { class: "rs-list rs-list--text",
                SwitchRow {
                    id: "gapless-playback".to_string(),
                    title: "Gapless playback".to_string(),
                    subtitle: "Start the next song the moment this one ends.".to_string(),
                    checked: gapless,
                    on_toggle: move |on| ctx.update(|s| s.gapless_playback = on),
                }
                if CROSSFADE_AVAILABLE {
                    SwitchRow {
                        id: "crossfade".to_string(),
                        title: "Crossfade".to_string(),
                        subtitle: "Fade each song into the next. Songs from the same album and radio play gapless instead.".to_string(),
                        checked: crossfade,
                        on_toggle: move |on| ctx.update(|s| s.crossfade_enabled = on),
                    }
                    if crossfade {
                        NumberRow {
                            id: "crossfade-duration".to_string(),
                            title: "Crossfade length".to_string(),
                            value: crossfade_secs,
                            min: CROSSFADE_MIN_SECS as i64,
                            max: CROSSFADE_MAX_SECS as i64,
                            unit: "seconds".to_string(),
                            on_commit: move |v: i64| ctx.update(|s| s.crossfade_duration = v as u32),
                        }
                    }
                }
            }
        }

        GroupTitle { title: "Queue".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "autoplay".to_string(),
                title: "Autoplay".to_string(),
                subtitle: "When your queue is about to end, keep playing similar songs.".to_string(),
                checked: autoplay,
                on_toggle: move |on| ctx.update(|s| s.autoplay_enabled = on),
            }
        }

        if EQ_AVAILABLE {
            GroupTitle { title: "Sound".to_string() }
            ul { class: "rs-list rs-list--text",
                LinkRow {
                    title: "Equalizer".to_string(),
                    detail: eq_summary.to_string(),
                    testid: "settings-equalizer".to_string(),
                    onclick: move |_| navigation.navigate_to(AppView::EqualizerView {}),
                }
            }
        }

        GroupTitle { title: "Volume Levelling".to_string() }
        ul { class: "rs-list rs-list--text",
            SelectRow {
                id: "volume-levelling".to_string(),
                title: "Volume levelling".to_string(),
                subtitle: "Plays songs at a similar loudness, from the server's ReplayGain tags. Auto keeps an album's own dynamics.".to_string(),
                value: levelling_key(levelling).to_string(),
                options: LEVELLING_CHOICES
                    .iter()
                    .map(|(_, key, label)| (key.to_string(), label.to_string()))
                    .collect::<Vec<_>>(),
                on_change: move |v: String| ctx.update(|s| s.volume_levelling = parse_levelling(&v)),
            }
            if levelling != VolumeLevelling::Off {
                SelectRow {
                    id: "replay-gain-preamp".to_string(),
                    title: "Pre-amp".to_string(),
                    subtitle: "Added to every song's gain. Peaks are never pushed past full scale.".to_string(),
                    value: format!("{preamp:.0}"),
                    options: PREAMP_CHOICES
                        .iter()
                        .map(|db| (format!("{db:.0}"), preamp_label(*db)))
                        .collect::<Vec<_>>(),
                    on_change: move |v: String| {
                        if let Ok(db) = v.parse::<f64>() {
                            ctx.update(|s| s.replay_gain_preamp_db = db);
                        }
                    },
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
                subtitle: "Load where this device left off, paused, when the app starts: its synced queue, or your latest bookmark.".to_string(),
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

        GroupTitle { title: "Other Devices".to_string() }
        ul { class: "rs-list rs-list--text",
            SwitchRow {
                id: "queue-sync".to_string(),
                title: "Sync queue with server".to_string(),
                subtitle: "Save your queue and place to the server, and offer to continue from another device.".to_string(),
                checked: queue_sync,
                on_toggle: move |on| ctx.update(|s| s.queue_sync_enabled = on),
            }
        }
        Footnote { "The server keeps one queue for your account; the device that saves last wins. Up to 500 songs around the current one are saved." }

        // Network speakers (Mac and iPhone; nothing on the web).
        super::speakers::SpeakersGroup {}

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
