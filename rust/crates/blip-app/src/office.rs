//! Colleagues: the Office part of the menu, calls (ring cards and quick replies), messages and the
//! inbox, on top of `net.rs`. Ported from the network half of `blip/app.py`.

use std::collections::HashMap;
use std::path::PathBuf;

use blip_pet::body::Action;
use blip_pet::sprites::Color;
use blip_ui::menu::{Item, Person};
use blip_ui::theme::Theme;
use egui::Ui;
use serde_json::{json, Map, Value};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::cards::ShowCard;
use crate::net::{self, Net, NetEvent};

pub const DEFAULT_GROUP: &str = "blip-office";
const CALL_RING_SECS: f64 = 60.0;
const SHOW: usize = 5;

#[derive(Clone, Debug, Default)]
pub struct Msg {
    pub ts: f64,
    pub text: String,
    pub name: String,
    pub peer: String,
    pub out: bool,
}

pub use crate::net::Peer;

pub struct Office {
    pub net: Option<Net>,
    pub error: Option<String>,
    inbox_path: PathBuf,
    pub inbox: Vec<Value>,
    calls_out: HashMap<String, (String, String)>,
    calls_in: HashMap<String, (String, String)>,
    restart_at: Option<f64>,
    profile_sig: String,
    next_profile: f64,
}

impl Office {
    pub fn load(dir: &std::path::Path) -> Office {
        let inbox_path = dir.join("inbox.json");
        let inbox = match blip_core::store::read_json(&inbox_path) {
            Some(Value::Array(v)) => v,
            _ => vec![],
        };
        Office {
            net: None,
            error: None,
            inbox_path,
            inbox,
            calls_out: HashMap::new(),
            calls_in: HashMap::new(),
            restart_at: None,
            profile_sig: String::new(),
            next_profile: 0.0,
        }
    }
}

fn profile(app: &App) -> Map<String, Value> {
    let m = json!({
        "name": app.cfg.str("display_name"),
        "status": if app.cfg.bool("dnd") { "busy" } else { "available" },
        "stage": app.pet.stage(),
        "palette": app.pet.palette(),
        "species": app.pet.species(),
        "voice": crate::voice::available(),
        "opus": crate::voice::opus_ok(),
        "ver": crate::windows::VERSION,
    });
    m.as_object().cloned().unwrap_or_default()
}

/// Start (or restart) the network from the settings.
pub fn start(app: &mut App) {
    if let Some(n) = app.office.net.take() {
        n.stop();
    }
    app.office.error = None;
    if !app.cfg.bool("network") {
        return;
    }
    let server = app.cfg.str("server").to_string();
    let team = blip_proto::link::team_for(&server, app.cfg.str("team"));
    let pass = if app.cfg.str("passphrase").is_empty() { DEFAULT_GROUP.to_string() } else { app.cfg.str("passphrase").to_string() };
    match Net::start(app.cfg.str("node_id"), profile(app), &pass, &server, &team, app.cfg.str("manual_peers")) {
        Ok(n) => app.office.net = Some(n),
        Err(e) => app.office.error = Some(e),
    }
}

pub fn shutdown(app: &mut App) {
    if let Some(n) = app.office.net.take() {
        n.stop();
    }
}

/// Called every frame: network events, debounced restarts, profile changes.
pub fn tick(app: &mut App, el: &ActiveEventLoop) {
    let now = blip_core::now();
    if app.office.restart_at.map_or(false, |t| now >= t) {
        app.office.restart_at = None;
        start(app);
        app.windows_dirty = true;
    }
    if now >= app.office.next_profile {
        app.office.next_profile = now + 1.0;
        let p = profile(app);
        let sig = Value::Object(p.clone()).to_string();
        if sig != app.office.profile_sig {
            let first = app.office.profile_sig.is_empty();
            app.office.profile_sig = sig;
            if let Some(n) = &app.office.net {
                n.set_profile(p);
                if !first {
                    n.announce_now();
                }
            }
        }
    }
    let mut evs = vec![];
    if let Some(n) = &app.office.net {
        while let Ok(e) = n.events.try_recv() {
            evs.push(e);
        }
    }
    for e in evs {
        match e {
            NetEvent::Peers => app.windows_dirty = true,
            NetEvent::Message(d) => on_message(app, el, d),
            NetEvent::Voice(src, pkt) => crate::voice::packet(app, &src, &pkt),
            NetEvent::Sent(tag, ok) => sent(app, el, &tag, ok),
        }
    }
}

