//! The Mac menu bar item (a status item through dioxus-desktop's tray icon):
//! a template music-note glyph whose menu shows the current song, with
//! Play/Pause, Next, Previous and "Show RustySound".
//!
//! The menu follows playback through an effect on the now-playing and
//! is-playing signals, so it changes when they do and costs nothing in
//! between. The transport items press the player bar's own buttons
//! (`press_player_button`), like the Playback menu.
//!
//! Menu events: muda keeps the first event handler it is given, and
//! dioxus-desktop installs its menu bar receiver before its tray receiver, so
//! tray menu clicks arrive as muda menu events. Both hooks are listened to,
//! in case that order ever changes.

/// Longest song line before it is cut with an ellipsis.
const TITLE_CHARS: usize = 48;
/// The glyph's pixel size (18 pt at 2x; tray-icon draws it 18 pt tall).
const GLYPH_PX: u32 = 36;

// The status item itself (the pure helpers below also build for tests).
#[cfg(all(feature = "desktop", target_os = "macos"))]
pub use tray::use_menu_bar_controls;

#[cfg(all(feature = "desktop", target_os = "macos"))]
mod tray {
    use crate::api::Song;
    use crate::components::press_player_button;
    use dioxus::desktop::trayicon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use dioxus::desktop::trayicon::{Icon, TrayIcon, TrayIconBuilder};
    use dioxus::desktop::{use_muda_event_handler, use_tray_menu_event_handler};
    use super::{note_glyph, song_line, GLYPH_PX};
    use dioxus::prelude::*;
    use std::rc::Rc;

    const ID_NOW: &str = "rustysound-tray-now";
    const ID_PLAY_PAUSE: &str = "rustysound-tray-play-pause";
    const ID_NEXT: &str = "rustysound-tray-next";
    const ID_PREVIOUS: &str = "rustysound-tray-previous";
    const ID_SHOW: &str = "rustysound-tray-show";

    struct MenuBarItem {
        tray: TrayIcon,
        now: MenuItem,
        play_pause: MenuItem,
        next: MenuItem,
        previous: MenuItem,
    }

    /// Puts the item in the menu bar and keeps it in step with playback. Call
    /// once, from the shell.
    pub fn use_menu_bar_controls(now_playing: Signal<Option<Song>>, is_playing: Signal<bool>) {
        let item = use_hook(|| build().map(Rc::new));

        use_effect(move || {
            let song = now_playing.read();
            let playing = is_playing();
            let Some(item) = item.as_ref() else {
                return;
            };
            let line = song
                .as_ref()
                .map(|s| song_line(&s.title, s.artist.as_deref().unwrap_or_default()));
            item.now.set_text(line.clone().unwrap_or_else(|| "Not Playing".to_string()));
            item.play_pause.set_text(if playing { "Pause" } else { "Play" });
            for control in [&item.play_pause, &item.next, &item.previous] {
                control.set_enabled(song.is_some());
            }
            let _ = item.tray.set_tooltip(Some(line.unwrap_or_else(|| "RustySound".to_string())));
        });

        use_muda_event_handler(handle);
        use_tray_menu_event_handler(handle);
    }

    fn handle(event: &MenuEvent) {
        match event.id().as_ref() {
            ID_PLAY_PAUSE => press_player_button("play-pause-btn"),
            ID_NEXT => press_player_button("next-btn"),
            ID_PREVIOUS => press_player_button("prev-btn"),
            ID_SHOW => {
                let window = &dioxus::desktop::window().window;
                window.set_visible(true);
                window.set_minimized(false);
                window.set_focus();
            }
            _ => {}
        }
    }

    fn build() -> Option<MenuBarItem> {
        let now = MenuItem::with_id(ID_NOW, "Not Playing", false, None);
        let play_pause = MenuItem::with_id(ID_PLAY_PAUSE, "Play", false, None);
        let next = MenuItem::with_id(ID_NEXT, "Next", false, None);
        let previous = MenuItem::with_id(ID_PREVIOUS, "Previous", false, None);
        let show = MenuItem::with_id(ID_SHOW, "Show RustySound", true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &now,
            &PredefinedMenuItem::separator(),
            &play_pause,
            &next,
            &previous,
            &PredefinedMenuItem::separator(),
            &show,
            &PredefinedMenuItem::quit(Some("Quit RustySound")),
        ])
        .ok()?;
        let icon = Icon::from_rgba(note_glyph(GLYPH_PX), GLYPH_PX, GLYPH_PX).ok()?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip("RustySound")
            .build()
            .ok()?;
        Some(MenuBarItem {
            tray,
            now,
            play_pause,
            next,
            previous,
        })
    }
}

/// "Title — Artist", cut to fit a menu.
fn song_line(title: &str, artist: &str) -> String {
    let title = if title.trim().is_empty() { "Unknown Song" } else { title.trim() };
    let line = match artist.trim() {
        "" => title.to_string(),
        artist => format!("{title} — {artist}"),
    };
    if line.chars().count() <= TITLE_CHARS {
        return line;
    }
    let cut: String = line.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

/// A beamed pair of eighth notes, black on transparent (a template image:
/// macOS tints it for the menu bar). Drawn with 4x4 supersampling.
fn note_glyph(size: u32) -> Vec<u8> {
    let inside = |x: f64, y: f64| {
        // Note heads: tilted ellipses.
        let head = |cx: f64, cy: f64| {
            let (dx, dy) = (x - cx, y - cy);
            let (sin, cos) = (-0.35f64).sin_cos();
            let (u, v) = (dx * cos - dy * sin, dx * sin + dy * cos);
            (u / 0.15).powi(2) + (v / 0.105).powi(2) <= 1.0
        };
        let stem = |left: f64, top: f64, bottom: f64| x >= left && x <= left + 0.06 && y >= top && y <= bottom;
        let beam_top = 0.16 - 0.2 * (x - 0.395);
        let beam = x >= 0.395 && x <= 0.915 && y >= beam_top && y <= beam_top + 0.12;
        head(0.30, 0.78) || head(0.76, 0.69) || stem(0.395, 0.16, 0.76) || stem(0.855, 0.07, 0.67) || beam
    };
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (px as f64 + (sx as f64 + 0.5) / 4.0) / size as f64;
                    let y = (py as f64 + (sy as f64 + 0.5) / 4.0) / size as f64;
                    if inside(x, y) {
                        hits += 1;
                    }
                }
            }
            rgba.extend_from_slice(&[0, 0, 0, (hits * 255 / 16) as u8]);
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_lines_are_short() {
        assert_eq!(song_line("Blue", "Joni Mitchell"), "Blue — Joni Mitchell");
        assert_eq!(song_line(" Blue ", " "), "Blue");
        assert_eq!(song_line("", "Joni Mitchell"), "Unknown Song — Joni Mitchell");
        let long = song_line(&"a".repeat(80), "Artist");
        assert_eq!(long.chars().count(), TITLE_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn the_glyph_is_a_template_image() {
        let glyph = note_glyph(GLYPH_PX);
        assert_eq!(glyph.len(), (GLYPH_PX * GLYPH_PX * 4) as usize);
        // Black only; the corners are clear and the note is drawn.
        assert!(glyph.chunks(4).all(|px| px[..3] == [0, 0, 0]));
        assert_eq!(glyph[3], 0);
        assert!(glyph.chunks(4).filter(|px| px[3] == 255).count() > 100);
    }
}
