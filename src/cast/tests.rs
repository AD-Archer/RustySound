//! Casting tests: the pure builders and parsers, then the whole flow
//! against an in-process fake MediaRenderer (SSDP on loopback plus the
//! AVTransport / RenderingControl SOAP actions, every call recorded).
//! Nothing here talks to a real speaker.

use super::chromecast::{self, CastMessage};
use super::discovery::merge_speakers;
use super::lan_server::{parse_range, parse_request_head, LanServer};
use super::session::{RemoteState, SessionSnapshot};
use super::stream::{cast_plays, mime_for_suffix, normalize_mime, sinks_play, song_mime, transcoded_url, CastMedia, MediaSource};
use super::upnp::{self, didl_lite, format_duration, parse_duration, parse_soap_response, soap_envelope};
use super::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn track(id: &str, url: &str) -> CastTrack {
    CastTrack {
        song_id: id.to_string(),
        url: url.to_string(),
        mime: "audio/mpeg".to_string(),
        title: format!("Song {id}"),
        artist: "Artist".to_string(),
        album: "Album".to_string(),
        cover_url: Some("http://music.test/rest/getCoverArt?id=al-1&size=600".to_string()),
        duration: 200.0,
        live: false,
    }
}

// ---- SSDP ----------------------------------------------------------------

#[test]
fn ssdp_replies_parse_with_any_header_case() {
    let raw = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nlocation: http://192.168.1.20:1400/xml/device_description.xml\r\nServer: Linux UPnP/1.0 Sonos/80.1\r\nST: urn:schemas-upnp-org:device:ZonePlayer:1\r\nUSN: uuid:RINCON_000E58A0123401400::urn:schemas-upnp-org:device:ZonePlayer:1\r\n\r\n";
    let reply = ssdp::parse_response(raw).expect("a reply");
    assert_eq!(reply.location, "http://192.168.1.20:1400/xml/device_description.xml");
    assert_eq!(reply.udn(), "uuid:RINCON_000E58A0123401400");
    assert_eq!(reply.st, "urn:schemas-upnp-org:device:ZonePlayer:1");
    assert!(reply.server.contains("Sonos"));
}

#[test]
fn ssdp_ignores_notifies_searches_and_bad_locations() {
    assert!(ssdp::parse_response("NOTIFY * HTTP/1.1\r\nLOCATION: http://a/\r\n\r\n").is_none());
    assert!(ssdp::parse_response("M-SEARCH * HTTP/1.1\r\nST: ssdp:all\r\n\r\n").is_none());
    assert!(ssdp::parse_response("HTTP/1.1 200 OK\r\nLOCATION: file:///etc/passwd\r\n\r\n").is_none());
    assert!(ssdp::parse_response("HTTP/1.1 404 Not Found\r\nLOCATION: http://a/\r\n\r\n").is_none());
    // Bare newlines happen too.
    assert!(ssdp::parse_response("HTTP/1.1 200 OK\nLOCATION: http://10.0.0.2/d.xml\n\n").is_some());
}

#[test]
fn m_search_asks_for_the_target() {
    let request = ssdp::m_search("urn:schemas-upnp-org:device:MediaRenderer:1", 2);
    assert!(request.starts_with("M-SEARCH * HTTP/1.1\r\n"));
    assert!(request.contains("MAN: \"ssdp:discover\"\r\n"));
    assert!(request.contains("ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n"));
    assert!(request.contains("MX: 2\r\n"));
    assert!(request.ends_with("\r\n\r\n"));
}

// ---- UPnP descriptions and SOAP -------------------------------------------

const SONOS_DESCRIPTION: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <device>
    <deviceType>urn:schemas-upnp-org:device:ZonePlayer:1</deviceType>
    <friendlyName>192.168.1.20 - Sonos One - RINCON_000E58A0123401400</friendlyName>
    <manufacturer>Sonos, Inc.</manufacturer>
    <modelName>Sonos One</modelName>
    <UDN>uuid:RINCON_000E58A0123401400</UDN>
    <roomName>Living Room</roomName>
    <deviceList>
      <device>
        <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
        <friendlyName>Living Room - Sonos One Media Renderer</friendlyName>
        <manufacturer>Sonos, Inc.</manufacturer>
        <UDN>uuid:RINCON_000E58A0123401400_MR</UDN>
        <serviceList>
          <service>
            <serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType>
            <controlURL>/MediaRenderer/RenderingControl/Control</controlURL>
          </service>
          <service>
            <serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>
            <controlURL>/MediaRenderer/ConnectionManager/Control</controlURL>
          </service>
          <service>
            <serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>
            <controlURL>/MediaRenderer/AVTransport/Control</controlURL>
          </service>
        </serviceList>
      </device>
    </deviceList>
  </device>
</root>"#;

#[test]
fn a_sonos_description_finds_the_embedded_renderer_and_the_room() {
    let device = upnp::parse_description(SONOS_DESCRIPTION, "http://192.168.1.20:1400/xml/device_description.xml")
        .expect("a renderer");
    assert_eq!(device.display_name(), "Living Room");
    assert!(device.is_sonos());
    assert_eq!(device.model_name, "Sonos One");
    assert_eq!(device.udn, "uuid:RINCON_000E58A0123401400");
    assert_eq!(device.av_transport.control_url, "http://192.168.1.20:1400/MediaRenderer/AVTransport/Control");
    assert_eq!(device.av_transport.service_type, "urn:schemas-upnp-org:service:AVTransport:1");
    assert_eq!(
        device.rendering_control.as_ref().map(|s| s.control_url.as_str()),
        Some("http://192.168.1.20:1400/MediaRenderer/RenderingControl/Control")
    );
    assert!(device.connection_manager.is_some());
}

