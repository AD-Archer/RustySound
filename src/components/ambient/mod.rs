//! Ambient mode: a full-screen, distraction-free "now playing" for a desk,
//! a TV or a bedside. A big cover, the title and artist, a slow drifting
//! colour field from the album palette (`--art-glow` / `--art-tint`), and,
//! if wanted, the time of day and the lyric line being sung.
//!
//! The field is three soft gradients moved by CSS transforms over a minute
//! or more (views/ambient.css): no canvas, no per-frame work, and still
//! under reduced motion. The controls fade after 3 s without input and come
//! back on any pointer move, tap or key (`ambient.js`, which also closes on
//! Escape or a swipe down and keeps the screen awake).
//!
//! ```ignore
//! AmbientMode { song, lines, on_close: move |_| open.set(false) }
//! ```

use crate::api::{LyricLine, Song};
use crate::components::song_details::use_active_line;
use crate::components::ui::{Artwork, IconButton, IconButtonSize};
use crate::components::{update_settings, use_player, use_setting};
use crate::db::AppSettings;
use dioxus::prelude::*;
use std::rc::Rc;

const AMBIENT_JS: &str = include_str!("ambient.js");

/// The ambient screen for `song`.
/// * `lines`: synced lyrics for the optional lyric line (empty: no lyrics,
///   and no lyrics switch). Pass a new `lyrics_key` with each result.
/// * `on_close`: Escape, a swipe down or the close button.
#[component]
pub fn AmbientMode(
    song: Song,
    #[props(default)] lines: Vec<LyricLine>,
    #[props(default)] lyrics_key: String,
    on_close: EventHandler<()>,
) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let show_clock = use_setting(|s| s.ambient_clock);
    let show_lyrics = use_setting(|s| s.ambient_lyrics);
    let mut player = use_player();
    let playing = (player.is_playing)();
    let mut time = use_signal(String::new);

    use_hook(move || {
        let mut script = document::eval(AMBIENT_JS);
        spawn(async move {
            while let Ok(message) = script.recv::<String>().await {
                if message == "close" {
                    on_close.call(());
                } else if let Some(text) = message.strip_prefix("time:") {
                    time.set(text.to_string());
                }
            }
        });
        keep_screen_awake(true);
    });
    use_drop(|| {
        let _ = document::eval("window.__rsAmbient && window.__rsAmbient.stop();");
        keep_screen_awake(false);
    });

    let has_lyrics = !lines.is_empty();
    let title = if song.title.trim().is_empty() { "Unknown Song".to_string() } else { song.title.clone() };
    let artist = song.artist.clone().or(song.album.clone()).unwrap_or_default();

    rsx! {
        div {
            id: "rs-ambient",
            class: "rs-ambient",
            role: "dialog",
            aria_modal: "true",
            aria_label: "Ambient Mode",
            tabindex: "-1",
            "data-testid": "ambient",
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            div { class: "rs-ambient__field", aria_hidden: "true", "data-testid": "ambient-field",
                div { class: "rs-ambient__blob rs-ambient__blob--a" }
                div { class: "rs-ambient__blob rs-ambient__blob--b" }
                div { class: "rs-ambient__blob rs-ambient__blob--c" }
            }
            if show_clock() {
                p { class: "rs-ambient__clock", "data-testid": "ambient-clock", "{time}" }
            }
            div { class: "rs-ambient__body",
                div { class: "rs-ambient__cover",
                    Artwork {
                        cover_art: song.cover_art.clone(),
                        server_id: song.server_id.clone(),
                        size_px: 320,
                        fill: true,
                        hero: true,
                        eager: true,
                    }
                }
                div { class: "rs-ambient__text",
                    h2 { class: "rs-ambient__title", "{title}" }
                    if !artist.is_empty() {
                        p { class: "rs-ambient__artist", "{artist}" }
                    }
                    if has_lyrics && show_lyrics() {
                        AmbientLyric { key: "{lyrics_key}", lines: lines.clone() }
                    }
                }
            }
            div { class: "rs-ambient__chrome",
                IconButton {
                    icon: "clock".to_string(),
                    label: "Show Clock".to_string(),
                    pressed: show_clock(),
                    testid: "ambient-clock-toggle".to_string(),
                    onclick: move |_| {
                        update_settings(settings, |s| s.ambient_clock = !s.ambient_clock);
                    },
                }
                if has_lyrics {
                    IconButton {
                        icon: "lyrics".to_string(),
                        label: "Show Lyrics".to_string(),
                        pressed: show_lyrics(),
                        testid: "ambient-lyrics-toggle".to_string(),
                        onclick: move |_| {
                            update_settings(settings, |s| s.ambient_lyrics = !s.ambient_lyrics);
                        },
                    }
                }
                IconButton {
                    icon: "x".to_string(),
                    label: "Exit Ambient Mode".to_string(),
                    testid: "ambient-close".to_string(),
                    onclick: move |_| on_close.call(()),
                }
            }
            div { class: "rs-ambient__transport",
                IconButton {
                    icon: if playing { "pause".to_string() } else { "play".to_string() },
                    label: if playing { "Pause".to_string() } else { "Play".to_string() },
                    size: IconButtonSize::Large,
                    icon_class: "rs-ic rs-ic--xl".to_string(),
                    testid: "ambient-play-pause".to_string(),
                    onclick: move |_| player.toggle_play(),
                }
            }
        }
    }
}

/// The line being sung, fading in as it changes. Keyed per lyrics result:
/// the timestamps are read once (`use_active_line`, the Lyrics panel's own).
#[component]
fn AmbientLyric(lines: Vec<LyricLine>) -> Element {
    let stamps: Rc<Vec<f64>> = use_hook(|| Rc::new(lines.iter().map(|line| line.timestamp_seconds).collect()));
    let active = use_active_line(stamps, false);
    // Nothing before the first line; a note for an instrumental break.
    let current = active().and_then(|index| Some((index, lines.get(index)?.text.trim().to_string())));
    rsx! {
        p { class: "rs-ambient__lyric", "data-testid": "ambient-lyric",
            for (index, text) in current {
                span { key: "{index}", class: "rs-ambient__line",
                    if text.is_empty() { "\u{266A}" } else { "{text}" }
                }
            }
        }
    }
}

/// iPhone: keep the screen on while ambient mode shows (UIKit's idle timer;
/// the page's Wake Lock request covers the web).
#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn keep_screen_awake(on: bool) {
    use objc::runtime::{Object, NO, YES};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: *mut Object = msg_send![class!(UIApplication), sharedApplication];
        if !app.is_null() {
            let _: () = msg_send![app, setIdleTimerDisabled: if on { YES } else { NO }];
        }
    }
}

#[cfg(not(all(not(target_arch = "wasm32"), target_os = "ios")))]
fn keep_screen_awake(_on: bool) {}
