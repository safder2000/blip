//! The office network, a port of `blip/net.py`: pets find each other with UDP broadcast and talk
//! over TCP; every packet is a JSON envelope signed with HMAC-SHA256 under a key from the office
//! passphrase. Pets can also meet through a Blip server over a WebSocket, where everything is
//! sealed with the team code first (the server only relays boxes it cannot read).
//! Byte-compatible with the Python pets: same ports, envelopes, signatures and boxes.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use base64::Engine;
use blip_proto::{envelope, link, team};
use serde_json::{json, Map, Value};

pub const UDP_PORT: u16 = 47610;
pub const TCP_PORT: u16 = 47611;
const MAX_FRAME: usize = 32 * 1024;
const HELLO_EVERY: f64 = 15.0;
pub const OFFLINE_AFTER: f64 = 50.0;
const MAX_SKEW: f64 = 600.0;
const MAX_SHARED: usize = 64;

#[derive(Clone, Debug, Default)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub status: String,
    pub stage: String,
    pub palette: String,
    pub species: String,
    pub ver: String,
    pub port: u16,
    pub seen: f64,
    pub voice: bool,
    pub opus: bool,
    pub ip: String,
    /// "lan" or "server"
    pub via: String,
}

pub enum NetEvent {
    /// someone came, went or changed
    Peers,
    /// a message for the app: call, call_cancel, call_reply, voice_end, msg (the whole envelope)
    Message(Map<String, Value>),
    /// call audio through the server
    Voice(String, Vec<u8>),
    /// result of `send` with a tag
    Sent(String, bool),
}

struct Shared {
    id: String,
    key: [u8; 32],
    tkey: Option<[u8; 32]>,
    room: String,
    profile: Mutex<Map<String, Value>>,
    peers: Mutex<HashMap<String, Peer>>,
    learned: Mutex<HashSet<String>>,
    nonces: Mutex<HashMap<String, f64>>,
    manual: Vec<String>,
    running: AtomicBool,
    wake: (Mutex<bool>, Condvar),
    udp: UdpSocket,
    announce_port: u16,
    tcp_port: u16,
    events: Sender<NetEvent>,
    /// (state, detail): off | connecting | connected | error
    server: Mutex<(String, String)>,
    to_server: Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>,
}

pub struct Net {
    sh: Arc<Shared>,
    pub events: Receiver<NetEvent>,
}

fn now() -> f64 {
    blip_core::now()
}

fn port_env(name: &str, default: u16) -> u16 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// This computer's IPv4 addresses on the office network (not loopback / link-local).
pub fn local_ips() -> Vec<String> {
    let mut ips: HashSet<String> = HashSet::new();
    if cfg!(windows) {
        if let Ok(host) = std::env::var("COMPUTERNAME") {
            use std::net::ToSocketAddrs;
            if let Ok(addrs) = (host.as_str(), 0).to_socket_addrs() {
                for a in addrs {
                    if let IpAddr::V4(v) = a.ip() {
                        ips.insert(v.to_string());
                    }
                }
            }
        }
    }
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        // connect() on UDP sends nothing; it just picks the default route's address
        if s.connect("10.255.255.255:1").is_ok() {
            if let Ok(a) = s.local_addr() {
                ips.insert(a.ip().to_string());
            }
        }
    }
    let mut v: Vec<String> = ips.into_iter().filter(|ip| !ip.starts_with("127.") && !ip.starts_with("169.254.") && ip != "0.0.0.0").collect();
    v.sort();
    v
}

fn broadcast_addrs() -> Vec<String> {
    let mut out: Vec<String> = vec!["255.255.255.255".into()];
    for ip in local_ips() {
        let p: Vec<&str> = ip.split('.').collect();
        if p.len() == 4 {
            out.push(format!("{}.{}.{}.255", p[0], p[1], p[2]));
        }
    }
    out.sort();
    out.dedup();
    out
}

fn is_private_v4(ip: &Ipv4Addr) -> bool {
    ip.is_private() && !ip.is_loopback()
}

