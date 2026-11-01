//! Google Cast (Cast v2) client for the Default Media Receiver: TLS to
//! port 8009, length-prefixed `CastMessage` protobufs carrying JSON on
//! four namespaces (connection, heartbeat, receiver, media).
//!
//! `CastMessage` has seven scalar fields, so it's encoded by hand here
//! rather than through a protobuf crate. Cast devices present self-signed
//! certificates, so the TLS session doesn't verify them (as every
//! third-party sender does; Google's device-auth challenge isn't used).
//!
//! One reader task keeps `CastState` current from RECEIVER_STATUS and
//! MEDIA_STATUS and answers heartbeat PINGs; another sends our PINGs.

use super::{CastResult, CastTrack, RendererStatus, Transport};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_rustls::client::TlsStream;

pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";
/// The Default Media Receiver app.
pub const DEFAULT_RECEIVER: &str = "CC1AD845";
const SENDER: &str = "sender-0";
const RECEIVER: &str = "receiver-0";
/// Larger messages aren't ones we sent for.
const MAX_MESSAGE: usize = 64 * 1024;

/// One Cast v2 message with a UTF-8 (JSON) payload.
#[derive(Clone, Debug, PartialEq)]
pub struct CastMessage {
    pub source: String,
    pub destination: String,
    pub namespace: String,
    pub payload: String,
}

impl CastMessage {
    pub fn new(destination: &str, namespace: &str, payload: &Value) -> Self {
        CastMessage {
            source: SENDER.to_string(),
            destination: destination.to_string(),
            namespace: namespace.to_string(),
            payload: payload.to_string(),
        }
    }

