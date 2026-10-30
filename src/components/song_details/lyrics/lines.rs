//! The lyric lines: big synced lines (active white, others dimmed) with
//! auto-scroll and tap-to-seek, or plain text.
//!
//! Perf: the active line is a memo over the playback clock. The memo re-runs
//! on each tick (a binary search), but the list re-renders only when the
//! active line changes.

use crate::api::LyricLine;
use crate::components::song_details::actions::use_player;
use crate::components::ui::env::sleep_ms;
use crate::components::AudioState;
use crate::db::AppSettings;
use dioxus::prelude::*;
use std::rc::Rc;

/// After a manual scroll, auto-scroll waits this long.
const MANUAL_HOLD_MS: f64 = 1800.0;
/// Scroll events within this window after our own scroll are ignored.
const PROGRAMMATIC_MS: f64 = 700.0;

fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

/// Scrolls line `index` of list `dom_id` to 40% of the list's height.
fn center_line(dom_id: &str, index: usize, instant: bool) {
    let id = serde_json::to_string(dom_id).unwrap_or_default();
    let script = format!(
        r#"(function(){{
  const c = document.getElementById({id});
  const l = document.getElementById({id} + "-line-{index}");
  if (!c || !l) return;
  const cr = c.getBoundingClientRect(), lr = l.getBoundingClientRect();
  const top = c.scrollTop + (lr.top - cr.top) - cr.height * 0.4 + lr.height / 2;
  const still = {instant} || matchMedia("(prefers-reduced-motion: reduce)").matches;
  c.scrollTo({{ top, behavior: still ? "auto" : "smooth" }});
}})();"#
    );
    let _ = document::eval(&script);
}

/// Synced lines. Mount with a `key` per lyrics result: the line timestamps
/// are captured once.
#[component]
pub(super) fn SyncedLyrics(lines: Vec<LyricLine>, live: bool, dom_id: String) -> Element {
    let audio = use_context::<Signal<AudioState>>();
    let settings = use_context::<Signal<AppSettings>>();
    let player = use_player();
    let clock = use_hook(move || audio.peek().current_time);
    let stamps: Rc<Vec<f64>> =
        use_hook(|| Rc::new(lines.iter().map(|line| line.timestamp_seconds).collect()));

    let active = use_memo({
        let stamps = stamps.clone();
        move || {
            if live {
                return None;
            }
            let offset = settings.read().lyrics_offset_ms as f64 / 1000.0;
            let at = clock() + offset;
            stamps.partition_point(|stamp| *stamp <= at).checked_sub(1)
        }
    });

    let mut hold_until = use_signal(|| 0.0f64);
    let mut programmatic_until = use_signal(|| 0.0f64);
    let mut scroll_generation = use_signal(|| 0u64);
    let mut centered_once = use_signal(|| false);

    {
        let dom_id = dom_id.clone();
        use_effect(move || {
            let Some(index) = active() else {
                return;
            };
            if now_ms() < *hold_until.peek() {
                return;
            }
            let instant = !*centered_once.peek();
            centered_once.set(true);
            programmatic_until.set(now_ms() + PROGRAMMATIC_MS);
            center_line(&dom_id, index, instant);
        });
    }

    let on_scroll = {
        let dom_id = dom_id.clone();
        move |_| {
            let now = now_ms();
            if now < *programmatic_until.peek() {
                return;
            }
            hold_until.set(now + MANUAL_HOLD_MS);
            scroll_generation += 1;
            let generation = *scroll_generation.peek();
            let dom_id = dom_id.clone();
            spawn(async move {
                sleep_ms(MANUAL_HOLD_MS as u64 + 50).await;
                if *scroll_generation.peek() != generation {
                    return;
                }
                if let Some(index) = *active.peek() {
                    programmatic_until.set(now_ms() + PROGRAMMATIC_MS);
                    center_line(&dom_id, index, false);
                }
            });
        }
    };

    let current = active();
    rsx! {
        ol {
            id: "{dom_id}",
            class: "rs-player-lyrics__lines",
            aria_label: "Synced lyrics",
            onscroll: on_scroll,
            for (index, line) in lines.iter().enumerate() {
                li { key: "{index}",
                    button {
                        r#type: "button",
                        id: "{dom_id}-line-{index}",
                        class: "rs-player-lyric",
                        class: if Some(index) == current { "rs-player-lyric--now" },
                        class: if current.map(|c| index < c).unwrap_or(false) { "rs-player-lyric--past" },
                        aria_current: if Some(index) == current { Some("true") } else { None },
                        disabled: live,
                        onclick: {
                            let stamp = line.timestamp_seconds;
                            let mut player = player;
                            move |_| {
                                let offset = settings.peek().lyrics_offset_ms as f64 / 1000.0;
                                player.seek(stamp - offset);
                            }
                        },
                        if line.text.trim().is_empty() {
                            span { aria_label: "Instrumental", "\u{266A}" }
                        } else {
                            "{line.text}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn PlainLyrics(lines: Vec<String>) -> Element {
    rsx! {
        div { class: "rs-player-lyrics__lines rs-player-lyrics__lines--plain",
            for (index, line) in lines.iter().enumerate() {
                p { key: "{index}", class: "rs-player-lyric rs-player-lyric--plain", "{line}" }
            }
        }
    }
}
