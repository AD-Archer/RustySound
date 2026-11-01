//! SSDP search (UPnP discovery): an M-SEARCH sent to the multicast group,
//! and the unicast replies collected for a short window. Each reply names
//! a device description URL (`LOCATION`), which `upnp.rs` reads.
//!
//! iPhone: sending to a multicast group needs the
//! `com.apple.developer.networking.multicast` entitlement (granted by Apple
//! on request) on top of the local network permission. Without it the send
//! fails and the search finds nothing.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;

/// The SSDP multicast group and port.
pub const MULTICAST: &str = "239.255.255.250:1900";

/// What to search for: MediaRenderers, and Sonos players (ZonePlayer,
/// which some firmware answers instead of its embedded MediaRenderer).
pub const SEARCH_TARGETS: [&str; 2] = [
    "urn:schemas-upnp-org:device:MediaRenderer:1",
    "urn:schemas-upnp-org:device:ZonePlayer:1",
];

/// One reply to an M-SEARCH.
#[derive(Clone, Debug, PartialEq)]
pub struct SsdpResponse {
    /// The device description URL.
    pub location: String,
    /// `uuid:…::urn:…`; the part before `::` identifies the device.
    pub usn: String,
    pub st: String,
    pub server: String,
}

impl SsdpResponse {
    /// The device's UDN (`uuid:…`), from the USN.
    pub fn udn(&self) -> &str {
        self.usn.split("::").next().unwrap_or(&self.usn)
    }
}

/// The M-SEARCH request. `mx`: how many seconds devices may wait before
/// answering (they spread their replies over it).
pub fn m_search(target: &str, mx: u8) -> String {
    format!(
        "M-SEARCH * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         MAN: \"ssdp:discover\"\r\n\
         MX: {mx}\r\n\
         ST: {target}\r\n\
         USER-AGENT: RustySound/3 UPnP/1.1\r\n\
         \r\n"
    )
}

/// Parses one reply: a `HTTP/1.1 200 OK` status line and headers (names
/// in any case). Notifies, other searches and replies without a location
/// are not replies to us.
pub fn parse_response(raw: &str) -> Option<SsdpResponse> {
    let mut lines = raw.split("\r\n").flat_map(|line| line.split('\n'));
    let status = lines.next()?.trim();
    if !status.to_ascii_uppercase().starts_with("HTTP/1.") || !status.contains(" 200") {
        return None;
    }
    let mut response = SsdpResponse {
        location: String::new(),
        usn: String::new(),
        st: String::new(),
        server: String::new(),
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().to_string();
        match name.trim().to_ascii_lowercase().as_str() {
            "location" => response.location = value,
            "usn" => response.usn = value,
            "st" => response.st = value,
            "server" => response.server = value,
            _ => {}
        }
    }
    let location = response.location.to_ascii_lowercase();
    if !(location.starts_with("http://") || location.starts_with("https://")) {
        return None;
    }
    Some(response)
}

/// Sends an M-SEARCH for each target to `to` (the multicast group, or a
/// test's loopback responder) and collects replies for `window`, one per
/// description URL.
pub async fn search(to: SocketAddr, targets: &[&str], window: Duration) -> std::io::Result<Vec<SsdpResponse>> {
    let bind: SocketAddr = if to.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().unwrap();
    let socket = UdpSocket::bind(bind).await?;
    if to.ip().is_multicast() {
        let _ = socket.set_multicast_ttl_v4(2);
    }
    let mx = window.as_secs().clamp(1, 5) as u8;
    // Twice: UDP gets lost, and a sleepy device may miss the first one.
    for _ in 0..2 {
        for target in targets {
            socket.send_to(m_search(target, mx).as_bytes(), to).await?;
        }
    }
    let mut found: Vec<SsdpResponse> = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    let mut buf = vec![0u8; 4096];
    loop {
        let received = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await;
        let Ok(Ok((len, _))) = received else {
            break;
        };
        let Some(response) = parse_response(&String::from_utf8_lossy(&buf[..len])) else {
            continue;
        };
        if !found.iter().any(|seen| seen.location == response.location) {
            found.push(response);
        }
    }
    Ok(found)
}
