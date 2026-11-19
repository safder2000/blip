//! Where the pet is and how it moves: walking along the taskbar, being dragged, thrown,
//! bouncing, rolling as a ball and (now and then) floating down on a parachute.
//! Ported from `physics`/`fly`/`roll` in `blip/app.py`. Positions are the window's top-left
//! corner in physical screen pixels; one physics step is 30 ms.

use crate::sprites::{self, Color, Sprite, GH, GW};

pub const STEP: f64 = 0.030; // seconds per physics step
pub const ANIM: f64 = 0.250; // seconds per animation frame
const HOLD: f64 = 0.55; // press and hold this long (without moving) for the menu
const BALL_SPEED: f64 = 14.0; // px per step (at 100%) that turns a throw into a ball
const CHUTE_CHANCE: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

/// Sizes, all derived from the screen scale (1.0 = 100%, 1.25 = 125%...).
#[derive(Clone, Copy, Debug)]
pub struct Geo {
    pub s: f64,
    /// screen pixels per sprite cell
    pub p: i32,
    pub w: i32,
    pub h: i32,
    /// x offset of the sprite grid inside the window
    pub ox: i32,
    /// room above the pet for bubbles and the parachute
    pub bub_h: i32,
}

impl Geo {
    pub fn new(scale: f64) -> Geo {
        let s = scale.max(1.0);
        let p = 3.max((4.0 * s).round() as i32);
        let bub_h = (48.0 * s).round() as i32;
        let w = (GW * p).max((200.0 * s).round() as i32);
        Geo { s, p, w, h: bub_h + GH * p, ox: (w - GW * p) / 2, bub_h }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Idle,
    Walk,
    Held,
    Fall,
    Unfold,
    Eat,
    Play,
    Sleep,
    /// a call or alarm: dash to the middle of the screen and hop
    Ring,
    /// the pet left: float gently as a ghost
    Gone,
}

/// What a press turned into, for the app to act on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Pat,
    Menu,
    Thrown { ball: bool },
}

pub struct Body {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub airborne: bool,
    pub land: u32,
    pub action: Action,
    action_until: f64,
    pub dir: i32,
    pub frame: u32,
    pub ball: bool,
    pub spin: f64,
    pub chute: u32,
    pub bounces: u32,
    lucky: bool,
    pub wander: bool,
    /// walks slowly (sick)
    pub slow: bool,
    /// screen scale, for jump height
    pub scale_hint: f64,
    pub happy_until: f64,
    /// where to dash to while ringing
    pub ring_target: Option<f64>,
    /// sideways shake while ringing, for drawing
    pub jitter: f64,
    phase: f64,
    trail: Vec<(f64, f64, f64)>,
    press: Option<Press>,
    /// the current press turned into "open the menu" (so letting go isn't a pat)
    held_menu: bool,
    menu_pending: bool,
    rng: u64,
}

#[derive(Clone, Copy)]
struct Press {
    off_x: f64,
    off_y: f64,
    at_x: f64,
    at_y: f64,
    t: f64,
    moved: bool,
}

impl Body {
    pub fn new(x: f64, y: f64, seed: u64) -> Body {
        Body {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            airborne: false,
            land: 0,
            action: Action::Idle,
            action_until: 0.0,
            dir: 1,
            frame: 0,
            ball: false,
            spin: 0.0,
            chute: 0,
            bounces: 0,
            lucky: false,
            wander: true,
            slow: false,
            scale_hint: 1.0,
            happy_until: 0.0,
            ring_target: None,
            jitter: 0.0,
            phase: 0.0,
            trail: Vec::new(),
            press: None,
            held_menu: false,
            menu_pending: false,
            rng: seed | 1,
        }
    }