pub fn peers(app: &App) -> Vec<Peer> {
    app.office.net.as_ref().map(|n| n.online()).unwrap_or_default()
}

pub fn inbox(app: &App) -> Vec<Msg> {
    app.office
        .inbox
        .iter()
        .map(|e| Msg {
            ts: e.get("ts").and_then(Value::as_f64).unwrap_or(0.0),
            text: e.get("text").and_then(Value::as_str).unwrap_or("").to_string(),
            name: e.get("name").and_then(Value::as_str).unwrap_or("?").to_string(),
            peer: e.get("peer").and_then(Value::as_str).unwrap_or("").to_string(),
            out: e.get("dir").and_then(Value::as_str) == Some("out"),
        })
        .collect()
}

pub fn peer_name(app: &App, id: &str) -> Option<String> {
    app.office.net.as_ref().and_then(|n| n.peer(id)).map(|p| p.name).filter(|n| !n.is_empty())
}

fn name_of(app: &App, id: &str) -> String {
    peer_name(app, id).unwrap_or_else(|| "Someone".into())
}

fn log_inbox(app: &mut App, dir: &str, peer: &str, name: &str, text: &str) {
    let text: String = text.chars().take(500).collect();
    app.office.inbox.push(json!({"ts": blip_core::now(), "dir": dir, "peer": peer, "name": name, "text": text}));
    let n = app.office.inbox.len();
    if n > 100 {
        app.office.inbox.drain(..n - 100);
    }
    let _ = blip_core::store::write_json(&app.office.inbox_path, &Value::Array(app.office.inbox.clone()));
    app.windows_dirty = true;
}

/// The one-line network state and its colour.
pub fn status(app: &App) -> (String, Color) {
    let t = &app.theme;
    if !app.cfg.bool("network") {
        return ("Office network is off (Settings).".into(), t.danger);
    }
    let Some(n) = &app.office.net else {
        return (format!("Network problem: {}", app.office.error.clone().unwrap_or_default()), t.danger);
    };
    let k = n.online().len();
    let (state, _) = n.server_state();
    let srv = match state.as_str() {
        "connected" => "  - online",
        "connecting" => "  - connecting to server",
        "error" => "  - server not reachable",
        _ => "",
    };
    (format!("{k} pet{} nearby{srv}", if k == 1 { "" } else { "s" }), if k > 0 { t.ok } else { t.warn })
}

pub fn server_status(app: &App) -> (String, Color) {
    let t = &app.theme;
    if app.cfg.str("server").is_empty() {
        return ("Not connected. Your pet only meets pets on this network.".into(), t.muted);
    }
    let (state, detail) = app.office.net.as_ref().map(|n| n.server_state()).unwrap_or(("off".into(), String::new()));
    match state.as_str() {
        "connected" => {
            let n = peers(app).iter().filter(|p| p.via == "server").count();
            let host = blip_proto::link::parse(app.cfg.str("server")).map(|l| l.host).unwrap_or_else(|| "server".into());
            (format!("Connected to {host}  \u{b7}  {n} teammate{} online", if n == 1 { "" } else { "s" }), t.ok)
        }
        "error" => (format!("Can't connect: {detail}. Trying again\u{2026}"), t.danger),
        "off" if !app.cfg.bool("network") => ("Office network is off.".into(), t.muted),
        _ => ("Connecting\u{2026}".into(), t.warn),
    }
}

pub fn this_pc() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::process::Command::new("hostname").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()))
        .unwrap_or_else(|| "this computer".into());
    let ips = net::local_ips();
    format!("This PC: {host}  \u{b7}  {}", if ips.is_empty() { "no network".into() } else { ips.join(", ") })
}

