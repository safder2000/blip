//! Voice calls, a port of `blip/voice.py`: 16 kHz mono, 20 ms frames. On the office network audio
//! goes straight between the two PCs over UDP; between networks it goes through the Blip server,
//! compressed with Opus. Each call has its own keys (from the group or team key and a random
//! nonce); every packet is encrypted and authenticated (`blip_proto::voice`), so it interoperates
//! with the Python pets.

use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use blip_proto::voice::{self as proto, Keys};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};
use serde_json::{json, Map, Value};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::cards::ShowCard;
use crate::opus::{self, Decoder, Encoder, FRAME};

const RATE: u32 = 16000;
/// frames (20 ms each) collected before playback starts
const PREBUFFER: usize = 2;
/// above this the queue is trimmed back to PREBUFFER: delay never builds up past ~100 ms
const MAX_BUFFER: usize = 5;
const IDLE_TIMEOUT: f64 = 15.0;

/// A mic and a speaker exist (checked once).
pub fn available() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| {
        if std::env::var_os("BLIP_NO_VOICE").is_some() {
            return false;
        }
        let host = cpal::default_host();
        host.default_input_device().is_some() && host.default_output_device().is_some()
    })
}

pub fn opus_ok() -> bool {
    available() && opus::available()
}

// ================================================================ one side of a call
struct Rx {
    queue: VecDeque<Vec<i16>>,
    playing: bool,
    last_seq: i64,
    received: u64,
    last_rx: f64,
}

struct Shared {
    keys: Keys,
    role: u8,
    running: AtomicBool,
    muted: AtomicBool,
    has_mic: AtomicBool,
    seq: Mutex<u32>,
    enc: Mutex<Option<Encoder>>,
    dec: Mutex<Option<Decoder>>,
    rx: Mutex<Rx>,
    peer: Mutex<Option<SocketAddr>>,
    sock: Option<UdpSocket>,
    transport: Option<Box<dyn Fn(&[u8]) + Send + Sync>>,
    lost: AtomicBool,
    /// their voice volume: 1.0 normal, up to 3.0 with the card's boost slider (f32 bits)
    gain: std::sync::atomic::AtomicU32,
}

impl Shared {
    fn send_frame(&self, pcm: &[i16]) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        let seq = {
            let mut s = self.seq.lock().unwrap();
            let v = *s;
            *s += 1;
            v
        };
        let payload: Vec<u8> = if pcm.len() == FRAME {
            match self.enc.lock().unwrap().as_mut() {
                Some(e) => match e.encode(pcm) {
                    Some(p) => p,
                    None => return,
                },
                None => pcm.iter().flat_map(|v| v.to_le_bytes()).collect(),
            }
        } else {
            vec![]
        };
        let packet = proto::seal(&self.keys, self.role, seq, &payload);
        if let Some(t) = &self.transport {
            t(&packet);
            return;
        }
        if let (Some(sock), Some(peer)) = (&self.sock, *self.peer.lock().unwrap()) {
            let _ = sock.send_to(&packet, peer);
        }
    }

    /// One packet from the other side (UDP or server). True if it was genuine.
    fn feed(&self, data: &[u8]) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }
        let Some((direction, seq, payload)) = proto::open(&self.keys, data) else { return false };
        if direction == self.role {
            return false; // our own direction reflected back
        }
        let mut rx = self.rx.lock().unwrap();
        rx.last_rx = blip_core::now();
        rx.received += 1;
        let seq = seq as i64;
        if seq <= rx.last_seq || payload.len() < 3 {
            return true; // late, duplicate or keepalive
        }
        let pcm: Vec<i16> = match self.dec.lock().unwrap().as_mut() {
            Some(d) => {
                if seq > rx.last_seq + 1 && rx.last_seq >= 0 && seq - rx.last_seq < 4 {
                    for _ in 0..(seq - rx.last_seq - 1) {
                        if let Some(fill) = d.decode(None) {
                            rx.queue.push_back(fill);
                        }
                    }
                }
                match d.decode(Some(&payload)) {
                    Some(p) => p,
                    None => return true,
                }
            }
            None => payload.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect(),
        };
        rx.last_seq = seq;
        if pcm.len() == FRAME {
            rx.queue.push_back(pcm);
            if rx.queue.len() > MAX_BUFFER {
                // the other side sends a bit faster than we play (or a burst came in): drop the
                // oldest audio instead of letting the delay grow
                while rx.queue.len() > PREBUFFER {
                    rx.queue.pop_front();
                }
            }
        }
        true
    }

    /// The next 20 ms to play (silence while buffering).
    fn next_frame(&self) -> Vec<i16> {
        let mut rx = self.rx.lock().unwrap();
        if !rx.playing && rx.queue.len() >= PREBUFFER {
            rx.playing = true;
        }
        if rx.playing {
            if let Some(f) = rx.queue.pop_front() {
                let g = f32::from_bits(self.gain.load(Ordering::Relaxed));
                if g > 1.01 {
                    // louder, with a soft limit so a boosted voice doesn't crackle
                    return f.iter().map(|&v| ((v as f32 / 32768.0 * g).tanh() * 32767.0) as i16).collect();
                }
                return f;
            }
            rx.playing = false; // ran dry: build the buffer up again
        }
        vec![0; FRAME]
    }
}

