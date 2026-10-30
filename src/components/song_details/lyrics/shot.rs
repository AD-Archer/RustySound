//! Lyrics Shot: pick up to five lines, style a square card, then save or
//! share it as an image.

use super::shot_card::{CustomizePanel, ShotCard, ThemePicker};
use super::shot_share::{
    lyrics_share_delay_ms, screenshot_bar_label, screenshot_lyrics_bars,
    screenshot_share_file_name, screenshot_share_intent_key, screenshot_share_status_message,
    share_screenshot_lyrics_image, ScreenshotShareIntent, ScreenshotShotTheme,
    ScreenshotTextPalette,
};
use crate::api::{LyricsResult, ServerConfig, Song};
use crate::components::ui::env::cover_art_url;
use crate::components::ui::{IconButton, IconButtonSize, Button, ButtonVariant};
use crate::components::AudioState;
use crate::db::AppSettings;
use dioxus::prelude::*;

const MAX_LINES: usize = 5;
pub(super) const SHOT_CARD_ID: &str = "rs-lyrics-shot-card";
const SHOT_LIST_ID: &str = "rs-lyrics-shot-lines";

/// Shot styling state, shared with the card and the panels.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ShotStyle {
    pub theme: Signal<ScreenshotShotTheme>,
    pub palette: Signal<ScreenshotTextPalette>,
    pub font_scale: Signal<i32>,
    pub blur: Signal<i32>,
}

pub(super) fn theme_from_setting(raw: &str) -> ScreenshotShotTheme {
    match raw {
        "lagoon" => ScreenshotShotTheme::Lagoon,
        "ember" => ScreenshotShotTheme::Ember,
        "midnight" => ScreenshotShotTheme::Midnight,
        _ => ScreenshotShotTheme::Cover,
    }
}

pub(super) fn theme_setting(theme: ScreenshotShotTheme) -> &'static str {
    match theme {
        ScreenshotShotTheme::Lagoon => "lagoon",
        ScreenshotShotTheme::Ember => "ember",
        ScreenshotShotTheme::Midnight => "midnight",
        ScreenshotShotTheme::Cover => "cover",
    }
}

/// Sets the shot background and remembers it as the default.
pub(super) fn choose_theme(style: ShotStyle, mut settings: Signal<AppSettings>, theme: ScreenshotShotTheme) {
    let mut theme_signal = style.theme;
    theme_signal.set(theme);
    settings.with_mut(|s| s.lyrics_default_theme = theme_setting(theme).to_string());
    let snapshot = settings.peek().clone();
    spawn(async move {
        let _ = crate::db::save_settings(snapshot).await;
    });
}

fn scroll_to_line(index: usize) {
    let script = format!(
        r#"(function(){{
  const c = document.getElementById("{SHOT_LIST_ID}");
  const l = document.getElementById("{SHOT_LIST_ID}-{index}");
  if (!c || !l) return;
  const cr = c.getBoundingClientRect(), lr = l.getBoundingClientRect();
  c.scrollTo({{ top: c.scrollTop + (lr.top - cr.top) - cr.height / 2 + lr.height / 2, behavior: "auto" }});
}})();"#
    );
    let _ = document::eval(&script);
}