#[test]
fn relative_control_urls_resolve_against_url_base() {
    let xml = r#"<root xmlns="urn:schemas-upnp-org:device-1-0"><URLBase>http://10.0.0.5:49152/upnp/</URLBase>
      <device><friendlyName>Kitchen</friendlyName><manufacturer>Acme</manufacturer><UDN>uuid:k</UDN>
      <serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:2</serviceType>
      <controlURL>control/avt</controlURL></service></serviceList></device></root>"#;
    let device = upnp::parse_description(xml, "http://10.0.0.5:49152/desc.xml").expect("a renderer");
    assert_eq!(device.display_name(), "Kitchen");
    assert_eq!(device.av_transport.control_url, "http://10.0.0.5:49152/upnp/control/avt");
    assert!(device.rendering_control.is_none());
    // A device without AVTransport isn't a renderer.
    let server = xml.replace("AVTransport", "ContentDirectory");
    assert!(upnp::parse_description(&server, "http://10.0.0.5/").is_none());
}

#[test]
fn soap_envelopes_escape_arguments_and_name_the_action() {
    let body = soap_envelope(
        "urn:schemas-upnp-org:service:AVTransport:1",
        "SetAVTransportURI",
        &[("InstanceID", "0".to_string()), ("CurrentURI", "http://h/s?a=1&b=<2>".to_string())],
    );
    assert!(body.contains(r#"<u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">"#));
    assert!(body.contains("<InstanceID>0</InstanceID>"));
    assert!(body.contains("<CurrentURI>http://h/s?a=1&amp;b=&lt;2&gt;</CurrentURI>"));
    // Well-formed, and the value reads back unchanged.
    let doc = roxmltree::Document::parse(&body).unwrap();
    let uri = doc.descendants().find(|n| n.tag_name().name() == "CurrentURI").unwrap();
    assert_eq!(uri.text(), Some("http://h/s?a=1&b=<2>"));
    assert_eq!(
        upnp::soap_action("urn:schemas-upnp-org:service:AVTransport:1", "Play"),
        "\"urn:schemas-upnp-org:service:AVTransport:1#Play\""
    );
}

#[test]
fn soap_replies_give_position_and_transport_state() {
    let position = r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
      <u:GetPositionInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <Track>1</Track><TrackDuration>0:03:20</TrackDuration><TrackMetaData></TrackMetaData>
      <TrackURI>http://h/s</TrackURI><RelTime>0:01:05.500</RelTime><AbsTime>NOT_IMPLEMENTED</AbsTime>
      </u:GetPositionInfoResponse></s:Body></s:Envelope>"#;
    let reply = parse_soap_response(position, "GetPositionInfo").unwrap();
    assert_eq!(parse_duration(&reply["RelTime"]), Some(65.5));
    assert_eq!(parse_duration(&reply["TrackDuration"]), Some(200.0));
    assert_eq!(parse_duration(&reply["AbsTime"]), None);

    let transport = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body>
      <u:GetTransportInfoResponse xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">
      <CurrentTransportState>PAUSED_PLAYBACK</CurrentTransportState><CurrentTransportStatus>OK</CurrentTransportStatus>
      </u:GetTransportInfoResponse></s:Body></s:Envelope>"#;
    let reply = parse_soap_response(transport, "GetTransportInfo").unwrap();
    assert_eq!(upnp::parse_transport_state(&reply["CurrentTransportState"]), Transport::Paused);
    assert_eq!(upnp::parse_transport_state("PLAYING"), Transport::Playing);
    assert_eq!(upnp::parse_transport_state("TRANSITIONING"), Transport::Buffering);
    assert_eq!(upnp::parse_transport_state("NO_MEDIA_PRESENT"), Transport::Stopped);
}

#[test]
fn soap_faults_read_as_errors() {
    let fault = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><s:Fault>
      <faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring><detail>
      <UPnPError xmlns="urn:schemas-upnp-org:control-1-0"><errorCode>701</errorCode>
      <errorDescription>Transition not available</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"#;
    assert_eq!(parse_soap_response(fault, "Pause").unwrap_err(), "Transition not available (701)");
    assert!(parse_soap_response("not xml", "Play").is_err());
}

#[test]
fn durations_format_and_parse() {
    assert_eq!(format_duration(65.4), "0:01:05");
    assert_eq!(format_duration(3725.0), "1:02:05");
    assert_eq!(format_duration(-3.0), "0:00:00");
    assert_eq!(parse_duration("1:02:05"), Some(3725.0));
    assert_eq!(parse_duration("0:00:10.5/1"), Some(10.5));
    assert_eq!(parse_duration("garbage"), None);
}

#[test]
fn didl_lite_escapes_everything_and_names_the_format() {
    let mut song = track("1", "http://h/rest/stream?u=a&t=b&id=1");
    song.title = "Tom & Jerry <Live> \"Encore\"".to_string();
    song.artist = "Simon & Garfunkel".to_string();
    let didl = didl_lite(&song);
    assert!(didl.contains("<dc:title>Tom &amp; Jerry &lt;Live&gt; &quot;Encore&quot;</dc:title>"));
    assert!(didl.contains("<upnp:artist>Simon &amp; Garfunkel</upnp:artist>"));
    assert!(didl.contains("<upnp:class>object.item.audioItem.musicTrack</upnp:class>"));
    assert!(didl.contains(r#"protocolInfo="http-get:*:audio/mpeg:*" duration="0:03:20.000""#));
    assert!(didl.contains(">http://h/rest/stream?u=a&amp;t=b&amp;id=1</res>"));
    // It parses, and reads back as given.
    let doc = roxmltree::Document::parse(&didl).unwrap();
    let title = doc.descendants().find(|n| n.tag_name().name() == "title").unwrap();
    assert_eq!(title.text(), Some(song.title.as_str()));
    let art = doc.descendants().find(|n| n.tag_name().name() == "albumArtURI").unwrap();
    assert_eq!(art.text(), song.cover_url.as_deref());

    // Radio is a broadcast with no duration.
    let mut radio = track("r", "http://radio.test/stream");
    radio.live = true;
    let didl = didl_lite(&radio);
    assert!(didl.contains("audioBroadcast"));
    assert!(!didl.contains("duration="));
}

#[test]
fn sonos_plays_radio_as_mp3radio() {
    let device = upnp::parse_description(SONOS_DESCRIPTION, "http://192.168.1.20:1400/d.xml").unwrap();
    let mut radio = track("r", "http://radio.test/stream");
    radio.live = true;
    assert_eq!(upnp::transport_uri(&device, &radio), "x-rincon-mp3radio://radio.test/stream");
    let song = track("1", "http://h/s");
    assert_eq!(upnp::transport_uri(&device, &song), "http://h/s");
}

// ---- Formats ---------------------------------------------------------------

#[test]
fn sink_lists_decide_whether_the_original_plays() {
    let sinks = upnp::parse_sink_protocols(
        "http-get:*:audio/mpeg:*,http-get:*:audio/x-flac:DLNA.ORG_PN=FLAC,rtsp-rtp-udp:*:audio/L16:*,x-rincon:*:*:*",
    );
    assert_eq!(sinks, ["audio/mpeg", "audio/x-flac"]);
    assert!(sinks_play(&sinks, "audio/flac"));
    assert!(sinks_play(&sinks, "audio/mp3"));
    assert!(!sinks_play(&sinks, "audio/ogg"));
    // A renderer that lists nothing gets MP3.
    assert!(sinks_play(&[], "audio/mpeg"));
    assert!(!sinks_play(&[], "audio/flac"));
    assert!(cast_plays("audio/flac") && cast_plays("audio/x-m4a") && !cast_plays("audio/x-ms-wma"));
}

#[test]
fn mimes_come_from_content_type_or_suffix() {
    assert_eq!(mime_for_suffix("FLAC"), "audio/flac");
    assert_eq!(mime_for_suffix(".m4a"), "audio/mp4");
    assert_eq!(normalize_mime("audio/x-flac; rate=44100"), "audio/flac");
    let mut song = crate::api::Song { suffix: Some("ogg".into()), ..Default::default() };
    assert_eq!(song_mime(&song), "audio/ogg");
    song.content_type = Some("audio/x-flac".into());
    assert_eq!(song_mime(&song), "audio/flac");
}

#[test]
fn transcoding_asks_the_server_for_mp3() {
    let url = transcoded_url("https://music.test/rest/stream?u=me&t=tok&s=salt&id=42&format=raw", false);
    assert!(url.starts_with("https://music.test/rest/stream?"));
    assert!(url.contains("id=42") && url.contains("t=tok"));
    assert!(url.contains("format=mp3") && !url.contains("format=raw"));
    assert!(url.contains("maxBitRate=320"));
    let jelly = transcoded_url("https://jf.test/Audio/9/universal?container=mp3%2Cflac&ApiKey=k", true);
    assert!(jelly.contains("container=mp3") && !jelly.contains("flac"));
    assert!(jelly.contains("ApiKey=k"));
}

#[test]
fn server_songs_stream_from_the_server_and_transcode_when_needed() {
    let server = crate::api::ServerConfig {
        id: "srv".into(),
        name: "Home".into(),
        url: "http://music.test".into(),
        username: "me".into(),
        password: "pw".into(),
        active: true,
        ..Default::default()
    };
    let song = crate::api::Song {
        id: "42".into(),
        server_id: "srv".into(),
        server_name: "Home".into(),
        suffix: Some("flac".into()),
        ..Default::default()
    };
    let original = cast_media(&song, &[server.clone()], |mime| mime == "audio/flac").expect("media");
    assert_eq!(original.mime, "audio/flac");
    assert!(!original.transcoded);
    let MediaSource::Url(url) = &original.source else { panic!("a URL") };
    assert!(url.starts_with("http://music.test/rest/stream?") && url.contains("id=42"));

    let mp3 = cast_media(&song, &[server], |mime| mime == "audio/mpeg").expect("media");
    assert_eq!(mp3.mime, "audio/mpeg");
    assert!(mp3.transcoded);
    let MediaSource::Url(url) = &mp3.source else { panic!("a URL") };
    assert!(url.contains("format=mp3"));

    // No server, no media.
    assert!(cast_media(&song, &[], |_| true).is_none());
}

// ---- Discovery merging -----------------------------------------------------

fn sonos(id: &str, room: &str, model: &str) -> Speaker {
    let mut device = upnp::parse_description(SONOS_DESCRIPTION, "http://192.168.1.20:1400/d.xml").unwrap();
    device.room_name = Some(room.to_string());
    device.model_name = model.to_string();
    Speaker { id: id.into(), name: room.into(), model: model.into(), endpoint: Endpoint::Upnp(device) }
}

#[test]
fn merging_keeps_one_player_per_sonos_room_and_sorts() {
    let cast = Speaker {
        id: "cast:abc".into(),
        name: "bedroom speaker".into(),
        model: "Google Home".into(),
        endpoint: Endpoint::Cast("192.168.1.30:8009".parse().unwrap()),
    };
    let merged = merge_speakers(vec![
        sonos("a", "Living Room", "Sonos Sub"),
        sonos("b", "Living Room", "Sonos Arc"),
        sonos("b", "Living Room", "Sonos Arc"),
        cast.clone(),
        sonos("c", "Kitchen", "Sonos One"),
    ]);
    let names: Vec<(&str, &str)> = merged.iter().map(|s| (s.name.as_str(), s.model.as_str())).collect();
    assert_eq!(names, [("bedroom speaker", "Google Home"), ("Kitchen", "Sonos One"), ("Living Room", "Sonos Arc")]);
    assert_eq!(merged[0].kind_label(), "Chromecast");
    assert_eq!(merged[1].kind_label(), "Sonos");
}

// ---- mDNS -------------------------------------------------------------------

fn dns_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

fn dns_record(out: &mut Vec<u8>, name: &str, kind: u16, data: &[u8]) {
    dns_name(out, name);
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&0x8001u16.to_be_bytes());
    out.extend_from_slice(&120u32.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
}

fn cast_response(instance: &str, friendly: &str, ip: [u8; 4], port: u16) -> Vec<u8> {
    let mut packet = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 3];
    let full = format!("{instance}._googlecast._tcp.local");
    let mut ptr = Vec::new();
    dns_name(&mut ptr, &full);
    dns_record(&mut packet, "_googlecast._tcp.local", 12, &ptr);
    let mut srv = vec![0, 0, 0, 0];
    srv.extend_from_slice(&port.to_be_bytes());
    dns_name(&mut srv, "abc123.local");
    dns_record(&mut packet, &full, 33, &srv);
    let mut txt = Vec::new();
    for entry in ["id=abc123".to_string(), "md=Chromecast Audio".to_string(), format!("fn={friendly}")] {
        txt.push(entry.len() as u8);
        txt.extend_from_slice(entry.as_bytes());
    }
    dns_record(&mut packet, &full, 16, &txt);
    dns_record(&mut packet, "abc123.local", 1, &ip);
    packet
}

#[test]
fn mdns_query_asks_for_googlecast_pointers() {
    let packet = mdns::query_packet(mdns::CAST_SERVICE);
    assert_eq!(&packet[..12], &[0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(&packet[12..24], b"\x0b_googlecast");
    assert_eq!(&packet[packet.len() - 4..], &[0, 12, 0x80, 0x01]);
}

#[test]
fn mdns_answers_give_name_model_and_address() {
    let packet = cast_response("Chromecast-Audio-abc123", "Living Room speaker", [192, 168, 1, 31], 8009);
    let found = mdns::parse_response(&packet, mdns::CAST_SERVICE, "10.0.0.1".parse().unwrap());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Living Room speaker");
    assert_eq!(found[0].model, "Chromecast Audio");
    assert_eq!(found[0].id, "abc123");
    assert_eq!(found[0].addr, "192.168.1.31:8009".parse().unwrap());
    // Queries and other services are ignored; truncated packets don't panic.
    assert!(mdns::parse_response(&mdns::query_packet(mdns::CAST_SERVICE), mdns::CAST_SERVICE, "10.0.0.1".parse().unwrap()).is_empty());
    assert!(mdns::parse_response(&packet, "_airplay._tcp.local", "10.0.0.1".parse().unwrap()).is_empty());
    for cut in 0..packet.len() {
        let _ = mdns::parse_response(&packet[..cut], mdns::CAST_SERVICE, "10.0.0.1".parse().unwrap());
    }
}

// ---- Cast v2 messages --------------------------------------------------------

#[test]
fn cast_messages_round_trip_through_protobuf() {
    let message = CastMessage::new("receiver-0", chromecast::NS_RECEIVER, &chromecast::launch_payload(7));
    let framed = chromecast::frame(&message);
    let len = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
    assert_eq!(len, framed.len() - 4);
    // protocol_version 0 first, payload_type 0 before the payload.
    assert_eq!(&framed[4..6], &[0x08, 0x00]);
    let decoded = chromecast::decode(&framed[4..]).expect("decodes");
    assert_eq!(decoded, message);
    let json = decoded.json().unwrap();
    assert_eq!(json["type"], "LAUNCH");
    assert_eq!(json["appId"], "CC1AD845");
    assert_eq!(json["requestId"], 7);
    // Long payloads need multi-byte lengths.
    let long = CastMessage::new("t", chromecast::NS_MEDIA, &serde_json::json!({ "pad": "x".repeat(400) }));
    assert_eq!(chromecast::decode(&chromecast::encode(&long)), Some(long));
    assert!(chromecast::decode(&[0x0a, 0xff]).is_none());
}

#[test]
fn cast_load_carries_music_metadata() {
    let load = chromecast::load_payload(&track("1", "http://h/s"), 12.5, true, 3);
    assert_eq!(load["type"], "LOAD");
    assert_eq!(load["autoplay"], true);
    assert_eq!(load["currentTime"], 12.5);
    assert_eq!(load["media"]["contentId"], "http://h/s");
    assert_eq!(load["media"]["contentType"], "audio/mpeg");
    assert_eq!(load["media"]["streamType"], "BUFFERED");
    assert_eq!(load["media"]["metadata"]["metadataType"], 3);
    assert_eq!(load["media"]["metadata"]["title"], "Song 1");
    assert_eq!(load["media"]["metadata"]["albumName"], "Album");
    assert_eq!(load["media"]["metadata"]["images"][0]["url"], "http://music.test/rest/getCoverArt?id=al-1&size=600");
    assert_eq!(chromecast::seek_payload(5, 30.0, 4)["mediaSessionId"], 5);
    assert_eq!(chromecast::media_command("PAUSE", 5, 6)["type"], "PAUSE");
    assert_eq!(chromecast::volume_payload(1.4, 8)["volume"]["level"], 1.0);
}

#[test]
fn cast_statuses_parse_and_finish() {
    let receiver = serde_json::json!({
        "type": "RECEIVER_STATUS", "requestId": 1,
        "status": { "applications": [{ "appId": "CC1AD845", "sessionId": "s-1", "transportId": "web-5" }],
                    "volume": { "level": 0.4, "muted": false } }
    });
    let status = chromecast::parse_receiver_status(&receiver).unwrap();
    assert_eq!(status.transport_id.as_deref(), Some("web-5"));
    assert_eq!(status.session_id.as_deref(), Some("s-1"));
    assert_eq!(status.volume, Some(0.4));

    let playing = serde_json::json!({ "type": "MEDIA_STATUS", "status": [{
        "mediaSessionId": 9, "playerState": "PLAYING", "currentTime": 30.0, "media": { "duration": 200.0 } }] });
    let media = chromecast::parse_media_status(&playing).unwrap().unwrap();
    let now = chromecast::renderer_status(Some(&media), Duration::from_secs(2));
    assert_eq!(now.transport, Transport::Playing);
    assert_eq!(now.position, 32.0);
    assert_eq!(now.duration, 200.0);
    assert!(!now.finished);

    let finished = serde_json::json!({ "type": "MEDIA_STATUS", "status": [{
        "mediaSessionId": 9, "playerState": "IDLE", "idleReason": "FINISHED", "currentTime": 0.0 }] });
    let media = chromecast::parse_media_status(&finished).unwrap().unwrap();
    assert!(chromecast::renderer_status(Some(&media), Duration::ZERO).finished);
    let empty = serde_json::json!({ "type": "MEDIA_STATUS", "status": [] });
    assert_eq!(chromecast::parse_media_status(&empty), Some(None));
}

// ---- The session's state and the end-of-song handoff -------------------------

fn status(transport: Transport, position: f64) -> RendererStatus {
    RendererStatus { transport, position, duration: 200.0, finished: false }
}

#[test]
fn commands_are_optimistic_until_the_speaker_settles() {
    let start = Instant::now();
    let mut state = RemoteState::new(start);
    state.on_load(&track("1", "u"), 0.0, true, start);
    // Still stopped on the speaker a moment later: we asked for play.
    state.observe(&status(Transport::Stopped, 0.0), start + Duration::from_millis(500));
    let snap = state.snapshot(start + Duration::from_millis(600));
    assert!(!snap.paused);
    assert!(!snap.ended);
    assert_eq!(snap.song_id.as_deref(), Some("1"));

    // Playing: the clock moves the position between polls.
    let later = start + Duration::from_secs(5);
    state.observe(&status(Transport::Playing, 4.0), later);
    let snap = state.snapshot(later + Duration::from_millis(500));
    assert!((snap.current_time - 4.5).abs() < 0.01, "{}", snap.current_time);

    state.on_pause(later + Duration::from_secs(1));
    let snap = state.snapshot(later + Duration::from_secs(3));
    assert!(snap.paused);
    assert!((snap.current_time - 5.0).abs() < 0.01);
}

#[test]
fn a_stop_near_the_end_is_the_end_and_elsewhere_a_pause() {
    let start = Instant::now();
    let mut state = RemoteState::new(start);
    state.on_load(&track("1", "u"), 0.0, true, start);
    let t = |secs: u64| start + Duration::from_secs(secs);
    state.observe(&status(Transport::Playing, 195.0), t(195));
    state.observe(&status(Transport::Stopped, 0.0), t(200));
    let snap = state.snapshot(t(200));
    assert!(snap.ended && snap.paused);
    // It stays ended (the controller ignores repeats) until the next load.
    state.observe(&status(Transport::Playing, 3.0), t(201));
    assert!(state.snapshot(t(201)).ended);
    state.on_load(&track("2", "u2"), 0.0, true, t(202));
    let snap = state.snapshot(t(202));
    assert!(!snap.ended && !snap.paused);
    assert_eq!(snap.song_id.as_deref(), Some("2"));

    // Stopped mid-song on the speaker: a pause, told to the player once.
    state.observe(&status(Transport::Playing, 60.0), t(262));
    state.observe(&status(Transport::Stopped, 0.0), t(263));
    let snap: SessionSnapshot = state.snapshot(t(263));
    assert!(snap.paused && !snap.ended);
    assert_eq!(snap.action.as_deref(), Some("pause"));
    assert_eq!(state.snapshot(t(264)).action, None);

    // Played again from the speaker's app.
    state.observe(&status(Transport::Playing, 61.0), t(265));
    assert_eq!(state.snapshot(t(265)).action.as_deref(), Some("play"));
}

#[test]
fn cast_finished_ends_and_radio_never_does() {
    let start = Instant::now();
    let mut state = RemoteState::new(start);
    state.on_load(&track("1", "u"), 0.0, true, start);
    let finished = RendererStatus { transport: Transport::Stopped, position: 0.0, duration: 0.0, finished: true };
    state.observe(&finished, start + Duration::from_millis(100));
    assert!(state.snapshot(start + Duration::from_millis(100)).ended);

    let mut radio = track("r", "http://radio.test/");
    radio.live = true;
    radio.duration = 0.0;
    state.on_load(&radio, 0.0, true, start);
    state.observe(&finished, start + Duration::from_secs(10));
    assert!(!state.snapshot(start + Duration::from_secs(10)).ended);
}

#[test]
fn seeks_clamp_and_reset_the_end() {
    let start = Instant::now();
    let mut state = RemoteState::new(start);
    state.on_load(&track("1", "u"), 0.0, false, start);
    state.on_seek(500.0, start);
    assert_eq!(state.snapshot(start).current_time, 200.0);
    state.on_seek(42.0, start);
    let snap = state.snapshot(start + Duration::from_secs(5));
    assert_eq!(snap.current_time, 42.0);
    assert!(snap.paused);
}

// ---- LAN file server -----------------------------------------------------------

#[test]
fn ranges_and_request_heads_parse() {
    assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
    assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
    assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
    assert_eq!(parse_range("bytes=990-2000", 1000), Some((990, 999)));
    assert_eq!(parse_range("bytes=1000-", 1000), None);
    assert_eq!(parse_range("bytes=5-1", 1000), None);
    assert_eq!(parse_range("items=0-1", 1000), None);
    let (method, path, range) =
        parse_request_head("GET /abc/def.flac HTTP/1.1\r\nHost: x\r\nrange: bytes=10-\r\n\r\n").unwrap();
    assert_eq!((method.as_str(), path.as_str(), range.as_deref()), ("GET", "/abc/def.flac", Some("bytes=10-")));
}

#[tokio::test]
async fn the_lan_server_serves_only_shared_files_with_ranges() {
    let dir = std::env::temp_dir().join(format!("rs-cast-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("song.flac");
    let bytes: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    std::fs::write(&file, &bytes).unwrap();
    let secret_file = dir.join("secret.txt");
    std::fs::write(&secret_file, b"nope").unwrap();

    let server = LanServer::start("127.0.0.1".parse().unwrap()).await.unwrap();
    let url = server.share(file.clone(), "audio/flac");
    assert!(url.starts_with("http://127.0.0.1:") && url.ends_with(".flac"));
    let http = reqwest::Client::new();

    let whole = http.get(&url).send().await.unwrap();
    assert_eq!(whole.status(), 200);
    assert_eq!(whole.headers()["content-type"], "audio/flac");
    assert_eq!(whole.bytes().await.unwrap().to_vec(), bytes);

    let part = http.get(&url).header("Range", "bytes=1000-1099").send().await.unwrap();
    assert_eq!(part.status(), 206);
    assert_eq!(part.headers()["content-range"], "bytes 1000-1099/5000");
    assert_eq!(part.bytes().await.unwrap().to_vec(), bytes[1000..1100].to_vec());

    let head = http.head(&url).send().await.unwrap();
    assert_eq!(head.headers()["content-length"], "5000");

    // Other paths, other tokens and other secrets are all 404s.
    let base = url.rsplit_once('/').unwrap().0;
    for wrong in [format!("{base}/0000.txt"), url.replace(base, "http://x/../"), format!("{base}/../secret.txt")] {
        if let Ok(response) = http.get(&wrong).send().await {
            assert_ne!(response.status(), 200, "{wrong}");
        }
    }
    let root = url.split('/').take(3).collect::<Vec<_>>().join("/");
    assert_eq!(http.get(format!("{root}/")).send().await.unwrap().status(), 404);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- The fake MediaRenderer -------------------------------------------------------

/// What the fake renderer was asked, and what it reports.
#[derive(Default)]
struct FakeState {
    calls: Vec<(String, HashMap<String, String>)>,
    transport: String,
    rel_time: String,
}

struct FakeRenderer {
    state: Arc<Mutex<FakeState>>,
    ssdp: std::net::SocketAddr,
}

impl FakeRenderer {
    fn calls(&self) -> Vec<(String, HashMap<String, String>)> {
        self.state.lock().unwrap().calls.clone()
    }

    fn actions(&self) -> Vec<String> {
        self.calls().into_iter().map(|(action, _)| action).filter(|a| !a.starts_with("Get")).collect()
    }

    fn last(&self, action: &str) -> HashMap<String, String> {
        self.calls().into_iter().rev().find(|(a, _)| a == action).map(|(_, args)| args).unwrap_or_default()
    }

    fn report(&self, transport: &str, rel_time: &str) {
        let mut state = self.state.lock().unwrap();
        state.transport = transport.to_string();
        state.rel_time = rel_time.to_string();
    }

    fn clear(&self) {
        self.state.lock().unwrap().calls.clear();
    }
}

fn fake_description(port: u16) -> String {
    format!(
        r#"<?xml version="1.0"?><root xmlns="urn:schemas-upnp-org:device-1-0"><device>
        <deviceType>urn:schemas-upnp-org:device:MediaRenderer:1</deviceType>
        <friendlyName>Living Room</friendlyName><manufacturer>Fake Audio</manufacturer>
        <modelName>Test Speaker</modelName><UDN>uuid:fake-renderer-1</UDN><serviceList>
        <service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/avt</controlURL></service>
        <service><serviceType>urn:schemas-upnp-org:service:RenderingControl:1</serviceType><controlURL>/rc</controlURL></service>
        <service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType><controlURL>http://127.0.0.1:{port}/cm</controlURL></service>
        </serviceList></device></root>"#
    )
}

fn soap_reply(action: &str, values: &[(&str, &str)]) -> String {
    let body: String = values.iter().map(|(k, v)| format!("<{k}>{}</{k}>", upnp::xml_escape(v))).collect();
    format!(
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:{action}Response xmlns:u="urn:schemas-upnp-org:service:AVTransport:1">{body}</u:{action}Response></s:Body></s:Envelope>"#
    )
}

async fn handle_fake(mut stream: tokio::net::TcpStream, port: u16, state: Arc<Mutex<FakeState>>) {
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    let (head, body) = loop {
        let Ok(read) = stream.read(&mut buf).await else { return };
        if read == 0 {
            return;
        }
        data.extend_from_slice(&buf[..read]);
        if let Some(split) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..split]).into_owned();
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length").then(|| value.trim().parse().ok())?
                })
                .unwrap_or(0);
            while data.len() < split + 4 + length {
                let Ok(read) = stream.read(&mut buf).await else { return };
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..read]);
            }
            break (head, String::from_utf8_lossy(&data[split + 4..]).into_owned());
        }
    };
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_string();
    let reply = if path == "/description.xml" {
        fake_description(port)
    } else {
        let action = head
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("soapaction").then(|| value.trim().trim_matches('"').rsplit('#').next().map(str::to_string))?
            })
            .unwrap_or_default();
        let doc = roxmltree::Document::parse(&body).expect("well-formed SOAP");
        let request = doc.descendants().find(|n| n.tag_name().name() == action).expect("the action element");
        let args: HashMap<String, String> = request
            .children()
            .filter(|n| n.is_element())
            .map(|n| (n.tag_name().name().to_string(), n.text().unwrap_or("").to_string()))
            .collect();
        let mut state = state.lock().unwrap();
        state.calls.push((action.clone(), args));
        match action.as_str() {
            "GetProtocolInfo" => soap_reply(&action, &[("Source", ""), ("Sink", "http-get:*:audio/mpeg:*,http-get:*:audio/flac:*")]),
            "GetTransportInfo" => soap_reply(&action, &[("CurrentTransportState", &state.transport), ("CurrentTransportStatus", "OK")]),
            "GetPositionInfo" => soap_reply(&action, &[("TrackDuration", "0:03:20"), ("RelTime", &state.rel_time)]),
            "GetVolume" => soap_reply(&action, &[("CurrentVolume", "25")]),
            "Play" => {
                state.transport = "PLAYING".into();
                soap_reply(&action, &[])
            }
            "Pause" => {
                state.transport = "PAUSED_PLAYBACK".into();
                soap_reply(&action, &[])
            }
            "Stop" => {
                state.transport = "STOPPED".into();
                soap_reply(&action, &[])
            }
            _ => soap_reply(&action, &[]),
        }
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

/// Starts the fake: its HTTP server and an SSDP responder, both on loopback.
async fn fake_renderer() -> FakeRenderer {
    let state = Arc::new(Mutex::new(FakeState {
        transport: "NO_MEDIA_PRESENT".into(),
        rel_time: "0:00:00".into(),
        ..Default::default()
    }));
    let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = http.local_addr().unwrap().port();
    {
        let state = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = http.accept().await {
                tokio::spawn(handle_fake(stream, port, state.clone()));
            }
        });
    }
    let ssdp = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let ssdp_addr = ssdp.local_addr().unwrap();
    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        while let Ok((len, from)) = ssdp.recv_from(&mut buf).await {
            let request = String::from_utf8_lossy(&buf[..len]);
            if !request.starts_with("M-SEARCH") || !request.contains("MediaRenderer:1") {
                continue;
            }
            let reply = format!(
                "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nLOCATION: http://127.0.0.1:{port}/description.xml\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\nUSN: uuid:fake-renderer-1::urn:schemas-upnp-org:device:MediaRenderer:1\r\nSERVER: Fake/1.0 UPnP/1.0\r\n\r\n"
            );
            let _ = ssdp.send_to(reply.as_bytes(), from).await;
        }
    });
    FakeRenderer { state, ssdp: ssdp_addr }
}

