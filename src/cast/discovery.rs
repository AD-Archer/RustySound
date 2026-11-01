//! The one speaker search: SSDP (then each device's description) and the
//! Cast mDNS query run side by side for a couple of seconds, and their
//! answers become one list. Callers cache it (`components::cast`); nothing
//! here runs in the background on its own.

use super::upnp::{fetch_device, lan_http};
use super::{mdns, ssdp, Endpoint, Speaker};
use futures_util::future::join_all;
use std::net::SocketAddr;
use std::time::Duration;

/// Where the searches go and how long they listen. Tests point them at a
/// loopback responder; `mdns_to: None` skips Cast.
#[derive(Clone, Debug)]
pub struct DiscoveryOptions {
    pub ssdp_to: Option<SocketAddr>,
    pub mdns_to: Option<SocketAddr>,
    pub window: Duration,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        DiscoveryOptions {
            ssdp_to: ssdp::MULTICAST.parse().ok(),
            mdns_to: mdns::MULTICAST.parse().ok(),
            window: Duration::from_millis(2500),
        }
    }
}

async fn upnp_speakers(to: SocketAddr, window: Duration) -> Vec<Speaker> {
    let Ok(responses) = ssdp::search(to, &ssdp::SEARCH_TARGETS, window).await else {
        return Vec::new();
    };
    let http = lan_http();
    let devices = join_all(responses.iter().map(|response| fetch_device(&http, &response.location))).await;
    devices
        .into_iter()
        .zip(responses.iter())
        .filter_map(|(device, response)| {
            let device = device.ok()?;
            let id = Some(device.udn.clone())
                .filter(|udn| !udn.is_empty())
                .unwrap_or_else(|| response.udn().to_string());
            Some(Speaker {
                id,
                name: device.display_name(),
                model: device.model_name.clone(),
                endpoint: Endpoint::Upnp(device),
            })
        })
        .collect()
}

async fn cast_speakers(to: SocketAddr, window: Duration) -> Vec<Speaker> {
    let Ok(services) = mdns::query(to, mdns::CAST_SERVICE, window).await else {
        return Vec::new();
    };
    services
        .into_iter()
        .map(|service| Speaker {
            id: format!("cast:{}", service.id),
            name: service.name,
            model: service.model,
            endpoint: Endpoint::Cast(service.addr),
        })
        .collect()
}

/// One entry per speaker, by name. A Sonos home-theatre set answers once
/// per box under one room name; the Sub and the surrounds can't take a
/// stream, so the room keeps its main player.
pub fn merge_speakers(found: Vec<Speaker>) -> Vec<Speaker> {
    let mut merged: Vec<Speaker> = Vec::new();
    for speaker in found {
        if speaker.name.trim().is_empty() || merged.iter().any(|seen| seen.id == speaker.id) {
            continue;
        }
        let satellite = |s: &Speaker| s.model.to_ascii_lowercase().contains("sub");
        if let Some(index) = merged
            .iter()
            .position(|seen| seen.kind_label() == "Sonos" && speaker.kind_label() == "Sonos" && seen.name == speaker.name)
        {
            if satellite(&merged[index]) && !satellite(&speaker) {
                merged[index] = speaker;
            }
            continue;
        }
        merged.push(speaker);
    }
    merged.sort_by_key(|speaker| speaker.name.to_lowercase());
    merged
}

/// Searches once and returns what answered, sorted by name.
pub async fn discover(options: DiscoveryOptions) -> Vec<Speaker> {
    let upnp = async {
        match options.ssdp_to {
            Some(to) => upnp_speakers(to, options.window).await,
            None => Vec::new(),
        }
    };
    let cast = async {
        match options.mdns_to {
            Some(to) => cast_speakers(to, options.window).await,
            None => Vec::new(),
        }
    };
    let (mut found, cast) = futures_util::join!(upnp, cast);
    found.extend(cast);
    merge_speakers(found)
}
