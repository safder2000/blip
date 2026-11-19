//! Pop-up cards (calls, messages, alarms, reminders, tasks), ported from `PixelCard` in `blip/ui.py`:
//! a coloured pixel header with an icon and title, a close X, the message, and pixel buttons
//! (the first one highlighted). Layout, drawing and hit-testing only.

use blip_pet::pixfont;
use blip_pet::sprites::{col, Color};

use crate::text::Fonts;
use crate::theme::Theme;
use crate::Canvas;

pub fn accent(kind: &str) -> Color {
    match kind {
        "call" => 0x3fb67a,
        "msg" => 0x6b7fd7,
        "alarm" => 0xef5b6b,
        "timer" => 0xf0a020,
        "task" => 0x2f8fd8,
        "update" => 0x2fb5a8,
        _ => 0x9a7cc8, // info
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardHit {
    Close,
    Header,
    Button(usize),
    /// the slider row, at this level (0..=SLIDER_STEPS)
    Slider(usize),
}

/// Steps of the card slider (the voice boost: 1x .. 3x).
pub const SLIDER_STEPS: usize = 8;

#[derive(Clone, Debug)]
pub struct Slider {
    pub label: String,
    pub level: usize,
    /// run as "<cmd>:<level>" when it changes
    pub cmd: String,
}

pub struct Card {
    pub title: String,
    pub body: String,
    pub kind: String,
    pub icon: Option<&'static str>,
    /// (label, command)
    pub buttons: Vec<(String, String)>,
    pub w: i32,
    pub h: i32,
    p: i32,
    head_h: i32,
    pad: i32,
    lines: Vec<String>,
    body_y: i32,
    line_h: i32,
    rects: Vec<(i32, i32, i32, i32)>,
    pub hover: Option<CardHit>,
    s: f64,
    pub slider: Option<Slider>,
    slider_rect: (i32, i32, i32, i32),
}

const BODY: f32 = 13.3;

impl Card {
    pub fn new(title: &str, body: &str, kind: &str, icon: Option<&'static str>, buttons: Vec<(String, String)>, scale: f64, fonts: &mut Fonts) -> Card {
        Card::with_slider(title, body, kind, icon, buttons, None, scale, fonts)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_slider(title: &str, body: &str, kind: &str, icon: Option<&'static str>, buttons: Vec<(String, String)>, slider: Option<Slider>, scale: f64, fonts: &mut Fonts) -> Card {
        let s = scale.max(1.0);
        let p = 2.max((2.0 * s).round() as i32);
        let w = (300.0 * s).round() as i32;
        let pad = 6 * p;
        let head_h = 7 * p + 8 * p;
        let size = BODY * s as f32;
        let line_h = fonts.line_height(size);
        let mut y = p + head_h + pad;
        let lines = if body.is_empty() { vec![] } else { fonts.wrap(body, size, false, w - 4 * p - 2 * pad) };
        let body_y = y;
        if !lines.is_empty() {
            y += lines.len() as i32 * line_h + pad;
        }
        let mut slider_rect = (0, 0, 0, 0);
        if slider.is_some() {
            slider_rect = (2 * p + pad, y, w - 2 * p - pad, y + 10 * p);
            y += 10 * p + pad;
        }
        // buttons right-aligned, wrapping onto more rows if needed
        let bh = 7 * p + 8 * p;
        let avail = w - 4 * p - 2 * pad;
        let mut rows: Vec<Vec<(usize, i32)>> = vec![];
        let mut row: Vec<(usize, i32)> = vec![];
        let mut roww = 0;
        for (i, (label, _)) in buttons.iter().enumerate() {
            let bw = pixfont::text_width(&label.to_uppercase(), p) + 12 * p;
            if !row.is_empty() && roww + bw + 3 * p > avail {
                rows.push(std::mem::take(&mut row));
                roww = 0;
            }
            row.push((i, bw));
            roww += bw + 3 * p;
        }
        if !row.is_empty() {
            rows.push(row);
        }
        let mut rects = vec![(0, 0, 0, 0); buttons.len()];
        for r in &rows {
            let total: i32 = r.iter().map(|b| b.1).sum::<i32>() + 3 * p * (r.len() as i32 - 1);
            let mut bx = w - 2 * p - pad - total;
            for &(i, bw) in r {
                rects[i] = (bx, y, bx + bw, y + bh);
                bx += bw + 3 * p;
            }
            y += bh + 3 * p;
        }
        let h = if rows.is_empty() { y } else { y - 3 * p + pad } + 2 * p;
        Card {
            title: title.into(),
            body: body.into(),
            kind: kind.into(),
            icon,
            buttons,
            w,
            h,
            p,
            head_h,
            pad,
            lines,
            body_y,
            line_h,
            rects,
            hover: None,
            s,
            slider,
            slider_rect,
        }
    }

    fn close_rect(&self) -> (i32, i32, i32, i32) {
        let p = self.p;
        let cx0 = self.w - 2 * p - self.pad - 5 * p;
        (cx0 - 2 * p, p, cx0 + 7 * p, p + self.head_h)
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<CardHit> {
        let inside = |r: (i32, i32, i32, i32)| r.0 <= x && x < r.2 && r.1 <= y && y < r.3;
        if inside(self.close_rect()) {
            return Some(CardHit::Close);
        }
        if y < self.p + self.head_h && y >= 0 {
            return Some(CardHit::Header);
        }
        if self.slider.is_some() && inside(self.slider_rect) {
            return self.slider_level_at(x).map(CardHit::Slider);
        }
        self.rects.iter().position(|r| inside(*r)).map(CardHit::Button)
    }

    /// Where the bars start, and each bar's width and gap.
    fn bars(&self) -> (i32, i32, i32) {
        let p = self.p;
        let label_w = self.slider.as_ref().map_or(0, |s| pixfont::text_width(&s.label.to_uppercase(), p));
        (self.slider_rect.0 + label_w + 4 * p, 4 * p, 2 * p)
    }

    /// The level for a point on the slider row (None left of the bars).
    pub fn slider_level_at(&self, x: i32) -> Option<usize> {
        let (x0, bw, gap) = self.bars();
        if x < x0 - 3 * self.p {
            return None;
        }
        Some((((x - x0) / (bw + gap)) + 1).clamp(0, SLIDER_STEPS as i32) as usize).map(|l| if x < x0 { 0 } else { l })
    }

    pub fn draw(&self, cv: &mut Canvas, t: &Theme, fonts: &mut Fonts) {
        let (p, w, h) = (self.p, self.w, self.h);
        let ink = col::OUT;
        let accent = accent(&self.kind);
        cv.clear();
        // frame: shadow, ink border, paper, accent header
        cv.pixel_box(2 * p, 2 * p, w, h, p, ink);
        cv.pixel_box(0, 0, w - 2 * p, h - 2 * p, p, ink);
        cv.pixel_box(p, p, w - 3 * p, h - 3 * p, p, t.paper);
        let hh = self.head_h;
        cv.rect(2 * p, p, w - 4 * p, p + hh, accent);
        cv.rect(p, 2 * p, w - 3 * p, p + hh, accent);
        cv.rect(p, p + hh, w - 3 * p, p + hh + p, ink);
        let ty = p + (hh - 7 * p) / 2;
        let mut x = 2 * p + self.pad;
        if let Some(icon) = self.icon {
            cv.icon(x, ty, icon, p, 0xffffff);
            x += 5 * p + 3 * p;
        }
        let maxc = ((w - x - 14 * p - self.pad) / (6 * p)).max(4) as usize;
        let title: String = self.title.to_uppercase().chars().take(maxc).collect();
        cv.pixel_text(x, ty, &title, p, 0xffffff);
        let (cx0, ..) = self.close_rect();
        let close_hov = self.hover == Some(CardHit::Close);
        cv.pixel_text(cx0 + 2 * p, ty, "X", p, if close_hov { 0xffe08a } else { 0xffffff });
        let size = BODY * self.s as f32;
        for (i, line) in self.lines.iter().enumerate() {
            let cy = self.body_y + i as i32 * self.line_h + self.line_h / 2;
            fonts.draw(cv, 2 * p + self.pad, cy, line, size, false, t.paper_ink);
        }
        if let Some(sl) = &self.slider {
            let (sx, sy, _, sy1) = self.slider_rect;
            let mid = (sy + sy1) / 2;
            cv.pixel_text(sx, mid - 7 * p / 2, &sl.label.to_uppercase(), p, t.paper_ink);
            let (x0, bw, gap) = self.bars();
            const COLS: [Color; 8] = [0x7ad9b0, 0x9ad396, 0xc2ca78, 0xe6be66, 0xf6a26c, 0xf68a7e, 0xf07592, 0xe868a6];
            for i in 0..SLIDER_STEPS {
                let bx = x0 + i as i32 * (bw + gap);
                let bh = 3 * p + (sy1 - sy - 3 * p) * (i as i32 + 1) / SLIDER_STEPS as i32;
                let lit = i < sl.level;
                let fill = if lit { COLS[i] } else if t.dark { 0x3a3a3a } else { 0xd8d2e6 };
                cv.pixel_box(bx, sy1 - bh, bx + bw, sy1, p, fill);
            }
            let tag = format!("{}%", 100 + sl.level * 25);
            let tx = x0 + SLIDER_STEPS as i32 * (bw + gap) + 2 * p;
            cv.pixel_text(tx, mid - 7 * p / 2, &tag, p, t.paper_ink);
        }
        for (i, (label, _)) in self.buttons.iter().enumerate() {
            let (x0, y0, x1, y1) = self.rects[i];
            let first = i == 0;
            let base = if first { accent } else { t.paper.min(0xffffff) };
            let base = if first { base } else if t.dark { 0x2a2a2a } else { 0xffffff };
            let fill = if self.hover == Some(CardHit::Button(i)) { shade(base) } else { base };
            let fg = if first { 0xffffff } else { t.paper_ink };
            cv.pixel_box(x0, y0, x1, y1, p, ink);
            cv.pixel_box(x0 + p, y0 + p, x1 - p, y1 - p, p, fill);
            cv.pixel_text(x0 + 6 * p, y0 + 4 * p, &label.to_uppercase(), p, fg);
        }
    }
}

fn shade(c: Color) -> Color {
    let f = |v: u32| ((v as f64) * 0.88) as u32;
    f((c >> 16) & 255) << 16 | f((c >> 8) & 255) << 8 | f(c & 255)
}