#[tokio::test]
async fn casting_to_a_fake_renderer_end_to_end() {
    let fake = fake_renderer().await;

    // Discover: one speaker (the M-SEARCH is sent twice; it's listed once).
    let found = discover(DiscoveryOptions {
        ssdp_to: Some(fake.ssdp),
        mdns_to: None,
        window: Duration::from_millis(400),
    })
    .await;
    assert_eq!(found.len(), 1, "{found:?}");
    let speaker = found[0].clone();
    assert_eq!(speaker.name, "Living Room");
    assert_eq!(speaker.model, "Test Speaker");
    assert_eq!(speaker.kind_label(), "DLNA");

    // Connect: it says what it plays.
    let mut session = CastSession::start(speaker).await.expect("connects");
    session.set_settle(Duration::from_millis(50));
    assert!(session.plays("audio/flac") && !session.plays("audio/ogg"));
    assert_eq!(session.volume().await, Some(0.25));

    // Cast a song: SetAVTransportURI with the URL and metadata, then Play.
    let url = "http://music.test/rest/stream?u=me&t=tok&s=salt&id=1&format=mp3&maxBitRate=320";
    let mut first = track("1", url);
    first.title = "Fish & Chips".to_string();
    fake.clear();
    session.apply(SessionCommand::Load { track: first, position: 0.0, play: true }).await.unwrap();
    assert_eq!(fake.actions(), ["SetAVTransportURI", "Play"]);
    let set = fake.last("SetAVTransportURI");
    assert_eq!(set["CurrentURI"], url);
    let didl = roxmltree::Document::parse(&set["CurrentURIMetaData"]).expect("DIDL-Lite parses");
    let text = |name: &str| didl.descendants().find(|n| n.tag_name().name() == name).and_then(|n| n.text()).map(str::to_string);
    assert_eq!(text("title").as_deref(), Some("Fish & Chips"));
    assert_eq!(text("artist").as_deref(), Some("Artist"));
    assert_eq!(text("album").as_deref(), Some("Album"));
    assert_eq!(text("res").as_deref(), Some(url));
    assert_eq!(fake.last("Play")["Speed"], "1");
    assert!(!session.snapshot().paused);

    // Pause, seek and volume.
    fake.clear();
    session.apply(SessionCommand::Pause).await.unwrap();
    session.apply(SessionCommand::Seek(65.0)).await.unwrap();
    session.apply(SessionCommand::Volume(0.3)).await.unwrap();
    assert_eq!(fake.actions(), ["Pause", "Seek", "SetVolume"]);
    assert_eq!(fake.last("Seek")["Unit"], "REL_TIME");
    assert_eq!(fake.last("Seek")["Target"], "0:01:05");
    assert_eq!(fake.last("SetVolume")["DesiredVolume"], "30");
    assert_eq!(fake.last("SetVolume")["Channel"], "Master");
    assert!(session.snapshot().paused);

    // Play on to the end: the speaker stops near the end, so the song ended.
    session.apply(SessionCommand::Play).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    fake.report("PLAYING", "0:03:18");
    session.poll().await.unwrap();
    let snap = session.snapshot();
    assert!(!snap.paused && snap.current_time >= 198.0, "{snap:?}");
    fake.report("STOPPED", "0:00:00");
    session.poll().await.unwrap();
    let snap = session.snapshot();
    assert!(snap.ended, "{snap:?}");
    assert_eq!(snap.song_id.as_deref(), Some("1"));

    // The controller moves the queue on: the next song's URL goes to the speaker.
    fake.clear();
    let next_url = "http://music.test/rest/stream?u=me&t=tok&s=salt&id=2&format=mp3&maxBitRate=320";
    session.apply(SessionCommand::Load { track: track("2", next_url), position: 0.0, play: true }).await.unwrap();
    assert_eq!(fake.actions(), ["SetAVTransportURI", "Play"]);
    assert_eq!(fake.last("SetAVTransportURI")["CurrentURI"], next_url);
    let snap = session.snapshot();
    assert!(!snap.ended && !snap.paused);
    assert_eq!(snap.song_id.as_deref(), Some("2"));

    // Back to this device: the speaker stops.
    fake.clear();
    session.stop().await;
    assert_eq!(fake.actions(), ["Stop"]);
}