fn s(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(x)) => x.clone(),
        _ => String::new(),
    }
}

impl Net {
    /// Start listening and announcing. `server`/`team` as in config ("" = LAN only).
    pub fn start(node_id: &str, profile: Map<String, Value>, passphrase: &str, server: &str, team_code: &str, manual: &str) -> Result<Net, String> {
        let udp_port = port_env("BLIP_UDP_PORT", UDP_PORT);
        let want_tcp = port_env("BLIP_TCP_PORT", TCP_PORT);
        let tcp = TcpListener::bind(("0.0.0.0", want_tcp)).or_else(|_| TcpListener::bind("0.0.0.0:0")).map_err(|e| e.to_string())?;
        let tcp_port = tcp.local_addr().map_err(|e| e.to_string())?.port();
        let udp = {
            use socket2::{Domain, Protocol, Socket, Type};
            let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|e| e.to_string())?;
            let _ = sock.set_reuse_address(true);
            let _ = sock.set_broadcast(true);
            let addr: SocketAddr = (Ipv4Addr::UNSPECIFIED, udp_port).into();
            sock.bind(&addr.into()).map_err(|e| e.to_string())?;
            let u: UdpSocket = sock.into();
            let _ = u.set_read_timeout(Some(Duration::from_millis(500)));
            u
        };
        let info = if server.is_empty() { None } else { link::parse(server) };
        let code = if !team_code.is_empty() { team_code.to_string() } else { info.as_ref().map(|i| i.team.clone()).unwrap_or_default() };
        let tkey = (!code.is_empty()).then(|| team::team_key(&code));
        let room = tkey.as_ref().map(|k| team::room_id(k)).unwrap_or_default();
        let (tx, rx) = channel();
        let sh = Arc::new(Shared {
            id: node_id.to_string(),
            key: envelope::key_from_passphrase(passphrase),
            tkey,
            room,
            profile: Mutex::new(profile),
            peers: Mutex::new(HashMap::new()),
            learned: Mutex::new(HashSet::new()),
            nonces: Mutex::new(HashMap::new()),
            manual: manual.replace(';', ",").split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
            running: AtomicBool::new(true),
            wake: (Mutex::new(false), Condvar::new()),
            udp,
            announce_port: udp_port,
            tcp_port,
            events: tx,
            server: Mutex::new(("off".into(), String::new())),
            to_server: Mutex::new(None),
        });
        {
            let sh = sh.clone();
            std::thread::spawn(move || udp_loop(sh));
        }
        {
            let sh = sh.clone();
            std::thread::spawn(move || tcp_loop(sh, tcp));
        }
        {
            let sh = sh.clone();
            std::thread::spawn(move || announce_loop(sh));
        }
        if let (Some(info), Some(_)) = (info, tkey) {
            *sh.server.lock().unwrap() = ("connecting".into(), String::new());
            let sh = sh.clone();
            std::thread::spawn(move || server_thread(sh, info.url));
        }
        Ok(Net { sh, events: rx })
    }

    pub fn stop(&self) {
        let sh = &self.sh;
        if !sh.running.load(Ordering::SeqCst) {
            return;
        }
        send_udp_all(sh, &env(sh, "bye", None, json!({})));
        server_send(sh, &env(sh, "bye", None, json!({})));
        sh.running.store(false, Ordering::SeqCst);
        *sh.to_server.lock().unwrap() = None;
        announce_now(sh);
    }

    pub fn announce_now(&self) {
        announce_now(&self.sh);
    }

    pub fn set_profile(&self, p: Map<String, Value>) {
        *self.sh.profile.lock().unwrap() = p;
    }

    pub fn tcp_port(&self) -> u16 {
        self.sh.tcp_port
    }

    /// Pets seen in the last 50 s, by name.
    pub fn online(&self) -> Vec<Peer> {
        let t = now();
        let mut v: Vec<Peer> = self.sh.peers.lock().unwrap().values().filter(|p| t - p.seen < OFFLINE_AFTER).cloned().collect();
        v.sort_by_key(|p| p.name.to_lowercase());
        v
    }

    pub fn peer(&self, pid: &str) -> Option<Peer> {
        self.sh.peers.lock().unwrap().get(pid).cloned()
    }

    pub fn server_state(&self) -> (String, String) {
        self.sh.server.lock().unwrap().clone()
    }

    /// Send in the background; with a tag, a `Sent(tag, ok)` event follows.
    pub fn send(&self, pid: &str, typ: &str, body: Value, tag: Option<String>) {
        let sh = self.sh.clone();
        let (pid, typ) = (pid.to_string(), typ.to_string());
        std::thread::spawn(move || {
            let p = sh.peers.lock().unwrap().get(&pid).cloned();
            let e = env(&sh, &typ, Some(&pid), body);
            let mut ok = false;
            if let Some(p) = &p {
                if p.via == "lan" && !p.ip.is_empty() {
                    ok = send_tcp(&p.ip, if p.port > 0 { p.port } else { TCP_PORT }, &e);
                }
            }
            if !ok {
                ok = server_send(&sh, &e);
            }
            if let Some(tag) = tag {
                let _ = sh.events.send(NetEvent::Sent(tag, ok));
            }
        });
    }

    /// Call audio through the server (already sealed by the voice session).
    pub fn server_voice(&self, pid: &str, packet: &[u8]) {
        let sh = &self.sh;
        let m = json!({"room": sh.room, "from": sh.id, "to": pid, "v": base64::engine::general_purpose::STANDARD.encode(packet)});
        if let Some(tx) = sh.to_server.lock().unwrap().as_ref() {
            let _ = tx.send(m.to_string());
        }
    }

    /// The key a voice call with `pid` is sealed with (team key through the server).
    pub fn key_for(&self, pid: &str) -> Vec<u8> {
        match self.peer(pid) {
            Some(p) if p.via == "server" => self.sh.tkey.map(|k| k.to_vec()).unwrap_or_else(|| self.sh.key.to_vec()),
            _ => self.sh.key.to_vec(),
        }
    }

    pub fn lan_key(&self) -> Vec<u8> {
        self.sh.key.to_vec()
    }

    pub fn team_key(&self) -> Option<Vec<u8>> {
        self.sh.tkey.map(|k| k.to_vec())
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        self.stop();
    }
}

