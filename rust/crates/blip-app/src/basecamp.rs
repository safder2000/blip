//! Basecamp: which of my to-dos are overdue or due today, reminder cards, and ticking them off.
//! Port of `blip/basecamp.py`. Connect with a personal access token, or OAuth through a
//! Launchpad integration (Blip opens the browser and catches the reply on localhost:47615).
//! Network work runs on threads; results come back to the app as events.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use blip_ui::theme::Theme;
use egui::{Align, Layout, RichText, Ui, Vec2};
use serde_json::{json, Map, Value};
use winit::event_loop::ActiveEventLoop;

use crate::app::App;
use crate::cards::ShowCard;
use crate::gui::{self, c, Kind};
use crate::http::{self, Fail};

const LAUNCHPAD: &str = "https://launchpad.37signals.com";
const API_AUTH: &str = "https://3.basecampapi.com/authorization.json";
const REDIRECT_PORT: u16 = 47615;
const REDIRECT: &str = "http://localhost:47615/callback";
const SCOPES: [&str; 2] = ["overdue", "due_today"];
/// release builds get the integration from the repository's secrets
const BUILD_ID: Option<&str> = option_env!("BLIP_BC_CLIENT_ID");
const BUILD_SECRET: Option<&str> = option_env!("BLIP_BC_CLIENT_SECRET");

#[derive(Clone, Debug, Default)]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub content: String,
    pub due_on: String,
    pub project: String,
    pub url: String,
    pub scope: String,
}

struct Tokens {
    token: String,
    refresh: String,
    expires: f64,
}

enum Event {
    Items(Result<Vec<Task>, String>, Option<Tokens>),
    Connected(Result<(Tokens, Vec<(String, String)>), String>),
    Completed(String, Result<(), String>),
}

pub struct Basecamp {
    pub items: Vec<Task>,
    pub last_error: Option<String>,
    pub last_fetch: f64,
    pub accounts: Vec<(String, String)>,
    busy: bool,
    snooze_until: f64,
    next_poll: f64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Default for Basecamp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Basecamp { items: vec![], last_error: None, last_fetch: 0.0, accounts: vec![], busy: false, snooze_until: 0.0, next_poll: blip_core::now() + 15.0, tx, rx }
    }
}

pub fn connected(app: &App) -> bool {
    !app.cfg.str("bc_token").is_empty() && !app.cfg.str("bc_account").is_empty()
}

fn creds(app: &App) -> (String, String) {
    let id = if app.cfg.str("bc_client_id").trim().is_empty() { BUILD_ID.unwrap_or("") } else { app.cfg.str("bc_client_id") };
    let secret = if app.cfg.str("bc_client_secret").trim().is_empty() { BUILD_SECRET.unwrap_or("") } else { app.cfg.str("bc_client_secret") };
    (id.trim().to_string(), secret.trim().to_string())
}

fn have_creds(app: &App) -> bool {
    let (a, b) = creds(app);
    !a.is_empty() && !b.is_empty()
}

fn fail_text(f: Fail) -> String {
    match f {
        Fail::Status(401, _) => "Basecamp says the token is not valid any more - reconnect in Settings.".into(),
        Fail::Status(code, _) => format!("Basecamp error {code}"),
        Fail::Transport(t) => format!("Can't reach Basecamp ({t})"),
    }
}

/// GET with a couple of retries on busy / server errors.
fn api_get(url: &str, token: &str) -> Result<Value, String> {
    let mut attempt = 0;
    loop {
        match http::get_json(url, Some(token), "application/json") {
            Ok(v) => return Ok(v),
            Err(Fail::Status(code, after)) if matches!(code, 429 | 500 | 502 | 503 | 504) && attempt < 2 => {
                std::thread::sleep(Duration::from_secs(after.unwrap_or(2u64 << attempt).min(30)));
            }
            Err(Fail::Transport(_)) if attempt < 2 => std::thread::sleep(Duration::from_secs(2)),
            Err(e) => return Err(fail_text(e)),
        }
        attempt += 1;
    }
}

