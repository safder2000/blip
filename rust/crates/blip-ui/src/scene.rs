//! Everything drawn in the pet's own window besides the pet: the speech bubble, the signboard,
//! the timer sign, floating hearts / sparkles / zzz, and food (ported from `draw_*` in `blip/app.py`).
//! Coordinates are window pixels; the pet's grid starts at (ox, bub_h) with `p` pixels per cell.

use blip_pet::pixfont;
use blip_pet::sprites::{self, col, Cell, Color, Sprite, GH, GW};

use crate::text::Fonts;
use crate::Canvas;

#[derive(Clone, Copy)]
pub struct Frame {
    pub w: i32,
    pub ox: i32,
    pub bub_h: i32,
    pub p: i32,
    pub s: f64,
    /// top row of the pet's sprite (cells; can be negative)
    pub top_row: i32,
}

fn sc(f: &Frame, v: f64) -> i32 {
    (v * f.s).round() as i32
}

// ------------------------------------------------ particles
#[derive(Clone, Debug)]
pub struct Particle {
    pub kind: &'static str,
    pub x: i32,
    pub y: i32,
    pub life: i32,
}

/// New floating bits above the pet (`spawn` in app.py). `r` gives random numbers in [0, 1).
pub fn spawn(parts: &mut Vec<Particle>, kind: &'static str, n: u32, top_row: i32, r: &mut dyn FnMut() -> f64) {
    let ri = |r: &mut dyn FnMut() -> f64, a: i32, b: i32| a + ((r() * (b - a + 1) as f64) as i32).min(b - a);
    for _ in 0..n {
        let p = match kind {
            "heart" => Particle { kind, x: 16 + ri(r, -7, 3), y: top_row - 3, life: 7 },
            "spark" => Particle { kind, x: ri(r, 4, 26), y: ri(r, top_row - 2, GH - 4), life: 3 },
            "z" => Particle { kind, x: 19, y: top_row - 2, life: 6 },
            "bang" => Particle { kind, x: [5, 8, 23, 26][ri(r, 0, 3) as usize], y: top_row - 1, life: 3 },
            "star" => Particle { kind, x: ri(r, 9, 21), y: top_row - 1, life: 3 },
            "coin" => Particle { kind, x: ri(r, 11, 19), y: top_row - 1, life: 4 },
            "dust" => Particle { kind, x: [3, 5, 24, 26][ri(r, 0, 3) as usize], y: GH - 2, life: 2 },
            _ => Particle { kind: "note", x: ri(r, 4, 26), y: top_row - 2, life: 5 },
        };
        parts.push(p);
    }
}

/// One animation frame: particles rise and fade; zzz drift sideways.
pub fn age(parts: &mut Vec<Particle>, frame: u32) {
    for p in parts.iter_mut() {
        p.y -= 1;
        p.life -= 1;
        if p.kind == "z" && frame % 2 == 1 {
            p.x += 1;
        }
    }
    parts.retain(|p| p.life > 0);
}

pub fn add_particles(g: &mut Sprite, parts: &[Particle]) {
    for p in parts {
        let (cells, c): (&[Cell], Color) = match p.kind {
            "heart" => (&sprites::HEART, col::HEART),
            "z" => (&sprites::ZED, col::Z),
            "spark" => (&sprites::SPARK, col::SPARK),
            "bang" => (&sprites::BANG, col::ALERT),
            "star" => (&sprites::SPARK, 0xffd23f),
            "coin" => (&[(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)], 0xffc83d),
            "dust" => (&[(0, 0), (1, 0), (0, 1), (1, 1), (2, 1)], 0xcfc8dc),
            _ => (&sprites::NOTE, col::Z),
        };
        for (dx, dy) in cells {
            if (0..GW).contains(&(p.x + dx)) {
                g.insert((p.x + dx, p.y + dy), c);
            }
        }
    }
}

/// Food in front of the pet while it eats; `progress` 0..1 eats it away (bowl stays).
pub fn add_food(g: &mut Sprite, kind: &str, progress: f64, facing_right: bool) {
    let cells = sprites::food(kind);
    let edible: Vec<_> = cells.iter().filter(|c| !c.3).collect();
    let eaten = (edible.len() as f64 * progress.clamp(0.0, 1.0)) as usize;
    let fx = if facing_right { 26 } else { 1 };
    for c in edible.iter().skip(eaten).copied().chain(cells.iter().filter(|c| c.3)) {
        g.insert((fx + c.0, GH - 4 + c.1), c.2);
    }
}

