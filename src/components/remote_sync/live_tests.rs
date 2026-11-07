//! The client against a real rustysound-sync: this module's transport and
//! wire types, two "devices", presence, state, a transfer and a remote
//! pause. Needs a running sync server and the mock music server it checks
//! logins with, so it only runs when `RS_SYNC_TEST_URL` is set:
//!
//! ```sh
//! MOCK_PORT=4627 bun tests/mock-server/server.ts &
//! PORT=4790 ALLOWED_SERVERS=http://127.0.0.1:4627 rustysound-sync &
//! RS_SYNC_TEST_URL=http://127.0.0.1:4790 RS_SYNC_TEST_MOCK=http://127.0.0.1:4627 cargo test live_tests
//! ```
//!
//! Never point it at a real server.

use super::logic::{socket_url, Clock};
use super::protocol::*;
use super::transport::{connect, FrameSink, FrameStream, Incoming};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;

struct Peer {
    sink: FrameSink,
    stream: FrameStream,
}

fn env() -> Option<(String, String)> {
    let sync = std::env::var("RS_SYNC_TEST_URL").ok()?;
    let mock = std::env::var("RS_SYNC_TEST_MOCK").unwrap_or_else(|_| "http://127.0.0.1:4627".to_string());
    Some((sync, mock))
}

fn hello(mock: &str, id: &str, name: &str) -> ClientFrame {
    let salt = format!("s{id}");
    let token = format!("{:x}", md5::compute(format!("demo{salt}")));
    ClientFrame::with_id(
        "hello",
        ClientMessage::Hello(Hello {
            v: PROTOCOL_VERSION,
            server: mock.to_string(),
            user: "demo".to_string(),
            auth: Auth::Token { t: token, s: salt },
            device: DeviceInfo { id: id.into(), name: name.into(), platform: "web".into(), can_play: true },
        }),
    )
}

impl Peer {
    async fn join(sync: &str, mock: &str, id: &str, name: &str) -> (Self, Welcome) {
        let (sink, stream) = connect(&socket_url(sync).unwrap()).await.expect("connects");
        let mut peer = Peer { sink, stream };
        peer.send(hello(mock, id, name)).await;
        match peer.next().await.msg {
            ServerMessage::Welcome(welcome) => (peer, welcome),
            other => panic!("expected welcome, got {other:?}"),
        }
    }

    async fn send(&mut self, frame: ClientFrame) {
        self.sink.send(serde_json::to_string(&frame).unwrap()).await.expect("sends");
    }

    async fn next(&mut self) -> ServerFrame {
        loop {
            let incoming = tokio::time::timeout(Duration::from_secs(5), self.stream.next())
                .await
                .expect("a frame within 5 s")
                .expect("socket open");
            match incoming {
                Incoming::Text(text) => return serde_json::from_str(&text).expect("a protocol frame"),
                Incoming::Closed(code) => panic!("closed: {code:?}"),
            }
        }
    }

    /// The next frame matching `pick`, skipping others (acks, presence).
    async fn until<T>(&mut self, mut pick: impl FnMut(ServerMessage) -> Option<T>) -> T {
        for _ in 0..20 {
            if let Some(found) = pick(self.next().await.msg) {
                return found;
            }
        }
        panic!("no matching frame");
    }
}

