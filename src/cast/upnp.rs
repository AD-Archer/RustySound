//! UPnP/DLNA MediaRenderer client (Sonos included): the device
//! description, SOAP calls to AVTransport (what plays) and RenderingControl
//! (volume), and the DIDL-Lite metadata that names the song on the
//! speaker's own display and app.
//!
//! The builders and parsers are pure, for the tests; `UpnpRenderer` does
//! the HTTP.

use super::{CastResult, CastTrack, RendererStatus, Transport};
use std::collections::HashMap;
use std::time::Duration;

/// One UPnP service a device offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    /// `urn:schemas-upnp-org:service:AVTransport:1`: the SOAP action and
    /// namespace use it exactly.
    pub service_type: String,
    /// Absolute URL.
    pub control_url: String,
}

/// A MediaRenderer, from its description.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    /// The description URL (from SSDP).
    pub location: String,
    pub udn: String,
    pub friendly_name: String,
    /// Sonos names players by room (`roomName`).
    pub room_name: Option<String>,
    pub manufacturer: String,
    pub model_name: String,
    pub av_transport: Service,
    pub rendering_control: Option<Service>,
    pub connection_manager: Option<Service>,
}

impl Device {
    pub fn is_sonos(&self) -> bool {
        self.manufacturer.to_ascii_lowercase().contains("sonos")
    }

    /// The name to show: the room for Sonos, else the friendly name.
    pub fn display_name(&self) -> String {
        self.room_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| self.friendly_name.clone())
            .trim()
            .to_string()
    }
}

/// The IP in a URL's host, if it's an address (descriptions are).
pub fn host_ip(url: &str) -> Option<std::net::IpAddr> {
    let url = reqwest::Url::parse(url).ok()?;
    let host = url.host_str()?;
    host.trim_start_matches('[').trim_end_matches(']').parse().ok()
}

fn resolve_url(base: &str, href: &str) -> Option<String> {
    let base = reqwest::Url::parse(base).ok()?;
    base.join(href.trim()).ok().map(|url| url.to_string())
}

fn child_text<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(str::trim)
}

