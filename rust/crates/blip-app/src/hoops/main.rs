//! Hoops: a little add-on game for Blip, downloaded only by people who get it in the shop.
//! The pet walks over to a slingshot standing on the ground at the left of the screen, hops in
//! and curls into its ball; pull it back, let go, and get it through a hoop hanging on the right
//! edge. Wherever it lands it gets up and makes its own way back to the slingshot. No end: play
//! until you stop (right-click, or the little x by the score). Drag the slingshot by its wood to
//! take the game to another screen.
//!
//!     blip-hoops --species bunny --palette peach --stage adult --wear party,shades --best 4
//!                --coins-left 20 --at X,Y
//!
//! Prints a line of JSON for every coin a basket earns, {"coin":1}, and one when it ends:
//! {"score":N,"shots":M,"x":X,"y":Y} (where the pet is, for Blip to put its pet back there).
//! `--auto` plays by itself (tests); `--shots N` stops after N shots; `--move-to X,Y` (with
//! --auto) takes the game to the screen at X,Y after the second shot.

#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(dead_code)]

#[path = "../overlay.rs"]
mod overlay;
#[path = "../platform/mod.rs"]
mod platform;
#[path = "../shot.rs"]
mod shot;
mod sim;

use std::collections::VecDeque;
use std::io::Write;
use std::time::Instant;

use blip_pet::body::{add_trail, Geo, Rect};
use blip_pet::sprites::{self, build_species_ball, PetLook, Sprite, Stage, GH};
use blip_ui::Canvas;
use overlay::Overlay;
use sim::{Ev, Hoop, Sim};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

pub fn log(msg: &str) {
    if let Some(path) = std::env::var_os("BLIP_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let _ = writeln!(f, "{t:.1} hoops: {msg}");
        }
    }
}

/// Test hook: BLIP_SHOT=dir saves what each window shows (latest frame) as PNGs, and
/// BLIP_HOOPS_SEQ=dir also keeps numbered frames of the pet window (to check the animations).
fn shot(name: &str, cv: &Canvas) {
    shot::canvas(&format!("hoops-{name}"), cv);
}