// ------------------------------------------------ the menu
pub fn menu_items(app: &App) -> Vec<Item> {
    if !app.cfg.menu_on("office") {
        return vec![];
    }
    let ps = peers(app);
    let mut items = vec![Item::Section {
        title: "Office".into(),
        badge: Some((if ps.is_empty() { "nobody nearby".into() } else { format!("{} nearby", ps.len()) }, !ps.is_empty())),
        link: Some(("All".into(), "window:buddies".into())),
    }];
    for p in ps.iter().take(SHOW) {
        items.push(Item::Person(Person {
            name: p.name.clone(),
            species: if p.species.is_empty() { "blip".into() } else { p.species.clone() },
            palette: p.palette.clone(),
            stage: p.stage.clone(),
            busy: p.status == "busy",
            internet: p.via == "server",
            call_cmd: format!("call:{}", p.id),
            msg_cmd: format!("compose:{}", p.id),
        }));
    }
    if ps.len() > SHOW {
        items.push(Item::PanelRow { icon: "people", label: format!("{} more\u{2026}", ps.len() - SHOW), cmd: "window:buddies".into() });
    }
    if ps.is_empty() {
        items.push(Item::PanelRow { icon: "reset", label: "Look again".into(), cmd: "net:rescan".into() });
    }
    items
}

// ------------------------------------------------ commands
fn card(app: &mut App, el: &ActiveEventLoop, key: &str, title: &str, body: &str, buttons: Vec<(&str, String)>, timeout: Option<f64>, on_close: &str, icon: &'static str, kind: &str) {
    let buttons = buttons.into_iter().map(|(a, b)| (a.to_string(), b)).collect();
    app.show_card(el, ShowCard { key, title, body, kind, icon: Some(icon), buttons, timeout, on_close, slider: None });
}

fn cheer(app: &mut App) {
    if app.pet.hatched() {
        let now = app.now();
        app.body.happy_until = now + 2.0;
        if matches!(app.body.action, Action::Idle | Action::Walk) {
            app.body.jump();
        }
    }
}

pub fn run(app: &mut App, el: &ActiveEventLoop, cmd: &str) {
    let parts: Vec<&str> = cmd.splitn(3, ':').collect();
    match parts.as_slice() {
        ["net", "rescan"] => {
            if let Some(n) = &app.office.net {
                n.announce_now();
            }
        }
        ["net", "restart"] => app.office.restart_at = Some(blip_core::now() + 0.6),
        ["inbox", "clear"] => {
            app.office.inbox.clear();
            let _ = blip_core::store::write_json(&app.office.inbox_path, &json!([]));
            app.windows_dirty = true;
        }
        ["call", pid] => call(app, el, pid),
        ["compose", pid] => {
            if app.office.net.is_none() {
                app.board("Network off", 3.0);
            } else {
                app.want_window.push(format!("ask:message:{pid}"));
            }
        }
        ["msg", pid, text] => send_msg(app, pid, text),
        ["out-closed", cid, why] => {
            if let Some((pid, name)) = app.office.calls_out.remove(*cid) {
                if matches!(*why, "button" | "x" | "timeout") {
                    if let Some(n) = &app.office.net {
                        n.send(&pid, "call_cancel", json!({"call_id": cid}), None);
                    }
                }
                if *why == "timeout" {
                    card(app, el, "", "No answer", &format!("{name} didn't pick up."), vec![("Message", format!("compose:{pid}")), ("OK", String::new())], Some(30.0), "", "phone", "call");
                }
            }
        }
        ["answer", cid, code] => answer(app, el, cid, code),
        ["reply", cid, text] => {
            if let Some((src, name)) = app.office.calls_in.remove(*cid) {
                let text: String = text.chars().take(300).collect();
                if let Some(n) = &app.office.net {
                    n.send(&src, "call_reply", json!({"call_id": cid, "text": text}), None);
                }
                log_inbox(app, "out", &src, &name, &text);
                cheer(app);
            }
        }
        ["in-closed", cid, why] => {
            app.stop_alert(&format!("in:{cid}"));
            if matches!(*why, "timeout" | "cancel") {
                if let Some((src, name)) = app.office.calls_in.remove(*cid) {
                    card(app, el, &format!("missed:{src}"), "Missed call", &format!("{name} called you."), vec![("Call back", format!("call:{src}")), ("OK", String::new())], None, "", "phone", "call");
                }
            }
        }
        _ => crate::voice::run(app, el, cmd),
    }
}

