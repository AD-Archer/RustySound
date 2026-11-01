//! Settings › Playback › Equalizer (`/settings/equalizer`): the on/off switch,
//! the presets, the curve, ten band sliders and the pre-gain. Also opened from
//! Now Playing's "…" menu.
//!
//! Sliders change the sound while they move (the setting signal, which
//! `use_equalizer_sync` sends to the player) and save on release.

use super::shared::{Footnote, GroupTitle, SettingsCtx, SwitchRow};
use crate::components::ui::{EqCurve, VerticalSlider};
use crate::components::{bypass_note, AppView, Icon, Navigation, PageMotion, EQ_AVAILABLE};
use crate::db::{save_settings, AppSettings};
use crate::equalizer::{
    db_label, EqPreset, EqSettings, EQ_BANDS, EQ_GAIN_LIMIT_DB, EQ_PREAMP_MIN_DB,
};
use dioxus::prelude::*;

/// What the iPhone app can't equalize.
const PLATFORM_NOTE: Option<&str> = if cfg!(all(not(target_arch = "wasm32"), target_os = "ios")) {
    Some("Live radio streams play without the equalizer.")
} else {
    None
};

#[component]
pub fn EqualizerView() -> Element {
    let ctx = SettingsCtx::provide();
    let navigation = use_context::<Navigation>();
    let mut settings = use_context::<Signal<AppSettings>>();
    let mut note = use_signal(|| None::<String>);

    let eq = settings.read().equalizer.clone();
    let enabled = eq.enabled;
    let gains = eq.gains();
    let preamp = eq.preamp();

    // Changes the EQ live (no save): slider moves.
    let mut live = move |change: &dyn Fn(&mut EqSettings)| {
        settings.with_mut(|s| change(&mut s.equalizer));
    };
    // Saves what's live now: slider releases.
    let save = move || {
        let snapshot = settings.peek().clone();
        spawn(async move {
            let _ = save_settings(snapshot).await;
        });
    };

    // Whether the player can shape what's playing (a quiet note when not),
    // checked while the page is open and the EQ is on.
    use_future(move || async move {
        loop {
            let on = settings.peek().equalizer.enabled;
            let next = if on { bypass_note().await } else { None };
            if *note.peek() != next {
                note.set(next);
            }
            crate::components::ui::env::sleep_ms(1500).await;
        }
    });

    let toast = ctx.toast_text();
    let back = move |_| {
        if navigation.can_go_back() {
            navigation.go_back();
        } else {
            navigation.navigate_with(AppView::SettingsView {}, PageMotion::Back);
        }
    };

    rsx! {
        div { class: "rs-view rs-view--narrow rs-settings rs-settings--compact rs-equalizer", "data-testid": "equalizer",
            div { class: "rs-settings-back",
                button {
                    r#type: "button",
                    class: "rs-settings-back__btn",
                    "data-testid": "eq-back",
                    onclick: back,
                    Icon { name: "chevron-left".to_string(), class: "rs-ic".to_string() }
                    "Back"
                }
            }
            h1 { class: "rs-large-title rs-settings-detail-title", "Equalizer" }

            if !EQ_AVAILABLE {
                Footnote { "The Windows app has no equalizer yet." }
            } else {
                ul { class: "rs-list rs-list--text",
                    SwitchRow {
                        id: "eq-enabled".to_string(),
                        title: "Equalizer".to_string(),
                        subtitle: "Shape the sound with ten bands, from deep bass to air.".to_string(),
                        checked: enabled,
                        on_toggle: move |on| ctx.update(|s| s.equalizer.enabled = on),
                    }
                }
                if let Some(text) = note() {
                    p { class: "rs-equalizer__note", role: "status", "data-testid": "eq-note", "{text}" }
                }

                GroupTitle { title: "Preset".to_string() }
                div {
                    class: "rs-equalizer__presets",
                    role: "radiogroup",
                    aria_label: "Preset",
                    "data-testid": "eq-presets",
                    for preset in EqPreset::ALL {
                        button {
                            key: "{preset.key()}",
                            r#type: "button",
                            class: "rs-equalizer__preset",
                            role: "radio",
                            aria_checked: if eq.preset == preset { "true" } else { "false" },
                            "data-testid": "eq-preset-{preset.key()}",
                            onclick: move |_| ctx.update(|s| {
                                s.equalizer.set_preset(preset);
                                // Picking a preset is a request to hear it.
                                s.equalizer.enabled = true;
                            }),
                            "{preset.label()}"
                        }
                    }
                }

                div { class: "rs-equalizer__panel", class: if !enabled { "rs-equalizer__panel--off" },
                    EqCurve { gains, dimmed: !enabled }
                    div { class: "rs-equalizer__bands", role: "group", aria_label: "Bands",
                        for (index, band) in EQ_BANDS.iter().enumerate() {
                            div { key: "{band.label}", class: "rs-equalizer__band",
                                span { class: "rs-equalizer__gain", aria_hidden: "true", "{gain_text(gains[index])}" }
                                VerticalSlider {
                                    value: gains[index],
                                    min: -EQ_GAIN_LIMIT_DB,
                                    max: EQ_GAIN_LIMIT_DB,
                                    step: 0.5,
                                    origin: 0.0,
                                    label: band.spoken.to_string(),
                                    value_text: db_label(gains[index]),
                                    testid: format!("eq-band-{index}"),
                                    on_input: move |v: f64| live(&|eq: &mut EqSettings| eq.set_band(index, v)),
                                    on_commit: move |v: f64| {
                                        live(&|eq: &mut EqSettings| eq.set_band(index, v));
                                        save();
                                    },
                                }
                                span { class: "rs-equalizer__freq", aria_hidden: "true", "{band.label}" }
                            }
                        }
                    }
                }

                GroupTitle { title: "Pre-gain".to_string() }
                ul { class: "rs-list rs-list--text",
                    SwitchRow {
                        id: "eq-auto-preamp".to_string(),
                        title: "Automatic pre-gain".to_string(),
                        subtitle: "Turns the music down as much as the boosts turn it up, so loud songs don't distort.".to_string(),
                        checked: eq.auto_preamp,
                        on_toggle: move |on: bool| ctx.update(|s| {
                            if !on {
                                // Start the manual value where the automatic one was.
                                s.equalizer.preamp_db = s.equalizer.preamp();
                            }
                            s.equalizer.auto_preamp = on;
                        }),
                    }
                    li {
                        div { class: "rs-row rs-row--compact rs-settings-row rs-equalizer__preamp",
                            span { class: "rs-row__meta",
                                span { class: "rs-settings-row__title", "Pre-gain" }
                            }
                            input {
                                class: "rs-equalizer__preamp-input",
                                r#type: "range",
                                min: "{EQ_PREAMP_MIN_DB}",
                                max: "0",
                                step: "0.5",
                                value: "{preamp}",
                                disabled: eq.auto_preamp,
                                aria_label: "Pre-gain",
                                aria_valuetext: db_label(preamp),
                                "data-testid": "eq-preamp",
                                oninput: move |evt| {
                                    if let Ok(v) = evt.value().parse::<f64>() {
                                        live(&|eq: &mut EqSettings| eq.set_preamp(v));
                                    }
                                },
                                onchange: move |_| save(),
                            }
                            span { class: "rs-row__detail rs-equalizer__preamp-value", "{db_label(preamp)}" }
                        }
                    }
                }
                if eq.may_clip() {
                    Footnote { "Loud songs may distort at this pre-gain." }
                }
                if let Some(text) = PLATFORM_NOTE {
                    Footnote { "{text}" }
                }
            }
            div { class: "rs-settings-toast-host", role: "status", aria_live: "polite",
                if let Some(message) = toast {
                    span { class: "rs-settings-toast", "{message}" }
                }
            }
        }
    }
}

/// "+3", "0", "-4.5" above a band.
fn gain_text(db: f64) -> String {
    let rounded = (db * 2.0).round() / 2.0;
    let text = if rounded.fract() == 0.0 { format!("{rounded:.0}") } else { format!("{rounded:.1}") };
    if rounded > 0.0 {
        format!("+{text}")
    } else {
        text
    }
}
