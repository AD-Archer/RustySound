//! Sends the equalizer setting (`crate::equalizer`) to the player. Both audio
//! controllers call `use_equalizer_sync`.
//!
//! - Mac and web: the audio deck (`audio_deck.js`) builds a Web Audio graph,
//!   one `BiquadFilterNode` per band of `EQ_BANDS`, the first time the EQ is
//!   turned on. A stream Web Audio can't read (cross-origin without CORS)
//!   plays without it; `bypass_note` says so.
//! - iPhone: an `MTAudioProcessingTap` on each AVPlayerItem runs
//!   `EqProcessor` (`native_ios_eq.rs`).
//! - Windows: no equalizer yet.

use crate::db::AppSettings;
use crate::equalizer::{EqSettings, EQ_BANDS, EQ_BAND_Q};
use dioxus::prelude::*;

/// Whether this build's player has an equalizer.
pub const EQ_AVAILABLE: bool = !cfg!(all(not(target_arch = "wasm32"), target_os = "windows"));

/// The "eq" command: on/off, pre-gain and every band with its frequency and
/// Q, so the deck needs no table of its own.
#[cfg_attr(all(not(target_arch = "wasm32"), target_os = "ios"), allow(dead_code))]
pub fn eq_command(eq: &EqSettings) -> serde_json::Value {
    let bands: Vec<serde_json::Value> = EQ_BANDS
        .iter()
        .zip(eq.gains())
        .map(|(band, gain)| serde_json::json!({ "freq": band.freq, "q": EQ_BAND_Q, "gain": gain }))
        .collect();
    serde_json::json!({
        "type": "eq",
        "enabled": eq.enabled,
        "preamp_db": eq.preamp(),
        "bands": bands,
    })
}

#[cfg(target_arch = "wasm32")]
fn send(eq: &EqSettings) {
    let payload = eq_command(eq).to_string();
    if let Ok(value) = js_sys::JSON::parse(&payload) {
        let _ = super::web_deck_call("setEq", &[value]);
    }
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
fn send(eq: &EqSettings) {
    crate::equalizer::live::publish(eq);
    super::native_audio_command(serde_json::json!({ "type": "eq" }));
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn send(eq: &EqSettings) {
    super::native_audio_command(eq_command(eq));
}

/// Applies the equalizer setting whenever it changes. A setting that's never
/// been turned on sends nothing, so the Web Audio graph is only built for
/// people who use the EQ.
pub fn use_equalizer_sync(settings: Signal<AppSettings>) {
    let mut applied = use_signal(|| None::<EqSettings>);
    use_effect(move || {
        let eq = settings.read().equalizer.clone();
        if applied.peek().as_ref() == Some(&eq) {
            return;
        }
        let first = applied.peek().is_none();
        applied.set(Some(eq.clone()));
        if first && !eq.enabled {
            return;
        }
        send(&eq);
    });
}

/// Why the EQ isn't shaping what's playing right now, when it's on but can't
/// be: a stream Web Audio may not read, a browser without Web Audio, or audio
/// that waits for a click to start. `None` when all is well.
pub async fn bypass_note() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let value = super::web_deck_call("eqStatus", &[])?;
        let note = js_sys::Reflect::get(&value, &"note".into()).ok()?;
        note.as_string().filter(|n| !n.is_empty())
    }
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios"), not(target_os = "windows")))]
    {
        let mut eval = document::eval(
            r#"const b = window.__rustysoundAudioBridge;
            const s = b && b.deck && b.deck.eqStatus ? b.deck.eqStatus() : null;
            return (s && s.note) || "";"#,
        );
        eval.recv::<String>().await.ok().filter(|n| !n.is_empty())
    }
    #[cfg(all(not(target_arch = "wasm32"), any(target_os = "ios", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equalizer::EqPreset;

    #[test]
    fn command_carries_the_band_table() {
        let mut eq = EqSettings { enabled: true, ..EqSettings::default() };
        eq.set_preset(EqPreset::BassBoost);
        let cmd = eq_command(&eq);
        assert_eq!(cmd["type"], "eq");
        assert_eq!(cmd["enabled"], true);
        let bands = cmd["bands"].as_array().unwrap();
        assert_eq!(bands.len(), EQ_BANDS.len());
        assert_eq!(bands[0]["freq"], 31.0);
        assert_eq!(bands[0]["gain"], 6.0);
        assert!(cmd["preamp_db"].as_f64().unwrap() < 0.0);
    }
}
