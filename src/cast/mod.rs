//! Casting to network speakers (Mac and iPhone): finding them, and one
//! client per protocol to play a stream on them. No UI and no Dioxus here;
//! `components::cast` puts a speaker behind the audio manager's remote
//! output seam, so the player and the queue don't know they're casting.
//!
//! * `ssdp.rs` + `upnp.rs`: UPnP/DLNA MediaRenderers, Sonos included
//!   (SSDP search, the device description, AVTransport and RenderingControl
//!   over SOAP, DIDL-Lite metadata).
//! * `mdns.rs` + `chromecast.rs`: Google Cast (a one-shot mDNS query for
//!   `_googlecast._tcp`, then Cast v2 over TLS with the Default Media
//!   Receiver).
//! * `discovery.rs`: both searches at once, merged into one speaker list.
//! * `renderer.rs`: one handle over both clients.
//! * `session.rs`: a cast in progress: commands in, the speaker's state out
//!   (position, play state, the end of a song).
//! * `stream.rs`: the URL and format a speaker can fetch and play.
//! * `lan_server.rs`: songs stored on this device, served to the speaker
//!   over the local network while it plays them.
//!
//! The browser can't do any of this (no UDP), so the module is native only.

mod chromecast;
mod discovery;
mod lan_server;
mod mdns;
mod renderer;
mod session;
mod ssdp;
mod stream;
mod upnp;

pub use discovery::{discover, DiscoveryOptions};
pub use renderer::Renderer;
pub use session::{CastSession, RemoteState, SessionCommand};
pub use stream::cast_media;

#[cfg(test)]
mod tests;

/// A speaker found on the network.
#[derive(Clone, Debug, PartialEq)]
pub struct Speaker {
    /// Stable across searches: the UPnP UDN or the Cast device id.
    pub id: String,
    /// What people call it: "Living Room".
    pub name: String,
    /// "Sonos One", "Chromecast Audio"; may be empty.
    pub model: String,
    pub endpoint: Endpoint,
}

/// How to reach a speaker.
#[derive(Clone, Debug, PartialEq)]
pub enum Endpoint {
    Upnp(upnp::Device),
    Cast(std::net::SocketAddr),
}

impl Speaker {
    /// The speaker's address, for choosing the local interface that faces it.
    pub fn ip(&self) -> Option<std::net::IpAddr> {
        match &self.endpoint {
            Endpoint::Upnp(device) => upnp::host_ip(&device.location),
            Endpoint::Cast(addr) => Some(addr.ip()),
        }
    }

    /// "Sonos", "Chromecast" or "DLNA": a quiet hint under the name.
    pub fn kind_label(&self) -> &'static str {
        match &self.endpoint {
            Endpoint::Upnp(device) if device.is_sonos() => "Sonos",
            Endpoint::Upnp(_) => "DLNA",
            Endpoint::Cast(_) => "Chromecast",
        }
    }
}

/// What a speaker is told to play: a URL it can fetch, and the labels and
/// cover it shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CastTrack {
    /// The app's song id, reported back while it plays.
    pub song_id: String,
    pub url: String,
    /// `audio/mpeg`, `audio/flac`…
    pub mime: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: Option<String>,
    /// Seconds; 0 when unknown.
    pub duration: f64,
    /// A radio stream: no end, no seeking.
    pub live: bool,
}

/// The speaker's transport, in the terms both protocols share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Playing,
    Paused,
    /// Loading or buffering: counts as playing.
    Buffering,
    /// Stopped, or nothing loaded.
    Stopped,
}

/// One reading of a speaker's state.
#[derive(Clone, Debug, PartialEq)]
pub struct RendererStatus {
    pub transport: Transport,
    /// Seconds into the song.
    pub position: f64,
    /// Seconds; 0 when the speaker doesn't say.
    pub duration: f64,
    /// The speaker said the song played to its end (Cast's `FINISHED`).
    /// UPnP has no such signal; `session.rs` infers it.
    pub finished: bool,
}

/// Errors are plain strings: they end up in a toast, and nothing branches on them.
pub type CastResult<T> = Result<T, String>;
