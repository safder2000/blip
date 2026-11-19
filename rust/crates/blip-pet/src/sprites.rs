//! The pets, ported line by line from `blip/sprites.py` (tests check they draw the same pixels).
//! A sprite is a map of grid cells (x, y) to colours on a GW x GH grid, feet on the bottom row.
//! Cells can be outside the grid (ears and antennas poke out above the top).

use std::collections::{HashMap, HashSet};

pub const GW: i32 = 32;
pub const GH: i32 = 28;

pub type Color = u32; // 0xRRGGBB
pub type Cell = (i32, i32);
pub type Sprite = HashMap<Cell, Color>;
type Shape = HashSet<Cell>;

pub mod col {
    use super::Color;
    pub const OUT: Color = 0x2b2340;
    pub const WHITE: Color = 0xffffff;
    pub const CHEEK: Color = 0xff8fab;
    pub const TEAR: Color = 0x5ec8ff;
    pub const RED: Color = 0xe5484d;
    pub const HEART: Color = 0xff4d6d;
    pub const Z: Color = 0x6b7fd7;
    pub const SPARK: Color = 0xffd23f;
    pub const EGG: Color = 0xfff3d6;
    pub const EGG2: Color = 0xefd9a8;
    pub const SPOT: Color = 0xf4a6c0;
    pub const POOP: Color = 0x8b5a2b;
    pub const ALERT: Color = 0xff4d6d;
}

/// (body, shade, antenna tip)
pub fn palette(name: &str) -> (Color, Color, Color) {
    match name {
        "baby" => (0x9be8c8, 0x6cc9a5, 0xff8fab),
        "child" => (0x8fd3f0, 0x5fb0d6, 0xff8fab),
        "sky" => (0x8fb4ff, 0x6688e0, 0xffd166),
        "plum" => (0xc3a6e8, 0x9a7cc8, 0x7ee0c3),
        "sick" => (0xc4dc9a, 0x9cba70, 0x9cba70),
        "ghost" => (0xeef1ff, 0xc9d0f0, 0xc9d0f0),
        "mint" => (0x9be8c8, 0x6cc9a5, 0xff8fab),
        "peach" => (0xffc29e, 0xf09a6c, 0xff6f91),
        "pink" => (0xffb8d6, 0xec8bb4, 0x7ee0c3),
        "snow" => (0xf6f6fb, 0xd6d6e6, 0xff8fab),
        "cocoa" => (0xd6ae86, 0xab805a, 0xffd166),
        "lime" => (0xcdea7a, 0xa3c654, 0xff8fab),
        _ => (0xffd166, 0xe9a93c, 0xff5d73), // gold
    }
}

pub const PALETTES: [&str; 13] =
    ["baby", "child", "gold", "sky", "plum", "sick", "ghost", "mint", "peach", "pink", "snow", "cocoa", "lime"];
pub const SPECIES: [&str; 7] = ["blip", "kitty", "bunny", "bear", "pup", "chick", "bot"];
pub const MOODS: [&str; 8] = ["normal", "happy", "sad", "eat", "sleep", "sick", "held", "alert"];

pub const HEART: [Cell; 11] = [(1, 0), (3, 0), (0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (1, 2), (2, 2), (3, 2), (2, 3)];
pub const ZED: [Cell; 7] = [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)];
pub const SPARK: [Cell; 5] = [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)];
pub const BANG: [Cell; 4] = [(0, 0), (0, 1), (0, 2), (0, 4)];

// ---------------------------------------------------------------- shape helpers
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Shape {
    let mut s = Shape::new();
    for y in 0..GH {
        for x in 0..GW {
            let (dx, dy) = ((x as f64 + 0.5 - cx) / rx, (y as f64 + 0.5 - cy) / ry);
            if dx * dx + dy * dy <= 1.0 {
                s.insert((x, y));
            }
        }
    }
    s
}