#[tokio::test]
async fn two_devices_share_state_transfer_and_pause_each_other() {
    let Some((sync, mock)) = env() else {
        eprintln!("RS_SYNC_TEST_URL not set; skipping");
        return;
    };
    let suffix = std::process::id();
    let (a_id, b_id) = (format!("live-a-{suffix}"), format!("live-b-{suffix}"));
    let (mut a, welcome) = Peer::join(&sync, &mock, &a_id, "Mac").await;
    assert!(welcome.devices.iter().any(|d| d.info.id == a_id));

    let (mut b, welcome) = Peer::join(&sync, &mock, &b_id, "iPhone").await;
    assert_eq!(welcome.devices.len(), 2);
    let devices = a.until(|m| match m { ServerMessage::Devices { devices } => Some(devices), _ => None }).await;
    assert_eq!(devices.len(), 2, "A sees B join");

    // A plays: its queue and state reach B.
    a.send(ClientFrame::with_id("q1", ClientMessage::Queue(QueueUpdate { rev: 1, ids: vec!["tr-1".into(), "tr-2".into()] }))).await;
    let state = PlaybackState {
        track_id: Some("tr-2".into()),
        queue_rev: 1,
        index: Some(1),
        position_ms: 30_000,
        playing: true,
        rate: 1.0,
        volume: 0.7,
        sent_at: super::now_ms(),
    };
    a.send(ClientFrame::new(ClientMessage::State(state))).await;
    let queue = b.until(|m| match m { ServerMessage::Queue(q) => Some(q), _ => None }).await;
    assert_eq!(queue.ids, ["tr-1", "tr-2"]);
    let seen = b.until(|m| match m { ServerMessage::State(s) => Some(s), _ => None }).await;
    assert_eq!(seen.from, a_id);
    assert!(seen.state.playing);

    // B asks for the music ("Play here"): B gets the queue at A's position,
    // A is paused.
    b.send(ClientFrame::with_id("t1", ClientMessage::Cmd(CommandRequest { target: b_id.clone(), op: Op::Transfer }))).await;
    let cmd = b.until(|m| match m { ServerMessage::Cmd(c) => Some(c), _ => None }).await;
    assert!(cmd.transfer);
    let Op::PlayQueue { ids, index, ms } = cmd.op else { panic!("expected play_queue, got {:?}", cmd.op) };
    assert_eq!((ids.len(), index), (2, 1));
    assert!(ms >= 30_000, "moved on while playing: {ms}");
    let paused = a.until(|m| match m { ServerMessage::Cmd(c) => Some(c), _ => None }).await;
    assert_eq!((paused.op, paused.transfer), (Op::Pause, true));
    let devices = a.until(|m| match m { ServerMessage::Devices { devices } => Some(devices), _ => None }).await;
    assert!(devices.iter().any(|d| d.info.id == b_id && d.active), "B is active");

    // B pauses A remotely; the command arrives as sent.
    b.send(ClientFrame::with_id("c2", ClientMessage::Cmd(CommandRequest { target: a_id.clone(), op: Op::Seek { ms: 5_000 } }))).await;
    let seek = a.until(|m| match m { ServerMessage::Cmd(c) => Some(c), _ => None }).await;
    assert_eq!(seek.op, Op::Seek { ms: 5_000 });
    assert_eq!(seek.from, b_id);

    // Clock probe: the offset to a local server is tiny.
    let sent = super::now_ms();
    a.send(ClientFrame::new(ClientMessage::Time { client_at: sent })).await;
    let (client_at, server_at) = a.until(|m| match m { ServerMessage::Time { client_at, server_at } => Some((client_at, server_at)), _ => None }).await;
    let mut clock = Clock::default();
    clock.observe_probe(client_at, server_at, super::now_ms());
    assert!(clock.offset_ms.abs() < 1_000, "{}", clock.offset_ms);

    // A command for a device that isn't there comes back as unknown_target.
    a.send(ClientFrame::with_id("c3", ClientMessage::Cmd(CommandRequest { target: "nobody".into(), op: Op::Play }))).await;
    let error = a.until(|m| match m { ServerMessage::Error(e) => Some(e), _ => None }).await;
    assert_eq!((error.code, error.id.as_deref()), (ErrorCode::UnknownTarget, Some("c3")));
}

#[tokio::test]
async fn a_wrong_password_is_refused_without_retrying() {
    let Some((sync, mock)) = env() else {
        return;
    };
    let (sink, stream) = connect(&socket_url(&sync).unwrap()).await.expect("connects");
    let mut peer = Peer { sink, stream };
    let mut frame = hello(&mock, "live-bad", "Bad");
    if let ClientMessage::Hello(hello) = &mut frame.msg {
        hello.auth = Auth::Token { t: "0".repeat(32), s: "x".into() };
    }
    peer.send(frame).await;
    let ServerMessage::Error(error) = peer.next().await.msg else { panic!("expected an error") };
    assert_eq!(error.code, ErrorCode::Unauthorized);
    assert!(!super::logic::should_retry(Some(error.code)));
}
