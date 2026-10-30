//! The square Lyrics Shot card that gets captured, and its two option
//! panels (background theme, and size / blur / text colour).

use super::shot::{choose_theme, ShotStyle, SHOT_CARD_ID};
use super::shot_share::{
    screenshot_shot_text_colors, ScreenshotShotTheme, ScreenshotTextPalette, RUSTYSOUND_MARK,
};
use crate::components::Icon;
use crate::db::AppSettings;
use dioxus::prelude::*;

const THEMES: [(ScreenshotShotTheme, &str, &str); 3] = [
    (ScreenshotShotTheme::Lagoon, "Lagoon", "#62bac9"),
    (ScreenshotShotTheme::Ember, "Ember", "#df8a71"),
    (ScreenshotShotTheme::Midnight, "Midnight", "#1f2a44"),
];

const PALETTES: [(ScreenshotTextPalette, &str, &str); 5] = [
    (ScreenshotTextPalette::Auto, "Auto", "linear-gradient(135deg,#fff,#27272a)"),
    (ScreenshotTextPalette::Light, "Light", "#fff"),
    (ScreenshotTextPalette::Dark, "Dark", "#18181b"),
    (ScreenshotTextPalette::Gold, "Gold", "#fde68a"),
    (ScreenshotTextPalette::Cyan, "Cyan", "#a5f3fc"),
];

/// The theme actually used: Cover falls back to Lagoon without artwork.
fn active_theme(theme: ScreenshotShotTheme, has_cover: bool) -> ScreenshotShotTheme {
    if theme == ScreenshotShotTheme::Cover && !has_cover {
        ScreenshotShotTheme::Lagoon
    } else {
        theme
    }
}

fn card_background(theme: ScreenshotShotTheme) -> &'static str {
    match theme {
        ScreenshotShotTheme::Lagoon => "linear-gradient(180deg,#79d0da 0%,#4d9fb6 100%)",
        ScreenshotShotTheme::Ember => "linear-gradient(180deg,#f3cc8d 0%,#d17a66 100%)",
        ScreenshotShotTheme::Midnight => "linear-gradient(180deg,#253552 0%,#0d1422 100%)",
        ScreenshotShotTheme::Cover => "#111827",
    }
}

fn card_sheen(theme: ScreenshotShotTheme) -> &'static str {
    match theme {
        ScreenshotShotTheme::Lagoon => "linear-gradient(180deg,rgba(255,255,255,.16) 0%,rgba(255,255,255,.03) 100%)",
        ScreenshotShotTheme::Ember => "linear-gradient(180deg,rgba(255,255,255,.18) 0%,rgba(255,255,255,.02) 100%)",
        ScreenshotShotTheme::Midnight => "linear-gradient(180deg,rgba(255,255,255,.08) 0%,rgba(255,255,255,.02) 100%)",
        ScreenshotShotTheme::Cover => "linear-gradient(180deg,rgba(7,10,14,.18) 0%,rgba(7,10,14,.22) 28%,rgba(7,10,14,.78) 100%)",
    }
}

/// Font size (rem) and line height for 1–5 selected lines.
fn lyric_metrics(lines: usize) -> (f64, f64) {
    match lines {
        0 | 1 => (2.0, 1.02),
        2 => (1.7, 1.05),
        3 => (1.45, 1.08),
        4 => (1.2, 1.1),
        _ => (1.02, 1.1),
    }
}

#[component]
pub(super) fn ShotCard(
    style: ShotStyle,
    lines: Vec<String>,
    title: String,
    artist: Option<String>,
    cover_url: Option<String>,
) -> Element {
    let theme = active_theme((style.theme)(), cover_url.is_some());
    let dark_text = matches!(theme, ScreenshotShotTheme::Lagoon | ScreenshotShotTheme::Ember);
    let colors = screenshot_shot_text_colors((style.palette)(), dark_text);
    let scale = (style.font_scale)().clamp(80, 130) as f64 / 100.0;
    let blur = ((style.blur)().clamp(8, 48) as f64 * 0.72).round();
    let (size, height) = lyric_metrics(lines.len());

    rsx! {
        div {
            id: SHOT_CARD_ID,
            class: "rs-player-shotcard",
            style: "background:{card_background(theme)}",
            if theme == ScreenshotShotTheme::Cover {
                if let Some(url) = cover_url.clone() {
                    img {
                        class: "rs-player-shotcard__cover",
                        style: "filter:blur({blur}px);-webkit-filter:blur({blur}px)",
                        src: "{url}",
                        alt: "",
                    }
                }
            }
            div { class: "rs-player-shotcard__sheen", style: "background:{card_sheen(theme)}" }
            div { class: "rs-player-shotcard__body",
                div { class: "rs-player-shotcard__head",
                    if let Some(url) = cover_url.clone() {
                        img { class: "rs-player-shotcard__thumb", src: "{url}", alt: "{title}" }
                    } else {
                        div {
                            class: "rs-player-shotcard__thumb rs-player-shotcard__thumb--blank",
                            style: "background:{colors.fallback_cover_bg};color:{colors.fallback_cover_fg}",
                            Icon { name: "music".to_string(), class: "rs-ic rs-ic--lg".to_string() }
                        }
                    }
                    div { class: "rs-player-shotcard__meta",
                        p { class: "rs-player-shotcard__title", style: "color:{colors.primary}", "{title}" }
                        if let Some(artist) = artist {
                            p { class: "rs-player-shotcard__artist", style: "color:{colors.secondary}", "{artist}" }
                        }
                    }
                }
                div { class: "rs-player-shotcard__lines",
                    if lines.is_empty() {
                        p { style: "color:{colors.footer_primary};font-size:1.25rem;font-weight:600", "Lyrics unavailable." }
                    } else {
                        for (i, line) in lines.iter().enumerate() {
                            p {
                                key: "{i}",
                                class: "rs-player-shotcard__line",
                                style: "font-size:{size * scale:.3}rem;line-height:{height:.3};color:{colors.lyrics}",
                                "{line}"
                            }
                        }
                    }
                }
                div { class: "rs-player-shotcard__foot",
                    img { class: "rs-player-shotcard__mark", src: RUSTYSOUND_MARK, alt: "RustySound" }
                    div {
                        p { class: "rs-player-shotcard__brand", style: "color:{colors.footer_primary}", "RustySound" }
                        p { class: "rs-player-shotcard__tag", style: "color:{colors.footer_secondary}", "Shared lyrics" }
                    }
                }
            }
        }
    }
}

