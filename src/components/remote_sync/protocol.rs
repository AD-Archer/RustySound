//! The rustysound-sync wire format, version 1 (the sync server's
//! PROTOCOL.md). One JSON text message per WebSocket frame.
//!
//! The sync server keeps the source of truth in its `crates/protocol`
//! crate. That repository is private and this app is public, so a git
//! dependency would break public builds; these are written to match
//! PROTOCOL.md instead, and `tests` parses every JSON example from it
//! (`fixtures/protocol_examples.json`) so the copies can't drift. Switch to
//! the crate if the sync repository becomes public.

use serde::{Deserialize, Serialize};

/// The protocol version this app speaks (`hello.v`).
pub const PROTOCOL_VERSION: u32 = 1;

/// Unix milliseconds.
pub type UnixMillis = u64;

// ---- Client → server ------------------------------------------------------

/// Every client message: an optional `id`, echoed in the `ack` or `error`
/// it causes, next to the `type`-tagged body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientFrame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub msg: ClientMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Hello(Hello),
    State(PlaybackState),
    Queue(QueueUpdate),
    Cmd(CommandRequest),
    Time { client_at: UnixMillis },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub v: u32,
    /// The Subsonic base URL exactly as the app stores it.
    pub server: String,
    pub user: String,
    pub auth: Auth,
    pub device: DeviceInfo,
}

/// The same login the app sends on `/rest` calls; never logged.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Auth {
    Token { t: String, s: String },
    ApiKey {
        #[serde(rename = "apiKey")]
        api_key: String,
    },
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auth(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub can_play: bool,
}

/// One device's playback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackState {
    pub track_id: Option<String>,
    pub queue_rev: u64,
    pub index: Option<u32>,
    pub position_ms: u64,
    pub playing: bool,
    pub rate: f64,
    pub volume: f64,
    pub sent_at: UnixMillis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueUpdate {
    pub rev: u64,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub target: String,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Op {
    Play,
    Pause,
    Seek { ms: u64 },
    Next,
    Prev,
    Volume { v: f64 },
    PlayQueue { ids: Vec<String>, index: u32, ms: u64 },
    /// Sent by clients only; the target gets a `play_queue` instead.
    Transfer,
}

// ---- Server → client ------------------------------------------------------

/// Every server message carries `ts`, the server's clock when it sent it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerFrame {
    pub ts: UnixMillis,
    #[serde(flatten)]
    pub msg: ServerMessage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome(Welcome),
    Devices { devices: Vec<Device> },
    State(StateSnapshot),
    Queue(QueueSnapshot),
    Cmd(CommandDelivery),
    Ack { id: String },
    Error(ErrorBody),
    Time { client_at: UnixMillis, server_at: UnixMillis },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub v: u32,
    pub device_id: String,
    pub devices: Vec<Device>,
    pub queue: Option<QueueSnapshot>,
    pub state: Option<StateSnapshot>,
    pub limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    #[serde(flatten)]
    pub info: DeviceInfo,
    pub active: bool,
}

/// A device's playback as the room saw it: `at` is the server's clock when
/// it arrived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub from: String,
    pub at: UnixMillis,
    #[serde(flatten)]
    pub state: PlaybackState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub from: String,
    pub at: UnixMillis,
    pub rev: u64,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDelivery {
    pub from: String,
    pub op: Op,
    /// One half of a transfer (the `play_queue` or the `pause`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub transfer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub max_message_bytes: usize,
    pub max_queue_len: usize,
    pub max_devices: usize,
    pub rate_per_sec: u32,
    pub rate_burst: u32,
    pub ping_interval_ms: u64,
}