fn announce_now(sh: &Shared) {
    let (m, cv) = &sh.wake;
    *m.lock().unwrap() = true;
    cv.notify_all();
}

// ------------------------------------------------ envelopes
fn env(sh: &Shared, typ: &str, to: Option<&str>, body: Value) -> Map<String, Value> {
    let mut r = blip_core::Rng::new();
    let m = json!({"v": 1, "type": typ, "from": sh.id, "to": to, "ts": now(), "nonce": r.hex(32), "body": body});
    envelope::seal(&sh.key, m.as_object().cloned().unwrap_or_default())
}

fn hello(sh: &Shared, to: Option<&str>) -> Map<String, Value> {
    let mut body = sh.profile.lock().unwrap().clone();
    body.insert("port".into(), json!(sh.tcp_port));
    let mut ips: Vec<String> = sh.peers.lock().unwrap().values().filter(|p| p.via == "lan" && !p.ip.is_empty()).map(|p| p.ip.clone()).collect();
    ips.sort();
    ips.dedup();
    ips.truncate(MAX_SHARED);
    body.insert("peers".into(), json!(ips));
    env(sh, "hello", to, Value::Object(body))
}

fn learn(sh: &Shared, ips: Option<&Value>) {
    let Some(Value::Array(list)) = ips else { return };
    let mine: HashSet<String> = local_ips().into_iter().collect();
    let mut woke = false;
    for ip in list {
        let Some(txt) = ip.as_str() else { continue };
        let Ok(a) = txt.parse::<Ipv4Addr>() else { continue };
        if !is_private_v4(&a) || mine.contains(&a.to_string()) {
            continue;
        }
        let mut l = sh.learned.lock().unwrap();
        if !l.contains(&a.to_string()) && l.len() < 256 {
            l.insert(a.to_string());
            woke = true;
        }
    }
    if woke {
        announce_now(sh);
    }
}

