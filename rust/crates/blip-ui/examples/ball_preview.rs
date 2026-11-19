//! Preview sheet of the species balls, the wind and fire trails, and the parachutes:
//!     cargo run -p blip-ui --example ball_preview -- <out.bmp>

use blip_pet::body::{add_chute, add_trail, Geo};
use blip_pet::sprites::{self, build_pet, build_species_ball, PetLook, Stage, SPECIES};
use blip_ui::{sprite_bounds, Canvas};

fn save_bmp(cv: &Canvas, path: &str) {
    let (w, h) = (cv.w as u32, cv.h as u32);
    let mut out = Vec::new();
    out.extend_from_slice(b"BM");
    for v in [54 + w * h * 4, 0, 54, 40] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    for v in [0u32, w * h * 4, 2835, 2835, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for px in &cv.px {
        let c = if px >> 24 == 0 { 0x4a4f5a } else { *px };
        out.extend_from_slice(&(c | 0xff00_0000).to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "balls.bmp".into());
    let p = 4;
    let cell = 46 * p;
    let mut cv = Canvas::new(cell * 7, cell * 4);
    let geo = Geo::new(1.0);
    for (i, sp) in SPECIES.iter().enumerate() {
        let colors = sprites::palette("gold");
        // row 0: ball, row 1: ball turned, row 2: wind trail, row 3: fire trail (or a parachute)
        for (row, angle) in [0.0, 1.7, 0.4, 0.4].iter().enumerate() {
            let mut g = build_species_ball(sp, colors, *angle, true, 0.0);
            if row == 2 {
                add_trail(&mut g, 22.0, -6.0, &geo, false);
            }
            if row == 3 && i < 1 {
                add_trail(&mut g, 26.0, -8.0, &geo, true);
            }
            if row == 3 && i >= 1 {
                g = build_pet(&PetLook { stage: Stage::Adult, palette: "gold", mood: "happy", frame: 1, facing: 0, squash: false, blink: false, species: sp });
                let top = g.keys().map(|c| c.1).min().unwrap_or(0);
                add_chute(&mut g, top, 20, (i - 1) as u8);
            }
            let (bx, by, _, _) = sprite_bounds(&g);
            cv.sprite(&g, i as i32 * cell + 6 * p - bx * p + 6 * p, row as i32 * cell + 16 * p - by * p, p);
        }
    }
    save_bmp(&cv, &path);
    println!("wrote {path}");
}
