//! Google Cast discovery: a one-shot mDNS query for `_googlecast._tcp`.
//! Sent from an ephemeral port, it's a "legacy unicast" query (RFC 6762
//! §6.7), so devices answer straight back to us and nothing has to join
//! the multicast group or keep a responder running. Each answer carries
//! the device's PTR, SRV (port), TXT (`fn` = its name, `md` = model,
//! `id`) and A records.
//!
//! The same iPhone entitlement note as `ssdp.rs` applies.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;
use tokio::net::UdpSocket;

pub const MULTICAST: &str = "224.0.0.251:5353";
pub const CAST_SERVICE: &str = "_googlecast._tcp.local";

const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_AAAA: u16 = 28;
const TYPE_SRV: u16 = 33;

/// One device that answered.
#[derive(Clone, Debug, PartialEq)]
pub struct CastService {
    /// The TXT `id` (stable), or the instance name.
    pub id: String,
    /// The TXT `fn`: "Living Room speaker".
    pub name: String,
    /// The TXT `md`: "Chromecast Audio", "Google Home".
    pub model: String,
    pub addr: SocketAddr,
}

/// The query packet: one PTR question for `service`, asking for a unicast
/// reply (the QU bit).
pub fn query_packet(service: &str) -> Vec<u8> {
    let mut packet = vec![0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in service.trim_end_matches('.').split('.') {
        packet.push(label.len() as u8);
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
    packet.extend_from_slice(&TYPE_PTR.to_be_bytes());
    packet.extend_from_slice(&0x8001u16.to_be_bytes());
    packet
}

fn read_u16(packet: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*packet.get(at)?, *packet.get(at + 1)?]))
}

/// A (possibly compressed) name at `at`: the dotted name and the offset
/// just past it in the record.
fn read_name(packet: &[u8], at: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut pos = at;
    let mut end = None;
    for _ in 0..64 {
        let len = *packet.get(pos)? as usize;
        if len == 0 {
            return Some((labels.join("."), end.unwrap_or(pos + 1)));
        }
        if len & 0xC0 == 0xC0 {
            let pointer = (read_u16(packet, pos)? & 0x3FFF) as usize;
            end.get_or_insert(pos + 2);
            pos = pointer;
            continue;
        }
        let label = packet.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos += 1 + len;
    }
    None
}

fn txt_entries(data: &[u8]) -> HashMap<String, String> {
    let mut entries = HashMap::new();
    let mut pos = 0;
    while pos < data.len() {
        let len = data[pos] as usize;
        let Some(entry) = data.get(pos + 1..pos + 1 + len) else {
            break;
        };
        let entry = String::from_utf8_lossy(entry);
        if let Some((key, value)) = entry.split_once('=') {
            entries.insert(key.to_ascii_lowercase(), value.to_string());
        }
        pos += 1 + len;
    }
    entries
}

/// Reads the services of type `service` from one response. `from`: the
/// sender, used when the answer has no address record.
pub fn parse_response(packet: &[u8], service: &str, from: IpAddr) -> Vec<CastService> {
    let flags = read_u16(packet, 2).unwrap_or(0);
    if flags & 0x8000 == 0 {
        return Vec::new(); // a query, not a response
    }
    let counts: Vec<usize> = (0..4).filter_map(|i| read_u16(packet, 4 + i * 2).map(usize::from)).collect();
    if counts.len() != 4 {
        return Vec::new();
    }
    let mut pos = 12;
    for _ in 0..counts[0] {
        let Some((_, next)) = read_name(packet, pos) else {
            return Vec::new();
        };
        pos = next + 4;
    }

    let mut ptrs: Vec<(String, String)> = Vec::new();
    let mut srvs: HashMap<String, (String, u16)> = HashMap::new();
    let mut txts: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut addrs: HashMap<String, IpAddr> = HashMap::new();
    for _ in 0..counts[1] + counts[2] + counts[3] {
        let Some((owner, next)) = read_name(packet, pos) else {
            break;
        };
        let (Some(kind), Some(len)) = (read_u16(packet, next), read_u16(packet, next + 8)) else {
            break;
        };
        let data_at = next + 10;
        let Some(data) = packet.get(data_at..data_at + len as usize) else {
            break;
        };
        let owner = owner.to_ascii_lowercase();
        match kind {
            TYPE_PTR => {
                if let Some((target, _)) = read_name(packet, data_at) {
                    ptrs.push((owner, target));
                }
            }
            TYPE_SRV if data.len() >= 6 => {
                let port = u16::from_be_bytes([data[4], data[5]]);
                if let Some((target, _)) = read_name(packet, data_at + 6) {
                    srvs.insert(owner, (target.to_ascii_lowercase(), port));
                }
            }
            TYPE_TXT => {
                txts.insert(owner, txt_entries(data));
            }
            TYPE_A if data.len() == 4 => {
                addrs.insert(owner, IpAddr::V4(Ipv4Addr::new(data[0], data[1], data[2], data[3])));
            }
            TYPE_AAAA if data.len() == 16 => {
                let octets: [u8; 16] = data.try_into().unwrap_or([0; 16]);
                addrs.entry(owner).or_insert(IpAddr::V6(Ipv6Addr::from(octets)));
            }
            _ => {}
        }
        pos = data_at + len as usize;
    }

    let service = service.trim_end_matches('.').to_ascii_lowercase();
    ptrs.into_iter()
        .filter(|(owner, _)| *owner == service)
        .filter_map(|(_, instance)| {
            let key = instance.to_ascii_lowercase();
            let (host, port) = srvs.get(&key).cloned()?;
            let txt = txts.get(&key).cloned().unwrap_or_default();
            let ip = addrs.get(&host).copied().unwrap_or(from);
            let label = instance.split('.').next().unwrap_or(&instance).to_string();
            let name = txt.get("fn").cloned().filter(|name| !name.trim().is_empty()).unwrap_or(label.clone());
            Some(CastService {
                id: txt.get("id").cloned().filter(|id| !id.is_empty()).unwrap_or(label),
                name: name.trim().to_string(),
                model: txt.get("md").cloned().unwrap_or_default(),
                addr: SocketAddr::new(ip, port),
            })
        })
        .collect()
}

/// Asks `to` (the mDNS group, or a test's responder) and collects answers
/// for `window`, one per device.
pub async fn query(to: SocketAddr, service: &str, window: Duration) -> std::io::Result<Vec<CastService>> {
    let socket = UdpSocket::bind::<SocketAddr>("0.0.0.0:0".parse().unwrap()).await?;
    let packet = query_packet(service);
    socket.send_to(&packet, to).await?;
    let mut found: Vec<CastService> = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    let mut resent = false;
    let mut buf = vec![0u8; 9000];
    loop {
        // A second ask halfway through, for devices that missed the first.
        let half = deadline - window / 2;
        let wait_until = if resent { deadline } else { half };
        match tokio::time::timeout_at(wait_until, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, from))) => {
                for service in parse_response(&buf[..len], service, from.ip()) {
                    if !found.iter().any(|seen| seen.id == service.id) {
                        found.push(service);
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) if !resent => {
                resent = true;
                let _ = socket.send_to(&packet, to).await;
            }
            Err(_) => break,
        }
    }
    Ok(found)
}
