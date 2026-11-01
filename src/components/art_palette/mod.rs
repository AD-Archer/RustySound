//! Colour from album art: while a song plays, its cover's palette tints the
//! player (Now Playing's wash, the progress fills) and the playing row.
//!
//! * `colour`  the maths: median cut, WCAG contrast, lightness adjustment
//! * `cache`   palettes per cover, in memory and on disk
//! * `sample.js` reads the cover at 32×32 through a canvas
//!
//! Once per cover: the webview downsamples the image it already shows (the
//! same URL `Artwork` uses, so it is usually cached), Rust works out the
//! palette (on a thread on native), and the shell writes five colours onto
//! `<html>` (`--art-g/t/s/l/d`) with `data-art="on"`. `views/ambient.css`
//! turns those into `--art-accent`, `--art-accent-stage`, `--art-glow` and
//! `--art-tint` and cross-fades them. Nothing runs per frame or per render.
//! With the setting off, or for covers that can't be read, the attribute is
//! removed and every tint falls back to the theme accent.

mod cache;
mod colour;

use colour::ArtPalette;

use crate::api::{ServerConfig, Song};
use crate::components::ui::env::cover_art_src;
use crate::components::use_setting;
use dioxus::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

const SAMPLE_JS: &str = include_str!("sample.js");
/// Cover size to sample: the mini player's (40px at 2×), so the browser
/// usually has it already. Colours don't need more.
const SAMPLE_PX: u32 = 80;

/// Keeps the app's art colours in step with the playing song. Mounted once
/// by the shell.
pub fn use_art_palette() {
    let now_playing = use_context::<Signal<Option<Song>>>();
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let enabled = use_setting(|s| s.art_colors);
    // Re-runs on song changes, but only changes when the cover does.
    let cover = use_memo(move || {
        let song = now_playing.read();
        let song = song.as_ref()?;
        let art = song.cover_art.clone().filter(|art| !art.trim().is_empty())?;
        Some((song.server_id.clone(), art))
    });
    // The newest request; older ones finishing late are dropped.
    let latest = use_hook(|| Rc::new(Cell::new(0u64)));
    // Native: read the cache file on a thread before the first song needs it.
    #[cfg(not(target_arch = "wasm32"))]
    use_hook(|| {
        std::thread::spawn(cache::warm);
    });

    use_effect(move || {
        let (cover, on) = (cover(), enabled());
        let ticket = latest.get() + 1;
        latest.set(ticket);
        let Some((server_id, art)) = cover.filter(|_| on) else {
            apply(None);
            return;
        };
        let key = cache::palette_key(&server_id, &art);
        if let Some(palette) = cache::lookup(&key) {
            apply(Some(palette));
            return;
        }
        let url = cover_art_src(&servers.peek(), &server_id, Some(&art), SAMPLE_PX);
        let latest = latest.clone();
        spawn(async move {
            let palette = match url {
                Some(url) => match sample(&url).await {
                    Some(pixels) => compute(key, pixels).await,
                    None => None,
                },
                None => None,
            };
            // Keep the previous colours while a newer cover is on its way.
            if latest.get() == ticket {
                apply(palette);
            }
        });
    });
}

/// The cover's opaque pixels as RGB bytes, or None if it can't be read.
async fn sample(url: &str) -> Option<Vec<u8>> {
    let url = serde_json::to_string(url).ok()?;
    let mut eval = document::eval(&format!(
        "{SAMPLE_JS}\ndioxus.send(await window.__rsArtSample({url}));"
    ));
    eval.recv::<Option<Vec<u8>>>().await.ok().flatten().filter(|px| !px.is_empty())
}

/// Works out the palette and remembers it. Native builds do it on a thread
/// (with the cache file write); the web build does it inline, which for
/// 1024 pixels is well under a millisecond.
async fn compute(key: String, pixels: Vec<u8>) -> Option<ArtPalette> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let palette = ArtPalette::from_rgb_bytes(&pixels);
            let _ = tx.send(palette);
            if let Some(palette) = palette {
                cache::persist(cache::remember(key, palette));
            }
        });
        rx.await.ok().flatten()
    }
    #[cfg(target_arch = "wasm32")]
    {
        let palette = ArtPalette::from_rgb_bytes(&pixels)?;
        cache::persist(cache::remember(key, palette));
        Some(palette)
    }
}

/// Writes the palette onto `<html>`, or clears it.
fn apply(palette: Option<ArtPalette>) {
    let js = match palette {
        Some(p) => format!(
            "(function(){{var r=document.documentElement,s=r.style;s.setProperty('--art-g','{}');s.setProperty('--art-t','{}');s.setProperty('--art-s','{}');s.setProperty('--art-l','{}');s.setProperty('--art-d','{}');r.setAttribute('data-art','on');}})();",
            p.glow.hex(),
            p.tint.hex(),
            p.stage.hex(),
            p.light.hex(),
            p.dark.hex()
        ),
        None => "(function(){var r=document.documentElement,s=r.style;['--art-g','--art-t','--art-s','--art-l','--art-d'].forEach(function(k){s.removeProperty(k);});r.removeAttribute('data-art');})();".to_string(),
    };
    let _ = document::eval(&js);
}