/// `error.code`. Codes a newer server adds parse as `Unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    HelloRequired,
    HelloTimeout,
    UnsupportedVersion,
    ServerNotAllowed,
    Unauthorized,
    UpstreamUnavailable,
    RoomFull,
    Replaced,
    RateLimited,
    UnknownTarget,
    TargetCannotPlay,
    TargetBusy,
    StaleQueueRev,
    NothingToTransfer,
    ShuttingDown,
    NotFound,
    Internal,
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// Whether the server closes the socket after this error.
    pub fn closes(self) -> bool {
        matches!(
            self,
            Self::HelloRequired
                | Self::HelloTimeout
                | Self::UnsupportedVersion
                | Self::ServerNotAllowed
                | Self::Unauthorized
                | Self::UpstreamUnavailable
                | Self::RoomFull
                | Self::Replaced
                | Self::RateLimited
                | Self::ShuttingDown
        )
    }

    /// After a close with this code: retry with backoff, or wait until the
    /// user changes something (PROTOCOL.md "Reconnecting").
    pub fn should_reconnect(self) -> bool {
        !matches!(
            self,
            Self::Unauthorized | Self::ServerNotAllowed | Self::UnsupportedVersion | Self::Replaced
        )
    }

    /// The code a WebSocket close code stands for, when the `error` frame
    /// before it was lost.
    pub fn from_close_code(code: u16) -> Option<Self> {
        Some(match code {
            4400 => Self::BadRequest,
            4401 => Self::Unauthorized,
            4403 => Self::ServerNotAllowed,
            4408 => Self::HelloTimeout,
            4409 => Self::Replaced,
            4429 => Self::RoomFull,
            4430 => Self::RateLimited,
            4502 => Self::UpstreamUnavailable,
            1001 => Self::ShuttingDown,
            _ => return None,
        })
    }
}

impl ClientFrame {
    pub fn new(msg: ClientMessage) -> Self {
        Self { id: None, msg }
    }

    pub fn with_id(id: impl Into<String>, msg: ClientMessage) -> Self {
        Self { id: Some(id.into()), msg }
    }
}

