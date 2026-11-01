//! One WebSocket as a sink and a stream of text frames: tokio-tungstenite
//! (rustls, webpki roots) natively, the browser's WebSocket through gloo-net
//! on the web. The only target-split file of the client.

use futures_util::{Sink, Stream};
use std::pin::Pin;

/// What arrives on the socket.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Text(String),
    /// The socket closed, with the close code when there was one.
    Closed(Option<u16>),
}

pub type FrameSink = Pin<Box<dyn Sink<String, Error = String>>>;
pub type FrameStream = Pin<Box<dyn Stream<Item = Incoming>>>;

#[cfg(not(target_arch = "wasm32"))]
pub async fn connect(url: &str) -> Result<(FrameSink, FrameStream), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let (socket, _) = tokio_tungstenite::connect_async(url)
        .await
        .map_err(|error| error.to_string())?;
    let (sink, stream) = socket.split();
    let sink = sink
        .with(|text: String| async move { Ok::<_, tokio_tungstenite::tungstenite::Error>(Message::Text(text.into())) })
        .sink_map_err(|error| error.to_string());
    // Pings are answered by tungstenite while the stream is read.
    let stream = stream.filter_map(|message| async move {
        match message {
            Ok(Message::Text(text)) => Some(Incoming::Text(text.to_string())),
            Ok(Message::Close(frame)) => Some(Incoming::Closed(frame.map(|f| u16::from(f.code)))),
            Ok(_) => None,
            Err(_) => Some(Incoming::Closed(None)),
        }
    });
    Ok((Box::pin(sink), Box::pin(stream)))
}

#[cfg(target_arch = "wasm32")]
pub async fn connect(url: &str) -> Result<(FrameSink, FrameStream), String> {
    use futures_util::{SinkExt, StreamExt};
    use gloo_net::websocket::{futures::WebSocket, Message, WebSocketError};

    let socket = WebSocket::open(url).map_err(|error| error.to_string())?;
    let (sink, stream) = socket.split();
    let sink = sink
        .with(|text: String| async move { Ok::<_, WebSocketError>(Message::Text(text)) })
        .sink_map_err(|error| error.to_string());
    let stream = stream.filter_map(|message| async move {
        match message {
            Ok(Message::Text(text)) => Some(Incoming::Text(text)),
            Ok(Message::Bytes(_)) => None,
            Err(WebSocketError::ConnectionClose(event)) => Some(Incoming::Closed(Some(event.code))),
            Err(_) => Some(Incoming::Closed(None)),
        }
    });
    Ok((Box::pin(sink), Box::pin(stream)))
}
