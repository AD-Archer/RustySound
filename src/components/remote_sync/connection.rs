//! The connection loop: connect, `hello`, `welcome`, then one stream of
//! events (frames in, frames out, clock probes) until the socket ends;
//! reconnects with backoff unless the server said not to. Runs as the
//! `RemoteSync` coroutine, so a settings change restarts it.

use super::logic::{backoff_delay_ms, error_message, probe_gap_ms, should_retry};
use super::protocol::{ClientFrame, ClientMessage, ErrorCode, Hello, ServerFrame, ServerMessage};
use super::transport::{connect, Incoming};
use super::{RemoteSync, SyncStatus};
use crate::components::ui::env::sleep_ms;
use dioxus::prelude::*;
use futures_util::future::select;
use futures_util::{stream, SinkExt, StreamExt};

/// What the rest of the app sends the connection.
pub enum Outgoing {
    Frame(ClientFrame),
    /// Reconnect now if not connected (the app came back to the
    /// foreground, or the web page got its first tap).
    Wake,
}

/// Where and as whom to connect.
#[derive(Clone, PartialEq)]
pub struct Target {
    pub socket_url: String,
    pub hello: Hello,
}

/// How a session ended.
struct End {
    welcomed: bool,
    code: Option<ErrorCode>,
}

enum Event {
    In(Incoming),
    Out(Outgoing),
    Probe,
}

/// Runs until the coroutine is restarted or dropped.
pub async fn run(sync: RemoteSync, target: Target, rx: &mut UnboundedReceiver<Outgoing>) {
    let mut attempt = 0u32;
    loop {
        sync.set_status(SyncStatus::Connecting);
        let end = session(sync, &target, rx).await;
        sync.on_disconnected();
        if end.welcomed {
            attempt = 0;
        }
        let message = end
            .code
            .map(|code| error_message(code, &target.hello.server))
            .unwrap_or_else(|| "Can't reach the sync server. Retrying…".to_string());
        if !should_retry(end.code) {
            sync.set_status(SyncStatus::Failed(message));
            wait_for_wake(rx).await;
            continue;
        }
        sync.set_status(SyncStatus::Retrying(message));
        let delay = backoff_delay_ms(attempt, rand::random::<f64>());
        attempt = attempt.saturating_add(1);
        // A wake (foreground, first tap) skips the rest of the wait.
        let _ = select(Box::pin(sleep_ms(delay)), Box::pin(wait_for_wake(rx))).await;
    }
}

/// Drops frames until a `Wake` arrives (nothing can be sent while
/// disconnected; state is sent afresh after the next `welcome`).
async fn wait_for_wake(rx: &mut UnboundedReceiver<Outgoing>) {
    while let Some(message) = rx.next().await {
        if matches!(message, Outgoing::Wake) {
            return;
        }
    }
    // The sender is gone: the app is closing.
    std::future::pending::<()>().await;
}

async fn session(sync: RemoteSync, target: &Target, rx: &mut UnboundedReceiver<Outgoing>) -> End {
    let mut end = End { welcomed: false, code: None };
    let Ok((mut sink, socket)) = connect(&target.socket_url).await else {
        return end;
    };
    let hello = ClientFrame::with_id("hello", ClientMessage::Hello(target.hello.clone()));
    if send(&mut sink, &hello).await.is_err() {
        return end;
    }

    // A stream that ended is a close without a code.
    let incoming = socket
        .map(Event::In)
        .chain(stream::once(async { Event::In(Incoming::Closed(None)) }));
    let outgoing = rx.by_ref().map(Event::Out);
    let probes = stream::unfold(0u32, |sent| async move {
        sleep_ms(probe_gap_ms(sent)).await;
        Some((Event::Probe, sent + 1))
    });
    let mut events = std::pin::pin!(stream::select(stream::select(incoming, outgoing), probes));

    while let Some(event) = events.next().await {
        let replies = match event {
            Event::In(Incoming::Text(text)) => {
                // Unknown message types (a newer server) are skipped.
                let Ok(frame) = serde_json::from_str::<ServerFrame>(&text) else {
                    continue;
                };
                let welcome = matches!(frame.msg, ServerMessage::Welcome(_));
                if let ServerMessage::Error(error) = &frame.msg {
                    if error.code.closes() {
                        end.code = Some(error.code);
                    }
                }
                let mut replies = sync.on_frame(frame);
                if welcome {
                    end.welcomed = true;
                    // The first clock probe goes with our state.
                    replies.push(time_probe());
                }
                replies
            }
            Event::In(Incoming::Closed(code)) => {
                end.code = end.code.or_else(|| code.and_then(ErrorCode::from_close_code));
                break;
            }
            Event::Out(Outgoing::Frame(frame)) if end.welcomed => vec![frame],
            Event::Out(_) => Vec::new(),
            Event::Probe if end.welcomed => vec![time_probe()],
            Event::Probe => Vec::new(),
        };
        for frame in replies {
            if send(&mut sink, &frame).await.is_err() {
                return end;
            }
        }
    }
    end
}

fn time_probe() -> ClientFrame {
    ClientFrame::new(ClientMessage::Time { client_at: super::now_ms() })
}

async fn send(sink: &mut super::transport::FrameSink, frame: &ClientFrame) -> Result<(), ()> {
    let text = serde_json::to_string(frame).map_err(|_| ())?;
    sink.send(text).await.map_err(|_| ())
}