fn rrect(cx: f64, cy: f64, rx: f64, ry: f64, corner: f64) -> Shape {
    let mut s = Shape::new();
    for y in 0..GH {
        for x in 0..GW {
            let (dx, dy) = ((x as f64 + 0.5 - cx).abs(), (y as f64 + 0.5 - cy).abs());
            if dx <= rx && dy <= ry {
                let (ex, ey) = (dx - (rx - corner), dy - (ry - corner));
                if ex > 0.0 && ey > 0.0 && ex * ex + ey * ey > corner * corner {
                    continue;
                }
                s.insert((x, y));
            }
        }
    }
    s
}

/// Python's int(): truncate toward zero.
fn int(v: f64) -> i32 {
    v as i32
}

fn tri(apex_x: f64, apex_y: i32, base_cx: f64, base_y: i32, half: f64) -> Shape {
    let mut s = Shape::new();
    let h = (base_y - apex_y).max(1);
    for i in 0..=h {
        let t = i as f64 / h as f64;
        let mx = apex_x + (base_cx - apex_x) * t;
        let hw = half * t;
        let y = apex_y + i;
        for x in int(mx - hw - 2.0)..int(mx + hw + 2.0) {
            if (x as f64 + 0.5 - mx).abs() <= hw + 0.5 {
                s.insert((x, y));
            }
        }
    }
    s
}

fn outline(g: &mut Sprite, shape: &Shape) {
    for &(x, y) in shape {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let p = (x + dx, y + dy);
            if !shape.contains(&p) && (0..GW).contains(&p.0) && (0..GH).contains(&p.1) {
                g.entry(p).or_insert(col::OUT);
            }
        }
    }
}

pub fn darker(c: Color, f: f64) -> Color {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    ((r as f64 * f) as u32) << 16 | ((g as f64 * f) as u32) << 8 | (b as f64 * f) as u32
}

// ---------------------------------------------------------------- egg
pub fn build_egg(frame: u32, progress: f64) -> Sprite {
    let wob = if progress > 0.8 {
        [1.0, -1.0][(frame % 2) as usize]
    } else if progress > 0.3 {
        [0.0, 1.0, 0.0, -1.0][(frame % 4) as usize]
    } else {
        0.0
    };
    let (cx, rx, ry) = (GW as f64 / 2.0 + wob, 6.0, 7.5);
    let cy = (GH - 1) as f64 - ry;
    let shape = ellipse(cx, cy, rx, ry);
    let mut g: Sprite =
        shape.iter().map(|&p| (p, if p.1 as f64 + 0.5 > cy + ry * 0.5 { col::EGG2 } else { col::EGG })).collect();
    let (ci, cj) = (int(cx), int(cy));
    for (dx, dy) in [(-3, -3), (-2, -3), (2, 1), (3, 1), (2, 2), (-2, 3), (-1, 3)] {
        if shape.contains(&(ci + dx, cj + dy)) {
            g.insert((ci + dx, cj + dy), col::SPOT);
        }
    }
    let hl = (int(cx - rx * 0.5), int(cy - ry * 0.5));
    if shape.contains(&hl) {
        g.insert(hl, col::WHITE);
    }
    if progress > 0.6 {
        for (i, dx) in (-4..4).enumerate() {
            let p = (ci + dx, cj - 1 + (i as i32 % 2));
            if shape.contains(&p) {
                g.insert(p, col::OUT);
            }
        }
    }
    outline(&mut g, &shape);
    g
}

// ---------------------------------------------------------------- pets
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Baby,
    Child,
    Adult,
}

impl Stage {
    pub fn from_name(s: &str) -> Stage {
        match s {
            "baby" => Stage::Baby,
            "child" => Stage::Child,
            _ => Stage::Adult,
        }
    }
}

pub struct PetLook<'a> {
    pub stage: Stage,
    pub palette: &'a str,
    pub mood: &'a str,
    pub frame: u32,
    pub facing: i32,
    pub squash: bool,
    pub blink: bool,
    pub species: &'a str,
}

