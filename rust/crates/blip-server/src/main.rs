//! Blip server: lets pets reach each other over the internet (or across office networks).
//! A drop-in replacement for the Python `blip/server.py`: same port, same messages, same status page.
//!
//!     blip-server                 # listens on 0.0.0.0:47620 (or $PORT if set)
//!     blip-server --port 8080
//!
//! Pets connect with a WebSocket (works behind Cloudflare Tunnel, Render, Railway, nginx, Caddy...)
//! or, for pets older than v0.7, plain TCP lines on the same port. A browser gets a status page.
//!
//! The server can't read anything: each team's pets encrypt every message and every bit of call audio
//! with their team key. It only sees a room id, which pet a message is for, and its size. It forwards.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_PORT: u16 = 47620;
const MAX_FRAME: usize = 64 * 1024;
const MAX_CLIENTS: usize = 5000;
const MAX_PER_ROOM: usize = 1000;
const RATE: f64 = 250.0; // messages per second per connection (voice is 50/s)
const BURST: f64 = 500.0;
const QUEUE: usize = 512; // per-connection send queue; a pet that can't keep up loses messages, not memory
const VERSION: &str = concat!("rust-", env!("CARGO_PKG_VERSION"));

struct Member {
    conn: u64,
    tx: mpsc::Sender<String>,
}

#[derive(Default)]
struct Rooms {
    rooms: HashMap<String, HashMap<String, Member>>,
}

struct Server {
    rooms: Mutex<Rooms>,
    connections: AtomicUsize,
    next_id: AtomicU64,
    started: Instant,
}

impl Server {
    fn stats(&self) -> (usize, usize, usize) {
        let r = self.rooms.lock().unwrap();
        (r.rooms.values().map(|m| m.len()).sum(), r.rooms.len(), self.connections.load(Ordering::Relaxed))
    }
}

/// One connection's view: who it registered as, and its rate limit.
struct Peer {
    conn: u64,
    tx: mpsc::Sender<String>,
    room: Option<String>,
    id: Option<String>,
    tokens: f64,
    last: Instant,
}

impl Peer {
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f64() * RATE).min(BURST);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

fn route(srv: &Server, peer: &mut Peer, text: &str) {
    if text.is_empty() || text.len() > MAX_FRAME || !peer.allow() {
        return;
    }
    let Ok(Value::Object(d)) = serde_json::from_str::<Value>(text) else { return };
    let Some(Value::String(src)) = d.get("from") else { return };
    let room = match d.get("room") {
        Some(Value::String(r)) if !r.is_empty() => r.as_str(),
        None | Some(Value::Null) => "legacy", // pets before v0.7 had no rooms
        Some(Value::String(_)) => "legacy",
        _ => return,
    };
    let to = match d.get("to") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.as_str()),
        _ => return,
    };
    if src.len() > 64 || room.len() > 64 {
        return;
    }
    let targets: Vec<mpsc::Sender<String>> = {
        let mut r = srv.rooms.lock().unwrap();
        if peer.room.as_deref() != Some(room) || peer.id.as_deref() != Some(src.as_str()) {
            unregister(&mut r, peer);
            let members = r.rooms.entry(room.to_string()).or_default();
            if members.len() >= MAX_PER_ROOM && !members.contains_key(src.as_str()) {
                return;
            }
            members.insert(src.clone(), Member { conn: peer.conn, tx: peer.tx.clone() });
            peer.room = Some(room.to_string());
            peer.id = Some(src.clone());
        }
        let Some(members) = r.rooms.get(room) else { return };
        members
            .iter()
            .filter(|(pid, m)| m.conn != peer.conn && to.map_or(true, |t| t == pid.as_str()))
            .map(|(_, m)| m.tx.clone())
            .collect()
    };
    for tx in targets {
        let _ = tx.try_send(text.to_string()); // full queue: drop rather than stall everyone
    }
}