fn call(app: &mut App, el: &ActiveEventLoop, pid: &str) {
    if app.office.net.is_none() {
        app.board("Network off", 3.0);
        return;
    }
    let name = name_of(app, pid);
    let cid = blip_core::Rng::new().hex(12);
    app.office.calls_out.insert(cid.clone(), (pid.to_string(), name.clone()));
    card(
        app,
        el,
        &format!("out:{cid}"),
        &format!("Calling {name}"),
        &format!("Ringing {name}'s Blip..."),
        vec![("Hang up", String::new())],
        Some(CALL_RING_SECS + 5.0),
        &format!("out-closed:{cid}:{{why}}"),
        "phone",
        "call",
    );
    log_inbox(app, "out", pid, &name, "\u{1F4DE} called");
    if app.pet.hatched() {
        app.body.jump();
    }
    if let Some(n) = &app.office.net {
        n.send(pid, "call", json!({"call_id": cid}), Some(format!("call:{cid}")));
    }
}

fn send_msg(app: &mut App, pid: &str, text: &str) {
    let text: String = text.trim().chars().take(500).collect();
    if text.is_empty() {
        return;
    }
    let name = name_of(app, pid);
    log_inbox(app, "out", pid, &name, &text);
    if let Some(n) = &app.office.net {
        n.send(pid, "msg", json!({"text": text}), Some(format!("msg:{name}")));
    } else {
        app.board("Network off", 3.0);
    }
}

fn sent(app: &mut App, el: &ActiveEventLoop, tag: &str, ok: bool) {
    if let Some(cid) = tag.strip_prefix("call:") {
        if !ok {
            if let Some((_, name)) = app.office.calls_out.remove(cid) {
                app.cards.remove(&format!("out:{cid}"));
                card(app, el, "", "Can't reach", &format!("Couldn't reach {name}'s Blip. They may be offline."), vec![("OK", String::new())], Some(20.0), "", "phone", "call");
            }
        }
    } else if let Some(name) = tag.strip_prefix("msg:") {
        app.board(&if ok { "Sent!".to_string() } else { format!("Can't reach {name}") }, 3.0);
    }
}

fn answer(app: &mut App, el: &ActiveEventLoop, cid: &str, code: &str) {
    let Some((src, name)) = app.office.calls_in.get(cid).cloned() else { return };
    match code {
        "voice" => {
            app.office.calls_in.remove(cid);
            crate::voice::answer(app, el, &src, &name, cid);
        }
        "custom" => app.want_window.push(format!("ask:reply:{cid}")),
        _ => {
            let text = match code {
                "coming" => "On my way!",
                "2min" => "Give me 2 minutes.",
                _ => "Busy right now - I'll get back to you.",
            };
            app.office.calls_in.remove(cid);
            if let Some(n) = &app.office.net {
                n.send(&src, "call_reply", json!({"call_id": cid, "text": text}), None);
            }
            log_inbox(app, "out", &src, &name, text);
            cheer(app);
        }
    }
}

fn on_message(app: &mut App, el: &ActiveEventLoop, d: Map<String, Value>) {
    let kind = d.get("type").and_then(Value::as_str).unwrap_or("").to_string();
    let src = d.get("from").and_then(Value::as_str).unwrap_or("").to_string();
    let empty = Map::new();
    let body = d.get("body").and_then(Value::as_object).unwrap_or(&empty).clone();
    let name = name_of(app, &src);
    crate::log(&format!("net {kind} from {src}"));
    let cid: String = body.get("call_id").map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default().chars().take(40).collect();
    match kind.as_str() {
        "call" => incoming_call(app, el, &src, &name, &cid),
        "call_cancel" => {
            if app.office.calls_in.contains_key(&cid) {
                app.cards.remove(&format!("in:{cid}"));
                run(app, el, &format!("in-closed:{cid}:cancel"));
            }
        }
        "call_reply" => {
            let text: String = body.get("text").and_then(Value::as_str).unwrap_or("").chars().take(300).collect();
            call_answered(app, el, &src, &name, &cid, &text, body.get("voice").cloned());
        }
        "voice_end" => crate::voice::remote_end(app, &cid),
        "msg" => {
            let text: String = body.get("text").and_then(Value::as_str).unwrap_or("").chars().take(500).collect();
            incoming_msg(app, el, &src, &name, &text);
        }
        _ => {}
    }
}