#[component]
pub(super) fn LyricsShot(song: Song, lyrics: LyricsResult, sync: bool, on_close: EventHandler<()>) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let audio = use_context::<Signal<AudioState>>();
    let live = super::is_live_song(&song);

    let bars = use_hook(|| screenshot_lyrics_bars(&lyrics, sync));
    let synced_stamps: Vec<f64> = use_hook(|| {
        if sync {
            lyrics.synced_lines.iter().map(|l| l.timestamp_seconds).collect()
        } else {
            Vec::new()
        }
    });
    let clock = use_hook(move || audio.peek().current_time);
    // The active synced line, recomputed per tick but only notifying on change.
    let active = use_memo({
        let stamps = synced_stamps.clone();
        move || {
            if stamps.is_empty() || live {
                return None;
            }
            let offset = settings.read().lyrics_offset_ms as f64 / 1000.0;
            let at = clock() + offset;
            stamps.partition_point(|s| *s <= at).checked_sub(1)
        }
    });

    let mut start = use_signal(|| active.peek().unwrap_or(0));
    let mut count = use_signal(|| 1usize);
    let mut manual = use_signal(|| false);
    let mut shot_mode = use_signal(|| false);
    let mut customize_open = use_signal(|| false);
    let mut theme_open = use_signal(|| false);
    let mut feedback = use_signal(|| None::<String>);
    let mut feedback_generation = use_signal(|| 0u64);
    let mut pending = use_signal(|| false);
    let style = ShotStyle {
        theme: use_signal(|| theme_from_setting(&settings.peek().lyrics_default_theme)),
        palette: use_signal(|| ScreenshotTextPalette::Auto),
        font_scale: use_signal(|| 100),
        blur: use_signal(|| 28),
    };

    // Follow the playing line until the user picks lines themselves.
    use_effect(move || {
        let Some(index) = active() else {
            return;
        };
        if *manual.peek() {
            return;
        }
        if *start.peek() != index {
            start.set(index);
        }
        if *count.peek() != 1 {
            count.set(1);
        }
    });
    // Keep the first selected line in view while browsing.
    use_effect(move || {
        let first = start();
        if !shot_mode() {
            scroll_to_line(first);
        }
    });

    let total = bars.len();
    let first = start().min(total.saturating_sub(1));
    let selected_count = count().clamp(1, MAX_LINES).min(total.saturating_sub(first)).max(usize::from(total > 0));
    let last = first + selected_count.saturating_sub(1);
    let selected: Vec<String> = bars
        .iter()
        .skip(first)
        .take(selected_count)
        .map(|bar| bar.text.clone())
        .collect();
    let show_stamps = settings.read().lyrics_screenshot_timestamps;
    let cover_url = cover_art_url(&servers.read(), &song.server_id, song.cover_art.as_deref(), 450);
    let title = if song.title.trim().is_empty() { "Unknown Song".to_string() } else { song.title.clone() };
    let artist = song.artist.clone().filter(|a| !a.trim().is_empty());

    let share = {
        let selected = selected.clone();
        let title = title.clone();
        move |intent: ScreenshotShareIntent| {
            if *pending.peek() {
                return;
            }
            if selected.iter().all(|line| line.trim().is_empty()) {
                feedback.set(Some("Select one or more lyric lines to share.".to_string()));
                return;
            }
            pending.set(true);
            feedback_generation += 1;
            let generation = *feedback_generation.peek();
            let file_name = screenshot_share_file_name(&title);
            spawn(async move {
                let status = share_screenshot_lyrics_image(
                    SHOT_CARD_ID.to_string(),
                    file_name,
                    screenshot_share_intent_key(intent).to_string(),
                )
                .await;
                pending.set(false);
                feedback.set(Some(screenshot_share_status_message(&status, intent)));
                lyrics_share_delay_ms(2600).await;
                if *feedback_generation.peek() == generation {
                    feedback.set(None);
                }
            });
        }
    };

    let on_backdrop = move |_| {
        if theme_open() {
            theme_open.set(false);
        } else if customize_open() {
            customize_open.set(false);
        } else if shot_mode() {
            shot_mode.set(false);
        } else {
            on_close.call(());
        }
    };

    rsx! {
        div {
            class: "rs-player-shot",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Lyrics Shot",
            tabindex: "-1",
            "data-testid": "lyrics-shot",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    evt.prevent_default();
                    on_close.call(());
                }
            },
            if let Some(url) = cover_url.clone() {
                img {
                    class: "rs-player-shot__backdrop",
                    style: "filter:blur({(style.blur)()}px)",
                    src: "{url}",
                    alt: "",
                    aria_hidden: "true",
                }
            }
            div { class: "rs-player-shot__veil", onclick: on_backdrop }

            div { class: "rs-player-shot__bar",
                if shot_mode() {
                    Button {
                        label: "Lines".to_string(),
                        icon: "chevron-left".to_string(),
                        variant: ButtonVariant::Tinted,
                        onclick: move |_| {
                            shot_mode.set(false);
                            customize_open.set(false);
                        },
                    }
                    Button {
                        label: "Customize".to_string(),
                        variant: ButtonVariant::Tinted,
                        onclick: move |_| {
                            customize_open.toggle();
                            theme_open.set(false);
                        },
                    }
                } else {
                    Button {
                        label: "Shot".to_string(),
                        icon: "eye".to_string(),
                        disabled: selected.is_empty(),
                        testid: "lyrics-shot-make".to_string(),
                        onclick: move |_| {
                            shot_mode.set(true);
                            theme_open.set(false);
                        },
                    }
                }
                span { class: "rs-player-shot__spacer" }
                if shot_mode() {
                    Button {
                        label: if pending() { "Saving…".to_string() } else { "Save".to_string() },
                        variant: ButtonVariant::Filled,
                        disabled: selected.is_empty() || pending(),
                        onclick: {
                            let mut share = share.clone();
                            move |_| share(ScreenshotShareIntent::Save)
                        },
                    }
                    Button {
                        label: "Share".to_string(),
                        variant: ButtonVariant::Tinted,
                        disabled: selected.is_empty() || pending(),
                        onclick: {
                            let mut share = share.clone();
                            move |_| share(ScreenshotShareIntent::Social)
                        },
                    }
                }
                IconButton {
                    icon: "swatch".to_string(),
                    label: "Background theme".to_string(),
                    pressed: theme_open(),
                    size: IconButtonSize::Small,
                    onclick: move |_| {
                        theme_open.toggle();
                        customize_open.set(false);
                    },
                }
                IconButton {
                    icon: "x".to_string(),
                    label: "Close Lyrics Shot".to_string(),
                    size: IconButtonSize::Small,
                    onclick: move |_| on_close.call(()),
                }
            }

            if let Some(message) = feedback() {
                div { class: "rs-player-shot__toast", role: "status", "{message}" }
            }
            if theme_open() {
                ThemePicker { style, has_cover: cover_url.is_some(), on_pick: move |_| theme_open.set(false) }
            }
            if shot_mode() && customize_open() {
                CustomizePanel { style, has_cover: cover_url.is_some() }
            }

            if shot_mode() {
                div { class: "rs-player-shot__stage",
                    ShotCard {
                        style,
                        lines: selected.clone(),
                        title: title.clone(),
                        artist: artist.clone(),
                        cover_url: cover_url.clone(),
                    }
                }
            } else {
                div { class: "rs-player-shot__browse",
                    h2 { class: "rs-player-shot__title", "{title}" }
                    if let Some(artist) = artist.clone() {
                        p { class: "rs-player-shot__artist", "{artist}" }
                    }
                    p { class: "rs-player-shot__hint",
                        "Tap a line to start, then tap a later one to extend (up to {MAX_LINES} lines)."
                    }
                    ol { id: SHOT_LIST_ID, class: "rs-player-shot__lines", aria_label: "Lyric lines",
                        for (index, bar) in bars.iter().enumerate() {
                            li { key: "{index}",
                                button {
                                    r#type: "button",
                                    id: "{SHOT_LIST_ID}-{index}",
                                    class: "rs-player-shot__line",
                                    class: if index >= first && index <= last { "rs-player-shot__line--on" },
                                    aria_pressed: if index >= first && index <= last { "true" } else { "false" },
                                    onclick: move |_| {
                                        let (first_now, last_now) = (first, last);
                                        if *manual.peek() && index >= first_now && index <= last_now {
                                            // Tapping inside a manual pick goes back to following.
                                            manual.set(false);
                                            count.set(1);
                                            if let Some(active) = *active.peek() {
                                                start.set(active);
                                            }
                                            return;
                                        }
                                        manual.set(true);
                                        if index >= first_now && index - first_now < MAX_LINES {
                                            count.set(index - first_now + 1);
                                        } else {
                                            start.set(index);
                                            count.set(1);
                                        }
                                    },
                                    "{screenshot_bar_label(bar, show_stamps)}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