    pub fn random(&mut self) -> f64 {
        // xorshift64*: plenty for a pet's whims
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn until(&self) -> f64 {
        self.action_until
    }

    pub fn set(&mut self, a: Action, secs: f64, now: f64) {
        self.action = a;
        self.action_until = now + secs;
    }

    pub fn ground(geo: &Geo, work: &Rect) -> f64 {
        work.bottom - geo.h as f64
    }

    pub fn bounds(geo: &Geo, work: &Rect) -> (f64, f64) {
        let pad = (geo.ox + 6 * geo.p) as f64;
        (work.left - pad, work.right - geo.w as f64 + pad)
    }

    // ------------------------------------------------ every 30 ms
    pub fn step(&mut self, geo: &Geo, work: &Rect, now: f64) {
        let s = geo.s;
        let (lo, hi) = Self::bounds(geo, work);
        let ground = Self::ground(geo, work);
        self.jitter = 0.0;
        match self.action {
            Action::Held => {}
            Action::Gone => {
                self.phase += 0.04;
                let target = ground - 30.0 * s + self.phase.sin() * 6.0 * s;
                self.y += (target - self.y) * 0.05;
            }
            Action::Fall if self.airborne || self.ball || self.y < ground - 0.5 => {
                self.airborne = true;
                self.fly(geo, work, ground, lo, hi, now);
            }
            _ if self.airborne || self.y < ground - 0.5 => {
                self.airborne = true;
                self.chute = 0;
                self.vy += 0.9 * s;
                self.y += self.vy;
                if self.action == Action::Play {
                    self.x += self.dir as f64 * 2.0 * s;
                }
                if self.y >= ground {
                    self.y = ground;
                    self.vy = 0.0;
                    self.airborne = false;
                    self.land = 2;
                    if self.action == Action::Fall {
                        self.set(Action::Idle, 1.5, now);
                    } else if self.action == Action::Play && now < self.action_until {
                        self.jump();
                    }
                }
            }
            _ => {
                self.y = ground;
                if self.vx.abs() > 0.3 * s {
                    self.x += self.vx; // skid after a throw
                    self.vx *= 0.8;
                } else {
                    self.vx = 0.0;
                }
                match self.action {
                    Action::Walk => self.x += self.dir as f64 * if self.slow { 0.7 } else { 1.5 } * s,
                    Action::Play if now < self.action_until => self.jump(),
                    Action::Ring => {
                        if let Some(t) = self.ring_target.filter(|t| (t - self.x).abs() > 4.0) {
                            let step = 9.0 * s;
                            self.dir = if t > self.x { 1 } else { -1 };
                            self.x += (t - self.x).clamp(-step, step);
                        } else {
                            self.jitter = if self.frame % 2 == 1 { 2.0 } else { -2.0 } * s;
                            if self.random() < 0.02 {
                                self.jump();
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if self.action != Action::Held {
            if self.x < lo {
                self.x = lo;
                self.dir = 1;
            } else if self.x > hi {
                self.x = hi;
                self.dir = -1;
            }
        }
        // press and hold, without moving: the menu
        if let Some(p) = self.press {
            if !p.moved && now - p.t >= HOLD {
                self.press = None;
                self.held_menu = true;
                self.menu_pending = true;
            }
        }
    }

    pub fn jump(&mut self) {
        self.vy = -8.5 * 1.0f64.max(self.scale_hint);
        self.airborne = true;
        if self.action == Action::Play && self.random() < 0.3 {
            self.dir = -self.dir;
        }
    }

    fn fly(&mut self, geo: &Geo, work: &Rect, ground: f64, lo: f64, hi: f64, now: f64) {
        let s = geo.s;
        let top = work.top - geo.bub_h as f64;
        if self.ball {
            self.roll(geo, work, ground, lo, hi, now);
            return;
        }
        if self.chute > 0 {
            self.chute += 1;
            self.vy += (1.7 * s - self.vy) * 0.12; // drift down slowly
            self.vx *= 0.96;
            self.x += self.vx + (self.chute as f64 / 10.0).sin() * 0.8 * s;
        } else {
            self.vy += 0.9 * s;
            self.vx *= 0.995;
            self.x += self.vx;
            if self.lucky && self.vy > 3.0 * s && ground - self.y > 190.0 * s && self.bounces == 0 {
                self.chute = 1;
            }
        }
        self.y += self.vy;
        if self.y < top {
            self.y = top;
            self.vy = self.vy.abs() * 0.4;
        }
        if self.x < lo {
            self.x = lo;
            self.vx = self.vx.abs() * 0.6;
        } else if self.x > hi {
            self.x = hi;
            self.vx = -self.vx.abs() * 0.6;
        }
        if self.y < ground {
            return;
        }
        self.y = ground;
        self.land = 2;
        if self.chute == 0 && self.vy > 5.0 * s && self.bounces < 5 {
            self.vy = -self.vy * 0.48;
            self.vx *= 0.75;
            self.bounces += 1;
            return;
        }
        let soft = self.chute > 0;
        self.vy = 0.0;
        self.airborne = false;
        self.chute = 0;
        self.set(Action::Idle, 1.5, now);
        if soft {
            self.happy_until = now + 2.0;
        }
    }

    fn roll(&mut self, geo: &Geo, work: &Rect, ground: f64, lo: f64, hi: f64, now: f64) {
        let (s, p) = (geo.s, geo.p as f64);
        let top = work.top - geo.bub_h as f64 - 12.0 * p;
        self.vy += 0.75 * s;
        self.vx *= 0.998;
        self.x += self.vx;
        self.y += self.vy;
        self.spin += self.vx / (7.5 * p);
        if self.x < lo {
            self.x = lo;
            self.vx = self.vx.abs() * 0.78;
        } else if self.x > hi {
            self.x = hi;
            self.vx = -self.vx.abs() * 0.78;
        }
        if self.y < top {
            self.y = top;
            self.vy = self.vy.abs() * 0.7;
        }
        if self.y < ground {
            return;
        }
        self.y = ground;
        if self.vy > 2.5 * s {
            self.vy = -self.vy * 0.62;
            self.vx *= 0.92;
            self.bounces += 1;
            self.land = 1;
            return;
        }
        self.vy = 0.0;
        self.vx *= 0.955; // rolling along the taskbar
        if self.vx.abs() < 0.5 * s {
            self.vx = 0.0;
            self.ball = false;
            self.airborne = false;
            self.land = 3;
            self.set(Action::Unfold, 0.6, now);
        }
    }

    // ------------------------------------------------ every 250 ms
    pub fn anim(&mut self, now: f64) {
        self.frame = self.frame.wrapping_add(1);
        self.land = self.land.saturating_sub(1);
        match self.action {
            Action::Unfold if now >= self.action_until => {
                self.set(Action::Idle, 1.5, now);
                self.happy_until = now + 2.0;
            }
            Action::Idle | Action::Walk if now >= self.action_until => self.choose(now),
            Action::Play if now >= self.action_until && !self.airborne => self.set(Action::Idle, 2.0, now),
            _ => {}
        }
    }

    fn choose(&mut self, now: f64) {
        if self.wander && self.random() < 0.55 {
            self.dir = if self.random() < 0.5 { -1 } else { 1 };
            let secs = 2.0 + self.random() * 4.0;
            self.set(Action::Walk, secs, now);
        } else {
            let secs = 2.0 + self.random() * 5.0;
            self.set(Action::Idle, secs, now);
        }
    }

    // ------------------------------------------------ mouse
    /// Left button down at screen position (sx, sy).
    pub fn press(&mut self, sx: f64, sy: f64, now: f64) {
        self.press = Some(Press { off_x: sx - self.x, off_y: sy - self.y, at_x: sx, at_y: sy, t: now, moved: false });
        self.trail.clear();
        self.held_menu = false;
    }

    /// Mouse moved with the button down. True once it has become a drag.
    pub fn drag(&mut self, sx: f64, sy: f64, now: f64) -> bool {
        let Some(mut p) = self.press else { return false };
        if !p.moved && (sx - p.at_x).abs() + (sy - p.at_y).abs() > 5.0 {
            p.moved = true;
            if self.action != Action::Gone {
                self.ball = false;
                self.chute = 0;
                self.ring_target = None;
                self.set(Action::Held, 1e18, now);
            }
        }
        if p.moved && self.action != Action::Gone {
            self.x = sx - p.off_x;
            self.y = sy - p.off_y;
            self.trail.push((now, sx, sy));
            if self.trail.len() > 8 {
                self.trail.remove(0);
            }
        }
        self.press = Some(p);
        p.moved
    }

    /// Button up. None when the press already opened the menu.
    pub fn release(&mut self, geo: &Geo, now: f64) -> Option<Gesture> {
        if std::mem::take(&mut self.held_menu) {
            return None;
        }
        let p = self.press.take()?;
        if !p.moved {
            self.happy_until = now + 1.5;
            return Some(Gesture::Pat);
        }
        if self.action == Action::Gone {
            return Some(Gesture::Pat);
        }
        let (vx, vy) = self.throw_speed(geo, now);
        self.vx = vx;
        self.vy = vy;
        self.airborne = true;
        self.chute = 0;
        self.bounces = 0;
        self.ball = vx.hypot(vy) > BALL_SPEED * geo.s;
        self.lucky = !self.ball && self.random() < CHUTE_CHANCE;
        self.set(Action::Fall, 1e18, now);
        Some(Gesture::Thrown { ball: self.ball })
    }

    /// True once when a press has been held long enough to open the menu (the app checks each step).
    pub fn take_menu(&mut self) -> bool {
        std::mem::take(&mut self.menu_pending)
    }

    /// Mouse speed over the last ~0.1 s, in pixels per physics step.
    fn throw_speed(&self, geo: &Geo, now: f64) -> (f64, f64) {
        let pts: Vec<_> = self.trail.iter().filter(|t| now - t.0 < 0.12).collect();
        if pts.len() < 2 || now - pts[pts.len() - 1].0 > 0.08 {
            return (0.0, 0.0);
        }
        let (a, b) = (pts[0], pts[pts.len() - 1]);
        let dt = (b.0 - a.0).max(0.016);
        let lim = 40.0 * geo.s;
        let vx = (b.1 - a.1) / dt * STEP;
        let vy = (b.2 - a.2) / dt * STEP;
        (vx.clamp(-lim, lim), vy.clamp(-lim, lim))
    }

    // ------------------------------------------------ looks
    pub fn mood(&self, now: f64) -> &'static str {
        match self.action {
            Action::Held => "held",
            Action::Unfold => "happy",
            Action::Fall if self.airborne => {
                if self.chute > 0 {
                    "happy"
                } else {
                    "held"
                }
            }
            _ if now < self.happy_until => "happy",
            _ => "normal",
        }
    }

    pub fn facing(&self) -> i32 {
        if self.action == Action::Walk {
            self.dir
        } else {
            0
        }
    }

    pub fn squash(&self) -> bool {
        self.land > 0 || if self.action == Action::Walk { self.frame % 2 == 0 } else { self.frame % 6 == 0 }
    }

    /// The pet as drawn right now: normal pose, ball, parachute, flame trail.
    pub fn sprite(&mut self, look: &Look, geo: &Geo, now: f64) -> Sprite {
        let mood = self.mood(now);
        self.sprite_as(look, geo, mood, None)
    }

    /// Like `sprite`, with the face chosen by the app; `egg` = hatch progress for an egg.
    pub fn sprite_as(&mut self, look: &Look, geo: &Geo, mood: &str, egg: Option<f64>) -> Sprite {
        if self.ball {
            let colors = if egg.is_some() { (sprites::col::EGG, sprites::col::EGG2, sprites::col::SPOT) } else { sprites::palette(&look.palette) };
            let mut g = sprites::build_ball(colors, self.spin, egg.is_none(), if self.land > 0 { 0.25 } else { 0.0 });
            add_trail(&mut g, self.vx, self.vy, geo);
            return g;
        }
        if let Some(progress) = egg {
            return sprites::build_egg(self.frame, progress);
        }
        let facing = if matches!(self.action, Action::Walk | Action::Play | Action::Eat) || (self.action == Action::Ring && self.ring_target.is_some()) { self.dir } else { 0 };
        let squash = self.land > 0
            || match self.action {
                Action::Walk | Action::Ring => self.frame % 2 == 0,
                Action::Sleep => self.frame % 8 < 4,
                _ => self.frame % 6 == 0,
            };
        let blink = (mood == "normal" || mood == "sad") && self.random() < 0.08;
        let mut g = sprites::build_pet(&sprites::PetLook {
            stage: look.stage,
            palette: &look.palette,
            mood,
            frame: self.frame,
            facing,
            squash,
            blink,
            species: &look.species,
        });
        if self.chute > 0 && self.action == Action::Fall {
            let top_row = g.keys().map(|c| c.1).min().unwrap_or(0);
            add_chute(&mut g, top_row, self.chute);
        }
        g
    }
}

/// What the pet looks like (from the saved state).
#[derive(Clone, Debug)]
pub struct Look {
    pub species: String,
    pub palette: String,
    pub stage: sprites::Stage,
}

/// Rocket flame and smoke puffs behind a fast ball.
pub fn add_trail(g: &mut Sprite, vx: f64, vy: f64, geo: &Geo) {
    let v = vx.hypot(vy);
    if v < 8.0 * geo.s {
        return;
    }
    let (ux, uy) = (-vx / v, -vy / v);
    let (cx, cy) = (16.0, GH as f64 - 8.5);
    let (xmin, xmax) = (-geo.ox / geo.p, (geo.w - geo.ox) / geo.p - 1);
    let ymin = -geo.bub_h / geo.p;
    let n = 2 + 3.min((v / (8.0 * geo.s)) as i32);
    let colors: [Color; 5] = [0xfff1a8, 0xffb347, 0xff6b3d, 0xcfc8dc, 0xb3abc4];
    for k in 0..n {
        let d = 9.0 + 3.2 * k as f64;
        let size = if k < 2 { 3 } else { 2 };
        let (px, py) = ((cx + ux * d) as i32, (cy + uy * d) as i32);
        for dx in 0..size {
            for dy in 0..size {
                let (x, y) = (px + dx - size / 2, py + dy - size / 2);
                if (xmin..=xmax).contains(&x) && ymin <= y && y < GH {
                    g.entry((x, y)).or_insert(colors[(k as usize).min(4)]);
                }
            }
        }
    }
}

/// A striped pixel parachute above the pet's head, swaying a little.
pub fn add_chute(g: &mut Sprite, top_row: i32, chute: u32) {
    let sway = ((chute as f64 / 10.0).sin() * 1.5).round_ties_even() as i32; // Python's round()
    let cx = GW / 2 + sway;
    let top = top_row - 11;
    let widths = [6, 10, 12, 13, 13];
    let (red, cream, ink) = (0xef5b6b, 0xfff4e6, sprites::col::OUT);
    for (i, hw) in widths.iter().enumerate() {
        let y = top + i as i32;
        for x in (cx - hw)..(cx + hw) {
            let edge = x == cx - hw || x == cx + hw - 1 || i == 0;
            let stripe = (x - cx + 13).div_euclid(4).rem_euclid(2) == 1;
            g.insert((x, y), if edge { ink } else if stripe { red } else { cream });
        }
    }
    let y = top + widths.len() as i32;
    for x in (cx - 13)..(cx + 13) {
        if matches!((x - cx).rem_euclid(4), 0 | 1) {
            g.insert((x, y), ink);
        }
    }
    for (sx, ex) in [(cx - 12, GW / 2 - 6), (cx - 4, GW / 2 - 3), (cx + 3, GW / 2 + 2), (cx + 11, GW / 2 + 5)] {
        let (y0, y1) = (y + 1, top_row + 2);
        let n = (y1 - y0).max(0);
        for k in 0..n {
            let x = (sx as f64 + (ex - sx) as f64 * k as f64 / ((y1 - y0 - 1).max(1)) as f64).round_ties_even() as i32;
            g.entry((x, y0 + k)).or_insert(0x9a90b0);
        }
    }
}