fn pick_accounts(doc: &Value) -> Vec<(String, String)> {
    doc.get("accounts")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|x| x.get("product").and_then(Value::as_str) == Some("bc3"))
                .filter_map(|x| Some((x.get("href")?.as_str()?.to_string(), x.get("name").and_then(Value::as_str).unwrap_or("?").to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn normalise(items: &Value, scope: &str) -> Vec<Task> {
    items
        .as_array()
        .map(|a| {
            a.iter()
                .map(|it| Task {
                    id: text_of(it.get("id")),
                    kind: it.get("type").and_then(Value::as_str).unwrap_or("todo").to_string(),
                    content: it.get("content").or_else(|| it.get("title")).and_then(Value::as_str).unwrap_or("").trim().to_string(),
                    due_on: it.get("due_on").and_then(Value::as_str).unwrap_or("").to_string(),
                    project: it.get("bucket").and_then(|b| b.get("name")).and_then(Value::as_str).unwrap_or("").to_string(),
                    url: it.get("app_url").and_then(Value::as_str).unwrap_or("").to_string(),
                    scope: scope.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------ actions
pub fn refresh(app: &mut App) {
    if !connected(app) || app.bc.busy {
        return;
    }
    app.bc.busy = true;
    let base = app.cfg.str("bc_account").trim_end_matches('/').to_string();
    let token = app.cfg.str("bc_token").to_string();
    let (refresh_tok, expires) = (app.cfg.str("bc_refresh").to_string(), app.cfg.f64("bc_expires"));
    let (cid, secret) = creds(app);
    let tx = app.bc.tx.clone();
    std::thread::spawn(move || {
        let mut token = token;
        let mut newtok = None;
        if !refresh_tok.is_empty() && expires > 0.0 && expires - blip_core::now() < 2.0 * 86400.0 {
            let form = [("grant_type", "refresh_token"), ("refresh_token", refresh_tok.as_str()), ("client_id", cid.as_str()), ("client_secret", secret.as_str()), ("redirect_uri", REDIRECT)];
            if let Ok(t) = http::post(&format!("{LAUNCHPAD}/authorization/token"), None, Some(&form)) {
                if let Some(at) = t.get("access_token").and_then(Value::as_str) {
                    token = at.to_string();
                    let ex = t.get("expires_in").and_then(Value::as_f64).unwrap_or(1_209_600.0);
                    let rf = t.get("refresh_token").and_then(Value::as_str).unwrap_or(&refresh_tok).to_string();
                    newtok = Some(Tokens { token: token.clone(), refresh: rf, expires: blip_core::now() + ex });
                }
            }
        }
        let mut out = vec![];
        for scope in SCOPES {
            match api_get(&format!("{base}/my/assignments/due.json?scope={scope}"), &token) {
                Ok(v) => out.extend(normalise(&v, scope)),
                Err(e) => {
                    let _ = tx.send(Event::Items(Err(e), newtok));
                    return;
                }
            }
        }
        let _ = tx.send(Event::Items(Ok(out), newtok));
    });
}

fn finish_connect(token: &str) -> Result<Vec<(String, String)>, String> {
    let doc = api_get(API_AUTH, token)?;
    let accts = pick_accounts(&doc);
    if accts.is_empty() {
        return Err("That login has no Basecamp account.".into());
    }
    Ok(accts)
}

pub fn connect_token(app: &mut App, token: &str) {
    let token = token.trim().to_string();
    if token.is_empty() {
        return;
    }
    let tx = app.bc.tx.clone();
    std::thread::spawn(move || {
        let r = finish_connect(&token).map(|a| (Tokens { token: token.clone(), refresh: String::new(), expires: 0.0 }, a));
        let _ = tx.send(Event::Connected(r));
    });
}

/// One click: open the approval page in the browser and finish in the background.
pub fn connect_oauth(app: &mut App) {
    let (cid, secret) = creds(app);
    if cid.is_empty() || secret.is_empty() {
        app.want_window.push("settings:basecamp".into());
        return;
    }
    let listener = match TcpListener::bind(("127.0.0.1", REDIRECT_PORT)) {
        Ok(l) => l,
        Err(e) => {
            let _ = app.bc.tx.send(Event::Connected(Err(format!("Port {REDIRECT_PORT} is busy ({e})."))));
            return;
        }
    };
    app.board("Opening Basecamp", 4.0);
    let approve = format!("{LAUNCHPAD}/authorization/new?response_type=code&client_id={cid}&redirect_uri={}", REDIRECT.replace(':', "%3A").replace('/', "%2F"));
    let tx = app.bc.tx.clone();
    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let end = blip_core::now() + 600.0;
        while blip_core::now() < end {
            let Ok((mut s, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            };
            let _ = s.set_nonblocking(false);
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            let code = path
                .split_once('?')
                .map(|(_, q)| q.split('&').filter_map(|kv| kv.split_once('=')).find(|(k, _)| *k == "code").map(|(_, v)| v.to_string()).unwrap_or_default())
                .unwrap_or_default();
            let ok = !code.is_empty() && path.starts_with("/callback");
            let msg = if ok { "Blip is connected to Basecamp. You can close this tab." } else { "Basecamp didn't send a code. Try again from Blip's settings." };
            let page = format!("<html><body style='font:16px sans-serif;padding:40px'>{msg}</body></html>");
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            if !ok {
                continue;
            }
            let form = [("grant_type", "authorization_code"), ("client_id", cid.as_str()), ("client_secret", secret.as_str()), ("redirect_uri", REDIRECT), ("code", code.as_str())];
            let r = match http::post(&format!("{LAUNCHPAD}/authorization/token"), None, Some(&form)) {
                Ok(t) => match t.get("access_token").and_then(Value::as_str) {
                    Some(at) => finish_connect(at).map(|a| {
                        let ex = t.get("expires_in").and_then(Value::as_f64).unwrap_or(0.0);
                        let rf = t.get("refresh_token").and_then(Value::as_str).unwrap_or("").to_string();
                        (Tokens { token: at.to_string(), refresh: rf, expires: if ex > 0.0 { blip_core::now() + ex } else { 0.0 } }, a)
                    }),
                    None => Err("Couldn't finish connecting: no token".into()),
                },
                Err(e) => Err(format!("Couldn't finish connecting: {}", fail_text(e))),
            };
            let _ = tx.send(Event::Connected(r));
            return;
        }
    });
    http::open_url(&approve);
}

pub fn disconnect(app: &mut App) {
    for k in ["bc_token", "bc_refresh", "bc_account", "bc_account_name"] {
        app.cfg.set(k, json!(""));
    }
    app.cfg.set("bc_expires", json!(0));
    app.bc.items.clear();
    app.bc.accounts.clear();
    app.windows_dirty = true;
}

pub fn complete(app: &mut App, id: &str) {
    let Some(t) = app.bc.items.iter().find(|t| t.id == id).cloned() else { return };
    if !matches!(t.kind.as_str(), "todo" | "Todo" | "") {
        app.board("Only to-dos", 3.0);
        return;
    }
    let base = app.cfg.str("bc_account").trim_end_matches('/').to_string();
    let token = app.cfg.str("bc_token").to_string();
    let tx = app.bc.tx.clone();
    std::thread::spawn(move || {
        let r = http::post(&format!("{base}/todos/{}/completion.json", t.id), Some(&token), None).map(|_| ()).map_err(fail_text);
        let _ = tx.send(Event::Completed(t.id, r));
    });
}

pub fn run(app: &mut App, el: &ActiveEventLoop, cmd: &str) -> bool {
    let Some(rest) = cmd.strip_prefix("bc:") else { return false };
    let (k, arg) = rest.split_once(':').unwrap_or((rest, ""));
    match k {
        "connect" => connect_oauth(app),
        "token" => connect_token(app, arg),
        "refresh" => refresh(app),
        "done" => complete(app, arg),
        "disconnect" => disconnect(app),
        "account" => {
            if let Some((href, name)) = app.bc.accounts.iter().find(|a| a.0 == arg).cloned() {
                app.cfg.set("bc_account", json!(href));
                app.cfg.set("bc_account_name", json!(name));
                refresh(app);
            }
        }
        "snooze" => {
            app.bc.snooze_until = blip_core::now() + 3600.0;
            app.run_at(3605.0, "bc:remind");
        }
        "remind" => {
            if !app.bc.items.is_empty() {
                let items = app.bc.items.clone();
                card(app, el, &items, false);
            }
        }
        _ => {}
    }
    true
}

// ------------------------------------------------ the app side
pub fn tick(app: &mut App, el: &ActiveEventLoop) {
    let now = blip_core::now();
    if now >= app.bc.next_poll {
        app.bc.next_poll = now + 15.0 * 60.0;
        if connected(app) && app.cfg.bool("bc_enabled") {
            refresh(app);
        }
    }
    let evs: Vec<Event> = app.bc.rx.try_iter().collect();
    for e in evs {
        match e {
            Event::Items(r, tok) => {
                app.bc.busy = false;
                if let Some(t) = tok {
                    app.cfg.set("bc_token", json!(t.token));
                    app.cfg.set("bc_refresh", json!(t.refresh));
                    app.cfg.set("bc_expires", json!(t.expires));
                }
                match r {
                    Ok(items) => {
                        app.bc.items = items;
                        app.bc.last_error = None;
                        app.bc.last_fetch = now;
                        on_items(app, el);
                    }
                    Err(e) => app.bc.last_error = Some(e),
                }
                app.windows_dirty = true;
            }
            Event::Connected(r) => {
                let (msg, ok) = match r {
                    Ok((t, accts)) => {
                        let keep = accts.iter().find(|a| a.0 == app.cfg.str("bc_account")).cloned().unwrap_or_else(|| accts[0].clone());
                        app.cfg.set("bc_token", json!(t.token));
                        app.cfg.set("bc_refresh", json!(t.refresh));
                        app.cfg.set("bc_expires", json!(t.expires));
                        app.cfg.set("bc_account", json!(keep.0));
                        app.cfg.set("bc_account_name", json!(keep.1));
                        app.bc.accounts = accts;
                        refresh(app);
                        (format!("Connected to {}. Blip will remind you about what's due.", keep.1), true)
                    }
                    Err(e) => (e, false),
                };
                let buttons = if ok { vec![("Show my tasks".to_string(), "window:tasks".to_string()), ("OK".into(), String::new())] } else { vec![("OK".into(), String::new())] };
                app.show_card(el, ShowCard { key: "bc-connect", title: "Basecamp", body: &msg, kind: "task", icon: Some("task"), buttons, timeout: Some(30.0), on_close: "" });
                app.windows_dirty = true;
            }
            Event::Completed(id, r) => {
                match r {
                    Ok(()) => {
                        app.bc.items.retain(|t| t.id != id);
                        app.play_sound("done", false);
                        app.cheer_done();
                    }
                    Err(_) => app.board("Couldn't tick it", 4.0),
                }
                app.windows_dirty = true;
            }
        }
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn on_items(app: &mut App, el: &ActiveEventLoop) {
    if !app.cfg.bool("bc_enabled") || blip_core::now() < app.bc.snooze_until {
        return;
    }
    let day = today();
    let mut seen: Map<String, Value> = app.pet.st.get("bc_seen").and_then(Value::as_object).cloned().unwrap_or_default();
    seen.retain(|_, v| v.as_str() == Some(day.as_str()));
    let items = app.bc.items.clone();
    let new: Vec<Task> = items.iter().filter(|i| !seen.contains_key(&i.id)).cloned().collect();
    let (mh, mm) = blip_core::clock::parse_clock(app.cfg.str("bc_morning")).unwrap_or((9, 30));
    let now = chrono::Local::now();
    use chrono::Timelike;
    let after_morning = (now.hour(), now.minute()) >= (mh, mm);
    let summary_due = app.pet.s("bc_summary_day") != day && after_morning;
    let show = if summary_due { items.clone() } else { new };
    if !show.is_empty() {
        if summary_due {
            app.pet.set("bc_summary_day", json!(day));
        }
        for i in &items {
            seen.insert(i.id.clone(), json!(day));
        }
        card(app, el, &show, summary_due);
    }
    app.pet.set("bc_seen", Value::Object(seen));
}

fn card(app: &mut App, el: &ActiveEventLoop, items: &[Task], summary: bool) {
    let over: Vec<&Task> = items.iter().filter(|i| i.scope == "overdue").collect();
    let due: Vec<&Task> = items.iter().filter(|i| i.scope != "overdue").collect();
    let mut lines = vec![];
    for i in over.iter().chain(due.iter()).take(5) {
        let tag = if i.scope == "overdue" { "OVERDUE" } else { "today" };
        let proj = if i.project.is_empty() { String::new() } else { format!(", {}", i.project) };
        lines.push(format!("\u{2022} {}  ({tag}{proj})", i.content));
    }
    if items.len() > 5 {
        lines.push(format!("+ {} more", items.len() - 5));
    }
    let mut head = vec![];
    if !over.is_empty() {
        head.push(format!("{} overdue", over.len()));
    }
    if !due.is_empty() {
        head.push(format!("{} due today", due.len()));
    }
    app.play_sound("ding", false);
    if app.pet.hatched() {
        app.body.jump();
    }
    let body = format!("{}:\n{}", head.join(", "), lines.join("\n"));
    let buttons = vec![("Show all".to_string(), "window:tasks".to_string()), ("Snooze 1h".into(), "bc:snooze".into()), ("OK".into(), String::new())];
    app.show_card(el, ShowCard { key: "bc", title: if summary { "Your Basecamp day" } else { "Basecamp" }, body: &body, kind: "task", icon: Some("task"), buttons, timeout: None, on_close: "" });
}

// ------------------------------------------------ windows
pub fn tasks_ui(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop) {
    ui.horizontal(|ui| {
        let p = gui::px(ui, 2.0);
        gui::icon(ui, "task", p, c(t.accent));
        ui.add_space(4.0);
        gui::h2(ui, "Basecamp");
    });
    gui::muted(ui, t, "Your to-dos that are overdue or due today.");
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let info = if connected(app) {
            let when = if app.bc.last_fetch > 0.0 { crate::windows::hhmm(app.bc.last_fetch) } else { "\u{2026}".into() };
            let mut s = format!("{}  \u{b7}  updated {when}", app.cfg.str("bc_account_name"));
            if let Some(e) = &app.bc.last_error {
                s += &format!("  \u{b7}  {e}");
            }
            s
        } else {
            "Not connected".into()
        };
        gui::muted(ui, t, &info);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if gui::button(ui, t, "Refresh", Some("reset"), Kind::Ghost, true, connected(app)).clicked() {
                refresh(app);
            }
        });
    });
    ui.add_space(8.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        if !connected(app) {
            gui::empty_state(ui, t, "Basecamp isn't connected", if have_creds(app) { "Sign in once and Blip reminds you about what's due. Your browser opens; approve and you're done." } else { "Connect with an access token in Settings." });
            ui.vertical_centered(|ui| {
                if have_creds(app) && gui::button(ui, t, "Sign in with Basecamp", Some("open"), Kind::Primary, false, true).clicked() {
                    connect_oauth(app);
                }
                if gui::button(ui, t, if have_creds(app) { "Other ways to connect" } else { "Open settings" }, None, Kind::Ghost, true, true).clicked() {
                    app.want_window.push("settings:basecamp".into());
                }
            });
            return;
        }
        let mut items = app.bc.items.clone();
        items.sort_by(|a, b| (a.scope != "overdue").cmp(&(b.scope != "overdue")).then(a.due_on.cmp(&b.due_on)));
        if items.is_empty() && app.bc.last_fetch > 0.0 {
            gui::empty_state(ui, t, "All clear!", "Nothing overdue or due today.");
            return;
        }
        for it in &items {
            gui::card(ui, t, 10, |ui| {
                ui.horizontal_top(|ui| {
                    let over = it.scope == "overdue";
                    let (txt, fg, bg) = if over { ("OVERDUE", t.danger, gui::blend(t.surface, t.danger, 0.18)) } else { ("TODAY", t.warn, gui::blend(t.surface, t.warn, 0.18)) };
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(64.0, 20.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(4), c(bg));
                    let font = egui::FontId::new(10.5, egui::FontFamily::Name("bold".into()));
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, txt, font, c(fg));
                    let w = (ui.available_width() - 120.0).max(120.0);
                    ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(w);
                        ui.add(egui::Label::new(RichText::new(&it.content).size(14.0)).wrap());
                        let mut extra = it.project.clone();
                        if !it.due_on.is_empty() {
                            extra += &format!("{}due {}", if extra.is_empty() { "" } else { "  \u{b7}  " }, it.due_on);
                        }
                        gui::muted(ui, t, &extra);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                        if !it.url.is_empty() && gui::icon_button(ui, t, "open").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&it.url));
                        }
                        if matches!(it.kind.as_str(), "todo" | "Todo" | "") && gui::button(ui, t, "Done", Some("check"), Kind::Soft, true, true).clicked() {
                            complete(app, &it.id);
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    });
    let _ = el;
}

#[allow(clippy::too_many_arguments)]
pub fn settings_page(ui: &mut Ui, t: &Theme, app: &mut App, el: &ActiveEventLoop, tok: &mut String, id: &mut String, secret: &mut String, morning: &mut String) {
    gui::card(ui, t, 14, |ui| {
        if connected(app) {
            gui::h2(ui, &format!("Connected to {}", if app.cfg.str("bc_account_name").is_empty() { "Basecamp" } else { app.cfg.str("bc_account_name") }));
            let over = app.bc.items.iter().filter(|i| i.scope == "overdue").count();
            let due = app.bc.items.len() - over;
            let sub = match &app.bc.last_error {
                Some(e) => e.clone(),
                None if app.bc.last_fetch > 0.0 => format!("{over} overdue  \u{b7}  {due} due today"),
                None => "Fetching your to-dos\u{2026}".into(),
            };
            gui::muted(ui, t, &sub);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if gui::button(ui, t, "Open my tasks", Some("task"), Kind::Primary, true, true).clicked() {
                    app.want_window.push("tasks".into());
                }
                if gui::button(ui, t, "Disconnect", None, Kind::Danger, true, true).clicked() {
                    disconnect(app);
                }
            });
            if app.bc.accounts.len() > 1 {
                ui.add_space(8.0);
                let opts: Vec<(String, String)> = app.bc.accounts.clone();
                let refs: Vec<(&str, &str)> = opts.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                let mut sel = app.cfg.str("bc_account").to_string();
                if gui::chips(ui, t, &refs, &mut sel) {
                    app.run(el, &format!("bc:account:{sel}"));
                }
            }
        } else {
            gui::h2(ui, "Connect Basecamp");
            gui::muted(
                ui,
                t,
                if have_creds(app) {
                    "Blip will remind you about to-dos that are due today or overdue, and you can tick them off from here."
                } else {
                    "This build has no Basecamp integration. Use an access token below."
                },
            );
            ui.add_space(10.0);
            if gui::button(ui, t, "Sign in with Basecamp", Some("open"), Kind::Primary, false, have_creds(app)).clicked() {
                connect_oauth(app);
            }
        }
    });
    gui::section(ui, t, "Reminders");
    gui::card(ui, t, 14, |ui| {
        let mut on = app.cfg.bool("bc_enabled");
        if gui::toggle_row(ui, t, "Remind me", "Cards for to-dos that are overdue or due today, checked every 15 minutes.", &mut on) {
            app.cfg.set("bc_enabled", json!(on));
        }
        gui::hline(ui, t);
        gui::setting_row(ui, t, "Daily summary", "Your whole Basecamp day, once each morning.", |ui| {
            let r = gui::field(ui, morning, 90.0, "9:30", false);
            if gui::committed(ui, &r) {
                let v = morning.trim().to_string();
                if v.is_empty() || blip_core::clock::parse_clock(&v).is_some() {
                    app.cfg.set("bc_morning", json!(if v.is_empty() { "9:30".to_string() } else { v }));
                }
            }
        });
    });
    gui::section(ui, t, "Advanced");
    gui::card(ui, t, 14, |ui| {
        gui::muted(ui, t, "Use a personal access token instead of signing in, or your own Launchpad integration.");
        ui.add_space(8.0);
        gui::setting_row(ui, t, "Access token", "", |ui| {
            if gui::button(ui, t, "Use", None, Kind::Soft, true, !tok.trim().is_empty()).clicked() {
                connect_token(app, tok);
                tok.clear();
            }
            gui::field(ui, tok, 130.0, "Paste token", true);
        });
        gui::hline(ui, t);
        gui::setting_row(ui, t, "Client ID", "", |ui| {
            let r = gui::field(ui, id, 180.0, "", false);
            if gui::committed(ui, &r) {
                app.cfg.set("bc_client_id", json!(id.trim()));
            }
        });
        ui.add_space(6.0);
        gui::setting_row(ui, t, "Client secret", "", |ui| {
            let r = gui::field(ui, secret, 180.0, "", true);
            if gui::committed(ui, &r) {
                app.cfg.set("bc_client_secret", json!(secret.trim()));
            }
        });
    });
}
