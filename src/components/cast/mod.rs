//! Speakers on the network (Mac and iPhone): the list, picking one, and
//! the cast that follows. `crate::cast` does the protocols; this side
//! keeps the found speakers, and while one plays, puts it behind the audio
//! manager's remote output seam (`set_remote_output`), so the player views,
//! the queue and the controller drive it like the local player.
//!
//! * `host.rs`: one task in the shell's scope: searches (in the background,
//!   cached), connects, carries the player's commands to the speaker and
//!   polls it once a second while casting (never otherwise).
//! * `menu.rs`: the "Speakers" row in Now Playing's "…" menu, the
//!   "Playing on Living Room" labels.
//! * Settings › Playback › Speakers lives with the other settings
//!   (`views/settings/speakers.rs`).
//!
//! The web build can't search a network: the context exists (so views can
//! read it) but stays empty, and nothing renders.

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod host;
mod menu;

pub use menu::{use_playing_on, CastIndicator, SpeakersMenu};

use dioxus::prelude::*;

/// Whether this build casts: the Mac app and the iPhone app.
pub const CAST_AVAILABLE: bool = cfg!(any(all(feature = "desktop", target_os = "macos"), target_os = "ios"));

/// What the local choice is called.
pub const THIS_DEVICE: &str = if cfg!(target_os = "ios") {
    "This iPhone"
} else if cfg!(target_os = "macos") {
    "This Mac"
} else {
    "This Device"
};

/// A found speaker, as the views show it.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeakerInfo {
    pub id: String,
    pub name: String,
    /// "Sonos One · Sonos", "Chromecast".
    pub detail: String,
}

/// The speaker playing now, or being connected to.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveSpeaker {
    pub id: String,
    pub name: String,
    pub connecting: bool,
}

/// What the views ask of the host.
#[derive(Clone, Debug, PartialEq)]
pub enum CastRequest {
    /// Search now, or (`fresh_only`) only if the list is older than a minute.
    Search { fresh_only: bool },
    /// Play on this speaker; `None` back on this device.
    Cast(Option<String>),
    /// Settings: network discovery on or off.
    SetDiscovery(bool),
}

/// Context handle, provided by the shell (`use_cast_provider`).
#[derive(Clone, Copy, PartialEq)]
pub struct CastCtx {
    pub speakers: Signal<Vec<SpeakerInfo>>,
    pub searching: Signal<bool>,
    /// A search has finished (so "No speakers found" means it).
    pub searched: Signal<bool>,
    pub active: Signal<Option<ActiveSpeaker>>,
    /// Network discovery (Settings; saved).
    pub discovery: Signal<bool>,
    requests: Option<Coroutine<CastRequest>>,
}

impl CastCtx {
    pub fn send(&self, request: CastRequest) {
        if let Some(requests) = self.requests {
            requests.send(request);
        }
    }

    /// The speaker's name while one plays (not while connecting).
    pub fn playing_on(&self) -> Option<String> {
        self.active
            .read()
            .as_ref()
            .filter(|active| !active.connecting)
            .map(|active| active.name.clone())
    }
}

/// Saved with the settings values (`db::save_json_value`).
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CastPrefs {
    pub discovery: bool,
    /// The user has searched once (so the local network permission has
    /// been asked): later launches may search in the background.
    pub searched: bool,
}

impl Default for CastPrefs {
    fn default() -> Self {
        CastPrefs { discovery: true, searched: false }
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub const PREFS_KEY: &str = "cast_prefs";

/// Creates the speakers context and, where casting works, the host task.
/// Call once from the shell, after the playback contexts.
pub fn use_cast_provider() -> CastCtx {
    let speakers = use_signal(Vec::new);
    let searching = use_signal(|| false);
    let searched = use_signal(|| false);
    let active = use_signal(|| None);
    let discovery = use_signal(|| true);
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    let requests = CAST_AVAILABLE.then(|| {
        let ctx = CastCtx { speakers, searching, searched, active, discovery, requests: None };
        host::use_cast_host(ctx)
    });
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let requests = None;
    use_context_provider(|| CastCtx { speakers, searching, searched, active, discovery, requests })
}