    pub fn json(&self) -> Option<Value> {
        serde_json::from_str(&self.payload).ok()
    }
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_string(out: &mut Vec<u8>, field: u64, value: &str) {
    put_varint(out, field << 3 | 2);
    put_varint(out, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

/// The protobuf encoding (protocol version 0 = CASTV2_1_0, payload type
/// 0 = STRING).
pub fn encode(message: &CastMessage) -> Vec<u8> {
    let mut out = vec![0x08, 0x00];
    put_string(&mut out, 2, &message.source);
    put_string(&mut out, 3, &message.destination);
    put_string(&mut out, 4, &message.namespace);
    out.extend_from_slice(&[0x28, 0x00]);
    put_string(&mut out, 6, &message.payload);
    out
}

/// `encode` with the 4-byte big-endian length in front, as sent.
pub fn frame(message: &CastMessage) -> Vec<u8> {
    let body = encode(message);
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

fn get_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Decodes a message body (without the length). Binary payloads and
/// unknown fields are skipped.
pub fn decode(data: &[u8]) -> Option<CastMessage> {
    let mut message = CastMessage {
        source: String::new(),
        destination: String::new(),
        namespace: String::new(),
        payload: String::new(),
    };
    let mut pos = 0;
    while pos < data.len() {
        let key = get_varint(data, &mut pos)?;
        match key & 7 {
            0 => {
                get_varint(data, &mut pos)?;
            }
            1 => pos += 8,
            5 => pos += 4,
            2 => {
                let len = get_varint(data, &mut pos)? as usize;
                let bytes = data.get(pos..pos.checked_add(len)?)?;
                pos += len;
                let text = || String::from_utf8_lossy(bytes).into_owned();
                match key >> 3 {
                    2 => message.source = text(),
                    3 => message.destination = text(),
                    4 => message.namespace = text(),
                    6 => message.payload = text(),
                    _ => {}
                }
            }
            _ => return None,
        }
    }
    Some(message)
}

/// The LOAD request for the Default Media Receiver: the stream, its type,
/// and music metadata (`metadataType` 3) with the cover.
pub fn load_payload(track: &CastTrack, position: f64, autoplay: bool, request_id: u32) -> Value {
    let mut metadata = json!({
        "metadataType": 3,
        "title": track.title,
        "artist": track.artist,
        "albumName": track.album,
    });
    if let Some(cover) = track.cover_url.as_ref().filter(|url| !url.is_empty()) {
        metadata["images"] = json!([{ "url": cover }]);
    }
    let mut media = json!({
        "contentId": track.url,
        "contentType": track.mime,
        "streamType": if track.live { "LIVE" } else { "BUFFERED" },
        "metadata": metadata,
    });
    if track.duration > 0.0 && !track.live {
        media["duration"] = json!(track.duration);
    }
    json!({
        "type": "LOAD",
        "requestId": request_id,
        "autoplay": autoplay,
        "currentTime": position.max(0.0),
        "media": media,
    })
}

/// PLAY, PAUSE or STOP for the loaded media.
pub fn media_command(kind: &str, media_session_id: i64, request_id: u32) -> Value {
    json!({ "type": kind, "mediaSessionId": media_session_id, "requestId": request_id })
}

pub fn seek_payload(media_session_id: i64, position: f64, request_id: u32) -> Value {
    json!({
        "type": "SEEK",
        "mediaSessionId": media_session_id,
        "currentTime": position.max(0.0),
        "requestId": request_id,
    })
}

pub fn volume_payload(level: f64, request_id: u32) -> Value {
    json!({ "type": "SET_VOLUME", "volume": { "level": level.clamp(0.0, 1.0) }, "requestId": request_id })
}

pub fn launch_payload(request_id: u32) -> Value {
    json!({ "type": "LAUNCH", "appId": DEFAULT_RECEIVER, "requestId": request_id })
}

/// The receiver's running app (ours) and volume.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReceiverStatus {
    pub session_id: Option<String>,
    pub transport_id: Option<String>,
    pub volume: Option<f64>,
}

pub fn parse_receiver_status(payload: &Value) -> Option<ReceiverStatus> {
    if payload.get("type")?.as_str()? != "RECEIVER_STATUS" {
        return None;
    }
    let status = payload.get("status")?;
    let app = status
        .get("applications")
        .and_then(Value::as_array)
        .and_then(|apps| apps.iter().find(|app| app.get("appId").and_then(Value::as_str) == Some(DEFAULT_RECEIVER)));
    let text = |key: &str| app.and_then(|app| app.get(key)).and_then(Value::as_str).map(str::to_string);
    Some(ReceiverStatus {
        session_id: text("sessionId"),
        transport_id: text("transportId"),
        volume: status.get("volume").and_then(|v| v.get("level")).and_then(Value::as_f64),
    })
}

/// The loaded media's state, from MEDIA_STATUS.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaStatus {
    pub media_session_id: i64,
    /// IDLE, BUFFERING, PLAYING or PAUSED.
    pub player_state: String,
    /// With IDLE: FINISHED, CANCELLED, INTERRUPTED or ERROR.
    pub idle_reason: Option<String>,
    pub current_time: f64,
    pub duration: f64,
}

/// `Some(None)`: a MEDIA_STATUS with nothing loaded.
pub fn parse_media_status(payload: &Value) -> Option<Option<MediaStatus>> {
    if payload.get("type")?.as_str()? != "MEDIA_STATUS" {
        return None;
    }
    let Some(entry) = payload.get("status").and_then(Value::as_array).and_then(|list| list.first()) else {
        return Some(None);
    };
    Some(Some(MediaStatus {
        media_session_id: entry.get("mediaSessionId").and_then(Value::as_i64).unwrap_or(0),
        player_state: entry.get("playerState").and_then(Value::as_str).unwrap_or("IDLE").to_string(),
        idle_reason: entry.get("idleReason").and_then(Value::as_str).map(str::to_string),
        current_time: entry.get("currentTime").and_then(Value::as_f64).unwrap_or(0.0),
        duration: entry
            .get("media")
            .and_then(|media| media.get("duration"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    }))
}

/// Our status terms for a MEDIA_STATUS.
pub fn renderer_status(media: Option<&MediaStatus>, elapsed: Duration) -> RendererStatus {
    let Some(media) = media else {
        return RendererStatus { transport: Transport::Stopped, position: 0.0, duration: 0.0, finished: false };
    };
    let transport = match media.player_state.as_str() {
        "PLAYING" => Transport::Playing,
        "PAUSED" => Transport::Paused,
        "BUFFERING" => Transport::Buffering,
        _ => Transport::Stopped,
    };
    let mut position = media.current_time;
    if transport == Transport::Playing {
        position += elapsed.as_secs_f64();
    }
    if media.duration > 0.0 {
        position = position.min(media.duration);
    }
    RendererStatus {
        transport,
        position,
        duration: media.duration,
        finished: transport == Transport::Stopped && media.idle_reason.as_deref() == Some("FINISHED"),
    }
}

#[derive(Debug)]
struct NoVerify(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

#[derive(Default)]
struct CastState {
    receiver: ReceiverStatus,
    media: Option<MediaStatus>,
    media_at: Option<Instant>,
    /// LOAD_FAILED / INVALID_REQUEST since the last LOAD.
    failure: Option<String>,
    closed: bool,
}

type Writer = Arc<tokio::sync::Mutex<WriteHalf<TlsStream<TcpStream>>>>;

async fn send(writer: &Writer, message: &CastMessage) -> CastResult<()> {
    let mut writer = writer.lock().await;
    writer.write_all(&frame(message)).await.map_err(|err| err.to_string())?;
    writer.flush().await.map_err(|err| err.to_string())
}

/// A connected Cast device running the Default Media Receiver.
pub struct CastRenderer {
    name: String,
    writer: Writer,
    state: Arc<Mutex<CastState>>,
    changed: Arc<Notify>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    request_id: AtomicU32,
}

impl Drop for CastRenderer {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl CastRenderer {
    /// Connects, launches the Default Media Receiver (or joins it if it's
    /// already running) and connects to its transport.
    pub async fn connect(addr: SocketAddr, name: &str) -> CastResult<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|err| err.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify(provider)))
            .with_no_client_auth();
        let tcp = tokio::time::timeout(Duration::from_secs(4), TcpStream::connect(addr))
            .await
            .map_err(|_| format!("{name} didn't answer"))?
            .map_err(|err| format!("{name}: {err}"))?;
        let server_name = rustls::pki_types::ServerName::IpAddress(addr.ip().into());
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(server_name, tcp)
            .await
            .map_err(|err| format!("{name}: {err}"))?;
        let (reader, writer) = tokio::io::split(tls);
        let writer: Writer = Arc::new(tokio::sync::Mutex::new(writer));
        let state = Arc::new(Mutex::new(CastState::default()));
        let changed = Arc::new(Notify::new());

        let read_task = tokio::spawn(read_loop(reader, writer.clone(), state.clone(), changed.clone()));
        let beat_writer = writer.clone();
        let beat_task = tokio::spawn(async move {
            loop {
                let ping = CastMessage::new(RECEIVER, NS_HEARTBEAT, &json!({ "type": "PING" }));
                if send(&beat_writer, &ping).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        let renderer = CastRenderer {
            name: name.to_string(),
            writer,
            state,
            changed,
            tasks: vec![read_task, beat_task],
            request_id: AtomicU32::new(1),
        };

        renderer.send(RECEIVER, NS_CONNECTION, json!({ "type": "CONNECT" })).await?;
        let id = renderer.next_id();
        renderer.send(RECEIVER, NS_RECEIVER, launch_payload(id)).await?;
        let transport = renderer
            .wait_for(Duration::from_secs(10), |state| state.receiver.transport_id.clone())
            .await
            .ok_or_else(|| format!("{name} didn't start the player"))?;
        renderer.send(&transport, NS_CONNECTION, json!({ "type": "CONNECT" })).await?;
        let id = renderer.next_id();
        renderer.send(&transport, NS_MEDIA, json!({ "type": "GET_STATUS", "requestId": id })).await?;
        Ok(renderer)
    }

    fn next_id(&self) -> u32 {
        self.request_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn send(&self, destination: &str, namespace: &str, payload: Value) -> CastResult<()> {
        send(&self.writer, &CastMessage::new(destination, namespace, &payload)).await
    }

    async fn wait_for<T>(&self, limit: Duration, mut check: impl FnMut(&CastState) -> Option<T>) -> Option<T> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let notified = self.changed.notified();
            if let Ok(state) = self.state.lock() {
                if let Some(found) = check(&state) {
                    return Some(found);
                }
                if state.closed {
                    return None;
                }
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return None;
            }
        }
    }

    fn transport_id(&self) -> CastResult<String> {
        let state = self.state.lock().map_err(|_| "Cast state poisoned".to_string())?;
        if state.closed {
            return Err(format!("Lost {}", self.name));
        }
        state.receiver.transport_id.clone().ok_or_else(|| format!("{} isn't ready", self.name))
    }

    fn media_session(&self) -> Option<i64> {
        self.state.lock().ok()?.media.as_ref().map(|media| media.media_session_id)
    }

    pub async fn load(&self, track: &CastTrack, position: f64, autoplay: bool) -> CastResult<()> {
        let transport = self.transport_id()?;
        let previous = self.media_session();
        if let Ok(mut state) = self.state.lock() {
            state.failure = None;
        }
        let id = self.next_id();
        self.send(&transport, NS_MEDIA, load_payload(track, position, autoplay, id)).await?;
        let outcome = self
            .wait_for(Duration::from_secs(15), |state| {
                if let Some(failure) = state.failure.clone() {
                    return Some(Err(failure));
                }
                state
                    .media
                    .as_ref()
                    .filter(|media| Some(media.media_session_id) != previous && media.player_state != "IDLE")
                    .map(|_| Ok(()))
            })
            .await;
        outcome.unwrap_or_else(|| Err(format!("{} didn't load the song", self.name)))
    }

    async fn media(&self, kind: &str) -> CastResult<()> {
        let transport = self.transport_id()?;
        let Some(session) = self.media_session() else {
            return Ok(());
        };
        let id = self.next_id();
        self.send(&transport, NS_MEDIA, media_command(kind, session, id)).await
    }

    pub async fn play(&self) -> CastResult<()> {
        self.media("PLAY").await
    }

    pub async fn pause(&self) -> CastResult<()> {
        self.media("PAUSE").await
    }

    /// Stops the media and closes the receiver app.
    pub async fn stop(&self) -> CastResult<()> {
        let _ = self.media("STOP").await;
        let session = self.state.lock().ok().and_then(|state| state.receiver.session_id.clone());
        if let Some(session) = session {
            let id = self.next_id();
            self.send(RECEIVER, NS_RECEIVER, json!({ "type": "STOP", "sessionId": session, "requestId": id }))
                .await?;
        }
        Ok(())
    }

    pub async fn seek(&self, position: f64) -> CastResult<()> {
        let transport = self.transport_id()?;
        let Some(session) = self.media_session() else {
            return Ok(());
        };
        let id = self.next_id();
        self.send(&transport, NS_MEDIA, seek_payload(session, position, id)).await
    }

    pub async fn set_volume(&self, level: f64) -> CastResult<()> {
        let id = self.next_id();
        self.send(RECEIVER, NS_RECEIVER, volume_payload(level, id)).await
    }

    pub fn volume(&self) -> Option<f64> {
        self.state.lock().ok()?.receiver.volume
    }

    /// The latest MEDIA_STATUS (the position moved on by the time since),
    /// and a GET_STATUS so the next one is fresh.
    pub async fn status(&self) -> CastResult<RendererStatus> {
        let transport = self.transport_id()?;
        let id = self.next_id();
        self.send(&transport, NS_MEDIA, json!({ "type": "GET_STATUS", "requestId": id })).await?;
        let state = self.state.lock().map_err(|_| "Cast state poisoned".to_string())?;
        let elapsed = state.media_at.map(|at| at.elapsed()).unwrap_or_default();
        Ok(renderer_status(state.media.as_ref(), elapsed))
    }
}

async fn read_loop(
    mut reader: ReadHalf<TlsStream<TcpStream>>,
    writer: Writer,
    state: Arc<Mutex<CastState>>,
    changed: Arc<Notify>,
) {
    let mut len_buf = [0u8; 4];
    loop {
        if reader.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_MESSAGE {
            break;
        }
        let mut body = vec![0u8; len];
        if reader.read_exact(&mut body).await.is_err() {
            break;
        }
        let Some(message) = decode(&body) else {
            continue;
        };
        let Some(payload) = message.json() else {
            continue;
        };
        let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
        match (message.namespace.as_str(), kind) {
            (NS_HEARTBEAT, "PING") => {
                let pong = CastMessage::new(&message.source, NS_HEARTBEAT, &json!({ "type": "PONG" }));
                let _ = send(&writer, &pong).await;
            }
            (NS_CONNECTION, "CLOSE") => {
                if let Ok(mut state) = state.lock() {
                    state.closed = true;
                }
                changed.notify_waiters();
            }
            (NS_RECEIVER, _) => {
                if let Some(status) = parse_receiver_status(&payload) {
                    if let Ok(mut state) = state.lock() {
                        state.receiver = status;
                    }
                    changed.notify_waiters();
                }
            }
            (NS_MEDIA, "LOAD_FAILED" | "LOAD_CANCELLED" | "INVALID_REQUEST") => {
                if let Ok(mut state) = state.lock() {
                    state.failure = Some(match payload.get("reason").and_then(Value::as_str) {
                        Some(reason) => format!("The speaker couldn't play it ({reason})"),
                        None => "The speaker couldn't play it".to_string(),
                    });
                }
                changed.notify_waiters();
            }
            (NS_MEDIA, _) => {
                if let Some(mut media) = parse_media_status(&payload) {
                    if let Ok(mut state) = state.lock() {
                        // Updates leave `media` (and so the duration) out once it's known.
                        if let (Some(new), Some(old)) = (media.as_mut(), state.media.as_ref()) {
                            if new.duration <= 0.0 && new.media_session_id == old.media_session_id {
                                new.duration = old.duration;
                            }
                        }
                        state.media = media;
                        state.media_at = Some(Instant::now());
                    }
                    changed.notify_waiters();
                }
            }
            _ => {}
        }
    }
    if let Ok(mut state) = state.lock() {
        state.closed = true;
    }
    changed.notify_waiters();
}