#[tokio::test]
async fn a_paused_load_seeks_when_it_plays() {
    let fake = fake_renderer().await;
    let found = discover(DiscoveryOptions { ssdp_to: Some(fake.ssdp), mdns_to: None, window: Duration::from_millis(300) }).await;
    let mut session = CastSession::start(found[0].clone()).await.unwrap();
    fake.clear();
    session.apply(SessionCommand::Load { track: track("1", "http://h/s"), position: 42.0, play: false }).await.unwrap();
    assert_eq!(fake.actions(), ["SetAVTransportURI"]);
    let snap = session.snapshot();
    assert!(snap.paused);
    assert_eq!(snap.current_time, 42.0);
    session.apply(SessionCommand::Play).await.unwrap();
    assert_eq!(fake.actions(), ["SetAVTransportURI", "Play", "Seek"]);
    assert_eq!(fake.last("Seek")["Target"], "0:00:42");
}

#[tokio::test]
async fn local_files_cast_through_the_lan_server() {
    let fake = fake_renderer().await;
    let found = discover(DiscoveryOptions { ssdp_to: Some(fake.ssdp), mdns_to: None, window: Duration::from_millis(300) }).await;
    let mut session = CastSession::start(found[0].clone()).await.unwrap();
    let dir = std::env::temp_dir().join(format!("rs-cast-local-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("track.mp3");
    std::fs::write(&file, b"ID3 fake mp3").unwrap();
    let media = CastMedia { source: MediaSource::File(file), mime: "audio/mpeg".into(), transcoded: false };
    let url = session.media_url(&media).await.unwrap();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    let body = reqwest::get(&url).await.unwrap().bytes().await.unwrap();
    assert_eq!(&body[..], b"ID3 fake mp3");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn discovery_without_answers_is_empty_and_quick() {
    // Nobody listens on this port.
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let silent = socket.local_addr().unwrap();
    drop(socket);
    let started = Instant::now();
    let found = discover(DiscoveryOptions { ssdp_to: Some(silent), mdns_to: Some(silent), window: Duration::from_millis(200) }).await;
    assert!(found.is_empty());
    assert!(started.elapsed() < Duration::from_secs(2));
}

/// Lists the speakers on this network (read-only: the searches and the
/// description pages, never a command). Run by hand:
/// `cargo test list_speakers_on_this_network -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn list_speakers_on_this_network() {
    for speaker in discover(DiscoveryOptions::default()).await {
        println!("{} · {} · {} · {:?}", speaker.name, speaker.kind_label(), speaker.model, speaker.ip());
    }
}