/// Linear resampler (with a box filter when going down, e.g. 48 kHz -> 16 kHz).
struct Resample {
    step: f64,
    pos: f64,
    buf: Vec<f32>,
    taps: usize,
}

impl Resample {
    fn new(from: u32, to: u32) -> Resample {
        let step = from as f64 / to as f64;
        Resample { step, pos: 0.0, buf: vec![], taps: if step > 1.4 { step.round() as usize } else { 1 } }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.buf.extend_from_slice(input);
        while self.pos + (self.taps as f64) < self.buf.len() as f64 {
            let i = self.pos as usize;
            let v = if self.taps > 1 {
                self.buf[i..i + self.taps].iter().sum::<f32>() / self.taps as f32
            } else {
                let f = (self.pos - i as f64) as f32;
                self.buf[i] * (1.0 - f) + self.buf[i + 1] * f
            };
            out.push(v);
            self.pos += self.step;
        }
        let drop = (self.pos as usize).min(self.buf.len());
        self.buf.drain(..drop);
        self.pos -= drop as f64;
    }
}

pub struct Session {
    sh: Arc<Shared>,
    _input: Option<cpal::Stream>,
    _output: Option<cpal::Stream>,
    pub port: u16,
    started: f64,
    pub error: Option<String>,
}

fn input_stream<T: SizedSample>(dev: &cpal::Device, cfg: &cpal::StreamConfig, sh: Arc<Shared>) -> Result<cpal::Stream, String>
where
    f32: FromSample<T>,
{
    let ch = cfg.channels as usize;
    let mut rs = Resample::new(cfg.sample_rate.0, RATE);
    let mut acc: Vec<f32> = vec![];
    let mut mono: Vec<f32> = vec![];
    dev.build_input_stream(
        cfg,
        move |data: &[T], _| {
            mono.clear();
            for frame in data.chunks(ch) {
                mono.push(frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / ch as f32);
            }
            rs.push(&mono, &mut acc);
            while acc.len() >= FRAME {
                let pcm: Vec<i16> = if sh.muted.load(Ordering::SeqCst) {
                    vec![0; FRAME]
                } else {
                    acc[..FRAME].iter().map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16).collect()
                };
                acc.drain(..FRAME);
                sh.send_frame(&pcm);
            }
        },
        |e| eprintln!("mic: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

fn output_stream<T: SizedSample + FromSample<f32>>(dev: &cpal::Device, cfg: &cpal::StreamConfig, sh: Arc<Shared>) -> Result<cpal::Stream, String> {
    let ch = cfg.channels as usize;
    let mut rs = Resample::new(RATE, cfg.sample_rate.0);
    let mut pending: VecDeque<f32> = VecDeque::new();
    let mut tmp: Vec<f32> = vec![];
    dev.build_output_stream(
        cfg,
        move |data: &mut [T], _| {
            let need = data.len() / ch;
            while pending.len() < need {
                let f = sh.next_frame();
                let fl: Vec<f32> = f.iter().map(|v| *v as f32 / 32768.0).collect();
                tmp.clear();
                rs.push(&fl, &mut tmp);
                pending.extend(tmp.iter());
            }
            for frame in data.chunks_mut(ch) {
                let v = pending.pop_front().unwrap_or(0.0);
                for s in frame.iter_mut() {
                    *s = T::from_sample(v);
                }
            }
        },
        |e| eprintln!("speaker: {e}"),
        None,
    )
    .map_err(|e| e.to_string())
}

impl Session {
    /// role 0 = caller, 1 = the one who answered. `transport` set = through the server.
    fn start(keys: Keys, role: u8, peer: Option<SocketAddr>, transport: Option<Box<dyn Fn(&[u8]) + Send + Sync>>, codec: &str) -> Result<Session, String> {
        let sock = if transport.is_none() {
            let s = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
            let _ = s.set_read_timeout(Some(Duration::from_millis(500)));
            Some(s)
        } else {
            None
        };
        let port = sock.as_ref().and_then(|s| s.local_addr().ok()).map(|a| a.port()).unwrap_or(0);
        let opus = codec == "opus";
        let sh = Arc::new(Shared {
            keys,
            role,
            running: AtomicBool::new(true),
            muted: AtomicBool::new(false),
            has_mic: AtomicBool::new(false),
            seq: Mutex::new(0),
            enc: Mutex::new(if opus { Encoder::new() } else { None }),
            dec: Mutex::new(if opus { Decoder::new() } else { None }),
            rx: Mutex::new(Rx { queue: VecDeque::new(), playing: false, last_seq: -1, received: 0, last_rx: blip_core::now() }),
            peer: Mutex::new(peer),
            sock,
            transport,
            lost: AtomicBool::new(false),
            gain: std::sync::atomic::AtomicU32::new(1.0f32.to_bits()),
        });
        if let Some(sock) = sh.sock.as_ref().and_then(|s| s.try_clone().ok()) {
            let sh = sh.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while sh.running.load(Ordering::SeqCst) {
                    let Ok((n, addr)) = sock.recv_from(&mut buf) else { continue };
                    if sh.feed(&buf[..n]) {
                        let mut p = sh.peer.lock().unwrap();
                        if p.is_none() {
                            *p = Some(addr); // learn where to send from the first genuine packet
                        }
                    }
                }
            });
        }
        {
            let sh = sh.clone();
            std::thread::spawn(move || {
                // keep the line open (and let the other side learn our address) even without a mic
                let fake = std::env::var_os("BLIP_FAKE_MIC").is_some(); // test hook: a 440 Hz tone
                let mut n = 0u64;
                while sh.running.load(Ordering::SeqCst) {
                    if fake {
                        for _ in 0..25 {
                            let f: Vec<i16> = (0..FRAME).map(|i| ((((n * FRAME as u64 + i as u64) as f64) * 440.0 * std::f64::consts::TAU / 16000.0).sin() * 8000.0) as i16).collect();
                            n += 1;
                            sh.send_frame(&f);
                            std::thread::sleep(Duration::from_millis(20));
                        }
                    } else if !sh.has_mic.load(Ordering::SeqCst) {
                        sh.send_frame(&[]);
                    }
                    let (last, got) = {
                        let rx = sh.rx.lock().unwrap();
                        (rx.last_rx, rx.received)
                    };
                    if got > 0 && blip_core::now() - last > IDLE_TIMEOUT {
                        sh.lost.store(true, Ordering::SeqCst);
                        return;
                    }
                    if !fake {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
            });
        }
        let mut s = Session { sh: sh.clone(), _input: None, _output: None, port, started: blip_core::now(), error: None };
        if std::env::var_os("BLIP_NO_AUDIO").is_none() {
            let host = cpal::default_host();
            let open = || -> Result<(cpal::Stream, cpal::Stream), String> {
                let out_dev = host.default_output_device().ok_or("no speaker")?;
                let oc = out_dev.default_output_config().map_err(|e| e.to_string())?;
                let out = match oc.sample_format() {
                    cpal::SampleFormat::I16 => output_stream::<i16>(&out_dev, &oc.config(), sh.clone())?,
                    cpal::SampleFormat::U16 => output_stream::<u16>(&out_dev, &oc.config(), sh.clone())?,
                    _ => output_stream::<f32>(&out_dev, &oc.config(), sh.clone())?,
                };
                out.play().map_err(|e| e.to_string())?;
                let in_dev = host.default_input_device().ok_or("no microphone")?;
                let ic = in_dev.default_input_config().map_err(|e| e.to_string())?;
                let inp = match ic.sample_format() {
                    cpal::SampleFormat::I16 => input_stream::<i16>(&in_dev, &ic.config(), sh.clone())?,
                    cpal::SampleFormat::U16 => input_stream::<u16>(&in_dev, &ic.config(), sh.clone())?,
                    _ => input_stream::<f32>(&in_dev, &ic.config(), sh.clone())?,
                };
                inp.play().map_err(|e| e.to_string())?;
                Ok((inp, out))
            };
            match open() {
                Ok((i, o)) => {
                    sh.has_mic.store(true, Ordering::SeqCst);
                    s._input = Some(i);
                    s._output = Some(o);
                }
                Err(e) => s.error = Some(e),
            }
        }
        Ok(s)
    }

    fn stop(&mut self) {
        self.sh.running.store(false, Ordering::SeqCst);
        self._input = None;
        self._output = None;
    }

    fn seconds(&self) -> f64 {
        blip_core::now() - self.started
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

// ================================================================ the app side
struct Call {
    cid: String,
    peer: String,
    sess: Session,
    note: String,
    shown: String,
}

#[derive(Default)]
pub struct Voice {
    call: Option<Call>,
}

pub fn active(app: &App) -> bool {
    app.voice.call.is_some()
}

pub fn can_call(app: &App, pid: &str) -> bool {
    if !available() || active(app) {
        return false;
    }
    let p = app.office.net.as_ref().and_then(|n| n.peer(pid));
    p.map_or(false, |p| p.voice && (p.via == "lan" || p.via == "server"))
}

fn new_nonce() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[allow(clippy::too_many_arguments)]
fn start(app: &mut App, el: &ActiveEventLoop, pid: &str, name: &str, cid: &str, role: u8, nonce: &str, peer: Option<SocketAddr>, server: bool, codec: &str) -> Option<u16> {
    let net = app.office.net.as_ref()?;
    let gkey = if server { net.team_key()? } else { net.lan_key() };
    let keys = proto::session_keys(&gkey, cid, &unhex(nonce)?);
    let transport = if server { Some(net.voice_sender(pid)) } else { None };
    let sess = match Session::start(keys, role, peer, transport, codec) {
        Ok(s) => {
            if server {
                // call audio from the server goes straight into the call, not via the app loop
                let sh = s.sh.clone();
                net.set_voice_sink(Some((pid.to_string(), Box::new(move |pkt: &[u8]| {
                    sh.feed(pkt);
                }))));
            }
            s
        }
        Err(e) => {
            let body = format!("Couldn't start the call audio: {e}");
            app.show_card(el, ShowCard { key: "", title: "Voice", body: &body, kind: "call", icon: Some("phone"), buttons: vec![("OK".into(), String::new())], timeout: Some(20.0), on_close: "", slider: None });
            return None;
        }
    };
    let port = sess.port;
    let note = match &sess.error {
        Some(e) => format!("\nCouldn't open your microphone: {e}"),
        None => "\nTip: a headset stops echo.".to_string(),
    };
    crate::log(&format!("voice start role={role} server={server} codec={codec} audio_error={:?}", sess.error));
    app.voice.call = Some(Call { cid: cid.to_string(), peer: pid.to_string(), sess, note: note.clone(), shown: String::new() });
    crate::office::log(app, if role == 1 { "in" } else { "out" }, pid, name, "\u{1F3A7} voice call");
    let now = app.now();
    if app.pet.hatched() {
        app.body.happy_until = now + 2.0;
    }
    let body = format!("00:00{note}");
    app.show_card(
        el,
        ShowCard {
            key: "voice",
            title: &format!("Talking with {name}"),
            body: &body,
            kind: "call",
            icon: Some("phone"),
            buttons: vec![("Hang up".into(), "voice:hangup".into()), ("Mute".into(), "stay:voice:mute".into())],
            timeout: None,
            on_close: "voice:closed:{why}",
            slider: Some(("Boost", 0, "voice:boost")),
        },
    );
    Some(port)
}

/// We answered with "Talk".
pub fn answer(app: &mut App, el: &ActiveEventLoop, src: &str, name: &str, cid: &str) {
    let nonce = new_nonce();
    let Some(p) = app.office.net.as_ref().and_then(|n| n.peer(src)) else { return };
    let info = if p.via == "server" {
        let codec = if p.opus && opus_ok() { "opus" } else { "pcm" };
        if start(app, el, src, name, cid, 1, &nonce, None, true, codec).is_none() {
            return;
        }
        json!({"nonce": nonce, "route": "server", "codec": codec})
    } else {
        let Some(port) = start(app, el, src, name, cid, 1, &nonce, None, false, "pcm") else { return };
        json!({"port": port, "nonce": nonce})
    };
    crate::office::send(app, src, "call_reply", json!({"call_id": cid, "text": "Talking", "voice": info}));
}

/// The other side answered our call with "Talk": start our half. False = not a voice answer.
pub fn start_from_reply(app: &mut App, el: &ActiveEventLoop, src: &str, name: &str, cid: &str, v: &Map<String, Value>) -> bool {
    if !available() || active(app) {
        return false;
    }
    let Some(p) = app.office.net.as_ref().and_then(|n| n.peer(src)) else { return false };
    let nonce = match v.get("nonce") {
        Some(Value::String(s)) if unhex(s).is_some() => s.clone(),
        _ => return false,
    };
    let port = v.get("port").and_then(Value::as_u64).unwrap_or(0) as u16;
    let codec = match v.get("codec").and_then(Value::as_str) {
        Some("opus") if opus_ok() => "opus",
        _ => "pcm",
    };
    if v.get("route").and_then(Value::as_str) == Some("server") {
        return start(app, el, src, name, cid, 0, &nonce, None, true, codec).is_some();
    }
    if !p.ip.is_empty() && port > 0 {
        let addr: Option<SocketAddr> = format!("{}:{port}", p.ip).parse().ok();
        if let Some(a) = addr {
            return start(app, el, src, name, cid, 0, &nonce, Some(a), false, "pcm").is_some();
        }
    }
    false
}

/// Call audio that came through the server.
pub fn packet(app: &mut App, src: &str, pkt: &[u8]) {
    if let Some(c) = &app.voice.call {
        if c.peer == src {
            c.sess.sh.feed(pkt);
        }
    }
}

pub fn remote_end(app: &mut App, cid: &str) {
    if app.voice.call.as_ref().map_or(false, |c| c.cid == cid) {
        end(app, false, Some("They hung up"));
    }
}

fn end(app: &mut App, notify: bool, why: Option<&str>) {
    let Some(mut c) = app.voice.call.take() else { return };
    let secs = c.sess.seconds() as i64;
    let (got, heard) = {
        let rx = c.sess.sh.rx.lock().unwrap();
        (rx.received, rx.last_seq + 1)
    };
    crate::log(&format!("voice end why={why:?} received={got} heard_frames={heard}"));
    c.sess.stop();
    if let Some(n) = &app.office.net {
        n.set_voice_sink(None);
    }
    if notify {
        crate::office::send(app, &c.peer, "voice_end", json!({"call_id": c.cid}));
    }
    app.cards.remove("voice");
    let text = why.map(str::to_string).unwrap_or_else(|| format!("Call ended {}:{:02}", secs / 60, secs % 60));
    app.board(&text, 4.0);
}

pub fn run(app: &mut App, _el: &ActiveEventLoop, cmd: &str) {
    match cmd {
        "voice:hangup" | "voice:closed:x" => end(app, true, None),
        _ if cmd.starts_with("voice:boost:") => {
            let level: f32 = cmd["voice:boost:".len()..].parse().unwrap_or(0.0);
            if let Some(c) = &app.voice.call {
                c.sess.sh.gain.store((1.0 + 0.25 * level.clamp(0.0, 8.0)).to_bits(), Ordering::Relaxed);
            }
        }
        "voice:mute" => {
            if let Some(c) = &mut app.voice.call {
                let m = !c.sess.sh.muted.load(Ordering::SeqCst);
                c.sess.sh.muted.store(m, Ordering::SeqCst);
                c.shown.clear();
            }
        }
        _ => {}
    }
}

/// Every frame: the call timer on the card, and noticing a lost line.
pub fn tick(app: &mut App, _el: &ActiveEventLoop) {
    let Some(c) = &mut app.voice.call else { return };
    if c.sess.sh.lost.load(Ordering::SeqCst) {
        end(app, true, Some("Lost the connection"));
        return;
    }
    let t = c.sess.seconds() as i64;
    let mut state = String::new();
    if c.sess.sh.muted.load(Ordering::SeqCst) {
        state += "  \u{b7}  muted";
    }
    if c.sess.sh.rx.lock().unwrap().received == 0 && t > 4 {
        state += "  \u{b7}  connecting\u{2026}";
    }
    let body = format!("{:02}:{:02}{state}{}", t / 60, t % 60, c.note);
    if body != c.shown {
        c.shown = body.clone();
        let (theme, scale) = (app.theme, app.geo.s);
        app.cards.set_body("voice", &body, &theme, &mut app.fonts, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_lengths() {
        let mut down = Resample::new(48000, 16000);
        let mut out = vec![];
        down.push(&vec![0.5; 4800], &mut out);
        assert!((1590..=1600).contains(&out.len()), "{}", out.len());
        assert!((out[10] - 0.5).abs() < 1e-6);
        let mut up = Resample::new(16000, 44100);
        let mut out = vec![];
        up.push(&vec![0.25; 1600], &mut out);
        assert!((4400..=4410).contains(&out.len()), "{}", out.len());
    }

    #[test]
    fn pcm_roundtrip_between_sessions() {
        // two halves of a call over loopback UDP, no audio devices
        std::env::set_var("BLIP_NO_AUDIO", "1");
        let keys = || proto::session_keys(b"k", "call1", &[1u8; 16]);
        let b = Session::start(keys(), 1, None, None, "pcm").unwrap();
        let addr: SocketAddr = format!("127.0.0.1:{}", b.port).parse().unwrap();
        let a = Session::start(keys(), 0, Some(addr), None, "pcm").unwrap();
        let frame: Vec<i16> = (0..FRAME as i16).collect();
        for _ in 0..5 {
            a.sh.send_frame(&frame);
            std::thread::sleep(Duration::from_millis(30));
        }
        std::thread::sleep(Duration::from_millis(300));
        assert!(b.sh.rx.lock().unwrap().received >= 5);
        assert_eq!(b.sh.next_frame(), frame);
        // b learned a's address and can answer
        assert!(b.sh.peer.lock().unwrap().is_some());
    }
}