// ------------------------------------------------ bubble, board, sign
/// The speech bubble with the pet's chirp (and a little icon for what it wants). Returns its rect.
pub fn bubble(cv: &mut Canvas, fonts: &mut Fonts, f: &Frame, text: &str, icon: Option<&str>) -> (i32, i32, i32, i32) {
    let size = 12.5 * f.s as f32;
    let ip = 1.max(sc(f, 2.0));
    let iw = if icon.is_some() { 5 * ip + sc(f, 5.0) } else { 0 };
    let lines = fonts.wrap(text, size, true, sc(f, 150.0));
    let lh = fonts.line_height(size);
    let tw = lines.iter().map(|l| fonts.measure(l, size, true)).max().unwrap_or(0);
    let th = lh * lines.len() as i32;
    let cx = f.w / 2;
    let bottom = (f.bub_h + (f.top_row - 3) * f.p - sc(f, 8.0)).max(sc(f, 30.0));
    let (mut x1, mut y1) = (cx - (tw + iw) / 2 - sc(f, 6.0), bottom - th - sc(f, 3.0));
    let (x2, mut y2) = (x1 + tw + iw + sc(f, 12.0), bottom + sc(f, 3.0));
    if y1 < 1 {
        y2 += 1 - y1;
        y1 = 1;
    }
    x1 = x1.max(0);
    let lw = 1.max(sc(f, 1.0));
    // tail
    let tail = sc(f, 6.0);
    for i in 0..tail {
        let half = sc(f, 4.0) * (tail - i) / tail;
        cv.rect(cx - half - lw, y2 + i, cx + half + lw + 1, y2 + i + 1, col::OUT);
        cv.rect(cx - half, y2 + i, cx + half + 1, y2 + i + 1, col::WHITE);
    }
    cv.rect(x1, y1, x2, y2, col::OUT);
    cv.rect(x1 + lw, y1 + lw, x2 - lw, y2 - lw, col::WHITE);
    for (i, l) in lines.iter().enumerate() {
        let lwid = fonts.measure(l, size, true);
        let lx = x1 + sc(f, 6.0) + (tw - lwid) / 2;
        fonts.draw(cv, lx, y1 + sc(f, 3.0) + lh * i as i32 + lh / 2, l, size, true, col::OUT);
    }
    if let Some(icon) = icon {
        cv.icon(x2 - sc(f, 6.0) - 5 * ip, (y1 + y2) / 2 - 7 * ip / 2, icon, ip, col::Z);
    }
    (x1, y1, x2, y2 + tail)
}

/// The wooden signboard for real information ("Sent!", "Reminder!"). Returns its rect.
pub fn board(cv: &mut Canvas, f: &Frame, text: &str) -> (i32, i32, i32, i32) {
    let px = 1.max(sc(f, 2.0));
    let maxc = 6.max((f.w - 16 * px) / (6 * px)) as usize;
    let lines: Vec<String> = blip_core_wrap(&text.to_uppercase(), maxc).into_iter().take(3).collect();
    let lh = 9 * px;
    let tw = lines.iter().map(|l| pixfont::text_width(l, px)).max().unwrap_or(0);
    let (w, h) = (tw + 10 * px, lines.len() as i32 * lh + 5 * px);
    let x0 = f.w / 2 - w / 2;
    let y1 = (f.bub_h + (f.top_row - 2) * f.p - 3 * px).max(h + 1);
    let y0 = y1 - h;
    let (wood, cream) = (0x8b5a2b, 0xfff1d0);
    cv.rect(f.w / 2 - px, y1, f.w / 2 + px, y1 + 4 * px, wood);
    cv.pixel_box(x0, y0, x0 + w, y1, px, wood);
    cv.pixel_box(x0 + px, y0 + px, x0 + w - px, y1 - px, px, cream);
    for (i, l) in lines.iter().enumerate() {
        cv.pixel_text(f.w / 2 - pixfont::text_width(l, px) / 2, y0 + 3 * px + i as i32 * lh, l, px, col::OUT);
    }
    (x0, y0, x0 + w, y1 + 4 * px)
}

/// The little sign with the running timer. `state`: 0 running, 1 paused, 2 about to ring.
pub fn sign(cv: &mut Canvas, f: &Frame, icon: &str, text: &str, more: usize, state: u8, frame: u32) -> (i32, i32, i32, i32) {
    let px = 1.max(sc(f, 2.0));
    let tw = pixfont::text_width(&format!("  {text}"), px);
    let (w, h) = (tw + 8 * px, 13 * px);
    let x0 = f.w / 2 - w / 2;
    let y1 = f.bub_h + (f.top_row - 1) * f.p - 2 * px;
    let y0 = y1 - h;
    let fill = match state {
        1 => 0xfff4c2,
        2 if frame % 2 == 1 => 0xffd6dc,
        _ => 0xe9fbe9,
    };
    cv.pixel_box(x0, y0, x0 + w, y1, px, col::OUT);
    cv.pixel_box(x0 + px, y0 + px, x0 + w - px, y1 - px, px, fill);
    if !(state == 1 && frame % 4 == 0) {
        cv.icon(x0 + 4 * px, y0 + 3 * px, icon, px, col::OUT);
        cv.pixel_text(x0 + 4 * px + 12 * px, y0 + 3 * px, text, px, col::OUT);
    }
    if more > 0 {
        cv.pixel_text(x0 + w + 2 * px, y0, &format!("+{more}"), px, col::OUT);
    }
    (x0, y0, x0 + w, y1)
}

fn blip_core_wrap(text: &str, width: usize) -> Vec<String> {
    // same rule as wrap_words in app.py
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > width {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            lines.push(word.chars().take(width).collect());
            word = word.chars().skip(width).collect();
        }
        if cur.is_empty() {
            cur = word;
        } else if cur.chars().count() + 1 + word.chars().count() <= width {
            cur.push(' ');
            cur.push_str(&word);
        } else {
            lines.push(std::mem::replace(&mut cur, word));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// The poop window's pixels: `p` pixels per cell.
pub fn poop(p: i32) -> Canvas {
    let mut cv = Canvas::new(8 * p, sprites::POOP.len() as i32 * p);
    for (y, row) in sprites::POOP.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            let c = match ch {
                'o' => col::OUT,
                'b' => col::POOP,
                _ => continue,
            };
            cv.rect(x as i32 * p, y as i32 * p, (x as i32 + 1) * p, (y as i32 + 1) * p, c);
        }
    }
    cv
}
