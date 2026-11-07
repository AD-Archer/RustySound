//! One handle over the protocol clients, so the session doesn't care which
//! kind of speaker it's talking to.

use super::chromecast::CastRenderer;
use super::stream::{cast_plays, sinks_play};
use super::upnp::UpnpRenderer;
use super::{CastResult, CastTrack, Endpoint, RendererStatus, Speaker};

pub enum Renderer {
    Upnp(UpnpRenderer),
    Cast(CastRenderer),
}

impl Renderer {
    pub async fn connect(speaker: &Speaker) -> CastResult<Self> {
        match &speaker.endpoint {
            Endpoint::Upnp(device) => UpnpRenderer::connect(device.clone()).await.map(Renderer::Upnp),
            Endpoint::Cast(addr) => CastRenderer::connect(*addr, &speaker.name).await.map(Renderer::Cast),
        }
    }

    /// Whether the speaker plays this format as it is.
    pub fn plays(&self, mime: &str) -> bool {
        match self {
            Renderer::Upnp(upnp) => sinks_play(upnp.sinks(), mime),
            Renderer::Cast(_) => cast_plays(mime),
        }
    }

    /// Loads a song at `position`, playing it with `autoplay`. UPnP has no
    /// start position, so it seeks once playing (`session.rs` holds the
    /// position of a paused load until Play).
    pub async fn load(&self, track: &CastTrack, position: f64, autoplay: bool) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => {
                upnp.load(track, autoplay).await?;
                if autoplay && position > 1.0 && !track.live {
                    // Some renderers refuse to seek until the stream is open.
                    for attempt in 0..3 {
                        if upnp.seek(position).await.is_ok() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(400 * (attempt + 1))).await;
                    }
                }
                Ok(())
            }
            Renderer::Cast(cast) => cast.load(track, position, autoplay).await,
        }
    }

    pub async fn play(&self) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => upnp.play().await,
            Renderer::Cast(cast) => cast.play().await,
        }
    }

    pub async fn pause(&self) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => upnp.pause().await,
            Renderer::Cast(cast) => cast.pause().await,
        }
    }

    pub async fn stop(&self) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => upnp.stop().await,
            Renderer::Cast(cast) => cast.stop().await,
        }
    }

    pub async fn seek(&self, position: f64) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => upnp.seek(position).await,
            Renderer::Cast(cast) => cast.seek(position).await,
        }
    }

    /// 0–1.
    pub async fn set_volume(&self, level: f64) -> CastResult<()> {
        match self {
            Renderer::Upnp(upnp) => upnp.set_volume(level).await,
            Renderer::Cast(cast) => cast.set_volume(level).await,
        }
    }

    pub async fn volume(&self) -> Option<f64> {
        match self {
            Renderer::Upnp(upnp) => upnp.volume().await,
            Renderer::Cast(cast) => cast.volume(),
        }
    }

    pub async fn status(&self) -> CastResult<RendererStatus> {
        match self {
            Renderer::Upnp(upnp) => upnp.status().await,
            Renderer::Cast(cast) => cast.status().await,
        }
    }
}
