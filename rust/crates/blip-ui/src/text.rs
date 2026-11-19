//! Smooth text in the system's own font: Segoe UI on Windows, the system font on macOS.
//! Glyphs are rasterised once per size and cached.

use std::collections::HashMap;

use blip_pet::sprites::Color;
use fontdue::{Font, FontSettings};

use crate::Canvas;

pub struct Fonts {
    regular: Font,
    bold: Font,
    cache: HashMap<(char, u32, bool), (fontdue::Metrics, Vec<u8>)>,
}

fn as_refs(v: &[(String, u32)]) -> Vec<(&str, u32)> {
    v.iter().map(|(p, i)| (p.as_str(), *i)).collect()
}

fn load(paths: &[(&str, u32)]) -> Option<Font> {
    for (p, index) in paths {
        if let Ok(bytes) = std::fs::read(p) {
            let settings = FontSettings { collection_index: *index, ..FontSettings::default() };
            if let Ok(f) = Font::from_bytes(bytes, settings) {
                return Some(f);
            }
        }
    }
    None
}

impl Fonts {
    /// The system UI font, or None if none of the usual files exist.
    pub fn system() -> Option<Fonts> {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let win = |f: &str| format!("{windir}\\Fonts\\{f}");
        let (reg, bold): (Vec<(String, u32)>, Vec<(String, u32)>) = if cfg!(windows) {
            (
                vec![(win("segoeui.ttf"), 0), (win("arial.ttf"), 0)],
                vec![(win("seguisb.ttf"), 0), (win("segoeuib.ttf"), 0), (win("arialbd.ttf"), 0)],
            )
        } else {
            (
                vec![
                    ("/System/Library/Fonts/SFNS.ttf".into(), 0),
                    ("/System/Library/Fonts/Helvetica.ttc".into(), 0),
                    ("/Library/Fonts/Arial.ttf".into(), 0),
                ],
                vec![
                    ("/System/Library/Fonts/Supplemental/Arial Bold.ttf".into(), 0),
                    ("/System/Library/Fonts/Helvetica.ttc".into(), 1),
                    ("/System/Library/Fonts/SFNS.ttf".into(), 0),
                ],
            )
        };
        let regular = load(&as_refs(&reg))?;
        let bold = load(&as_refs(&bold)).unwrap_or_else(|| regular.clone());
        Some(Fonts { regular, bold, cache: HashMap::new() })
    }

    fn glyph(&mut self, c: char, size: f32, bold: bool) -> &(fontdue::Metrics, Vec<u8>) {
        let key = (c, (size * 10.0) as u32, bold);
        let font = if bold { &self.bold } else { &self.regular };
        self.cache.entry(key).or_insert_with(|| font.rasterize(c, size))
    }

    /// Width in pixels of `text` at `size` pixels.
    pub fn measure(&mut self, text: &str, size: f32, bold: bool) -> i32 {
        let mut w = 0.0;
        for c in text.chars() {
            w += self.glyph(c, size, bold).0.advance_width;
        }
        w.ceil() as i32
    }

    /// Draw `text` with its baseline-centred box vertically centred on `cy`; returns the width.
    pub fn draw(&mut self, cv: &mut Canvas, x: i32, cy: i32, text: &str, size: f32, bold: bool, c: Color) -> i32 {
        let font = if bold { &self.bold } else { &self.regular };
        let lm = font.horizontal_line_metrics(size);
        let (ascent, descent) = lm.map(|m| (m.ascent, m.descent)).unwrap_or((size * 0.8, -size * 0.2));
        let baseline = cy as f32 + (ascent + descent) / 2.0;
        let mut pen = x as f32;
        for ch in text.chars() {
            let (m, bitmap) = self.glyph(ch, size, bold).clone();
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col];
                    if a > 0 {
                        cv.blend(gx + col as i32, gy + row as i32, c, a);
                    }
                }
            }
            pen += m.advance_width;
        }
        (pen - x as f32).ceil() as i32
    }

    /// Cut `text` with an ellipsis so it fits in `max` pixels.
    pub fn fit(&mut self, text: &str, size: f32, bold: bool, max: i32) -> String {
        if self.measure(text, size, bold) <= max {
            return text.to_string();
        }
        let mut s: String = text.to_string();
        while !s.is_empty() && self.measure(&format!("{s}\u{2026}"), size, bold) > max {
            s.pop();
        }
        format!("{}\u{2026}", s.trim_end())
    }

    /// Break `text` into lines no wider than `max` pixels (keeps the text's own line breaks).
    pub fn wrap(&mut self, text: &str, size: f32, bold: bool, max: i32) -> Vec<String> {
        let mut out = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let trial = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if self.measure(&trial, size, bold) <= max || line.is_empty() {
                    line = trial;
                } else {
                    out.push(std::mem::take(&mut line));
                    line = word.to_string();
                }
            }
            out.push(line);
        }
        out
    }

    /// Line height in pixels for `size`.
    pub fn line_height(&self, size: f32) -> i32 {
        self.regular.horizontal_line_metrics(size).map(|m| m.new_line_size.ceil() as i32).unwrap_or((size * 1.3) as i32)
    }
}
