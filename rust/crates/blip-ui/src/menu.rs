//! The right-click menu, ported from `blip/menu.py`: the pet card on top, a row of care buttons,
//! then rows, sections, a row of small tool buttons, and colleagues with call and message buttons.
//! Layout, drawing and hit-testing only; the app decides what each command does.

use blip_pet::pixfont;
use blip_pet::sprites::{self, build_egg, build_pet, Color, PetLook, Stage};

use crate::text::Fonts;
use crate::theme::Theme;
use crate::{sprite_bounds, Canvas};

pub const CALL_GREEN: (Color, Color) = (0x2fb865, 0x28a359);
pub const MSG_BLUE: (Color, Color) = (0x3d8bfd, 0x2f78e6);
pub const NET_BLUE: Color = 0x4ba3ff;

#[derive(Clone, Debug)]
pub struct PetCard {
    pub name: String,
    pub status: String,
    pub species: String,
    pub palette: String,
    /// "egg", "baby", "child" or "adult"
    pub stage: String,
    pub mood: String,
    /// (icon, 0..=100, colour)
    pub bars: Vec<(&'static str, f64, Color)>,
}

#[derive(Clone, Debug)]
pub struct CareButton {
    pub icon: &'static str,
    pub label: String,
    pub cmd: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub icon: &'static str,
    pub label: String,
    pub cmd: String,
    pub badge: String,
}

#[derive(Clone, Debug)]
pub struct Person {
    pub name: String,
    pub species: String,
    pub palette: String,
    pub stage: String,
    pub busy: bool,
    /// reached through the online server, not the office network
    pub internet: bool,
    pub call_cmd: String,
    pub msg_cmd: String,
}

#[derive(Clone, Debug)]
pub enum Item {
    Row { icon: &'static str, label: String, hint: String, check: Option<bool>, cmd: String, enabled: bool },
    Section { title: String, badge: Option<(String, bool)>, link: Option<(String, String)> },
    Tools(Vec<Tool>),
    Person(Person),
    /// a row that sits on the people panel (e.g. "Look again")
    PanelRow { icon: &'static str, label: String, cmd: String },
    Sep,
    Gap,
}

impl Item {
    pub fn row(icon: &'static str, label: &str, cmd: &str) -> Item {
        Item::Row { icon, label: label.into(), hint: String::new(), check: None, cmd: cmd.into(), enabled: true }
    }
    pub fn toggle(icon: &'static str, label: &str, on: bool, cmd: &str) -> Item {
        Item::Row { icon, label: label.into(), hint: String::new(), check: Some(on), cmd: cmd.into(), enabled: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Card,
    Care(usize),
    Row(usize),
    Tool(usize, usize),
    Call(usize),
    Message(usize),
    Link(usize),
}

pub struct Menu {
    pub card: Option<PetCard>,
    pub care: Vec<CareButton>,
    pub items: Vec<Item>,
    pub card_cmd: String,
    s: f64,
    p: i32,
    pub w: i32,
    pub h: i32,
    head: (i32, i32, i32, i32),
    care_rects: Vec<(i32, i32, i32, i32)>,
    care_line: i32,
    rows: Vec<(i32, i32)>,
    pub hover: Option<Hit>,
}

fn sc(s: f64, v: f64) -> i32 {
    (v * s).round() as i32
}

/// Font sizes in pixels at 100%: like Segoe UI 10 pt / 9 pt / 8 pt.
const BODY: f32 = 13.3;
const SMALL: f32 = 12.0;
const TINY: f32 = 10.7;

impl Menu {
    pub fn new(card: Option<PetCard>, care: Vec<CareButton>, items: Vec<Item>, scale: f64) -> Menu {
        let s = scale.max(1.0);
        let p = 2.max(sc(s, 2.0));
        let w = sc(s, 252.0);
        let pad = sc(s, 10.0);
        let mut yy = p + sc(s, 6.0);
        let mut head = (0, 0, 0, 0);
        if card.is_some() {
            head = (p + pad, yy, w - p - pad, yy + sc(s, 58.0));
            yy += sc(s, 62.0);
        }
        let mut care_rects = Vec::new();
        let mut care_line = 0;
        if !care.is_empty() {
            let n = care.len() as i32;
            let gap = sc(s, 4.0);
            let bw = (w - 2 * p - 2 * pad - (n - 1) * gap) as f64 / n as f64;
            let bh = sc(s, 44.0);
            for i in 0..n {
                let ax = p + pad + (i as f64 * (bw + gap as f64)).round() as i32;
                care_rects.push((ax, yy, ax + bw.round() as i32, yy + bh));
            }
            yy += bh + sc(s, 8.0);
            care_line = yy;
            yy += sc(s, 5.0);
        }
        let mut rows = Vec::new();
        for it in &items {
            let h = match it {
                Item::Sep => sc(s, 9.0),
                Item::Gap => sc(s, 4.0),
                Item::Section { badge: Some(_), .. } => sc(s, 30.0),
                Item::Section { .. } => sc(s, 24.0),
                Item::Person(_) => sc(s, 34.0),
                Item::Tools(_) => sc(s, 40.0),
                _ => sc(s, 30.0),
            };
            rows.push((yy, yy + h));
            yy += h;
        }
        let h = yy + sc(s, 6.0) + p;
        Menu { card, care, items, card_cmd: "settings:pet".into(), s, p, w, h, head, care_rects, care_line, rows, hover: None }
    }

    /// Size of the window it needs (the frame plus its drop shadow).
    pub fn size(&self) -> (i32, i32) {
        (self.w + 2 * self.p, self.h + 2 * self.p)
    }

    fn tool_rects(&self, i: usize) -> Vec<(i32, i32, i32, i32)> {
        let (y0, y1) = self.rows[i];
        let Item::Tools(tools) = &self.items[i] else { return vec![] };
        let (s, p) = (self.s, self.p);
        let pad = sc(s, 10.0);
        let gap = sc(s, 4.0);
        let n = tools.len() as i32;
        let w = (self.w - 2 * p - 2 * pad - (n - 1) * gap) as f64 / n as f64;
        (0..n)
            .map(|k| {
                let x0 = p + pad + (k as f64 * (w + gap as f64)).round() as i32;
                (x0, y0 + sc(s, 3.0), x0 + w.round() as i32, y1 - sc(s, 3.0))
            })
            .collect()
    }

    fn person_buttons(&self, i: usize) -> [(i32, i32, i32, i32); 2] {
        let (y0, y1) = self.rows[i];
        let (s, p) = (self.s, self.p);
        let bw = sc(s, 28.0);
        let gap = sc(s, 4.0);
        let rx = self.w - p - sc(s, 10.0);
        let msg = (rx - bw, y0 + sc(s, 4.0), rx, y1 - sc(s, 4.0));
        let call = (rx - 2 * bw - gap, y0 + sc(s, 4.0), rx - bw - gap, y1 - sc(s, 4.0));
        [call, msg]
    }

    fn link_rect(&self, i: usize) -> (i32, i32, i32, i32) {
        let (y0, y1) = self.rows[i];
        let x1 = self.w - self.p - sc(self.s, 12.0);
        (x1 - sc(self.s, 44.0), y0 + sc(self.s, 4.0), x1, y1 - sc(self.s, 2.0))
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        let inside = |r: (i32, i32, i32, i32)| r.0 <= x && x <= r.2 && r.1 <= y && y <= r.3;
        for (i, r) in self.care_rects.iter().enumerate() {
            if inside(*r) {
                return self.care[i].enabled.then_some(Hit::Care(i));
            }
        }
        if self.card.is_some() && inside(self.head) {
            return Some(Hit::Card);
        }
        for (i, &(y0, y1)) in self.rows.iter().enumerate() {
            if y < y0 || y >= y1 {
                continue;
            }
            return match &self.items[i] {
                Item::Sep | Item::Gap => None,
                Item::Section { link, .. } => (link.is_some() && inside(self.link_rect(i))).then_some(Hit::Link(i)),
                Item::Tools(_) => self.tool_rects(i).iter().position(|r| inside(*r)).map(|k| Hit::Tool(i, k)),
                Item::Person(_) => {
                    let [call, msg] = self.person_buttons(i);
                    Some(if inside(call) {
                        Hit::Call(i)
                    } else if inside(msg) {
                        Hit::Message(i)
                    } else {
                        Hit::Row(i)
                    })
                }
                Item::Row { enabled: false, .. } => None,
                _ => Some(Hit::Row(i)),
            };
        }
        None
    }

    /// The command behind a hit, for the app to run.
    pub fn command(&self, hit: Hit) -> Option<String> {
        match hit {
            Hit::Card => Some(self.card_cmd.clone()),
            Hit::Care(i) => Some(self.care[i].cmd.clone()),
            Hit::Row(i) => match &self.items[i] {
                Item::Row { cmd, .. } | Item::PanelRow { cmd, .. } => Some(cmd.clone()),
                Item::Person(p) => Some(p.msg_cmd.clone()),
                _ => None,
            },
            Hit::Tool(i, k) => match &self.items[i] {
                Item::Tools(t) => Some(t[k].cmd.clone()),
                _ => None,
            },
            Hit::Call(i) => match &self.items[i] {
                Item::Person(p) => Some(p.call_cmd.clone()),
                _ => None,
            },
            Hit::Message(i) => match &self.items[i] {
                Item::Person(p) => Some(p.msg_cmd.clone()),
                _ => None,
            },
            Hit::Link(i) => match &self.items[i] {
                Item::Section { link: Some((_, cmd)), .. } => Some(cmd.clone()),
                _ => None,
            },
        }
    }

    // ------------------------------------------------ drawing
    pub fn draw(&self, cv: &mut Canvas, t: &Theme, fonts: &mut Fonts) {
        let (s, p, w, h) = (self.s, self.p, self.w, self.h);
        let ip = 1.max(sc(s, 2.0));
        let pad = sc(s, 10.0);
        let fs = |v: f32| v * s as f32;
        cv.clear();
        cv.pixel_box(2 * p, 2 * p, w + 2 * p, h + 2 * p, p, t.shadow);
        cv.pixel_box(0, 0, w, h, p, t.frame);
        cv.pixel_box(p, p, w - p, h - p, p, t.surface);

        if let Some(card) = &self.card {
            let (x0, y0, x1, y1) = self.head;
            cv.pixel_box(x0, y0, x1, y1, p, if self.hover == Some(Hit::Card) { t.hover } else { t.accent_soft });
            let g = if card.stage == "egg" {
                build_egg(1, 0.5)
            } else {
                build_pet(&PetLook {
                    stage: Stage::from_name(&card.stage),
                    palette: &card.palette,
                    mood: &card.mood,
                    frame: 1,
                    facing: 0,
                    squash: false,
                    blink: false,
                    species: &card.species,
                })
            };
            let (bx, by, bw, bh) = sprite_bounds(&g);
            let sp = 1.max(sc(s, 1.6));
            let ax = x0 + sc(s, 8.0) + (sc(s, 44.0) - bw * sp) / 2;
            let ay = y0 + (y1 - y0 - bh * sp) / 2;
            cv.sprite(&g, ax - bx * sp, ay - by * sp, sp);
            let tx = x0 + sc(s, 60.0);
            fonts.draw(cv, tx, y0 + sc(s, 14.0), &card.name, fs(BODY), true, t.ink);
            fonts.draw(cv, tx, y0 + sc(s, 31.0), &card.status, fs(TINY), false, t.muted);
            let gap = sc(s, 5.0);
            let n = card.bars.len().max(1) as i32;
            let bw = (x1 - tx - sc(s, 10.0) - (n - 1) * gap) / n;
            let by = y0 + sc(s, 44.0);
            for (k, (_, v, c)) in card.bars.iter().enumerate() {
                let bx = tx + k as i32 * (bw + gap);
                cv.rect(bx, by, bx + bw, by + sc(s, 5.0), t.track);
                cv.rect(bx, by, bx + (bw as f64 * v.clamp(0.0, 100.0) / 100.0) as i32, by + sc(s, 5.0), *c);
            }
        }
        for (i, &(ax0, ay0, ax1, ay1)) in self.care_rects.iter().enumerate() {
            let b = &self.care[i];
            let hov = self.hover == Some(Hit::Care(i)) && b.enabled;
            cv.pixel_box(ax0, ay0, ax1, ay1, p, if hov { t.accent } else { t.line });
            cv.pixel_box(ax0 + 1, ay0 + 1, ax1 - 1, ay1 - 1, p, if hov { t.accent_soft } else { t.tile });
            let col = if !b.enabled { t.off } else if hov { t.accent } else { t.ink };
            let gw = pixfont::text_width("x", ip);
            cv.icon((ax0 + ax1 - gw) / 2, ay0 + sc(s, 7.0), b.icon, ip, col);
            let lw = fonts.measure(&b.label, fs(TINY), false);
            fonts.draw(cv, (ax0 + ax1 - lw) / 2, ay1 - sc(s, 10.0), &b.label, fs(TINY), false, if b.enabled { t.ink } else { t.off });
        }
        if !self.care.is_empty() {
            cv.rect(p + pad, self.care_line, w - p - pad, self.care_line + 1, t.line);
        }
        // colleagues sit together on a soft panel
        let mut run: Option<(i32, i32)> = None;
        for (i, it) in self.items.iter().enumerate() {
            let on_panel = matches!(it, Item::Person(_) | Item::PanelRow { .. });
            let (y0, y1) = self.rows[i];
            match (on_panel, run) {
                (true, None) => run = Some((y0, y1)),
                (true, Some((a, _))) => run = Some((a, y1)),
                (false, Some((a, b))) => {
                    cv.pixel_box(p + sc(s, 6.0), a, w - p - sc(s, 6.0), b, p, t.tile);
                    run = None;
                }
                _ => {}
            }
        }
        if let Some((a, b)) = run {
            cv.pixel_box(p + sc(s, 6.0), a, w - p - sc(s, 6.0), b, p, t.tile);
        }
        for (i, it) in self.items.iter().enumerate() {
            let (y0, y1) = self.rows[i];
            let cy = (y0 + y1) / 2;
            match it {
                Item::Gap => {}
                Item::Sep => cv.rect(p + pad, cy, w - p - pad, cy + 1, t.line),
                Item::Section { title, badge, link } => {
                    let tp = 1.max(sc(s, 1.0));
                    let ty = if badge.is_some() { cy - 7 * tp / 2 + sc(s, 2.0) } else { y1 - sc(s, 9.0) - 7 * tp };
                    let tw = cv.pixel_text(p + pad + sc(s, 2.0), ty, &title.to_uppercase(), tp, t.muted);
                    if let Some((text, live)) = badge {
                        let col = if *live { t.ok } else { t.muted };
                        let bx = p + pad + sc(s, 2.0) + tw + sc(s, 10.0);
                        let d = sc(s, 7.0);
                        let by = cy + sc(s, 2.0);
                        if *live {
                            let halo = mix(col, t.surface, 0.3);
                            dot(cv, bx - sc(s, 2.0), by - d / 2 - sc(s, 2.0), d + sc(s, 4.0), halo);
                        }
                        dot(cv, bx, by - d / 2, d, col);
                        fonts.draw(cv, bx + d + sc(s, 6.0), by, text, fs(if *live { BODY } else { SMALL }), *live, col);
                    }
                    if let Some((label, _)) = link {
                        let (x0, ly0, x1, ly1) = self.link_rect(i);
                        if self.hover == Some(Hit::Link(i)) {
                            cv.pixel_box(x0, ly0, x1, ly1, p, t.hover);
                        }
                        let lw = fonts.measure(label, fs(SMALL), false);
                        let gw = pixfont::text_width("x", tp);
                        let sx = (x0 + x1 - lw - gw - sc(s, 4.0)) / 2;
                        fonts.draw(cv, sx, (ly0 + ly1) / 2, label, fs(SMALL), false, t.accent);
                        cv.icon(sx + lw + sc(s, 4.0), (ly0 + ly1) / 2 - 7 * tp / 2, "chevron", tp, t.accent);
                    }
                }
                Item::Tools(tools) => {
                    for (k, r) in self.tool_rects(i).into_iter().enumerate() {
                        let tool = &tools[k];
                        let hov = self.hover == Some(Hit::Tool(i, k));
                        cv.pixel_box(r.0, r.1, r.2, r.3, p, if hov { t.accent_soft } else { t.tile });
                        let col = if hov { t.accent } else { t.ink };
                        let gw = pixfont::text_width("x", ip);
                        let mut size = fs(SMALL);
                        let mut lw = fonts.measure(&tool.label, size, false);
                        if gw + sc(s, 5.0) + lw > (r.2 - r.0) - sc(s, 8.0) {
                            size = fs(TINY);
                            lw = fonts.measure(&tool.label, size, false);
                        }
                        let sx = (r.0 + r.2 - (gw + sc(s, 4.0) + lw)) / 2;
                        cv.icon(sx, (r.1 + r.3 - 7 * ip) / 2, tool.icon, ip, col);
                        fonts.draw(cv, sx + gw + sc(s, 4.0), (r.1 + r.3) / 2, &tool.label, size, false, col);
                        if !tool.badge.is_empty() {
                            let bw = fonts.measure(&tool.badge, fs(TINY), false);
                            fonts.draw(cv, r.2 - sc(s, 4.0) - bw, r.1 + sc(s, 7.0), &tool.badge, fs(TINY), false, t.accent);
                        }
                    }
                }
                Item::Person(pr) => self.draw_person(cv, t, fonts, i, pr),
                Item::PanelRow { icon, label, .. } => {
                    let hov = self.hover == Some(Hit::Row(i));
                    if hov {
                        cv.pixel_box(p + sc(s, 8.0), y0 + 1, w - p - sc(s, 8.0), y1 - 1, p, t.accent_soft);
                    }
                    let ix = p + pad + sc(s, 2.0);
                    cv.icon(ix, cy - 7 * ip / 2, icon, ip, if hov { t.accent } else { t.muted });
                    fonts.draw(cv, ix + sc(s, 24.0), cy, label, fs(BODY), false, t.ink);
                }
                Item::Row { icon, label, hint, check, enabled, .. } => {
                    let hov = *enabled && self.hover == Some(Hit::Row(i));
                    if hov {
                        cv.pixel_box(p + sc(s, 4.0), y0 + 1, w - p - sc(s, 4.0), y1 - 1, p, t.hover);
                    }
                    let fg = if *enabled { t.ink } else { t.off };
                    let ix = p + pad + sc(s, 2.0);
                    cv.icon(ix, cy - 7 * ip / 2, icon, ip, if !enabled { t.off } else if hov { t.accent } else { t.muted });
                    let label_max = w - ix - sc(s, 24.0) - sc(s, 60.0);
                    let label = fonts.fit(label, fs(BODY), false, label_max.max(40));
                    fonts.draw(cv, ix + sc(s, 24.0), cy, &label, fs(BODY), false, fg);
                    let rx = w - p - pad - sc(s, 2.0);
                    if let Some(on) = check {
                        let (tw, th) = (sc(s, 30.0), sc(s, 16.0));
                        let (tx0, ty0) = (rx - tw, cy - th / 2);
                        cv.pixel_box(tx0, ty0, rx, ty0 + th, 1.max(p / 2 + 1), if *on { t.accent } else { t.track });
                        let k = th - sc(s, 6.0);
                        let kx = if *on { rx - k - sc(s, 3.0) } else { tx0 + sc(s, 3.0) };
                        cv.pixel_box(kx, ty0 + sc(s, 3.0), kx + k, ty0 + sc(s, 3.0) + k, 1, t.knob);
                    } else if !hint.is_empty() {
                        let hw = fonts.measure(hint, fs(SMALL), false);
                        fonts.draw(cv, rx - hw, cy, hint, fs(SMALL), false, t.muted);
                    }
                }
            }
        }
    }

    fn draw_person(&self, cv: &mut Canvas, t: &Theme, fonts: &mut Fonts, i: usize, pr: &Person) {
        let (s, p, w) = (self.s, self.p, self.w);
        let (y0, y1) = self.rows[i];
        let fs = |v: f32| v * s as f32;
        let row_hov = self.hover == Some(Hit::Row(i));
        if row_hov || matches!(self.hover, Some(Hit::Call(j)) | Some(Hit::Message(j)) if j == i) {
            cv.pixel_box(p + sc(s, 8.0), y0 + 1, w - p - sc(s, 8.0), y1 - 1, p, t.accent_soft);
        }
        let mood = if row_hov { "happy" } else { "normal" };
        let g = if pr.stage == "egg" {
            build_egg(1, 0.5)
        } else {
            let stage = if matches!(pr.stage.as_str(), "baby" | "child" | "adult") { pr.stage.as_str() } else { "adult" };
            let palette = if sprites::PALETTES.contains(&pr.palette.as_str()) { pr.palette.as_str() } else { "gold" };
            build_pet(&PetLook {
                stage: Stage::from_name(stage),
                palette,
                mood,
                frame: 1,
                facing: 0,
                squash: false,
                blink: false,
                species: &pr.species,
            })
        };
        let (bx, by, _, _) = sprite_bounds(&g);
        let sp = 1.max(sc(s, 0.9));
        let ax = p + sc(s, 10.0);
        let rows_fit = ((y1 - y0) - sc(s, 6.0)) / sp;
        let top = y0 + sc(s, 3.0);
        for (&(cx, cy), &c) in &g {
            if cy - by < rows_fit {
                let (x, y) = (ax + (cx - bx) * sp, top + (cy - by) * sp);
                cv.rect(x, y, x + sp, y + sp, c);
            }
        }
        let tx = ax + sc(s, 34.0);
        let cy = (y0 + y1) / 2 - if pr.busy { sc(s, 6.0) } else { 0 };
        let name = fonts.fit(&pr.name, fs(BODY), false, w - tx - sc(s, 92.0));
        let nw = fonts.draw(cv, tx, cy, &name, fs(BODY), false, t.ink);
        if pr.internet {
            let bp = 2.max(sc(s, 1.6));
            cv.bitmap(tx + nw + sc(s, 7.0), cy - 7 * bp / 2, pixfont::big("globe"), bp, NET_BLUE);
        }
        if pr.busy {
            fonts.draw(cv, tx, (y0 + y1) / 2 + sc(s, 8.0), "busy", fs(TINY), false, t.warn);
        }
        for (k, r) in self.person_buttons(i).into_iter().enumerate() {
            let hov = self.hover == Some(if k == 0 { Hit::Call(i) } else { Hit::Message(i) });
            let (base, over) = if k == 0 { CALL_GREEN } else { MSG_BLUE };
            cv.pixel_box(r.0, r.1, r.2, r.3, p, if hov { over } else { base });
            let rows = pixfont::big(if k == 0 { "phone" } else { "chat" });
            let bp = 1.max(sc(s, 1.5));
            let (bw, bh) = (rows[0].len() as i32 * bp, rows.len() as i32 * bp);
            cv.bitmap((r.0 + r.2 - bw) / 2, (r.1 + r.3 - bh) / 2, rows, bp, 0xffffff);
        }
    }
}

fn dot(cv: &mut Canvas, x: i32, y: i32, d: i32, c: Color) {
    let r = d as f64 / 2.0;
    for yy in 0..d {
        for xx in 0..d {
            let (dx, dy) = (xx as f64 + 0.5 - r, yy as f64 + 0.5 - r);
            if dx * dx + dy * dy <= r * r {
                cv.rect(x + xx, y + yy, x + xx + 1, y + yy + 1, c);
            }
        }
    }
}

fn mix(a: Color, b: Color, t: f64) -> Color {
    let ch = |c: Color, sh: u32| ((c >> sh) & 255) as f64;
    let m = |sh: u32| ((ch(a, sh) * t + ch(b, sh) * (1.0 - t)).round() as u32) << sh;
    m(16) | m(8) | m(0)
}