fn accept(sh: &Shared, d: &Map<String, Value>, key: &[u8]) -> bool {
    if !envelope::check(key, d) {
        return false;
    }
    let from = d.get("from").and_then(Value::as_str).unwrap_or("");
    if from == sh.id {
        return false;
    }
    match d.get("to") {
        None | Some(Value::Null) => {}
        Some(Value::String(t)) if *t == sh.id => {}
        _ => return false,
    }
    let ts = d.get("ts").and_then(Value::as_f64).unwrap_or(0.0);
    if (now() - ts).abs() > MAX_SKEW {
        return false;
    }
    let n = s(d.get("nonce"));
    let t = now();
    let mut nonces = sh.nonces.lock().unwrap();
    if nonces.contains_key(&n) {
        return false;
    }
    nonces.insert(n, t);
    if nonces.len() > 2000 {
        nonces.retain(|_, v| t - *v < MAX_SKEW);
    }
    true
}

fn handle(sh: &Shared, d: Map<String, Value>, ip: Option<String>, via: &str, key: &[u8]) {
    if !accept(sh, &d, key) {
        return;
    }
    let typ = s(d.get("type"));
    let src = s(d.get("from"));
    let empty = Map::new();
    let body = d.get("body").and_then(Value::as_object).unwrap_or(&empty);
    if typ == "hello" {
        let new;
        {
            let mut peers = sh.peers.lock().unwrap();
            new = !peers.contains_key(&src);
            let p = peers.entry(src.clone()).or_insert_with(|| Peer { id: src.clone(), ..Peer::default() });
            if via == "lan" || p.via != "lan" || now() - p.seen > OFFLINE_AFTER {
                p.via = via.to_string();
                if let Some(ip) = &ip {
                    p.ip = ip.clone();
                }
            }
            p.name = s(body.get("name")).chars().take(24).collect();
            if p.name.is_empty() {
                p.name = "?".into();
            }
            p.status = body.get("status").and_then(Value::as_str).unwrap_or("available").to_string();
            p.stage = s(body.get("stage"));
            p.palette = s(body.get("palette"));
            p.species = s(body.get("species"));
            p.ver = s(body.get("ver"));
            p.port = body.get("port").and_then(Value::as_u64).filter(|v| *v > 0 && *v < 65536).unwrap_or(TCP_PORT as u64) as u16;
            p.seen = now();
            p.voice = body.get("voice").and_then(Value::as_bool).unwrap_or(false);
            p.opus = body.get("opus").and_then(Value::as_bool).unwrap_or(false);
        }
        if via == "lan" {
            learn(sh, body.get("peers"));
        }
        let to_none = matches!(d.get("to"), None | Some(Value::Null));
        if new && to_none {
            let reply = hello(sh, Some(&src));
            match (via, &ip) {
                ("lan", Some(ip)) => send_udp(sh, &reply, ip),
                _ => {
                    server_send(sh, &reply);
                }
            }
        }
        let _ = sh.events.send(NetEvent::Peers);
        return;
    }
    if typ == "bye" {
        sh.peers.lock().unwrap().remove(&src);
        let _ = sh.events.send(NetEvent::Peers);
        return;
    }
    if let Some(p) = sh.peers.lock().unwrap().get_mut(&src) {
        p.seen = now();
    }
    let _ = sh.events.send(NetEvent::Message(d));
}

// ------------------------------------------------ LAN
fn send_tcp(ip: &str, port: u16, e: &Map<String, Value>) -> bool {
    let Ok(addr) = format!("{ip}:{port}").parse::<SocketAddr>() else { return false };
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_secs(4)) else { return false };
    let _ = s.set_read_timeout(Some(Duration::from_secs(4)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(4)));
    let mut line = Value::Object(e.clone()).to_string();
    line.push('\n');
    if s.write_all(line.as_bytes()).is_err() {
        return false;
    }
    let mut ack = [0u8; 64];
    match s.read(&mut ack) {
        Ok(n) => ack[..n].starts_with(b"ok"),
        Err(_) => false,
    }
}

