//! Drawing for Blip's own windows (the menu and the cards) in plain memory: no GPU, no web view.
//! A [`Canvas`] is premultiplied ARGB pixels (`0xAARRGGBB`); transparent pixels let the desktop
//! show through. Text uses the system font through `fontdue`.

pub mod card;
pub mod menu;
pub mod scene;
pub mod theme;
pub mod text;

use blip_pet::pixfont;
use blip_pet::sprites::{Color, Sprite};

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: i32, h: i32) -> Canvas {
        Canvas { w, h, px: vec![0; (w.max(0) * h.max(0)) as usize] }
    }

    pub fn clear(&mut self) {
        self.px.iter_mut().for_each(|p| *p = 0);
    }

    /// Opaque rectangle [x0, x1) x [y0, y1).
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Color) {
        let (x0, x1) = (x0.max(0), x1.min(self.w));
        let (y0, y1) = (y0.max(0), y1.min(self.h));
        for y in y0..y1 {
            let row = (y * self.w) as usize;
            for x in x0..x1 {
                self.px[row + x as usize] = 0xff00_0000 | c;
            }
        }
    }

    /// A rectangle with one-step pixel corners (Blip's look).
    pub fn pixel_box(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, p: i32, c: Color) {
        self.rect(x0 + p, y0, x1 - p, y1, c);
        self.rect(x0, y0 + p, x1, y1 - p, c);
    }

    /// Blend colour `c` with coverage `a` (0..=255) over what's there (assumed opaque).
    pub fn blend(&mut self, x: i32, y: i32, c: Color, a: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        let dst = self.px[i];
        if a == 255 || dst >> 24 == 0 {
            if a == 255 {
                self.px[i] = 0xff00_0000 | c;
            } else {
                // over transparency: premultiplied colour with partial alpha
                let m = |v: u32| (v * a as u32 / 255) & 255;
                self.px[i] = (a as u32) << 24 | m(c >> 16) << 16 | m(c >> 8) << 8 | m(c);
            }
            return;
        }
        let mix = |s: u32, d: u32| (s * a as u32 + d * (255 - a as u32)) / 255;
        let r = mix((c >> 16) & 255, (dst >> 16) & 255);
        let g = mix((c >> 8) & 255, (dst >> 8) & 255);
        let b = mix(c & 255, dst & 255);
        self.px[i] = 0xff00_0000 | r << 16 | g << 8 | b;
    }

    /// 5x7 rows (bit 0x10 = leftmost) drawn with cells of `p` pixels.
    pub fn glyph(&mut self, x: i32, y: i32, rows: [u8; 7], p: i32, c: Color) {
        for (r, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0x10 >> col) != 0 {
                    self.rect(x + col * p, y + r as i32 * p, x + (col + 1) * p, y + (r as i32 + 1) * p, c);
                }
            }
        }
    }

    pub fn icon(&mut self, x: i32, y: i32, name: &str, p: i32, c: Color) {
        self.glyph(x, y, pixfont::icon(name), p, c);
    }

    /// Pixel-font text (upper-case look); returns the width drawn.
    pub fn pixel_text(&mut self, x: i32, y: i32, text: &str, p: i32, c: Color) -> i32 {
        for (i, ch) in text.chars().enumerate() {
            self.glyph(x + i as i32 * 6 * p, y, pixfont::glyph(ch), p, c);
        }
        pixfont::text_width(text, p)
    }

    /// A big icon (rows of '0'/'1').
    pub fn bitmap(&mut self, x: i32, y: i32, rows: &[&str], p: i32, c: Color) {
        for (r, line) in rows.iter().enumerate() {
            for (col, b) in line.bytes().enumerate() {
                if b == b'1' {
                    let (cx, cy) = (x + col as i32 * p, y + r as i32 * p);
                    self.rect(cx, cy, cx + p, cy + p, c);
                }
            }
        }
    }

    /// A pet sprite, cell (0, 0) at (x, y), `p` pixels per cell.
    pub fn sprite(&mut self, g: &Sprite, x: i32, y: i32, p: i32) {
        for (&(cx, cy), &c) in g {
            self.rect(x + cx * p, y + cy * p, x + (cx + 1) * p, y + (cy + 1) * p, c);
        }
    }
}

/// Size a sprite would take when cropped to its cells: (min_x, min_y, width, height) in cells.
pub fn sprite_bounds(g: &Sprite) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for &(x, y) in g.keys() {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if x0 > x1 {
        return (0, 0, 0, 0);
    }
    (x0, y0, x1 - x0 + 1, y1 - y0 + 1)
}