pub fn build_pet(look: &PetLook) -> Sprite {
    let PetLook { stage, palette: pal, mood, frame, facing, squash, blink, species } = *look;
    let species = if SPECIES.contains(&species) { species } else { "blip" };
    let (mut rx, mut ry) = match stage {
        Stage::Baby => (5.0, 4.5),
        Stage::Child => (7.0, 6.0),
        Stage::Adult => (9.0, 7.5),
    };
    let k: i32 = match stage {
        Stage::Baby => 0,
        Stage::Child => 1,
        Stage::Adult => 2,
    };
    let kf = k as f64;
    if squash {
        rx += 0.7;
        ry -= 0.8;
    }
    let (base, shade, tip) = palette(pal);
    let o = col::OUT;
    let cx = GW as f64 / 2.0;
    let cy = (GH - 1) as f64 - ry;
    let mut shape = if species == "bot" { rrect(cx, cy, rx, ry, 2.0 + kf) } else { ellipse(cx, cy, rx, ry) };
    if pal == "ghost" {
        let bottom = shape.iter().map(|p| p.1).max().unwrap_or(0);
        shape.retain(|&(x, y)| !(y == bottom && (x + frame as i32).rem_euclid(3) == 0));
    }
    let top = shape.iter().map(|p| p.1).min().unwrap_or(0);
    let mut g: Sprite =
        shape.iter().map(|&p| (p, if p.1 as f64 + 0.5 > cy + ry * 0.45 { shade } else { base })).collect();

    // ears and other bits that share the outline
    let mut extra: Vec<(Cell, Color)> = Vec::new();
    let mut inner: Vec<(Cell, Color)> = Vec::new();
    match species {
        "kitty" => {
            let (h, half) = (2 + k, 1.0 + 0.6 * kf);
            for s in [-1.0, 1.0] {
                let ecx = cx + s * rx * 0.55;
                for p in tri(ecx + s * 1.0, top - h, ecx, top + 1, half) {
                    extra.push((p, base));
                }
                if k > 0 {
                    for p in tri(ecx + s * 1.0, top - h + 1, ecx, top + 1, half - 1.0) {
                        if top - h < p.1 && p.1 < top + 1 {
                            inner.push((p, col::CHEEK));
                        }
                    }
                }
            }
            if k > 0 {
                let side = if facing <= 0 { 1 } else { -1 };
                let tx = int(cx + side as f64 * rx) - if side > 0 { 0 } else { 1 };
                for i in 0..(2 + k) {
                    extra.push(((tx + side * (i / 2), GH - 3 - i), shade));
                }
            }
        }
        "bunny" => {
            let (length, erx) = ((4 + 2 * k) as f64, 1.2 + 0.4 * kf);
            for s in [-1.0, 1.0] {
                let droop = if s > 0.0 && frame % 8 < 4 { 0.5 } else { 0.0 };
                let ecx = cx + s * ((rx * 0.35).max(1.6) + droop);
                for p in ellipse(ecx, top as f64 - length / 2.0 + 1.0, erx, length / 2.0) {
                    extra.push((p, base));
                    if (p.0 as f64 + 0.5 - ecx).abs() < erx * 0.5 && p.1 < top && k > 0 {
                        inner.push((p, col::CHEEK));
                    }
                }
            }
        }
        "bear" => {
            let r = 1.6 + 0.5 * kf;
            for s in [-1.0, 1.0] {
                let ecx = cx + s * rx * 0.72;
                for p in ellipse(ecx, top as f64 + 1.0, r, r) {
                    extra.push((p, base));
                    let (a, b) = (p.0 as f64 + 0.5 - ecx, p.1 as f64 + 0.5 - top as f64 - 1.0);
                    if a * a + b * b < (r * 0.55) * (r * 0.55) {
                        inner.push((p, shade));
                    }
                }
            }
        }
        "pup" => {
            let (erx, ery) = (1.1 + 0.45 * kf, 2.4 + 1.1 * kf);
            let ear = darker(shade, 0.85);
            let swing = if matches!(frame % 4, 1 | 2) { 0.4 } else { 0.0 };
            for s in [-1.0, 1.0] {
                let ecx = cx + s * (rx * 0.78 + swing);
                for p in ellipse(ecx, top as f64 + ery * 0.9, erx, ery) {
                    extra.push((p, ear));
                }
            }
        }
        "chick" => {
            let (wrx, wry) = (1.0 + 0.35 * kf, 1.7 + 0.5 * kf);
            let flap = if frame % 4 == 0 { -1.0 } else { 0.0 };
            for s in [-1.0, 1.0] {
                for p in ellipse(cx + s * (rx - 0.6), cy + ry * 0.2 + flap, wrx, wry) {
                    extra.push((p, shade));
                }
            }
        }
        _ => {}
    }
    // a dict in Python: the last value for a cell wins
    let extra: HashMap<Cell, Color> = extra.into_iter().collect();
    let inner: HashMap<Cell, Color> = inner.into_iter().collect();
    let mut full = shape.clone();
    full.extend(extra.keys().copied());
    for (&p, &c) in &extra {
        if !shape.contains(&p) || species == "pup" || species == "chick" {
            g.insert(p, c);
        }
    }
    let (hx, hy) = if species == "bot" { (int(cx - rx + 1.0), int(cy - ry + 1.0)) } else { (int(cx - rx * 0.55), int(cy - ry * 0.55)) };
    for p in [(hx, hy), (hx + 1, hy), (hx, hy + 1)] {
        if shape.contains(&p) && !extra.contains_key(&p) {
            g.insert(p, col::WHITE);
        }
    }
    for (&p, &c) in &inner {
        if full.contains(&p) {
            g.insert(p, c);
        }
    }
    outline(&mut g, &full);

    // antenna / tuft
    let ax = if facing > 0 { 16 } else { 15 };
    let sway = [0, 1, 0, -1][(frame % 4) as usize];
    if species == "blip" || species == "bot" {
        if stage == Stage::Baby {
            g.insert((ax, top - 2), o);
            g.insert((ax + 1, top - 3), o);
        } else {
            let n = if stage == Stage::Child { 2 } else { 3 };
            for i in 0..n {
                g.insert((ax + if i == n - 1 { sway } else { 0 }, top - 2 - i), o);
            }
            let (tx, ty) = (ax + sway, top - 2 - n);
            let tips: Vec<Cell> =
                if stage == Stage::Child { vec![(tx, ty)] } else { vec![(tx, ty), (tx + 1, ty), (tx, ty - 1), (tx + 1, ty - 1)] };
            for &p in &tips {
                g.insert(p, if mood == "alert" && frame % 2 == 1 { col::ALERT } else { tip });
            }
            for &(x, y) in &tips {
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let q = (x + dx, y + dy);
                    if !tips.contains(&q) && (0..GW).contains(&q.0) && q.1 >= 0 {
                        g.entry(q).or_insert(o);
                    }
                }
            }
        }
    } else if species == "chick" {
        for p in [(ax, top - 2), (ax - 1, top - 3), (ax + 1, top - 3), (ax + 1 + sway.rem_euclid(2), top - 4)] {
            g.insert(p, o);
        }
    }

    // face
    let mut fc = o;
    if species == "bot" {
        fc = 0x7df9ff;
        for x in int(cx - rx + 2.0)..int(cx + rx - 1.0) {
            for y in int(cy - ry + 2.0 + if k > 0 { 1.0 } else { 0.0 })..=int(cy + ry * 0.35) {
                if shape.contains(&(x, y)) {
                    g.insert((x, y), 0x263049);
                }
            }
        }
        for s in [-1.0, 1.0] {
            let bolt = (int(cx + s * rx) - if s > 0.0 { 1 } else { 0 }, int(cy + ry * 0.55));
            if shape.contains(&bolt) {
                g.insert(bolt, 0xc9c9d6);
            }
        }
    }
    let put = |g: &mut Sprite, cells: &[Cell], c: Color| {
        for p in cells {
            if shape.contains(p) {
                g.insert(*p, c);
            }
        }
    };

    let f = facing.signum();
    let d = 3.max(int(rx * 0.45));
    let ey = int(cy - ry * 0.15);
    let eh = if stage == Stage::Baby { 1 } else { 2 };
    let (l, r) = (16 - d + f, 15 + d + f);
    let big = stage == Stage::Adult && species != "bot";
    let mut my = ey + eh + if stage == Stage::Baby { 1 } else { 2 };
    let m = 15 + f;

    if species == "bear" {
        for p in ellipse((16 + f) as f64, my as f64 + 0.8, 2.2 + 0.7 * kf, 1.4 + 0.4 * kf) {
            if shape.contains(&p) {
                g.insert(p, 0xfff1df);
            }
        }
    }
    for (ex, side) in [(l, -1), (r, 1)] {
        if mood == "happy" {
            put(&mut g, &[(ex - 1, ey + 1), (ex, ey), (ex + 1, ey + 1)], fc);
        } else if mood == "sleep" || (blink && (mood == "normal" || mood == "sad")) {
            put(&mut g, &[(ex - 1, ey + eh - 1), (ex, ey + eh - 1), (ex + 1, ey + eh - 1)], fc);
        } else if mood == "sick" {
            put(&mut g, &[(ex - 1, ey - 1), (ex + 1, ey - 1), (ex, ey), (ex - 1, ey + 1), (ex + 1, ey + 1)], fc);
        } else if mood == "held" {
            let kk = -side;
            put(&mut g, &[(ex + kk, ey - 1), (ex, ey), (ex + kk, ey + 1)], fc);
        } else if big || mood == "alert" {
            let x0 = if side < 0 { ex } else { ex - 1 };
            put(&mut g, &[(x0, ey), (x0 + 1, ey), (x0, ey + 1), (x0 + 1, ey + 1)], fc);
            if species != "bot" {
                put(&mut g, &[(if side < 0 { x0 } else { x0 + 1 }, ey)], col::WHITE);
            }
        } else {
            let cells: Vec<Cell> = (0..eh).map(|i| (ex, ey + i)).collect();
            put(&mut g, &cells, fc);
        }
    }
    if mood == "sad" {
        put(&mut g, &[(l, ey + eh + (frame % 3) as i32)], col::TEAR);
    }
    if matches!(mood, "normal" | "happy" | "eat" | "alert") && species != "bot" {
        let cyk = ey + eh;
        let off = if big { 2 } else { 1 };
        put(&mut g, &[(l - off, cyk), (r + off, cyk)], col::CHEEK);
    }
    if species == "chick" {
        let (orange, dark) = (0xffa53a, 0xe07b1a);
        put(&mut g, &[(m, my), (m + 1, my)], orange);
        if matches!(mood, "happy" | "eat" | "held" | "alert") && frame % 2 == 0 {
            put(&mut g, &[(m, my + 1), (m + 1, my + 1)], col::RED);
            put(&mut g, &[(m, my + 2), (m + 1, my + 2)], dark);
        } else {
            put(&mut g, &[(m, my + 1), (m + 1, my + 1)], dark);
        }
        return g;
    }
    if (species == "pup" || species == "bear") && stage != Stage::Baby {
        put(&mut g, &[(m, my - 1), (m + 1, my - 1)], o);
        my += 1;
    }
    match mood {
        "happy" => {
            put(&mut g, &[(m - 1, my), (m + 2, my), (m, my + 1), (m + 1, my + 1)], fc);
            put(&mut g, &[(m, my), (m + 1, my)], if species != "bot" { col::RED } else { fc });
            if species == "pup" {
                put(&mut g, &[(m + 1, my + 2)], 0xff7a93);
            }
        }
        "sad" => put(&mut g, &[(m - 1, my + 1), (m, my), (m + 1, my), (m + 2, my + 1)], fc),
        "eat" => {
            if frame % 2 == 0 {
                put(&mut g, &[(m, my), (m + 1, my), (m, my + 1), (m + 1, my + 1)], fc);
            } else {
                put(&mut g, &[(m, my), (m + 1, my)], fc);
            }
        }
        "sleep" => put(&mut g, &[(m, my)], fc),
        "sick" => put(&mut g, &[(m - 1, my + 1), (m, my), (m + 1, my + 1), (m + 2, my)], fc),
        "held" | "alert" => {
            put(&mut g, &[(m, my), (m + 1, my), (m, my + 1), (m + 1, my + 1)], fc);
            if mood == "alert" && species != "bot" {
                put(&mut g, &[(m, my + 1), (m + 1, my + 1)], col::RED);
            }
        }
        _ if species == "kitty" => put(&mut g, &[(m - 1, my), (m, my + 1), (m + 1, my), (m + 2, my + 1)], fc),
        _ => put(&mut g, &[(m - 1, my), (m, my + 1), (m + 1, my + 1), (m + 2, my)], fc),
    }
    g
}