fn unregister(r: &mut Rooms, peer: &Peer) {
    if let (Some(room), Some(id)) = (&peer.room, &peer.id) {
        if let Some(members) = r.rooms.get_mut(room) {
            if members.get(id).map_or(false, |m| m.conn == peer.conn) {
                members.remove(id);
            }
            if members.is_empty() {
                r.rooms.remove(room);
            }
        }
    }
}

async fn serve(srv: Arc<Server>, stream: TcpStream, _addr: SocketAddr) {
    let _ = stream.set_nodelay(true);
    // Look at the first bytes without consuming them: HTTP (WebSocket or status page) or plain lines.
    let mut head = vec![0u8; 8192];
    let deadline = Instant::now() + Duration::from_secs(15);
    let n = loop {
        let n = match tokio::time::timeout(Duration::from_secs(15), stream.peek(&mut head)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => return,
            Ok(Ok(n)) => n,
        };
        let h = &head[..n];
        let is_http = h.starts_with(b"GET ") || h.starts_with(b"HEAD");
        if !is_http || h.windows(4).any(|w| w == b"\r\n\r\n") || n == head.len() || Instant::now() > deadline {
            break n;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let h = String::from_utf8_lossy(&head[..n]).to_string();
    if h.starts_with("GET ") || h.starts_with("HEAD") {
        if h.to_ascii_lowercase().contains("upgrade: websocket") {
            websocket(srv, stream).await;
        } else {
            http(srv, stream, &h).await;
        }
    } else {
        lines(srv, stream).await;
    }
}

fn new_peer(srv: &Server, tx: mpsc::Sender<String>) -> Peer {
    Peer { conn: srv.next_id.fetch_add(1, Ordering::Relaxed), tx, room: None, id: None, tokens: BURST, last: Instant::now() }
}

async fn websocket(srv: Arc<Server>, stream: TcpStream) {
    let mut cfg = WebSocketConfig::default();
    cfg.max_message_size = Some(MAX_FRAME * 4);
    cfg.max_frame_size = Some(MAX_FRAME * 4);
    let Ok(ws) = tokio_tungstenite::accept_async_with_config(stream, Some(cfg)).await else { return };
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = mpsc::channel::<String>(QUEUE);
    let writer = tokio::spawn(async move {
        while let Some(text) = rx.recv().await {
            if sink.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });
    srv.connections.fetch_add(1, Ordering::Relaxed);
    let mut peer = new_peer(&srv, tx);
    while let Some(Ok(msg)) = source.next().await {
        match msg {
            Message::Text(t) => route(&srv, &mut peer, &t),
            Message::Close(_) => break,
            _ => {} // pings are answered by tungstenite; binary isn't used
        }
    }
    unregister(&mut srv.rooms.lock().unwrap(), &peer);
    srv.connections.fetch_sub(1, Ordering::Relaxed);
    drop(peer);
    writer.abort();
}

async fn lines(srv: Arc<Server>, stream: TcpStream) {
    let (read, mut write) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<String>(QUEUE);
    let writer = tokio::spawn(async move {
        while let Some(mut text) = rx.recv().await {
            text.push('\n');
            if write.write_all(text.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    srv.connections.fetch_add(1, Ordering::Relaxed);
    let mut peer = new_peer(&srv, tx);
    let mut reader = BufReader::new(read).take((MAX_FRAME * 4) as u64);
    let mut line = String::new();
    loop {
        line.clear();
        reader.set_limit((MAX_FRAME * 4) as u64);
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => route(&srv, &mut peer, line.trim_end_matches(['\r', '\n'])),
        }
        if !line.ends_with('\n') {
            break; // a line longer than the limit
        }
    }
    unregister(&mut srv.rooms.lock().unwrap(), &peer);
    srv.connections.fetch_sub(1, Ordering::Relaxed);
    drop(peer);
    writer.abort();
}

async fn http(srv: Arc<Server>, mut stream: TcpStream, head: &str) {
    // consume the request we peeked at
    let mut buf = vec![0u8; head.len().min(8192)];
    let _ = tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut buf)).await;
    let first = head.lines().next().unwrap_or("");
    let path = first.split(' ').nth(1).unwrap_or("/");
    let header = |name: &str| {
        head.lines()
            .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()))
            .unwrap_or_default()
    };
    let (pets, teams, connections) = srv.stats();
    let (ctype, body) = if path.starts_with("/health") {
        (
            "application/json",
            format!(
                "{{\"pets\": {pets}, \"teams\": {teams}, \"connections\": {connections}, \"ok\": true, \"version\": \"{VERSION}\", \"uptime\": {}}}",
                srv.started.elapsed().as_secs()
            ),
        )
    } else {
        let host = Some(header("host")).filter(|h| !h.is_empty()).unwrap_or_else(|| "this-server".into());
        let https = header("x-forwarded-proto") == "https" || header("cf-visitor").contains("https");
        let addr = format!("{}://{}", if https { "https" } else { "http" }, html_escape(&host));
        ("text/html; charset=utf-8", PAGE.replace("{pets}", &pets.to_string()).replace("{teams}", &teams.to_string()).replace("{addr}", &addr))
    };
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        body.len(),
        if first.starts_with("HEAD") { "" } else { body.as_str() }
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const PAGE: &str = r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Blip server</title><style>
body{font:16px/1.5 system-ui,sans-serif;background:#191919;color:#efefef;max-width:560px;margin:48px auto;padding:0 16px}
h1{font-size:22px}code{background:#262626;padding:3px 7px;border-radius:4px;word-break:break-all}
.m{color:#9c9c9c}button{font:inherit;background:#8a9bef;color:#fff;border:0;border-radius:6px;padding:8px 14px;cursor:pointer}
#link{display:block;margin:10px 0;padding:10px;background:#262626;border-radius:6px}</style></head><body>
<h1>Blip server is running</h1>
<p class="m">{pets} pets online in {teams} teams.</p>
<p>To connect, enter this address in Blip &rarr; Settings &rarr; Office &rarr; Online server:</p>
<code>{addr}</code>
<p>Everyone who enters it is on one shared team. Want a team of your own on this server?
Make a private link and give it only to your teammates.</p>
<button onclick="mk()">Make a private team link</button><code id="link" hidden></code>
<p class="m">The part after <code>#</code> is your team's secret. Browsers and Blip never send it to this server,
so the server can't read your messages or listen to calls.</p>
<script>function mk(){var a=new Uint8Array(12);crypto.getRandomValues(a);
var t=Array.from(a,function(b){return b.toString(16).padStart(2,'0')}).join('');
var e=document.getElementById('link');e.hidden=false;e.textContent=location.origin+'/#team='+t;}</script>
</body></html>"#;

fn port_from_args() -> u16 {
    let args: Vec<String> = std::env::args().collect();
    let mut port = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().expect("--port takes a number");
                i += 1;
            }
            "-h" | "--help" => {
                println!("blip-server [--port N]   (default {DEFAULT_PORT}, or $PORT)");
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    port
}

#[tokio::main]
async fn main() {
    let port = port_from_args();
    let listener = TcpListener::bind(("0.0.0.0", port)).await.unwrap_or_else(|e| {
        eprintln!("can't listen on port {port}: {e}");
        std::process::exit(1);
    });
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    println!("Blip server listening on 0.0.0.0:{port}");
    let srv = Arc::new(Server {
        rooms: Mutex::new(Rooms::default()),
        connections: AtomicUsize::new(0),
        next_id: AtomicU64::new(1),
        started: Instant::now(),
    });
    loop {
        let Ok((stream, addr)) = listener.accept().await else { continue };
        if srv.connections.load(Ordering::Relaxed) >= MAX_CLIENTS {
            continue; // drops the connection
        }
        tokio::spawn(serve(srv.clone(), stream, addr));
    }
}