fn send_udp(sh: &Shared, e: &Map<String, Value>, addr: &str) {
    let data = Value::Object(e.clone()).to_string();
    let _ = sh.udp.send_to(data.as_bytes(), (addr, sh.announce_port));
}

fn send_udp_all(sh: &Shared, e: &Map<String, Value>) {
    let mut direct: HashSet<String> = sh.learned.lock().unwrap().clone();
    for p in sh.peers.lock().unwrap().values() {
        if p.via == "lan" && !p.ip.is_empty() {
            direct.insert(p.ip.clone());
        }
    }
    for m in &sh.manual {
        direct.insert(m.clone());
    }
    let mut direct: Vec<String> = direct.into_iter().collect();
    direct.sort();
    for a in broadcast_addrs().into_iter().chain(direct) {
        send_udp(sh, e, &a);
    }
}

fn announce_loop(sh: Arc<Shared>) {
    while sh.running.load(Ordering::SeqCst) {
        let h = hello(&sh, None);
        send_udp_all(&sh, &h);
        if sh.to_server.lock().unwrap().is_some() {
            server_send(&sh, &hello(&sh, None));
        }
        let (m, cv) = &sh.wake;
        let mut woke = m.lock().unwrap();
        if !*woke {
            woke = cv.wait_timeout(woke, Duration::from_secs_f64(HELLO_EVERY)).unwrap().0;
        }
        *woke = false;
    }
}

fn udp_loop(sh: Arc<Shared>) {
    let mut buf = vec![0u8; MAX_FRAME];
    while sh.running.load(Ordering::SeqCst) {
        let Ok((n, from)) = sh.udp.recv_from(&mut buf) else { continue };
        if let Ok(Value::Object(d)) = serde_json::from_slice::<Value>(&buf[..n]) {
            handle(&sh, d, Some(from.ip().to_string()), "lan", &sh.key.clone());
        }
    }
}

