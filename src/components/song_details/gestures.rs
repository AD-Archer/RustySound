//! Compact Now Playing gestures: swipe the panel into its focused view and
//! back, and swipe the big cover to change song.
//!
//! The finger tracking is JavaScript (`gestures.js`), like the edge swipe, so
//! it runs in the iOS WKWebView too and does no Rust work per move. Rust only
//! acts on what the script posts: "focus" / "unfocus" set the focused view,
//! and "next:<token>" / "prev:<token>" step one song, but only while the
//! token still names the playing entry, so a gesture can never move two.

use super::actions::{is_live_song, song_key, use_player};
use super::types::NowPlayingFocus;
use dioxus::prelude::*;

const GESTURES_JS: &str = concat!(include_str!("../ui/gesture_core.js"), include_str!("gestures.js"));

/// Names the queue entry a cover swipe starts on: `index|server::id`.
pub(super) fn song_token(index: usize, key: &str) -> String {
    format!("{index}|{key}")
}

/// Installs the script once and handles its messages. Call from the
/// always-mounted overlay root.
pub(super) fn use_now_playing_gestures(focus: NowPlayingFocus) {
    let player = use_player();
    use_hook(move || {
        let mut script = document::eval(GESTURES_JS);
        spawn(async move {
            let NowPlayingFocus(mut focused) = focus;
            while let Ok(message) = script.recv::<String>().await {
                match message.as_str() {
                    "focus" => focused.set(true),
                    "unfocus" => focused.set(false),
                    other => {
                        let (delta, token) = match other.split_once(':') {
                            Some(("next", token)) => (1, token),
                            Some(("prev", token)) => (-1, token),
                            _ => continue,
                        };
                        let current = player.now_playing.peek().clone();
                        let Some(song) = current else {
                            continue;
                        };
                        let index = *player.queue_index.peek();
                        if is_live_song(&song) || song_token(index, &song_key(&song)) != token {
                            continue;
                        }
                        let mut player = player;
                        player.step(delta);
                    }
                }
            }
        });
    });
}

/// The compact sheet is in the DOM: the script measures it.
pub(super) fn sheet_mounted() {
    let _ = document::eval("window.__rsNpGestures && window.__rsNpGestures.mounted();");
}