impl StateSnapshot {
    /// The position at `server_now`, moved on from `position_ms` while
    /// playing (PROTOCOL.md "Clocks").
    pub fn position_at(&self, server_now: UnixMillis) -> u64 {
        if !self.state.playing {
            return self.state.position_ms;
        }
        let elapsed = server_now.saturating_sub(self.at) as f64 * self.state.rate.max(0.0);
        self.state.position_ms.saturating_add(elapsed as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Every JSON example in PROTOCOL.md, copied with its direction.
    const EXAMPLES: &str = include_str!("fixtures/protocol_examples.json");

    fn examples() -> Vec<(String, Value)> {
        let all: Vec<Value> = serde_json::from_str(EXAMPLES).expect("fixture is JSON");
        all.into_iter()
            .map(|entry| (entry["dir"].as_str().unwrap().to_string(), entry["json"].clone()))
            .collect()
    }

    /// Parses, re-serialises and parses again: nothing in an example is
    /// lost or renamed on the way through these types.
    #[test]
    fn every_protocol_example_round_trips() {
        let examples = examples();
        // PROTOCOL.md v1 has 18 (`json client` / `json server` blocks).
        assert_eq!(examples.len(), 18, "re-copy the examples when PROTOCOL.md changes");
        for (dir, json) in examples {
            match dir.as_str() {
                "client" => {
                    let frame: ClientFrame = serde_json::from_value(json.clone())
                        .unwrap_or_else(|e| panic!("client example {json}: {e}"));
                    let back = serde_json::to_value(&frame).unwrap();
                    assert_eq!(back, json, "client example changed on the way back");
                }
                "server" => {
                    let frame: ServerFrame = serde_json::from_value(json.clone())
                        .unwrap_or_else(|e| panic!("server example {json}: {e}"));
                    let back = serde_json::to_value(&frame).unwrap();
                    assert_eq!(back, json, "server example changed on the way back");
                }
                other => panic!("unknown direction {other}"),
            }
        }
    }

    #[test]
    fn examples_cover_every_message_and_op() {
        let examples = examples();
        let types = |dir: &str| -> Vec<String> {
            examples
                .iter()
                .filter(|(d, _)| d == dir)
                .map(|(_, json)| json["type"].as_str().unwrap().to_string())
                .collect()
        };
        for t in ["hello", "state", "queue", "cmd", "time"] {
            assert!(types("client").iter().any(|x| x == t), "client {t}");
        }
        for t in ["welcome", "devices", "state", "queue", "cmd", "ack", "error", "time"] {
            assert!(types("server").iter().any(|x| x == t), "server {t}");
        }
        let kinds: Vec<String> = examples
            .iter()
            .filter_map(|(_, json)| json["op"]["kind"].as_str().map(str::to_string))
            .collect();
        for kind in ["pause", "seek", "play_queue", "transfer"] {
            assert!(kinds.iter().any(|k| k == kind), "op {kind}");
        }
    }

    #[test]
    fn hello_with_a_token_and_with_an_api_key() {
        let hello = examples()
            .into_iter()
            .filter(|(d, json)| d == "client" && json["type"] == "hello")
            .map(|(_, json)| serde_json::from_value::<ClientFrame>(json).unwrap())
            .collect::<Vec<_>>();
        assert!(matches!(&hello[0].msg, ClientMessage::Hello(h) if matches!(h.auth, Auth::Token { .. })));
        assert!(matches!(&hello[1].msg, ClientMessage::Hello(h) if matches!(h.auth, Auth::ApiKey { .. })));
        assert_eq!(format!("{:?}", hello[0]).contains("26719a"), false, "credentials never print");
    }

    #[test]
    fn welcome_in_a_new_room_has_null_queue_and_state() {
        let json = r#"{"ts":1,"type":"welcome","v":1,"device_id":"d","devices":[],"queue":null,"state":null,
            "limits":{"max_message_bytes":1,"max_queue_len":1,"max_devices":1,"rate_per_sec":1,"rate_burst":1,"ping_interval_ms":1}}"#;
        let frame: ServerFrame = serde_json::from_str(json).unwrap();
        let ServerMessage::Welcome(welcome) = frame.msg else { panic!() };
        assert!(welcome.queue.is_none() && welcome.state.is_none());
    }

    #[test]
    fn unknown_fields_are_ignored_and_unknown_codes_tolerated() {
        let json = r#"{"ts":1,"type":"error","code":"brand_new","message":"m","extra":true}"#;
        let frame: ServerFrame = serde_json::from_str(json).unwrap();
        assert!(matches!(frame.msg, ServerMessage::Error(ErrorBody { code: ErrorCode::Unknown, .. })));
    }

    #[test]
    fn a_cmd_without_transfer_leaves_it_out() {
        let cmd = ServerFrame {
            ts: 1,
            msg: ServerMessage::Cmd(CommandDelivery { from: "a".into(), op: Op::Next, transfer: false }),
        };
        assert!(!serde_json::to_string(&cmd).unwrap().contains("transfer"));
    }

    #[test]
    fn reconnect_policy_follows_the_protocol() {
        for code in [ErrorCode::UpstreamUnavailable, ErrorCode::ShuttingDown, ErrorCode::RateLimited, ErrorCode::HelloTimeout] {
            assert!(code.should_reconnect(), "{code:?}");
        }
        for code in [ErrorCode::Unauthorized, ErrorCode::ServerNotAllowed, ErrorCode::UnsupportedVersion, ErrorCode::Replaced] {
            assert!(!code.should_reconnect(), "{code:?}");
        }
        assert_eq!(ErrorCode::from_close_code(4409), Some(ErrorCode::Replaced));
        assert_eq!(ErrorCode::from_close_code(1000), None);
    }

    #[test]
    fn a_playing_state_moves_on_with_the_server_clock() {
        let snap = |playing| StateSnapshot {
            from: "a".into(),
            at: 1_000,
            state: PlaybackState {
                track_id: Some("t".into()),
                queue_rev: 1,
                index: Some(0),
                position_ms: 60_000,
                playing,
                rate: 1.5,
                volume: 1.0,
                sent_at: 990,
            },
        };
        assert_eq!(snap(true).position_at(3_000), 63_000);
        assert_eq!(snap(false).position_at(3_000), 60_000);
        // A server clock behind `at` never moves it back.
        assert_eq!(snap(true).position_at(500), 60_000);
    }
}