fn tcp_loop(sh: Arc<Shared>, tcp: TcpListener) {
    let _ = tcp.set_nonblocking(true);
    while sh.running.load(Ordering::SeqCst) {
        match tcp.accept() {
            Ok((conn, addr)) => {
                let sh = sh.clone();
                std::thread::spawn(move || serve(sh, conn, addr.ip().to_string()));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn serve(sh: Arc<Shared>, conn: TcpStream, ip: String) {
    let _ = conn.set_nonblocking(false);
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let mut r = BufReader::new(conn.take(MAX_FRAME as u64));
    let mut line = Vec::new();
    if r.read_until(b'\n', &mut line).is_err() {
        return;
    }
    let mut conn = r.into_inner().into_inner();
    let d = match serde_json::from_slice::<Value>(&line) {
        Ok(Value::Object(d)) => d,
        _ => return,
    };
    let ok = envelope::check(&sh.key, &d);
    let _ = conn.write_all(if ok { b"ok\n" } else { b"no\n" });
    if ok {
        handle(&sh, d, Some(ip), "lan", &sh.key.clone());
    }
}

// ------------------------------------------------ online server
/// Send a signed envelope through the server, sealed for the team.
fn server_send(sh: &Shared, e: &Map<String, Value>) -> bool {
    let Some(tkey) = sh.tkey else { return false };
    let guard = sh.to_server.lock().unwrap();
    let Some(tx) = guard.as_ref() else { return false };
    let mut e = e.clone();
    e.remove("sig");
    if e.get("type").and_then(Value::as_str) == Some("hello") {
        if let Some(Value::Object(b)) = e.get_mut("body") {
            b.remove("peers"); // office IPs mean nothing to other networks
        }
    }
    let e = envelope::seal(&tkey, e);
    let data = blip_proto::pyjson::dumps(&Value::Object(e.clone()));
    let msg = json!({"room": sh.room, "from": sh.id, "to": e.get("to").cloned().unwrap_or(Value::Null), "box": team::seal_box(&tkey, data.as_bytes())});
    tx.send(msg.to_string()).is_ok()
}

fn server_message(sh: &Shared, d: Map<String, Value>) {
    let src = s(d.get("from"));
    if let Some(v) = d.get("v").and_then(Value::as_str) {
        if let Ok(pkt) = base64::engine::general_purpose::STANDARD.decode(v) {
            let _ = sh.events.send(NetEvent::Voice(src, pkt));
        }
        return;
    }
    let Some(tkey) = sh.tkey else { return };
    let Some(raw) = team::open_box(&tkey, d.get("box").and_then(Value::as_str).unwrap_or("")) else { return };
    let Ok(Value::Object(e)) = serde_json::from_slice::<Value>(&raw) else { return };
    if s(e.get("from")) == src {
        handle(sh, e, None, "server", &tkey);
    }
}

fn short_error(e: &str) -> String {
    let l = e.to_lowercase();
    if l.contains("dns") || l.contains("resolve") || l.contains("no such host") || l.contains("nodename") || l.contains("getaddrinfo") {
        "can't find that address".into()
    } else if l.contains("refused") {
        "nothing is listening there".into()
    } else if l.contains("timed out") || l.contains("timeout") {
        "no answer (timed out)".into()
    } else if l.contains("certificate") || l.contains("tls") {
        "certificate problem".into()
    } else {
        e.chars().take(80).collect()
    }
}

fn server_thread(sh: Arc<Shared>, url: String) {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            *sh.server.lock().unwrap() = ("error".into(), e.to_string());
            return;
        }
    };
    rt.block_on(async move {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        let mut delay = 1.0f64;
        while sh.running.load(Ordering::SeqCst) {
            *sh.server.lock().unwrap() = ("connecting".into(), String::new());
            let conn = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async(url.as_str())).await;
            let ws = match conn {
                Ok(Ok((ws, _))) => ws,
                Ok(Err(e)) => {
                    *sh.server.lock().unwrap() = ("error".into(), short_error(&e.to_string()));
                    sleep_while_running(&sh, delay).await;
                    delay = (delay * 2.0).min(30.0);
                    continue;
                }
                Err(_) => {
                    *sh.server.lock().unwrap() = ("error".into(), "no answer (timed out)".into());
                    sleep_while_running(&sh, delay).await;
                    delay = (delay * 2.0).min(30.0);
                    continue;
                }
            };
            delay = 1.0;
            let (mut sink, mut stream) = ws.split();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            *sh.to_server.lock().unwrap() = Some(tx);
            *sh.server.lock().unwrap() = ("connected".into(), String::new());
            server_send(&sh, &hello(&sh, None));
            let mut tick = tokio::time::interval(Duration::from_millis(300));
            loop {
                tokio::select! {
                    out = rx.recv() => match out {
                        Some(text) => {
                            if sink.send(Message::Text(text)).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    },
                    msg = stream.next() => match msg {
                        Some(Ok(Message::Text(t))) => {
                            if let Ok(Value::Object(d)) = serde_json::from_str::<Value>(&t) {
                                server_message(&sh, d);
                            }
                        }
                        Some(Ok(Message::Ping(p))) => {
                            let _ = sink.send(Message::Pong(p)).await;
                        }
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        _ => {}
                    },
                    _ = tick.tick() => {
                        if !sh.running.load(Ordering::SeqCst) {
                            let _ = sink.send(Message::Close(None)).await;
                            break;
                        }
                    }
                }
            }
            *sh.to_server.lock().unwrap() = None;
            if sh.running.load(Ordering::SeqCst) {
                *sh.server.lock().unwrap() = ("error".into(), "connection dropped".into());
                sleep_while_running(&sh, 1.0).await;
            }
        }
        *sh.server.lock().unwrap() = ("off".into(), String::new());
    });
}

async fn sleep_while_running(sh: &Shared, secs: f64) {
    let end = now() + secs;
    while sh.running.load(Ordering::SeqCst) && now() < end {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
