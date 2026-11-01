//! A tiny HTTP server that lets a speaker fetch songs stored on this
//! device ("On This Device"), which it can't reach otherwise. Only while a
//! cast plays one:
//!
//! * bound to the local address that faces the speaker, on a random port;
//! * every URL has an unguessable per-session secret and a per-file token,
//!   and only files the cast handed over (`share`, the last few) are
//!   served; anything else is a 404;
//! * GET and HEAD with byte ranges (speakers seek and buffer that way).

use rand::Rng;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// How many shared files stay reachable (the current song, plus a couple
/// the speaker may still be reading).
const KEEP_FILES: usize = 4;
const MAX_HEAD: usize = 8 * 1024;

#[derive(Clone)]
struct Shared {
    token: String,
    path: PathBuf,
    mime: String,
}

pub struct LanServer {
    addr: SocketAddr,
    secret: String,
    files: Arc<Mutex<Vec<Shared>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for LanServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn random_hex(bytes: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..bytes).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

/// This device's address on the network that reaches `peer` (a UDP
/// "connect" sends nothing; it only picks the route).
pub fn local_ip_facing(peer: IpAddr) -> Option<IpAddr> {
    let bind: SocketAddr = if peer.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" }.parse().ok()?;
    let socket = std::net::UdpSocket::bind(bind).ok()?;
    socket.connect(SocketAddr::new(peer, 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

impl LanServer {
    /// Starts serving on the interface that faces `peer`.
    pub async fn start(peer: IpAddr) -> std::io::Result<Self> {
        let ip = local_ip_facing(peer)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no route to the speaker"))?;
        let listener = TcpListener::bind(SocketAddr::new(ip, 0)).await?;
        let addr = listener.local_addr()?;
        let secret = random_hex(16);
        let files: Arc<Mutex<Vec<Shared>>> = Arc::new(Mutex::new(Vec::new()));
        let task = {
            let secret = secret.clone();
            let files = files.clone();
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let secret = secret.clone();
                    let files = files.clone();
                    tokio::spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(30 * 60), serve(stream, &secret, &files)).await;
                    });
                }
            })
        };
        Ok(LanServer { addr, secret, files, task })
    }

    /// A URL the speaker can fetch `path` from, until a few more files
    /// have been shared or the server stops.
    pub fn share(&self, path: PathBuf, mime: &str) -> String {
        let token = random_hex(8);
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("bin").to_ascii_lowercase();
        if let Ok(mut files) = self.files.lock() {
            files.push(Shared { token: token.clone(), path, mime: mime.to_string() });
            let excess = files.len().saturating_sub(KEEP_FILES);
            files.drain(..excess);
        }
        format!("http://{}/{}/{token}.{ext}", self.addr, self.secret)
    }
}

/// The request line's method and path, and the `Range` header.
pub fn parse_request_head(head: &str) -> Option<(String, String, Option<String>)> {
    let mut lines = head.split("\r\n");
    let mut request = lines.next()?.split_whitespace();
    let method = request.next()?.to_ascii_uppercase();
    let path = request.next()?.to_string();
    let range = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.trim().to_string());
    Some((method, path, range))
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` against a file of `size` bytes:
/// the inclusive byte span, or `None` when it can't be satisfied.
pub fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    let (start, end) = match (start.trim(), end.trim()) {
        ("", suffix) => {
            let suffix: u64 = suffix.parse().ok()?;
            (size.saturating_sub(suffix), size - 1)
        }
        (start, "") => (start.parse().ok()?, size - 1),
        (start, end) => (start.parse().ok()?, end.parse::<u64>().ok()?.min(size - 1)),
    };
    (start <= end && start < size).then_some((start, end))
}

async fn respond(stream: &mut TcpStream, status: &str) {
    let _ = stream
        .write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
        .await;
}

async fn serve(mut stream: TcpStream, secret: &str, files: &Mutex<Vec<Shared>>) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        if head.len() > MAX_HEAD {
            return Ok(());
        }
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf))
            .await
            .map_err(|_| std::io::ErrorKind::TimedOut)??;
        if read == 0 {
            return Ok(());
        }
        head.extend_from_slice(&buf[..read]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let Some((method, path, range)) = parse_request_head(&head) else {
        respond(&mut stream, "400 Bad Request").await;
        return Ok(());
    };
    if method != "GET" && method != "HEAD" {
        respond(&mut stream, "405 Method Not Allowed").await;
        return Ok(());
    }
    let token = path
        .strip_prefix('/')
        .and_then(|rest| rest.strip_prefix(secret))
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.split('.').next());
    let shared = token.and_then(|token| {
        files.lock().ok()?.iter().find(|file| file.token == token).cloned()
    });
    let Some(shared) = shared else {
        respond(&mut stream, "404 Not Found").await;
        return Ok(());
    };
    let Ok(mut file) = tokio::fs::File::open(&shared.path).await else {
        respond(&mut stream, "404 Not Found").await;
        return Ok(());
    };
    let size = file.metadata().await?.len();
    let span = match range.as_deref() {
        Some(range) => match parse_range(range, size) {
            Some(span) => Some(span),
            None => {
                let _ = stream
                    .write_all(
                        format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                            .as_bytes(),
                    )
                    .await;
                return Ok(());
            }
        },
        None => None,
    };
    let (start, end) = span.unwrap_or((0, size.saturating_sub(1)));
    let length = if size == 0 { 0 } else { end - start + 1 };
    let mut reply = match span {
        Some(_) => format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}\r\n"),
        None => "HTTP/1.1 200 OK\r\n".to_string(),
    };
    reply.push_str(&format!(
        "Content-Type: {}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\n\
         transferMode.dlna.org: Streaming\r\ncontentFeatures.dlna.org: DLNA.ORG_OP=01;DLNA.ORG_CI=0\r\n\
         Connection: close\r\n\r\n",
        shared.mime
    ));
    stream.write_all(reply.as_bytes()).await?;
    if method == "GET" && length > 0 {
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut body = file.take(length);
        tokio::io::copy(&mut body, &mut stream).await?;
    }
    stream.flush().await
}