// ---------------------------------------------------------------- the curled-up ball for throws
pub fn build_ball(colors: (Color, Color, Color), angle: f64, face: bool, squash: f64) -> Sprite {
    let (body, shade, accent) = colors;
    let out = col::OUT;
    let (cx, r) = (16.0, 7.5);
    let ry = r * (1.0 - squash);
    let cy2 = (GH - 1) as f64 - ry;
    let mut inside = Shape::new();
    for y in (int(cy2 - ry) - 1)..GH {
        for x in (int(cx - r) - 1)..(int(cx + r) + 2) {
            let (dx, dy) = ((x as f64 + 0.5 - cx) / r, (y as f64 + 0.5 - cy2) / ry);
            if dx * dx + dy * dy <= 1.0 {
                inside.insert((x, y));
            }
        }
    }
    let mut g = Sprite::new();
    for &(x, y) in &inside {
        let edge = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)].iter().any(|n| !inside.contains(n));
        if edge {
            g.insert((x, y), out);
        } else {
            let (dx, dy) = ((x as f64 + 0.5 - cx) / r, (y as f64 + 0.5 - cy2) / ry);
            g.insert((x, y), if dx + dy > 0.75 { shade } else { body });
        }
    }
    let (hx, hy) = (int(cx - 3.5), int(cy2 - 3.5 * (1.0 - squash)));
    for p in [(hx, hy), (hx + 1, hy), (hx, hy + 1)] {
        if g.get(&p) == Some(&body) {
            g.insert(p, 0xffffff);
        }
    }
    let fill = |g: &mut Sprite, p: Cell, c: Color| {
        if matches!(g.get(&p), Some(v) if *v != out) {
            g.insert(p, c);
        }
    };
    for k in 0..5 {
        let a = angle + std::f64::consts::PI + 0.35 * k as f64;
        let rr = 2.0 + 0.8 * k as f64;
        let p = (int(cx + a.cos() * rr), int(cy2 + a.sin() * rr * (1.0 - squash)));
        fill(&mut g, p, shade);
    }
    if face {
        for side in [-0.45, 0.45] {
            let a = angle + side;
            let (ex, ey) = (int(cx + a.cos() * 4.2), int(cy2 + a.sin() * 4.2 * (1.0 - squash)));
            for p in [(ex, ey), (ex, ey + 1)] {
                fill(&mut g, p, out);
            }
        }
        let p = (int(cx + angle.cos() * 5.2), int(cy2 + angle.sin() * 5.2 * (1.0 - squash)));
        fill(&mut g, p, accent);
    }
    g
}