fn incoming_call(app: &mut App, el: &ActiveEventLoop, src: &str, name: &str, cid: &str) {
    log_inbox(app, "in", src, name, "\u{1F4DE} called you");
    if app.cfg.bool("dnd") {
        if let Some(n) = &app.office.net {
            n.send(src, "call_reply", json!({"call_id": cid, "text": "Busy right now (do not disturb)."}), None);
        }
        card(app, el, &format!("missed:{src}"), "Missed call", &format!("{name} called while you were on do not disturb."), vec![("Call back", format!("call:{src}")), ("OK", String::new())], None, "", "phone", "call");
        return;
    }
    let key = format!("in:{cid}");
    app.office.calls_in.insert(cid.to_string(), (src.to_string(), name.to_string()));
    app.start_alert(&key, "ring", true);
    let mut buttons = vec![
        ("Coming", format!("answer:{cid}:coming")),
        ("2 min", format!("answer:{cid}:2min")),
        ("Busy", format!("answer:{cid}:busy")),
        ("Reply...", format!("answer:{cid}:custom")),
    ];
    if crate::voice::can_call(app, src) {
        buttons.insert(0, ("Talk", format!("answer:{cid}:voice")));
    }
    card(app, el, &key, "Incoming call", &format!("{name} is calling you!"), buttons, Some(CALL_RING_SECS), &format!("in-closed:{cid}:{{why}}"), "phone", "call");
}

fn call_answered(app: &mut App, el: &ActiveEventLoop, src: &str, name: &str, cid: &str, text: &str, voice: Option<Value>) {
    if app.office.calls_out.remove(cid).is_none() {
        return;
    }
    app.cards.remove(&format!("out:{cid}"));
    if let Some(Value::Object(v)) = voice {
        if crate::voice::start_from_reply(app, el, src, name, cid, &v) {
            return;
        }
    }
    log_inbox(app, "in", src, name, text);
    app.play_sound("ding", false);
    let body = if text.is_empty() { "answered" } else { text };
    card(app, el, "", name, body, vec![("OK", String::new()), ("Message", format!("compose:{src}"))], Some(45.0), "", "phone", "call");
    cheer(app);
    if app.pet.hatched() {
        app.body.jump();
    }
}

fn incoming_msg(app: &mut App, el: &ActiveEventLoop, src: &str, name: &str, text: &str) {
    log_inbox(app, "in", src, name, text);
    if !app.cfg.bool("dnd") {
        app.play_sound("ding", false);
    }
    if app.pet.hatched() && matches!(app.body.action, Action::Idle | Action::Walk) {
        app.body.jump();
    }
    card(app, el, "", "Message", &format!("{name}: {text}"), vec![("Reply", format!("compose:{src}")), ("OK", String::new())], None, "", "mail", "msg");
}

pub fn in_call(app: &App) -> bool {
    !app.office.calls_in.is_empty() || !app.office.calls_out.is_empty()
}

/// Send something over the network (used by voice.rs).
pub fn send(app: &App, pid: &str, typ: &str, body: Value) {
    if let Some(n) = &app.office.net {
        n.send(pid, typ, body, None);
    }
}

pub fn log(app: &mut App, dir: &str, peer: &str, name: &str, text: &str) {
    log_inbox(app, dir, peer, name, text);
}

// ------------------------------------------------ Basecamp (filled in with the Basecamp step)
pub fn tasks_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop) {
    crate::basecamp::tasks_ui(ui, t, app, el);
}

#[allow(clippy::too_many_arguments)]
pub fn basecamp_page(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, tok: &mut String, id: &mut String, secret: &mut String, morning: &mut String) {
    crate::basecamp::settings_page(ui, t, app, el, tok, id, secret, morning);
}

