//! The focused view's header strip (compact): small cover, title and artist,
//! play/pause. Tapping it, or dragging it down, goes back to the full player.
//! Always mounted on compact so the script can measure where the big cover
//! shrinks to; hidden and inert while the full player shows.

use super::actions::{is_live_song, use_player};
use super::types::NowPlayingFocus;
use crate::api::Song;
use crate::components::ui::{Artwork, IconButton};
use dioxus::prelude::*;

#[component]
pub(super) fn NowPlayingStrip(song: Song, focused: bool) -> Element {
    let mut player = use_player();
    let NowPlayingFocus(mut focus) = use_context::<NowPlayingFocus>();
    let playing = (player.is_playing)();
    let radio = is_live_song(&song);
    let title = if song.title.trim().is_empty() {
        if radio { "Internet Radio".to_string() } else { "Unknown Song".to_string() }
    } else {
        song.title.clone()
    };
    let artist = if radio {
        song.album.clone().or(song.artist.clone()).unwrap_or_default()
    } else {
        song.artist.clone().unwrap_or_else(|| "Unknown Artist".to_string())
    };

    rsx! {
        div {
            class: "rs-player-np__strip",
            inert: if !focused { Some("true") } else { None },
            "data-testid": "now-playing-strip",
            button {
                r#type: "button",
                class: "rs-player-np__strip-main",
                aria_label: "Show full player",
                "data-testid": "now-playing-collapse",
                onclick: move |_| {
                    focus.set(false);
                    // The strip goes inert; keep keyboard focus in the sheet.
                    let _ = document::eval(
                        "requestAnimationFrame(()=>{const b=document.querySelector('[data-testid=\"now-playing-expand\"]');if(b)b.focus({preventScroll:true});});",
                    );
                },
                div { class: "rs-player-np__strip-art",
                    // Same size as the big cover, so it is the cached image.
                    Artwork {
                        cover_art: song.cover_art.clone(),
                        server_id: song.server_id.clone(),
                        size_px: 320,
                        fill: true,
                        hero: true,
                        placeholder: if radio { "radio".to_string() } else { "music".to_string() },
                    }
                }
                span { class: "rs-player-np__strip-text",
                    span { class: "rs-player-np__strip-title", "{title}" }
                    span { class: "rs-player-np__strip-artist", "{artist}" }
                }
            }
            IconButton {
                icon: if playing { "pause".to_string() } else { "play".to_string() },
                label: if playing { "Pause".to_string() } else { "Play".to_string() },
                testid: "now-playing-strip-play".to_string(),
                icon_class: "rs-ic rs-ic--lg".to_string(),
                class: "rs-player-np__strip-play".to_string(),
                onclick: move |_| player.toggle_play(),
            }
        }
    }
}