// ---------------------------------------------------------------- props (from blip/sprites.py)
pub const NOTE: [Cell; 6] = [(1, 0), (2, 0), (1, 1), (1, 2), (0, 3), (1, 3)];
pub const BOWL: Color = 0xc0703a;
pub const BERRY: Color = 0xc2185b;
pub const LEAF: Color = 0x4caf50;
pub const COOKIE: Color = 0xe0a458;
pub const CHIP: Color = 0x5a3a1a;
pub const PILL: Color = 0xff6b6b;

/// Food cells (x, y, colour, is_bowl) for meal / snack / pill.
pub fn food(kind: &str) -> Vec<(i32, i32, Color, bool)> {
    match kind {
        "meal" => vec![(2, 0, LEAF, false), (1, 0, BERRY, false), (3, 0, BERRY, false), (0, 1, BERRY, false), (1, 1, BERRY, false), (2, 1, BERRY, false), (3, 1, BERRY, false), (4, 1, BERRY, false), (0, 2, BOWL, true), (1, 2, BOWL, true), (2, 2, BOWL, true), (3, 2, BOWL, true), (4, 2, BOWL, true), (1, 3, BOWL, true), (2, 3, BOWL, true), (3, 3, BOWL, true)],
        "snack" => vec![(1, 0, COOKIE, false), (2, 0, COOKIE, false), (0, 1, COOKIE, false), (1, 1, CHIP, false), (2, 1, COOKIE, false), (3, 1, COOKIE, false), (0, 2, COOKIE, false), (1, 2, COOKIE, false), (2, 2, CHIP, false), (3, 2, COOKIE, false), (1, 3, COOKIE, false), (2, 3, COOKIE, false)],
        "pill" => vec![(0, 2, PILL, false), (1, 2, PILL, false), (2, 2, col::WHITE, false), (3, 2, col::WHITE, false), (0, 3, PILL, false), (1, 3, PILL, false), (2, 3, col::WHITE, false), (3, 3, col::WHITE, false)],
        _ => vec![],
    }
}

