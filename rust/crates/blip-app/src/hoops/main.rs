//! Hoops: a little add-on game for Blip, downloaded only by people who get it in the shop.
//! The pet curls into its ball and sits in a slingshot on the left of the taskbar; pull it back,
//! let go, and get it through a hoop hanging on the right edge of the screen. Ten balls a game.
//!
//!     blip-hoops --species bunny --palette peach --stage adult --wear party,shades --best 4 --at X,Y
//!
//! Prints one line of JSON when it ends: {"score":N,"shots":10}. `--auto` plays by itself (tests).

#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(dead_code)]

#[path = "../overlay.rs"]
mod overlay;
#[path = "../platform/mod.rs"]
mod platform;
#[path = "../shot.rs"]
mod shot;
mod sim;

use std::time::Instant;

use blip_pet::body::{add_trail, Geo, Rect};
use blip_pet::sprites::{self, build_species_ball, Sprite, GH};
use blip_ui::Canvas;
use overlay::Overlay;
use sim::{Ev, Hoop, Sim};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

pub fn log(msg: &str) {
    if let Some(path) = std::env::var_os("BLIP_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let _ = writeln!(f, "{t:.1} hoops: {msg}");
        }
    }
}

/// Test hook: BLIP_SHOT=dir saves what each window shows (latest frame) as PNGs.
fn shot(name: &str, cv: &Canvas) {
    shot::canvas(&format!("hoops-{name}"), cv);
}

const SHOTS: u32 = 10;
const WOOD: u32 = 0x8a5a2b;
const WOOD_D: u32 = 0x5e3b1a;
const BAND: u32 = 0x7a2a1a;
const OUT: u32 = 0x2b2340;

struct Args {
    species: String,
    palette: String,
    wear: Vec<String>,
    best: u32,
    at: (f64, f64),
    auto: bool,
}