/// Reads a device description. The renderer is whichever device in the
/// tree has an AVTransport service (the root for most renderers; an
/// embedded MediaRenderer on Sonos). Control URLs are made absolute
/// against `URLBase` or the description's own URL.
pub fn parse_description(xml: &str, location: &str) -> Option<Device> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let root = doc.root_element();
    let base = root
        .children()
        .find(|node| node.tag_name().name() == "URLBase")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(location)
        .to_string();
    let root_device = root.children().find(|node| node.tag_name().name() == "device")?;

    let services_of = |device: roxmltree::Node| -> Vec<Service> {
        device
            .children()
            .filter(|node| node.tag_name().name() == "serviceList")
            .flat_map(|list| list.children().filter(|node| node.tag_name().name() == "service"))
            .filter_map(|service| {
                let service_type = child_text(service, "serviceType")?.to_string();
                let control_url = resolve_url(&base, child_text(service, "controlURL")?)?;
                Some(Service { service_type, control_url })
            })
            .collect()
    };

    let renderer = root_device
        .descendants()
        .filter(|node| node.tag_name().name() == "device")
        .find(|device| {
            services_of(*device)
                .iter()
                .any(|service| service.service_type.contains(":AVTransport:"))
        })?;
    let services = services_of(renderer);
    let find = |kind: &str| services.iter().find(|service| service.service_type.contains(kind)).cloned();

    let text = |device: roxmltree::Node, name: &str| child_text(device, name).unwrap_or("").to_string();
    let friendly_name = Some(text(renderer, "friendlyName"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| text(root_device, "friendlyName"));
    let room_name = child_text(root_device, "roomName")
        .or_else(|| child_text(renderer, "roomName"))
        .map(str::to_string);
    let manufacturer = Some(text(renderer, "manufacturer"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| text(root_device, "manufacturer"));
    let model_name = Some(text(root_device, "modelName"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| text(renderer, "modelName"));
    let udn = Some(text(root_device, "UDN"))
        .filter(|udn| !udn.is_empty())
        .unwrap_or_else(|| text(renderer, "UDN"));

    Some(Device {
        location: location.to_string(),
        udn,
        friendly_name,
        room_name,
        manufacturer,
        model_name,
        av_transport: find(":AVTransport:")?,
        rendering_control: find(":RenderingControl:"),
        connection_manager: find(":ConnectionManager:"),
    })
}

/// Escapes text for an XML element or attribute.
pub fn xml_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters aren't allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The SOAP request body for `action` on `service_type`. Argument values
/// are escaped here (so DIDL-Lite metadata is escaped a second time, as
/// the spec wants: it travels as text).
pub fn soap_envelope(service_type: &str, action: &str, args: &[(&str, String)]) -> String {
    let mut body = String::new();
    for (name, value) in args {
        body.push_str(&format!("<{name}>{}</{name}>", xml_escape(value)));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
         s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
         <s:Body><u:{action} xmlns:u=\"{service_type}\">{body}</u:{action}></s:Body>\
         </s:Envelope>"
    )
}

/// The `SOAPACTION` header for an action.
pub fn soap_action(service_type: &str, action: &str) -> String {
    format!("\"{service_type}#{action}\"")
}

/// The out-arguments of a SOAP reply, or the fault's description.
pub fn parse_soap_response(xml: &str, action: &str) -> CastResult<HashMap<String, String>> {
    let doc = roxmltree::Document::parse(xml).map_err(|err| format!("Bad reply: {err}"))?;
    let reply_name = format!("{action}Response");
    if let Some(reply) = doc.descendants().find(|node| node.tag_name().name() == reply_name) {
        return Ok(reply
            .children()
            .filter(|node| node.is_element())
            .map(|node| (node.tag_name().name().to_string(), node.text().unwrap_or("").to_string()))
            .collect());
    }
    let find = |name: &str| {
        doc.descendants()
            .find(|node| node.tag_name().name() == name)
            .and_then(|node| node.text())
            .map(str::trim)
            .map(str::to_string)
    };
    match (find("errorCode"), find("errorDescription"), find("faultstring")) {
        (Some(code), Some(description), _) => Err(format!("{description} ({code})")),
        (Some(code), None, _) => Err(format!("UPnP error {code}")),
        (None, _, Some(fault)) => Err(fault),
        _ => Err(format!("No {reply_name} in the reply")),
    }
}

/// `H:MM:SS` (also `H:MM:SS.mmm` and `H:MM:SS.F0/F1`) to seconds.
/// `NOT_IMPLEMENTED` and garbage read as `None`.
pub fn parse_duration(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let mut parts = raw.split(':');
    let (hours, minutes, seconds) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let seconds = seconds.split('/').next()?;
    let hours: f64 = hours.trim_start_matches('+').parse().ok()?;
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    total.is_finite().then_some(total.max(0.0))
}

/// Seconds to `H:MM:SS`, for Seek and DIDL `duration`.
pub fn format_duration(seconds: f64) -> String {
    let total = if seconds.is_finite() { seconds.max(0.0).round() as u64 } else { 0 };
    format!("{}:{:02}:{:02}", total / 3600, (total / 60) % 60, total % 60)
}

/// AVTransport's `CurrentTransportState`.
pub fn parse_transport_state(raw: &str) -> Transport {
    match raw.trim() {
        "PLAYING" => Transport::Playing,
        "PAUSED_PLAYBACK" | "PAUSED_RECORDING" => Transport::Paused,
        "TRANSITIONING" => Transport::Buffering,
        _ => Transport::Stopped,
    }
}

/// The MIME types in a `GetProtocolInfo` `Sink` list
/// (`http-get:*:audio/flac:*,…`), lowercase.
pub fn parse_sink_protocols(sink: &str) -> Vec<String> {
    sink.split(',')
        .filter_map(|entry| {
            let mut fields = entry.trim().split(':');
            let protocol = fields.next()?;
            let _network = fields.next()?;
            let mime = fields.next()?.trim().to_ascii_lowercase();
            (protocol.eq_ignore_ascii_case("http-get") && !mime.is_empty()).then_some(mime)
        })
        .collect()
}

/// DIDL-Lite for one song: title, artist, album, cover and the resource
/// with its MIME type. The caller escapes it again for the SOAP body.
pub fn didl_lite(track: &CastTrack) -> String {
    let mut item = String::new();
    item.push_str(&format!("<dc:title>{}</dc:title>", xml_escape(&track.title)));
    if !track.artist.is_empty() {
        item.push_str(&format!(
            "<dc:creator>{0}</dc:creator><upnp:artist>{0}</upnp:artist>",
            xml_escape(&track.artist)
        ));
    }
    if !track.album.is_empty() {
        item.push_str(&format!("<upnp:album>{}</upnp:album>", xml_escape(&track.album)));
    }
    if let Some(cover) = track.cover_url.as_ref().filter(|url| !url.is_empty()) {
        item.push_str(&format!("<upnp:albumArtURI>{}</upnp:albumArtURI>", xml_escape(cover)));
    }
    let class = if track.live {
        "object.item.audioItem.audioBroadcast"
    } else {
        "object.item.audioItem.musicTrack"
    };
    item.push_str(&format!("<upnp:class>{class}</upnp:class>"));
    let duration = if track.duration > 0.0 && !track.live {
        format!(" duration=\"{}.000\"", format_duration(track.duration))
    } else {
        String::new()
    };
    item.push_str(&format!(
        "<res protocolInfo=\"http-get:*:{}:*\"{duration}>{}</res>",
        xml_escape(&track.mime),
        xml_escape(&track.url)
    ));
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\" \
         xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\">\
         <item id=\"1\" parentID=\"0\" restricted=\"1\">{item}</item></DIDL-Lite>"
    )
}

/// The URI a Sonos needs for a live stream: plain HTTP radio plays as
/// `x-rincon-mp3radio://` (Sonos treats a bare `http://` as a file with an
/// end). Everything else is sent as it is.
pub fn transport_uri(device: &Device, track: &CastTrack) -> String {
    if device.is_sonos() && track.live {
        if let Some(rest) = track.url.strip_prefix("http://") {
            return format!("x-rincon-mp3radio://{rest}");
        }
    }
    track.url.clone()
}

/// Shared by every LAN request: short timeouts, and no system proxy for
/// devices on the local network.
pub fn lan_http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(6))
        .build()
        .unwrap_or_default()
}