/// The poop: rows of '.', 'o' (outline) and 'b' (brown).
pub const POOP: [&str; 6] = ["...oo...", "..obbo..", "..obbbo.", ".obbbbo.", "obbbbbbo", ".oooooo."];

/// A stable fingerprint of a sprite, to compare with the Python version.
pub fn fingerprint(g: &Sprite) -> String {
    use std::fmt::Write;
    let mut cells: Vec<(&Cell, &Color)> = g.iter().collect();
    cells.sort();
    let mut s = String::new();
    for ((x, y), c) in cells {
        let _ = write!(s, "{x},{y},{c:06x};");
    }
    s
}

// ---------------------------------------------------------------- species balls
/// The pet curled into a ball. The outline stays perfectly round (it should bounce like a
/// basketball); each species shows who it is only in the markings on the ball, which spin with it.
pub fn build_species_ball(species: &str, colors: (Color, Color, Color), angle: f64, face: bool, squash: f64) -> Sprite {
    let (_body, shade, accent) = colors;
    let out = col::OUT;
    let mut g = if species == "bot" { build_ball((0xb8c4d0, 0x8a96a6, 0xff5a5a), angle, false, squash) } else { build_ball(colors, angle, face, squash) };
    let (cx, r) = (16.0, 7.5);
    let ry = r * (1.0 - squash);
    let cy = (GH - 1) as f64 - ry;
    let at = |a: f64, d: f64| -> Cell { ((cx + a.cos() * d).floor() as i32, (cy + a.sin() * d * (1.0 - squash)).floor() as i32) };
    let up = angle - std::f64::consts::FRAC_PI_2; // "top of the head" while it spins
    // only ever paint inside the ball, never past its outline
    let fill = |g: &mut Sprite, p: Cell, c: Color| {
        if matches!(g.get(&p), Some(v) if *v != out) {
            g.insert(p, c);
        }
    };
    match species {
        "kitty" => {
            // little ear patches near the rim and tabby stripes on the back
            for side in [-0.55, 0.55] {
                fill(&mut g, at(up + side, 5.8), shade);
                fill(&mut g, at(up + side, 4.9), accent);
            }
            for k in [-0.35, 0.0, 0.35] {
                let a = angle + std::f64::consts::PI + k;
                fill(&mut g, at(a, 3.2), shade);
                fill(&mut g, at(a, 4.2), shade);
            }
        }
        "bunny" => {
            // long ears folded flat along the ball
            for side in [-1.0, 1.0] {
                for k in 0..4 {
                    let a = up + side * (0.25 + 0.3 * k as f64);
                    fill(&mut g, at(a, 5.6), accent);
                }
            }
        }
        "bear" => {
            // round ear spots and a muzzle patch
            for side in [-0.8, 0.8] {
                let p = at(up + side, 5.4);
                for q in [p, (p.0 + 1, p.1), (p.0, p.1 + 1), (p.0 + 1, p.1 + 1)] {
                    fill(&mut g, q, shade);
                }
            }
            if face {
                fill(&mut g, at(angle, 2.6), accent);
            }
        }
        "pup" => {
            // floppy ear patches on both sides
            for side in [-1.4, 1.4] {
                for d in [4.6, 5.6] {
                    fill(&mut g, at(up + side, d), shade);
                    fill(&mut g, at(up + side * 0.85, d), shade);
                }
            }
        }
        "chick" => {
            // a tuft painted at the top and a wing curve
            for (da, d) in [(0.0, 5.8), (-0.25, 5.4), (0.25, 5.4)] {
                fill(&mut g, at(up + da, d), accent);
            }
            for k in 0..3 {
                fill(&mut g, at(angle + std::f64::consts::PI + 0.3 * k as f64 - 0.3, 4.4), shade);
            }
        }
        "bot" => {
            // a visor band with glowing eyes, a ring of rivets, a little light where the antenna folds in
            for k in -2..=2 {
                let a = angle + k as f64 * 0.28;
                fill(&mut g, at(a, 4.6), 0x24304a);
                fill(&mut g, at(a, 3.6), 0x24304a);
            }
            if face {
                fill(&mut g, at(angle - 0.25, 4.2), 0x5ef0ff);
                fill(&mut g, at(angle + 0.25, 4.2), 0x5ef0ff);
            }
            for k in 0..6 {
                fill(&mut g, at(angle + std::f64::consts::PI + k as f64 * 0.5 - 1.25, 6.0), 0x6b7787);
            }
            fill(&mut g, at(up, 5.8), 0xffd23f);
        }
        _ => {
            // blip: a sprout leaf painted near the top
            fill(&mut g, at(up, 5.8), 0x4caf50);
            fill(&mut g, at(up + 0.3, 5.2), 0x7ad97f);
        }
    }
    g
}

/// Parachute looks: (stripe A, stripe B, how often out of 100). The first is the usual one.
pub const CHUTES: [(Color, Color, u32); 6] = [
    (0xef5b6b, 0xfff4e6, 62), // classic red and cream
    (0x4ba3ff, 0xffffff, 16), // sky
    (0x7ad9b0, 0xff8fab, 9),  // mint and pink
    (0xffd23f, 0x2b2340, 6),  // gold and night
    (0x8a5cf6, 0x24164a, 4),  // galaxy (with stars)
    (0xff4d6d, 0x4ba3ff, 3),  // rainbow
];

/// Pick a parachute look by rarity (`roll` in 0..100).
pub fn chute_style(roll: u32) -> u8 {
    let mut acc = 0;
    for (i, c) in CHUTES.iter().enumerate() {
        acc += c.2;
        if roll < acc {
            return i as u8;
        }
    }
    0
}