fn args() -> Args {
    let mut a = Args { species: "blip".into(), palette: "gold".into(), wear: vec![], best: 0, at: (800.0, 800.0), auto: false };
    let v: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < v.len() {
        let next = v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--species" => a.species = next,
            "--palette" => a.palette = next,
            "--wear" => a.wear = next.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            "--best" => a.best = next.parse().unwrap_or(0),
            "--at" => {
                let mut it = next.split(',').filter_map(|x| x.parse::<f64>().ok());
                a.at = (it.next().unwrap_or(800.0), it.next().unwrap_or(800.0));
            }
            "--auto" => {
                a.auto = true;
                i += 1;
                continue;
            }
            "--stage" => {}
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    a
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// a new ball drops into the slingshot (since)
    Loading(f64),
    Ready,
    Pulling,
    Flying,
    /// between shots, until
    Wait(f64),
    /// the game is over, closing at
    Over(f64),
}

struct Game {
    a: Args,
    s: f64,
    p: i32,
    geo: Geo,
    work: Rect,
    sim: Sim,
    anchor: (f64, f64),
    pull_r: f64,
    vk: f64,
    sling: Overlay,
    sling_at: (i32, i32),
    sling_cv: Canvas,
    hoop_win: Overlay,
    ball_win: Overlay,
    close_rect: (i32, i32, i32, i32),
    phase: Phase,
    pull: (f64, f64),
    shots: u32,
    score: u32,
    t0: Instant,
    last: f64,
    released: f64,
    swish: f64,
    shake: f64,
    squash_until: f64,
    pop: f64,
    rng: u64,
    done: bool,
    /// auto player: aiming since (to show the pulled slingshot for a moment)
    aiming: Option<f64>,
}

impl Game {
    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    fn rand(&mut self) -> f64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }

    fn new(el: &ActiveEventLoop, a: Args) -> Option<Game> {
        let (work, scale) = platform::work_area_at(a.at.0, a.at.1);
        let s = scale.max(1.0);
        let geo = Geo::new(s);
        let p = geo.p;
        let r = 7.5 * p as f64;
        let floor = work.bottom;
        // the slingshot stands on the ground on the left; the hoop hangs on the right edge
        let anchor = (work.left + 150.0 * s, floor - 95.0 * s);
        let pull_r = 115.0 * s;
        // the slingshot's window: room for the ball pulled back all the way
        let half = pull_r + r + 30.0 * s;
        let sling_at = ((anchor.0 - half) as i32, (anchor.1 - half) as i32);
        let sling_cv = Canvas::new((half * 2.0) as i32, (floor - (anchor.1 - half)) as i32);
        let hoop = Hoop { x: 0.0, y: 0.0, w: 2.0 * r * 1.6, board_h: 80.0 * s, board_left: false };
        let sim = Sim::new(r, floor, work.left, work.right, work.top, 1500.0 * s, hoop);
        // enough power to cross the whole screen, whatever its width
        let vk = 23.0 * ((work.right - work.left) / (1920.0 * s)).max(0.6).sqrt();
        let sling = Overlay::new(el, sling_cv.w, sling_cv.h, sling_at.0, sling_at.1, true)?;
        let hoop_win = Overlay::new(el, 10, 10, 0, 0, true)?;
        let ball_win = Overlay::new(el, GH * p, GH * p, 0, 0, true)?;
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
        let mut g = Game {
            a,
            s,
            p,
            geo,
            work,
            sim,
            anchor,
            pull_r,
            vk,
            sling,
            sling_at,
            sling_cv,
            hoop_win,
            ball_win,
            close_rect: (0, 0, 0, 0),
            phase: Phase::Loading(0.0),
            pull: anchor,
            shots: 0,
            score: 0,
            t0: Instant::now(),
            last: 0.0,
            released: -9.0,
            swish: -9.0,
            shake: -9.0,
            squash_until: 0.0,
            pop: -9.0,
            rng: seed,
            done: false,
            aiming: None,
        };
        g.place_hoop();
        g.draw_hoop();
        Some(g)
    }

    /// Hang the hoop on the right edge of the screen at a new height, only where a good pull can
    /// actually reach it.
    fn place_hoop(&mut self) {
        let h = self.work.bottom - self.work.top;
        let hw = self.sim.hoop.w;
        let vmax = self.pull_r * self.vk * 0.85;
        let from = (self.anchor.0 - 0.6 * self.pull_r, self.anchor.1 + 0.4 * self.pull_r);
        let x = self.work.right - 14.0 * self.s - hw * 1.08;
        let old = self.sim.hoop.y;
        self.sim.hoop.x = x;
        for tries in 0..40 {
            let y = self.work.top + h * (0.2 + self.rand() * 0.42);
            if (y - old).abs() < 60.0 * self.s && tries < 30 {
                continue; // somewhere clearly new
            }
            self.sim.hoop.y = y;
            if self.sim.solve_step(from.0, from.1, vmax, 4).is_some() {
                log(&format!("hoop at {x:.0},{y:.0}"));
                return;
            }
        }
        self.sim.hoop.y = self.work.top + h * 0.55;
        log("hoop at the fallback height");
    }

    fn ball_sprite(&self, squash: f64, face: bool) -> Sprite {
        let colors = sprites::palette(&self.a.palette);
        build_species_ball(&self.a.species, colors, self.sim.spin, face, squash)
    }

    /// Draw a sprite cell-grid so the ball's centre lands at (cx, cy) on `cv` (window px).
    fn put_ball(&self, cv: &mut Canvas, g: &Sprite, cx: f64, cy: f64, squash: f64) {
        let p = self.p as f64;
        let ry = 7.5 * (1.0 - squash);
        let bx = cx - 16.0 * p;
        let by = cy + 7.5 * p - (GH as f64) * p + (7.5 - ry) * 0.0;
        cv.sprite(g, bx as i32, by as i32, self.p);
    }

    // ------------------------------------------------ drawing
    fn thick_line(cv: &mut Canvas, a: (f64, f64), b: (f64, f64), w: i32, c: u32) {
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / 2.0).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            cv.rect(x as i32 - w / 2, y as i32 - w / 2, x as i32 - w / 2 + w, y as i32 - w / 2 + w, c);
        }
    }

    fn draw_sling(&mut self, now: f64) {
        let p = self.p;
        let pf = p as f64;
        let mut cv = std::mem::replace(&mut self.sling_cv, Canvas::new(1, 1));
        cv.clear();
        let o = (self.sling_at.0 as f64, self.sling_at.1 as f64);
        let (ax, ay) = (self.anchor.0 - o.0, self.anchor.1 - o.1);
        let floor = cv.h as f64;
        let tips = [(ax - 7.0 * pf, ay - 1.0 * pf), (ax + 7.0 * pf, ay - 1.0 * pf)];
        let fork = (ax, ay + 7.0 * pf);
        // where the band's pouch is: on the ball when loaded, snapping back after a shot
        let loaded = matches!(self.phase, Phase::Ready | Phase::Pulling | Phase::Loading(_));
        let pouch = if loaded {
            (self.pull.0 - o.0, self.pull.1 - o.1)
        } else {
            let t = now - self.released;
            let k = (-(t * 9.0)).exp() * (t * 38.0).cos();
            let d = (self.pull.0 - self.anchor.0, self.pull.1 - self.anchor.1);
            (ax + d.0 * k.max(-0.4) * 0.6, ay + d.1 * k.max(-0.4) * 0.6)
        };
        // back prong and band
        Self::thick_line(&mut cv, (fork.0, fork.1), tips[0], 2 * p, WOOD_D);
        Self::thick_line(&mut cv, tips[0], pouch, p, BAND);
        // the ball in the pouch (dropping in when a new one loads)
        if loaded {
            let drop = match self.phase {
                Phase::Loading(t0) => {
                    let t = ((now - t0) / 0.35).min(1.0);
                    (1.0 - t) * (1.0 - t) * 140.0 * self.s
                }
                _ => 0.0,
            };
            let g = self.ball_sprite(0.0, true);
            self.put_ball(&mut cv, &g, pouch.0, pouch.1 - drop, 0.0);
        }
        // trunk, front prong and band
        let trunk_w = 3 * p;
        cv.rect((fork.0) as i32 - trunk_w / 2 - p / 2, fork.1 as i32, (fork.0) as i32 + trunk_w / 2 + p / 2, floor as i32, OUT);
        cv.rect((fork.0) as i32 - trunk_w / 2, fork.1 as i32, (fork.0) as i32 + trunk_w / 2, floor as i32, WOOD);
        cv.rect((fork.0) as i32 + trunk_w / 2 - p, fork.1 as i32, (fork.0) as i32 + trunk_w / 2, floor as i32, WOOD_D);
        Self::thick_line(&mut cv, (fork.0, fork.1), tips[1], 2 * p, WOOD);
        Self::thick_line(&mut cv, tips[1], pouch, p, BAND);
        // a little grass tuft at its foot
        for (dx, h) in [(-3, 2), (-2, 3), (2, 3), (3, 2), (4, 1)] {
            let x = fork.0 as i32 + dx * p;
            cv.rect(x, floor as i32 - h * p, x + p, floor as i32, 0x4caf50);
        }
        self.sling.present(&cv, self.sling_at.0, self.sling_at.1);
        shot(if self.phase == Phase::Pulling { "sling-pulled" } else { "sling" }, &cv);
        self.sling_cv = cv;
    }

    fn draw_hoop(&mut self) {
        let now = self.now();
        let p = self.p;
        let pf = p as f64;
        let h = self.sim.hoop;
        let bx = h.board_x();
        // the score, small and quiet, just above the rim
        let lp = 2.max(p / 2);
        let best = self.a.best.max(self.score);
        let label = match self.phase {
            Phase::Over(_) if self.score > self.a.best && self.score > 0 => format!("{} of {SHOTS} - new best", self.score),
            Phase::Over(_) => format!("{} of {SHOTS} - best {best}", self.score),
            _ => format!("{} in - {} left", self.score, SHOTS - self.shots),
        };
        let label_w = label.chars().count() as i32 * 6 * lp + 12 * lp;
        let x1 = bx + 4.0 * pf;
        let y0 = h.board_top() - 12.0 * lp as f64 - 2.0 * pf;
        let net_h = 11.0 * pf;
        let x0 = (h.x - 4.0 * pf).min(x1 - label_w as f64);
        let y1 = h.y + net_h + 6.0 * pf;
        let mut cv = Canvas::new((x1 - x0) as i32, (y1 - y0) as i32);
        let l = |x: f64| (x - x0) as i32;
        let t = |y: f64| (y - y0) as i32;
        // backboard on the wall side: white with a red square, dark outline
        cv.rect(l(bx) - p, t(h.board_top()) - p, l(bx) + 3 * p, t(h.board_bottom()) + p, OUT);
        cv.rect(l(bx), t(h.board_top()), l(bx) + 2 * p, t(h.board_bottom()), 0xf4f4f4);
        cv.rect(l(bx), t(h.y) - 5 * p, l(bx) + p, t(h.y), 0xef5b5b);
        // net: crisscross hanging under the rim, stretching down on a swish
        let sw = now - self.swish;
        let stretch = if sw < 0.6 { 1.0 + 0.45 * (1.0 - sw / 0.6) * (sw * 14.0).sin().abs() } else { 1.0 };
        let rows = (11.0 * stretch) as i32;
        for row in 0..rows {
            let k = row as f64 / rows as f64;
            let inset = h.w * 0.18 * k;
            let (a, b) = (h.x + inset, h.x + h.w - inset);
            let y = h.y + row as f64 * pf;
            let mut x = a;
            let mut i = 0;
            while x < b {
                if (i + row) % 4 == 0 || (i - row).rem_euclid(4) == 0 {
                    cv.rect(l(x), t(y), l(x) + p, t(y) + p, 0xffffff);
                }
                x += pf;
                i += 1;
            }
        }
        // rim, from the board out to its open end
        let shake = if now - self.shake < 0.2 { ((now - self.shake) * 60.0).sin() * pf * 0.6 } else { 0.0 };
        let ry = t(h.y + shake);
        cv.rect(l(h.x) - p, ry - p, l(bx) + 1, ry + p, OUT);
        cv.rect(l(h.x), ry - p / 2 - 1, l(bx), ry + p / 2, 0xff7a1a);
        // sparkles after a basket
        let sp = now - self.swish;
        if sp < 0.7 {
            let k = sp / 0.7;
            for i in 0..6 {
                let a = i as f64 * 1.047 + 0.3;
                let d = (6.0 + 14.0 * k) * pf * 0.6;
                let (sx, sy) = (h.x + h.w / 2.0 + a.cos() * d, h.y - 4.0 * pf + a.sin() * d * 0.6);
                if t(sy) > 0 && l(sx) > 0 && l(sx) < cv.w - p {
                    cv.rect(l(sx), t(sy), l(sx) + p, t(sy) + p, if i % 2 == 0 { 0xffd23f } else { 0xffffff });
                }
            }
        }
        // the label, with a tiny x to stop, ending at the backboard
        let lx = (l(bx) + 3 * p - label_w).max(0);
        let col = if now - self.pop < 0.4 { 0xffd23f } else { 0xb8b0c8 };
        let end = cv.pixel_text(lx, lp, &label, lp, col);
        let xx = end + 4 * lp;
        cv.pixel_text(xx, lp, "x", lp, 0x8a8299);
        self.close_rect = (x0 as i32 + xx - 2 * lp, y0 as i32, x0 as i32 + xx + 8 * lp, y0 as i32 + 10 * lp);
        self.hoop_win.present(&cv, x0 as i32, y0 as i32);
        shot("hoop", &cv);
    }

    fn draw_ball(&mut self, now: f64) {
        if self.phase != Phase::Flying && !matches!(self.phase, Phase::Wait(_)) {
            self.ball_win.hide();
            return;
        }
        let squash = if now < self.squash_until { 0.22 } else { 0.0 };
        let mut g = self.ball_sprite(squash, true);
        if self.phase == Phase::Flying {
            add_trail(&mut g, self.sim.vx * 0.035, self.sim.vy * 0.035, &self.geo, false);
        }
        let p = self.p;
        let mut cv = Canvas::new(32 * p, (GH + 6) * p);
        cv.sprite(&g, 0, 6 * p, p);
        // fade out while waiting for the next ball
        if let Phase::Wait(until) = self.phase {
            let k = ((until - now) / 0.5).clamp(0.0, 1.0);
            if k < 1.0 {
                for px in cv.px.iter_mut() {
                    let a = ((*px >> 24) as f64 * k) as u32;
                    let f = |c: u32| ((c as f64) * k) as u32;
                    *px = (a << 24) | (f((*px >> 16) & 255) << 16) | (f((*px >> 8) & 255) << 8) | f(*px & 255);
                }
            }
        }
        let x = (self.sim.x - 16.0 * p as f64) as i32;
        let y = (self.sim.y - (GH as f64 - 1.0 - 7.5) * p as f64 - 6.0 * p as f64 - 0.5 * p as f64) as i32;
        self.ball_win.present(&cv, x, y);
        shot("ball", &cv);
    }

    // ------------------------------------------------ the loop
    fn tick(&mut self) {
        let now = self.now();
        let dt = (now - self.last).clamp(0.0, 0.05);
        self.last = now;
        match self.phase {
            Phase::Loading(t0) => {
                self.pull = self.anchor;
                if now - t0 > 0.35 {
                    self.phase = Phase::Ready;
                }
            }
            Phase::Ready => {
                if self.a.auto {
                    self.auto_shot(now);
                }
            }
            Phase::Pulling => {
                let (mx, my) = platform::cursor();
                let (dx, dy) = (mx - self.anchor.0, my - self.anchor.1);
                let d = dx.hypot(dy).max(1e-6);
                let k = d.min(self.pull_r) / d;
                self.pull = (self.anchor.0 + dx * k, self.anchor.1 + dy * k);
                if !platform::left_button_down() {
                    self.release(now);
                }
            }
            Phase::Flying => {
                for e in self.sim.step(dt) {
                    match e {
                        Ev::Score => {
                            self.score += 1;
                            self.swish = now;
                            self.pop = now;
                            log(&format!("score! {}", self.score));
                        }
                        Ev::Rim | Ev::Board => self.shake = now,
                        Ev::Bounce(_) => self.squash_until = now + 0.08,
                        Ev::Stop => {
                            self.phase = Phase::Wait(now + 0.6);
                        }
                    }
                }
            }
            Phase::Wait(until) => {
                if now >= until {
                    if self.shots >= SHOTS {
                        log(&format!("game over: {}/{SHOTS}", self.score));
                        self.phase = Phase::Over(now + 2.5);
                    } else {
                        if self.sim.scored {
                            self.place_hoop();
                        }
                        self.phase = Phase::Loading(now);
                    }
                    self.draw_hoop();
                }
            }
            Phase::Over(until) => {
                if now >= until {
                    self.done = true;
                }
            }
        }
        self.draw_sling(now);
        self.draw_ball(now);
        if now - self.swish < 0.8 || now - self.shake < 0.25 || self.phase == Phase::Flying {
            self.draw_hoop();
        }
        if now - self.pop < 0.45 {
            self.draw_hoop();
        }
    }

    fn release(&mut self, now: f64) {
        let (dx, dy) = (self.anchor.0 - self.pull.0, self.anchor.1 - self.pull.1);
        if dx.hypot(dy) < 18.0 * self.s {
            // barely pulled: put it back
            self.pull = self.anchor;
            self.phase = Phase::Ready;
            return;
        }
        let (vx, vy) = (dx * self.vk, dy * self.vk);
        self.fire(now, vx, vy);
    }

    fn fire(&mut self, now: f64, vx: f64, vy: f64) {
        self.sim.launch(self.pull.0, self.pull.1, vx, vy);
        self.shots += 1;
        self.released = now;
        self.phase = Phase::Flying;
        log(&format!("shot {} v=({vx:.0},{vy:.0})", self.shots));
        self.draw_hoop();
    }

    /// Test player: aim at the hoop (with a little wobble so it doesn't always score).
    fn auto_shot(&mut self, now: f64) {
        let vmax = self.pull_r * self.vk;
        let from = (self.anchor.0 - 0.6 * self.pull_r, self.anchor.1 + 0.4 * self.pull_r);
        // pull back smoothly for a moment first, like a person would
        let t0 = *self.aiming.get_or_insert(now);
        let k = ((now - t0) / 0.6).min(1.0);
        self.pull = (self.anchor.0 + (from.0 - self.anchor.0) * k, self.anchor.1 + (from.1 - self.anchor.1) * k);
        if k < 1.0 {
            return;
        }
        self.aiming = None;
        if let Some((vx, vy)) = self.sim.solve(from.0, from.1, vmax) {
            self.pull = from;
            let w = 1.0 + (self.rand() - 0.5) * 0.03;
            self.fire(now, vx * w, vy * w);
        } else {
            self.pull = from;
            self.fire(now, 900.0, -900.0);
        }
    }

    fn press(&mut self, id: WindowId) {
        let (mx, my) = platform::cursor();
        if id == self.hoop_win.id() {
            let (x0, y0, x1, y1) = self.close_rect;
            if (mx as i32) >= x0 && (mx as i32) <= x1 && (my as i32) >= y0 && (my as i32) <= y1 {
                self.done = true;
            }
            return;
        }
        if id == self.sling.id() && self.phase == Phase::Ready {
            let r = self.sim.r * 1.4;
            if (mx - self.anchor.0).hypot(my - self.anchor.1) <= r {
                self.phase = Phase::Pulling;
            }
        }
    }
}

struct Runner {
    game: Option<Game>,
    reported: bool,
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.game.is_none() {
            match Game::new(el, args()) {
                Some(g) => self.game = Some(g),
                None => el.exit(),
            }
        }
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(g) = self.game.as_mut() else { return };
        if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } = event {
            g.press(id);
        }
        if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } = event {
            g.done = true; // right-click anywhere on the game: stop
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(g) = self.game.as_mut() else { return };
        g.tick();
        if g.done {
            if !std::mem::replace(&mut self.reported, true) {
                println!("{{\"score\":{},\"shots\":{}}}", g.score, g.shots);
            }
            el.exit();
            return;
        }
        el.set_control_flow(ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_millis(15)));
    }
}

fn main() {
    #[allow(unused_mut)]
    let mut builder = EventLoop::builder();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let Ok(event_loop) = builder.build() else { return };
    let mut runner = Runner { game: None, reported: false };
    let _ = event_loop.run_app(&mut runner);
}