/// Reads a description URL from an SSDP reply.
pub async fn fetch_device(http: &reqwest::Client, location: &str) -> CastResult<Device> {
    let xml = http
        .get(location)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .text()
        .await
        .map_err(|err| err.to_string())?;
    parse_description(&xml, location).ok_or_else(|| "Not a media renderer".to_string())
}

/// A connected MediaRenderer.
pub struct UpnpRenderer {
    pub device: Device,
    http: reqwest::Client,
    /// What the speaker says it plays (`GetProtocolInfo`); empty if it
    /// didn't say.
    sinks: Vec<String>,
}

impl UpnpRenderer {
    /// Asks what the speaker plays (best effort; some don't answer).
    pub async fn connect(device: Device) -> CastResult<Self> {
        let mut renderer = UpnpRenderer {
            device,
            http: lan_http(),
            sinks: Vec::new(),
        };
        if let Some(manager) = renderer.device.connection_manager.clone() {
            if let Ok(reply) = renderer.call(&manager, "GetProtocolInfo", &[]).await {
                renderer.sinks = parse_sink_protocols(reply.get("Sink").map(String::as_str).unwrap_or(""));
            }
        }
        // The transport answers, or this isn't worth showing as connected.
        renderer.transport_info().await?;
        Ok(renderer)
    }