fn say(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

const WOOD: u32 = 0x8a5a2b;
const WOOD_D: u32 = 0x5e3b1a;
const BAND: u32 = 0x7a2a1a;
const OUT: u32 = 0x2b2340;
const MUTED: u32 = 0xb8b0c8;
/// rows of room above the pet's grid in its window (ears, stars, cheer hops)
const HEAD: i32 = 10;

struct Args {
    species: String,
    palette: String,
    stage: String,
    wear: Vec<String>,
    best: u32,
    coins_left: u32,
    at: (f64, f64),
    auto: bool,
    shots: Option<u32>,
    move_to: Option<(f64, f64)>,
    /// tests: walk back with these routines, in turn
    routines: Vec<usize>,
}

fn point(s: &str) -> Option<(f64, f64)> {
    let mut it = s.split(',').filter_map(|x| x.trim().parse::<f64>().ok());
    Some((it.next()?, it.next()?))
}

fn args() -> Args {
    let mut a = Args {
        species: "blip".into(),
        palette: "gold".into(),
        stage: "adult".into(),
        wear: vec![],
        best: 0,
        coins_left: 0,
        at: (800.0, 800.0),
        auto: false,
        shots: None,
        move_to: None,
        routines: vec![],
    };
    let v: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < v.len() {
        let next = v.get(i + 1).cloned().unwrap_or_default();
        match v[i].as_str() {
            "--species" => a.species = next,
            "--palette" => a.palette = next,
            "--stage" => a.stage = next,
            "--wear" => a.wear = next.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            "--best" => a.best = next.parse().unwrap_or(0),
            "--coins-left" => a.coins_left = next.parse().unwrap_or(0),
            "--shots" => a.shots = next.parse().ok(),
            "--at" => a.at = point(&next).unwrap_or(a.at),
            "--move-to" => a.move_to = point(&next),
            "--routines" => a.routines = next.split(',').filter_map(|x| x.parse().ok()).collect(),
            "--auto" => {
                a.auto = true;
                i += 1;
                continue;
            }
            _ => {
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    a
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    /// the pet is on the ground, doing its plan (walking back, seeing stars...)
    Walking,
    /// hopping up into the pouch (since)
    Hopping(f64),
    Ready,
    Pulling,
    /// the slingshot is being dragged somewhere else
    Moving,
    Flying,
    /// stopping: hopping down out of the pouch (since)
    Leaving(f64),
}

/// One bit of what the pet does on the ground. Speeds are screen px a second at 100%.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// stand a moment: its face, which way it looks (0 = at you), shaking itself off, seeing
    /// stars, or a little cheering hop
    Pose { secs: f64, mood: &'static str, face: i32, shake: bool, stars: bool, cheer: bool },
    /// walk (or hop along, `hop` px high) towards the slingshot; `part` < 1 stops partway
    Walk { speed: f64, hop: f64, part: f64, mood: &'static str },
    /// stay curled up and roll back
    Roll { speed: f64 },
}

fn pose(secs: f64, mood: &'static str) -> Step {
    Step::Pose { secs, mood, face: 0, shake: false, stars: false, cheer: false }
}

fn walk(speed: f64, mood: &'static str) -> Step {
    Step::Walk { speed, hop: 0.0, part: 1.0, mood }
}

/// A coin flying from the net up to the score after a basket.
struct CoinFx {
    t0: f64,
}

struct Game {
    a: Args,
    stage: Stage,
    s: f64,
    p: i32,
    geo: Geo,
    work: Rect,
    sim: Sim,
    anchor: (f64, f64),
    /// where the anchor sits inside the slingshot's window
    anchor_off: (f64, f64),
    pull_r: f64,
    vk: f64,
    sling: Overlay,
    sling_at: (i32, i32),
    sling_cv: Canvas,
    hoop_win: Overlay,
    pet_win: Overlay,
    close_rect: (i32, i32, i32, i32),
    phase: Phase,
    pull: (f64, f64),
    shots: u32,
    score: u32,
    /// coins earned (paid when a basket pops one; counted on the label when it gets there)
    coins_paid: u32,
    coins_shown: u32,
    coin_fx: Vec<CoinFx>,
    t0: Instant,
    last: f64,
    released: f64,
    settled: f64,
    swish: f64,
    shake: f64,
    squash_until: f64,
    pop: f64,
    rng: u64,
    done: bool,
    /// auto player: aiming since (to show the pulled slingshot for a moment)
    aiming: Option<f64>,
    moved_once: bool,
    /// cursor offset into the slingshot's window while dragging it
    grab: (f64, f64),
    // ---- the pet on the ground
    px: f64,
    dir: i32,
    frame: u32,
    next_frame: f64,
    blink: bool,
    plan: VecDeque<Step>,
    step_t0: f64,
    goal: Option<f64>,
    walked: f64,
    roll_spin: f64,
    last_routine: usize,
    hop_from: (f64, f64),
    seq: u32,
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
        let geo = Geo::new(scale);
        let sling = Overlay::new(el, 10, 10, 0, 0, true)?;
        let hoop_win = Overlay::new(el, 10, 10, 0, 0, true)?;
        let pet_win = Overlay::new(el, 10, 10, 0, 0, true)?;
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
        let hoop = Hoop { x: 0.0, y: 0.0, w: 1.0, board_h: 1.0, board_left: false };
        let mut g = Game {
            stage: Stage::from_name(&a.stage),
            a,
            s: geo.s,
            p: geo.p,
            geo,
            work,
            sim: Sim::new(1.0, 0.0, 0.0, 1.0, 0.0, 1.0, hoop),
            anchor: (0.0, 0.0),
            anchor_off: (0.0, 0.0),
            pull_r: 1.0,
            vk: 1.0,
            sling,
            sling_at: (0, 0),
            sling_cv: Canvas::new(1, 1),
            hoop_win,
            pet_win,
            close_rect: (0, 0, 0, 0),
            phase: Phase::Walking,
            pull: (0.0, 0.0),
            shots: 0,
            score: 0,
            coins_paid: 0,
            coins_shown: 0,
            coin_fx: vec![],
            t0: Instant::now(),
            last: 0.0,
            released: -9.0,
            settled: -9.0,
            swish: -9.0,
            shake: -9.0,
            squash_until: 0.0,
            pop: -9.0,
            rng: seed,
            done: false,
            aiming: None,
            moved_once: false,
            grab: (0.0, 0.0),
            px: 0.0,
            dir: -1,
            frame: 0,
            next_frame: 0.0,
            blink: false,
            plan: VecDeque::new(),
            step_t0: 0.0,
            goal: None,
            walked: 0.0,
            roll_spin: 0.0,
            last_routine: 99,
            hop_from: (0.0, 0.0),
            seq: 0,
        };
        g.layout(work, scale);
        g.place_hoop();
        g.draw_hoop();
        // the pet comes over from wherever it was on the taskbar
        let (lo, hi) = (work.left + 20.0 * g.s, work.right - 20.0 * g.s);
        g.px = g.a.at.0.clamp(lo, hi);
        g.plan = VecDeque::from(vec![walk(420.0, "happy")]);
        Some(g)
    }

    /// Set the court up on the screen with work area `work`: slingshot on the left of its
    /// ground, hoop on its right edge.
    fn layout(&mut self, work: Rect, scale: f64) {
        let s = scale.max(1.0);
        self.geo = Geo::new(s);
        self.s = self.geo.s;
        self.p = self.geo.p;
        self.work = work;
        let r = 7.5 * self.p as f64;
        let floor = work.bottom;
        self.pull_r = 115.0 * s;
        // tall enough that pulling straight down never pushes the ball into the ground
        self.anchor = (work.left + 150.0 * s, floor - self.pull_r - r - 8.0 * s);
        // the slingshot's window: room for the ball pulled back all the way
        let half = self.pull_r + r + 30.0 * s;
        self.sling_at = ((self.anchor.0 - half) as i32, (self.anchor.1 - half) as i32);
        self.sling_cv = Canvas::new((half * 2.0) as i32, (floor - (self.anchor.1 - half)) as i32);
        self.anchor_off = (self.anchor.0 - self.sling_at.0 as f64, self.anchor.1 - self.sling_at.1 as f64);
        let hoop = Hoop { x: 0.0, y: 0.0, w: 2.0 * r * 1.6, board_h: 80.0 * s, board_left: false };
        // no ceiling: a high shot can sail over the top of the screen and come back down
        let (w, h) = (work.right - work.left, work.bottom - work.top);
        self.sim = Sim::new(r, floor, work.left, work.right, work.top - 3.0 * h, 1500.0 * s, hoop);
        // enough power for the far corners, whatever the screen's shape
        self.vk = 24.0 * (w / (1920.0 * s)).max(h / (1040.0 * s)).max(0.6).sqrt();
        self.pull = self.anchor;
    }

    fn label_px(&self) -> i32 {
        2.max(self.p / 2)
    }

    /// Hang the hoop on the right edge of the screen at a new height, anywhere from near the
    /// ground to near the top, but only where a good pull can reach it.
    fn place_hoop(&mut self) {
        let pf = self.p as f64;
        let hw = self.sim.hoop.w;
        let x = self.work.right - 14.0 * self.s - hw * 1.08;
        let top = self.work.top + 16.0 * self.label_px() as f64 + self.sim.hoop.board_h * 0.8 + 8.0 * self.s;
        let bottom = self.work.bottom - 11.0 * pf - 2.0 * self.sim.r - 30.0 * self.s;
        let vmax = self.pull_r * self.vk * 0.9;
        let from = self.aim_from();
        let old = self.sim.hoop.y;
        self.sim.hoop.x = x;
        let span = (bottom - top).max(1.0);
        for tries in 0..50 {
            let y = top + self.rand() * span;
            if (y - old).abs() < span * 0.15 && tries < 35 {
                continue; // somewhere clearly new
            }
            self.sim.hoop.y = y;
            if self.sim.solve_step(from.0, from.1, vmax, 3).is_some() {
                log(&format!("hoop at {x:.0},{y:.0} ({:.0}% up)", (bottom - y) / span * 100.0));
                return;
            }
        }
        self.sim.hoop.y = top + span * 0.5;
        log("hoop at the fallback height");
    }

    /// Where the auto player (and the reach check) pulls back to.
    fn aim_from(&self) -> (f64, f64) {
        (self.anchor.0 - 0.6 * self.pull_r, self.anchor.1 + 0.4 * self.pull_r)
    }

    /// Where the pet stands to hop into the pouch: just right of the slingshot.
    fn stand_x(&self) -> f64 {
        self.anchor.0 + 46.0 * self.s
    }

    fn ball_sprite(&self, spin: f64, squash: f64) -> Sprite {
        let colors = sprites::palette(&self.a.palette);
        build_species_ball(&self.a.species, colors, spin, true, squash)
    }

    fn pet_sprite(&self, mood: &str, face: i32, squash: bool) -> Sprite {
        let look = PetLook {
            stage: self.stage,
            palette: &self.a.palette,
            mood,
            frame: self.frame,
            facing: face,
            squash,
            blink: self.blink && mood == "normal",
            species: &self.a.species,
        };
        let mut g = sprites::build_pet(&look);
        sprites::add_wear(&mut g, &look, &self.a.wear);
        g
    }

    /// Draw the ball sprite so its centre lands at (cx, cy) on `cv` (window px).
    fn put_ball(&self, cv: &mut Canvas, g: &Sprite, cx: f64, cy: f64) {
        let p = self.p as f64;
        cv.sprite(g, (cx - 16.0 * p) as i32, (cy + 8.0 * p - GH as f64 * p) as i32, self.p);
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

    fn in_pouch(&self) -> bool {
        matches!(self.phase, Phase::Ready | Phase::Pulling | Phase::Moving)
    }

    fn draw_sling(&mut self, now: f64) {
        let p = self.p;
        let pf = p as f64;
        let mut cv = std::mem::replace(&mut self.sling_cv, Canvas::new(1, 1));
        cv.clear();
        let (ax, ay) = self.anchor_off;
        let o = (self.anchor.0 - ax, self.anchor.1 - ay);
        let floor = cv.h as f64;
        let tips = [(ax - 7.0 * pf, ay - 1.0 * pf), (ax + 7.0 * pf, ay - 1.0 * pf)];
        let fork = (ax, ay + 7.0 * pf);
        // the pouch: on the ball when it's in, snapping back after a shot, dipping as it hops in
        let pouch = if self.in_pouch() {
            let t = now - self.settled;
            let dip = if t < 0.6 { (-(t * 7.0)).exp() * (t * 26.0).sin() * 9.0 * self.s } else { 0.0 };
            (self.pull.0 - o.0, self.pull.1 - o.1 + dip)
        } else {
            let t = now - self.released;
            let k = (-(t * 9.0)).exp() * (t * 38.0).cos();
            let d = (self.pull.0 - self.anchor.0, self.pull.1 - self.anchor.1);
            let k = if t < 1.5 { k.max(-0.4) * 0.6 } else { 0.0 };
            (ax + d.0 * k, ay + d.1 * k)
        };
        // back prong and band
        Self::thick_line(&mut cv, (fork.0, fork.1), tips[0], 2 * p, WOOD_D);
        Self::thick_line(&mut cv, tips[0], pouch, p, BAND);
        if self.in_pouch() {
            let g = self.ball_sprite(0.0, if now - self.settled < 0.12 { 0.2 } else { 0.0 });
            self.put_ball(&mut cv, &g, pouch.0, pouch.1);
        }
        // trunk, front prong and band
        let trunk_w = 3 * p;
        let fx = fork.0 as i32;
        cv.rect(fx - trunk_w / 2 - p / 2, fork.1 as i32, fx + trunk_w / 2 + p / 2, floor as i32, OUT);
        cv.rect(fx - trunk_w / 2, fork.1 as i32, fx + trunk_w / 2, floor as i32, WOOD);
        cv.rect(fx + trunk_w / 2 - p, fork.1 as i32, fx + trunk_w / 2, floor as i32, WOOD_D);
        Self::thick_line(&mut cv, (fork.0, fork.1), tips[1], 2 * p, WOOD);
        Self::thick_line(&mut cv, tips[1], pouch, p, BAND);
        // a little grass tuft at its foot
        for (dx, h) in [(-3, 2), (-2, 3), (2, 3), (3, 2), (4, 1)] {
            let x = fx + dx * p;
            cv.rect(x, floor as i32 - h * p, x + p, floor as i32, 0x4caf50);
        }
        self.sling.present(&cv, self.sling_at.0, self.sling_at.1);
        shot(if self.phase == Phase::Pulling { "sling-pulled" } else { "sling" }, &cv);
        self.sling_cv = cv;
    }

    fn coin_at(cv: &mut Canvas, cx: i32, cy: i32, lp: i32, squeeze: f64) {
        // the coin's 6x6 picture, squeezed sideways as it spins
        let w = (6.0 * squeeze.abs()).round().max(1.0) as i32;
        for (row, line) in sprites::COIN.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let Some(c) = sprites::coin_color(ch) else { continue };
                let x = ((col as f64 + 0.5 - 3.0) * w as f64 / 6.0).round() as i32;
                cv.rect(cx + x * lp, cy + (row as i32 - 3) * lp, cx + x * lp + lp, cy + (row as i32 - 2) * lp, c);
            }
        }
    }

    fn draw_hoop(&mut self) {
        let now = self.now();
        let p = self.p;
        let pf = p as f64;
        let h = self.sim.hoop;
        let bx = h.board_x();
        // the score, small and quiet, just above the backboard
        let lp = self.label_px();
        let best = self.a.best.max(self.score);
        let label = if best > 0 { format!("{} in  best {best}", self.score) } else { format!("{} in", self.score) };
        let coins_w = if self.coins_paid > 0 { 6 * lp + 3 * lp + (self.coins_shown.max(1).to_string().len() as i32) * 6 * lp + 4 * lp } else { 0 };
        let label_w = label.chars().count() as i32 * 6 * lp + coins_w + 12 * lp;
        let x1 = bx + 4.0 * pf;
        let y0 = h.board_top() - 12.0 * lp as f64 - 2.0 * pf;
        let net_h = 11.0 * pf;
        let x0 = (h.x - 10.0 * pf).min(x1 - label_w as f64);
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
        // the label, ending at the backboard: score, coins, and a tiny x to stop
        let lx = (l(bx) + 3 * p - label_w).max(0);
        let col = if now - self.pop < 0.4 { 0xffd23f } else { MUTED };
        // (pixel_text gives back the width it drew)
        let mut end = lx + cv.pixel_text(lx, lp, &label, lp, col);
        let mut coin_spot = None;
        if self.coins_paid > 0 {
            end += 4 * lp;
            let (cx, cy) = (end + 3 * lp, lp + 3 * lp + lp / 2);
            coin_spot = Some((cx, cy));
            end += 6 * lp + 2 * lp;
            // the coin and its count show once the first coin gets there
            if self.coins_shown > 0 {
                let bump = self.coin_fx.iter().any(|c| (now - c.t0 - 0.9).abs() < 0.12);
                Self::coin_at(&mut cv, cx, cy - if bump { lp } else { 0 }, lp, 1.0);
                end += cv.pixel_text(end, lp, &self.coins_shown.to_string(), lp, if bump { 0xffd23f } else { MUTED });
            }
        }
        let xx = end + 6 * lp;
        cv.pixel_text(xx, lp, "x", lp, 0x8a8299);
        self.close_rect = (x0 as i32 + xx - 3 * lp, y0 as i32, x0 as i32 + xx + 9 * lp, y0 as i32 + 10 * lp);
        // coins on their way: out of the net, up and over to the label, spinning
        if let Some((tx, ty)) = coin_spot {
            for c in &self.coin_fx {
                let k = ((now - c.t0) / 0.9).clamp(0.0, 1.0);
                if k >= 1.0 {
                    continue;
                }
                let (sx, sy) = (l(h.x + h.w / 2.0) as f64, t(h.y + 6.0 * pf) as f64);
                let e = k * k * (3.0 - 2.0 * k);
                let x = sx + (tx as f64 - sx) * e;
                let y = sy + (ty as f64 - sy) * e - (4.0 * k * (1.0 - k)) * 30.0 * self.s;
                Self::coin_at(&mut cv, x as i32, y as i32, (lp + p) / 2, ((now - c.t0) * 14.0).cos());
            }
        }
        self.hoop_win.present(&cv, x0 as i32, y0 as i32);
        shot("hoop", &cv);
        if std::env::var_os("BLIP_HOOPS_SEQ").is_some() && self.coin_fx.iter().any(|c| now - c.t0 < 1.1) && self.seq < 4000 {
            self.seq += 1;
            shot(&format!("hoopseq-{:04}", self.seq), &cv);
        }
    }

    /// The pet's own window: the pet on the ground, in the air, or the ball in flight.
    fn draw_pet(&mut self, now: f64) {
        let p = self.p;
        let pf = p as f64;
        let floor = self.work.bottom;
        // (sprite, centre x, grid bottom y, extras: stars, dust)
        let (g, cx, bottom, stars, dust) = match self.phase {
            Phase::Flying => {
                let squash = if now < self.squash_until { 0.22 } else { 0.0 };
                let mut g = self.ball_sprite(self.sim.spin, squash);
                add_trail(&mut g, self.sim.vx * 0.035, self.sim.vy * 0.035, &self.geo, false);
                (g, self.sim.x, self.sim.y + 8.0 * pf, false, false)
            }
            Phase::Hopping(t0) | Phase::Leaving(t0) => {
                let up = matches!(self.phase, Phase::Hopping(_));
                let k = ((now - t0) / 0.5).clamp(0.0, 1.0);
                let (a, b) = if up { (self.hop_from, self.anchor) } else { (self.anchor, (self.stand_x(), floor - self.sim.r)) };
                let x = a.0 + (b.0 - a.0) * k;
                let y = a.1 + (b.1 - a.1) * k - 4.0 * k * (1.0 - k) * 70.0 * self.s;
                // it curls up on the way in, and uncurls on the way out
                let curled = if up { k > 0.55 } else { k < 0.35 };
                if curled {
                    (self.ball_sprite(self.roll_spin + k * 6.0, 0.0), x, y + 8.0 * pf, false, false)
                } else {
                    let dir = if b.0 < a.0 { -1 } else { 1 };
                    (self.pet_sprite("happy", dir, false), x, y + self.sim.r, false, !up && k > 0.9)
                }
            }
            Phase::Walking => match self.plan.front().copied() {
                Some(Step::Roll { .. }) => (self.ball_sprite(self.roll_spin, 0.0), self.px, floor + 0.5 * pf, false, false),
                Some(Step::Pose { mood, face, shake, stars, cheer, .. }) => {
                    let t = now - self.step_t0;
                    let face = if shake { if (t / 0.09) as i32 % 2 == 0 { -1 } else { 1 } } else { face };
                    let lift = if cheer { (t * std::f64::consts::PI / 0.42).sin().abs() * 16.0 * self.s * (t < 0.84) as i32 as f64 } else { 0.0 };
                    let squash = (cheer && lift < 2.0) || (!cheer && self.frame % 6 == 0);
                    // getting up out of the ball: squashed for a moment
                    let squash = squash || (t < 0.12 && !cheer);
                    (self.pet_sprite(mood, face, squash), self.px, floor - lift, stars, shake)
                }
                Some(Step::Walk { hop, mood, speed, .. }) => {
                    let lift = if hop > 0.0 { (self.walked / (40.0 * self.s) * std::f64::consts::PI).sin().abs() * hop * self.s } else { 0.0 };
                    let squash = if hop > 0.0 { lift < hop * self.s * 0.2 } else { self.frame % 2 == 0 };
                    (self.pet_sprite(mood, self.dir, squash), self.px, floor - lift, false, speed >= 700.0)
                }
                None => (self.pet_sprite("normal", self.dir, false), self.px, floor, false, false),
            },
            _ => {
                self.pet_win.hide();
                return;
            }
        };
        let mut cv = Canvas::new(32 * p, (GH + HEAD) * p);
        cv.sprite(&g, 0, HEAD * p, p);
        if stars {
            // three little stars circling over its head
            let top = g.keys().map(|c| c.1).min().unwrap_or(10) + HEAD;
            for i in 0..3 {
                let a = now * 5.0 + i as f64 * 2.094;
                let (x, y) = (16.0 + a.cos() * 7.0, top as f64 - 3.0 + a.sin() * 1.6);
                cv.rect((x * pf) as i32, (y * pf) as i32, (x * pf) as i32 + p, (y * pf) as i32 + p, 0xffd23f);
            }
        }
        if dust {
            // a few puffs at its feet
            let base = (GH + HEAD) * p - p;
            for i in 0..4 {
                let k = ((now * 3.0 + i as f64 * 0.25) % 1.0) as f64;
                let side = if i % 2 == 0 { -1.0 } else { 1.0 };
                let x = 16.0 + side * (5.0 + k * 6.0) - self.dir as f64 * 3.0;
                let y = base as f64 / pf - k * 2.0;
                if k < 0.8 {
                    cv.rect((x * pf) as i32, (y * pf) as i32, (x * pf) as i32 + p, (y * pf) as i32 + p, 0xd8d2c4);
                }
            }
        }
        let x = (cx - 16.0 * pf) as i32;
        let y = (bottom - (GH + HEAD) as f64 * pf) as i32;
        self.pet_win.present(&cv, x, y);
        shot("pet", &cv);
        if std::env::var_os("BLIP_HOOPS_SEQ").is_some() && self.seq < 4000 {
            self.seq += 1;
            let name = format!("seq-{:04}-{:?}", self.seq, self.phase).replace(['(', ')', '.'], "");
            shot(&name, &cv);
        }
    }

    // ------------------------------------------------ the pet on the ground
    /// What it does to get back to the slingshot: a different little routine every time.
    fn routine(&mut self, scored: bool) -> Vec<Step> {
        const N: usize = 8;
        let mut k = (self.rand() * N as f64) as usize % N;
        if k == self.last_routine {
            k = (k + 1 + (self.rand() * (N - 1) as f64) as usize) % N;
        }
        if !self.a.routines.is_empty() {
            k = self.a.routines[self.shots as usize % self.a.routines.len()] % N;
        }
        self.last_routine = k;
        let m = if scored { "happy" } else { "normal" };
        let up = pose(0.3, "normal");
        let r = match k {
            // a plain stroll back
            0 => vec![up, walk(380.0, m)],
            // a dash, kicking up dust
            1 => vec![pose(0.2, "happy"), walk(760.0, "happy")],
            // hopping along
            2 => vec![up, Step::Walk { speed: 470.0, hop: 13.0, part: 1.0, mood: m }],
            // sees stars for a moment, then walks it off
            3 => vec![Step::Pose { secs: 1.3, mood: "sick", face: 0, shake: false, stars: true, cheer: false }, walk(300.0, "normal")],
            // shakes itself off, then trots back
            4 => vec![Step::Pose { secs: 0.7, mood: "normal", face: 0, shake: true, stars: false, cheer: false }, walk(560.0, m)],
            // doesn't bother getting up: rolls back
            5 => vec![Step::Roll { speed: 900.0 }, pose(0.25, "happy")],
            // halfway there, stops to look back at the hoop
            6 => vec![
                up,
                Step::Walk { speed: 400.0, hop: 0.0, part: 0.5, mood: m },
                Step::Pose { secs: 0.8, mood: m, face: 1, shake: false, stars: false, cheer: false },
                walk(520.0, m),
            ],
            // a cheer after a basket (a sigh after a miss), then a skip back
            _ if scored => vec![
                Step::Pose { secs: 0.9, mood: "happy", face: 0, shake: false, stars: false, cheer: true },
                Step::Walk { speed: 520.0, hop: 6.0, part: 1.0, mood: "happy" },
            ],
            _ => vec![pose(0.8, "sad"), walk(300.0, "sad")],
        };
        log(&format!("walks back: routine {k}"));
        r
    }

    fn walk_tick(&mut self, now: f64, dt: f64) {
        let Some(step) = self.plan.front().copied() else {
            // there: hop into the pouch
            self.hop_from = (self.px, self.work.bottom - self.sim.r);
            self.phase = Phase::Hopping(now);
            return;
        };
        let t = now - self.step_t0;
        let stand = self.stand_x();
        let moving = |g: &mut Game, speed: f64, part: f64| -> bool {
            let goal = *g.goal.get_or_insert(g.px + (stand - g.px) * part);
            let v = speed * g.s * dt;
            let d = goal - g.px;
            if d.abs() > 1.0 {
                g.dir = if d < 0.0 { -1 } else { 1 };
            }
            g.walked += v.min(d.abs());
            if d.abs() <= v {
                g.px = goal;
                true
            } else {
                g.px += v * d.signum();
                false
            }
        };
        let done = match step {
            Step::Pose { secs, .. } => t >= secs,
            Step::Walk { speed, part, .. } => moving(self, speed, part),
            Step::Roll { speed } => {
                let before = self.px;
                let d = moving(self, speed, 1.0);
                self.roll_spin += (self.px - before) / self.sim.r;
                d
            }
        };
        // legs: quicker steps the faster it goes
        let every = match step {
            Step::Walk { speed, .. } => (40.0 / speed).clamp(0.05, 0.14),
            _ => 0.25,
        };
        if now >= self.next_frame {
            self.frame = self.frame.wrapping_add(1);
            self.blink = self.rand() < 0.08;
            self.next_frame = now + every;
        }
        if done {
            self.plan.pop_front();
            self.step_t0 = now;
            self.goal = None;
            self.walked = 0.0;
            if let Some(Step::Walk { .. }) = self.plan.front() {
                self.dir = if stand < self.px { -1 } else { 1 };
            }
        }
    }

    // ------------------------------------------------ the loop
    fn tick(&mut self) {
        let now = self.now();
        let dt = (now - self.last).clamp(0.0, 0.05);
        self.last = now;
        match self.phase {
            Phase::Walking => self.walk_tick(now, dt),
            Phase::Hopping(t0) => {
                if now - t0 >= 0.5 {
                    self.phase = Phase::Ready;
                    self.pull = self.anchor;
                    self.settled = now;
                }
            }
            Phase::Ready => {
                if self.a.shots.map_or(false, |n| self.shots >= n) {
                    self.stop(now);
                } else if let (true, Some(to), false, 2) = (self.a.auto, self.a.move_to, self.moved_once, self.shots) {
                    // test: take the game to another screen
                    self.moved_once = true;
                    let (w, sc) = platform::work_area_at(to.0, to.1);
                    self.relocate(w, sc, now);
                } else if self.a.auto && now - self.settled > 0.4 {
                    self.auto_shot(now);
                }
            }
            Phase::Pulling => {
                let (mx, my) = platform::cursor();
                let (dx, dy) = (mx - self.anchor.0, my - self.anchor.1);
                let d = dx.hypot(dy).max(1e-6);
                let k = d.min(self.pull_r) / d;
                // never below the ground
                let low = self.work.bottom - self.sim.r - 2.0 * self.s;
                self.pull = (self.anchor.0 + dx * k, (self.anchor.1 + dy * k).min(low));
                if !platform::left_button_down() {
                    self.release(now);
                }
            }
            Phase::Moving => {
                let (mx, my) = platform::cursor();
                self.sling_at = ((mx - self.grab.0) as i32, (my - self.grab.1) as i32);
                self.anchor = (self.sling_at.0 as f64 + self.anchor_off.0, self.sling_at.1 as f64 + self.anchor_off.1);
                self.pull = self.anchor;
                if !platform::left_button_down() {
                    let (w, sc) = platform::work_area_at(mx, my);
                    self.relocate(w, sc, now);
                }
            }
            Phase::Flying => {
                for e in self.sim.step(dt) {
                    match e {
                        Ev::Score => self.basket(now),
                        Ev::Rim | Ev::Board => self.shake = now,
                        Ev::Bounce(_) => self.squash_until = now + 0.08,
                        Ev::Stop => {
                            // it gets up where it stopped and makes its way back
                            self.px = self.sim.x;
                            self.roll_spin = self.sim.spin;
                            let scored = self.sim.scored;
                            self.plan = VecDeque::from(self.routine(scored));
                            self.step_t0 = now;
                            self.goal = None;
                            self.walked = 0.0;
                            self.dir = if self.stand_x() < self.px { -1 } else { 1 };
                            self.phase = Phase::Walking;
                            if scored {
                                self.place_hoop();
                            }
                            self.draw_hoop();
                        }
                    }
                }
            }
            Phase::Leaving(t0) => {
                if now - t0 >= 0.5 {
                    self.px = self.stand_x();
                    self.done = true;
                }
            }
        }
        // coins reach the label
        let arrived = self.coin_fx.iter().filter(|c| now - c.t0 >= 0.9).count() as u32;
        self.coins_shown = self.coins_shown.max(arrived);
        let busy_coins = self.coin_fx.iter().any(|c| now - c.t0 < 1.1);
        self.draw_sling(now);
        self.draw_pet(now);
        if now - self.swish < 0.8 || now - self.shake < 0.25 || now - self.pop < 0.45 || busy_coins {
            self.draw_hoop();
        }
    }

    fn basket(&mut self, now: f64) {
        self.score += 1;
        self.swish = now;
        self.pop = now;
        if self.coins_paid < self.a.coins_left {
            self.coins_paid += 1;
            self.coin_fx.push(CoinFx { t0: now + 0.15 });
            say("{\"coin\":1}");
        }
        log(&format!("score! {} (coins {})", self.score, self.coins_paid));
    }

    /// The slingshot was let go of at a new spot: set the game up on the screen it's over
    /// (or back where it was).
    fn relocate(&mut self, work: Rect, scale: f64, now: f64) {
        let moved = (work.left, work.top, work.right, work.bottom) != (self.work.left, self.work.top, self.work.right, self.work.bottom);
        self.layout(work, scale);
        if moved {
            self.place_hoop();
            log(&format!("moved to the screen at {:.0},{:.0}", work.left, work.top));
        }
        self.draw_hoop();
        self.phase = Phase::Ready;
        self.settled = now;
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
        self.sim.spin = 0.0;
        self.shots += 1;
        self.released = now;
        self.phase = Phase::Flying;
        log(&format!("shot {} v=({vx:.0},{vy:.0})", self.shots));
    }

    /// Stop the game: out of the pouch first if it's sitting in it.
    fn stop(&mut self, now: f64) {
        match self.phase {
            Phase::Ready | Phase::Pulling | Phase::Moving => {
                self.pull = self.anchor;
                self.released = now;
                self.phase = Phase::Leaving(now);
            }
            Phase::Leaving(_) => {}
            Phase::Flying => {
                self.px = self.sim.x;
                self.done = true;
            }
            _ => self.done = true,
        }
    }

    /// Test player: aim at the hoop (with a little wobble so it doesn't always score).
    fn auto_shot(&mut self, now: f64) {
        let vmax = self.pull_r * self.vk;
        let from = self.aim_from();
        // pull back smoothly for a moment first, like a person would
        let t0 = *self.aiming.get_or_insert(now);
        let k = ((now - t0) / 0.6).min(1.0);
        self.pull = (self.anchor.0 + (from.0 - self.anchor.0) * k, self.anchor.1 + (from.1 - self.anchor.1) * k);
        self.phase = Phase::Ready;
        if k < 1.0 {
            return;
        }
        self.aiming = None;
        self.pull = from;
        match self.sim.solve(from.0, from.1, vmax) {
            Some((vx, vy)) => {
                let w = 1.0 + (self.rand() - 0.5) * 0.03;
                self.fire(now, vx * w, vy * w);
            }
            None => self.fire(now, 900.0, -900.0),
        }
    }

    fn press(&mut self, id: WindowId) {
        let (mx, my) = platform::cursor();
        if id == self.hoop_win.id() {
            let (x0, y0, x1, y1) = self.close_rect;
            if (mx as i32) >= x0 && (mx as i32) <= x1 && (my as i32) >= y0 && (my as i32) <= y1 {
                let now = self.now();
                self.stop(now);
            }
            return;
        }
        if id == self.sling.id() && self.phase == Phase::Ready {
            let r = self.sim.r * 1.4;
            if (mx - self.anchor.0).hypot(my - self.anchor.1) <= r {
                self.phase = Phase::Pulling;
            } else {
                // grabbed the wood: carry the whole slingshot (pet and all) somewhere else
                self.grab = (mx - self.sling_at.0 as f64, my - self.sling_at.1 as f64);
                self.phase = Phase::Moving;
            }
        }
    }

    fn overlay_size(&self, id: WindowId) -> Option<winit::dpi::PhysicalSize<u32>> {
        [&self.sling, &self.hoop_win, &self.pet_win]
            .into_iter()
            .find(|o| o.id() == id)
            .map(|o| winit::dpi::PhysicalSize::new(o.w.max(1) as u32, o.h.max(1) as u32))
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

    fn window_event(&mut self, _el: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        let Some(g) = self.game.as_mut() else { return };
        if let WindowEvent::ScaleFactorChanged { inner_size_writer, .. } = &mut event {
            // the game's windows size themselves for each screen
            if let Some(size) = g.overlay_size(id) {
                let _ = inner_size_writer.request_inner_size(size);
            }
        }
        if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } = event {
            g.press(id);
        }
        if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } = event {
            let now = g.now();
            g.stop(now); // right-click anywhere on the game: stop
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(g) = self.game.as_mut() else { return };
        g.tick();
        if g.done {
            if !std::mem::replace(&mut self.reported, true) {
                say(&format!("{{\"score\":{},\"shots\":{},\"x\":{:.0},\"y\":{:.0}}}", g.score, g.shots, g.px, g.work.bottom - 2.0));
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