#[component]
fn ThemeChips(style: ShotStyle, has_cover: bool, on_pick: EventHandler<()>) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let active = active_theme((style.theme)(), has_cover);
    rsx! {
        div { class: "rs-player-chips", role: "group", aria_label: "Background theme",
            for (theme, label, swatch) in THEMES {
                button {
                    key: "{label}",
                    r#type: "button",
                    class: "rs-player-chip",
                    aria_pressed: if active == theme { "true" } else { "false" },
                    onclick: move |_| {
                        choose_theme(style, settings, theme);
                        on_pick.call(());
                    },
                    span { class: "rs-player-chip__dot", style: "background:{swatch}" }
                    "{label}"
                }
            }
            if has_cover {
                button {
                    r#type: "button",
                    class: "rs-player-chip",
                    aria_pressed: if active == ScreenshotShotTheme::Cover { "true" } else { "false" },
                    onclick: move |_| {
                        choose_theme(style, settings, ScreenshotShotTheme::Cover);
                        on_pick.call(());
                    },
                    span { class: "rs-player-chip__dot rs-player-chip__dot--cover" }
                    "Cover"
                }
            }
        }
    }
}

#[component]
pub(super) fn ThemePicker(style: ShotStyle, has_cover: bool, on_pick: EventHandler<()>) -> Element {
    rsx! {
        div { class: "rs-player-shotpanel", onclick: move |evt| evt.stop_propagation(),
            p { class: "rs-player-shotpanel__title", "Background theme" }
            ThemeChips { style, has_cover, on_pick }
        }
    }
}

#[component]
pub(super) fn CustomizePanel(style: ShotStyle, has_cover: bool) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let (mut font_scale, mut blur, mut palette) = (style.font_scale, style.blur, style.palette);
    let scale_value = font_scale().clamp(80, 130);
    let blur_value = blur().clamp(8, 48);
    let active_palette = palette();

    rsx! {
        div { class: "rs-player-shotpanel rs-player-shotpanel--left", onclick: move |evt| evt.stop_propagation(),
            div { class: "rs-player-shotpanel__row",
                p { class: "rs-player-shotpanel__title", "Customize shot" }
                button {
                    r#type: "button",
                    class: "rs-player-chip",
                    onclick: move |_| {
                        font_scale.set(100);
                        blur.set(28);
                        palette.set(ScreenshotTextPalette::Auto);
                        super::shot::choose_theme(style, settings, ScreenshotShotTheme::Cover);
                    },
                    "Reset"
                }
            }
            label { class: "rs-player-shotpanel__label", r#for: "rs-shot-size", "Lyrics size · {scale_value}%" }
            input {
                id: "rs-shot-size",
                class: "rs-player-range",
                r#type: "range",
                min: "80",
                max: "130",
                step: "5",
                value: "{scale_value}",
                oninput: move |evt| {
                    if let Ok(value) = evt.value().parse::<i32>() {
                        font_scale.set(value.clamp(80, 130));
                    }
                },
            }
            label { class: "rs-player-shotpanel__label", r#for: "rs-shot-blur", "Album blur · {blur_value}px" }
            input {
                id: "rs-shot-blur",
                class: "rs-player-range",
                r#type: "range",
                min: "8",
                max: "48",
                step: "2",
                value: "{blur_value}",
                oninput: move |evt| {
                    if let Ok(value) = evt.value().parse::<i32>() {
                        blur.set(value.clamp(8, 48));
                    }
                },
            }
            p { class: "rs-player-shotpanel__label", "Text colour" }
            div { class: "rs-player-chips", role: "group", aria_label: "Text colour",
                for (value, label, swatch) in PALETTES {
                    button {
                        key: "{label}",
                        r#type: "button",
                        class: "rs-player-chip",
                        aria_pressed: if active_palette == value { "true" } else { "false" },
                        onclick: move |_| palette.set(value),
                        span { class: "rs-player-chip__dot", style: "background:{swatch}" }
                        "{label}"
                    }
                }
            }
            p { class: "rs-player-shotpanel__label", "Background" }
            ThemeChips { style, has_cover, on_pick: move |_| {} }
        }
    }
}