    pub fn sinks(&self) -> &[String] {
        &self.sinks
    }

    async fn call(&self, service: &Service, action: &str, args: &[(&str, String)]) -> CastResult<HashMap<String, String>> {
        let response = self
            .http
            .post(&service.control_url)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPACTION", soap_action(&service.service_type, action))
            .body(soap_envelope(&service.service_type, action, args))
            .send()
            .await
            .map_err(|err| format!("{} didn't answer: {err}", self.device.display_name()))?;
        let text = response.text().await.map_err(|err| err.to_string())?;
        parse_soap_response(&text, action)
    }

    async fn transport(&self, action: &str, extra: &[(&str, String)]) -> CastResult<HashMap<String, String>> {
        let mut args = vec![("InstanceID", "0".to_string())];
        args.extend(extra.iter().cloned());
        self.call(&self.device.av_transport, action, &args).await
    }

    /// Sets the song (with its metadata) and, with `autoplay`, starts it.
    pub async fn load(&self, track: &CastTrack, autoplay: bool) -> CastResult<()> {
        self.transport(
            "SetAVTransportURI",
            &[
                ("CurrentURI", transport_uri(&self.device, track)),
                ("CurrentURIMetaData", didl_lite(track)),
            ],
        )
        .await?;
        if autoplay {
            self.play().await?;
        }
        Ok(())
    }

    pub async fn play(&self) -> CastResult<()> {
        self.transport("Play", &[("Speed", "1".to_string())]).await.map(|_| ())
    }

    pub async fn pause(&self) -> CastResult<()> {
        self.transport("Pause", &[]).await.map(|_| ())
    }

    pub async fn stop(&self) -> CastResult<()> {
        self.transport("Stop", &[]).await.map(|_| ())
    }

    pub async fn seek(&self, seconds: f64) -> CastResult<()> {
        self.transport(
            "Seek",
            &[("Unit", "REL_TIME".to_string()), ("Target", format_duration(seconds))],
        )
        .await
        .map(|_| ())
    }

    /// 0–1, on the speaker's 0–100 scale.
    pub async fn set_volume(&self, level: f64) -> CastResult<()> {
        let Some(control) = self.device.rendering_control.as_ref() else {
            return Ok(());
        };
        let volume = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
        self.call(
            control,
            "SetVolume",
            &[
                ("InstanceID", "0".to_string()),
                ("Channel", "Master".to_string()),
                ("DesiredVolume", volume.to_string()),
            ],
        )
        .await
        .map(|_| ())
    }

    pub async fn volume(&self) -> Option<f64> {
        let control = self.device.rendering_control.as_ref()?;
        let reply = self
            .call(control, "GetVolume", &[("InstanceID", "0".to_string()), ("Channel", "Master".to_string())])
            .await
            .ok()?;
        let volume: f64 = reply.get("CurrentVolume")?.trim().parse().ok()?;
        Some((volume / 100.0).clamp(0.0, 1.0))
    }

    async fn transport_info(&self) -> CastResult<Transport> {
        let reply = self.transport("GetTransportInfo", &[]).await?;
        Ok(parse_transport_state(reply.get("CurrentTransportState").map(String::as_str).unwrap_or("")))
    }

    pub async fn status(&self) -> CastResult<RendererStatus> {
        let transport = self.transport_info().await?;
        let position = self.transport("GetPositionInfo", &[]).await?;
        let read = |name: &str| position.get(name).and_then(|raw| parse_duration(raw));
        Ok(RendererStatus {
            transport,
            position: read("RelTime").unwrap_or(0.0),
            duration: read("TrackDuration").unwrap_or(0.0),
            finished: false,
        })
    }
}
