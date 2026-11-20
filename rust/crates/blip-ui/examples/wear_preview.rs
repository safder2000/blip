//! Preview sheet of what pets wear, the pickaxe swing and the coin:
//!     cargo run -p blip-ui --example wear_preview -- <out.bmp>

use blip_pet::sprites::{add_pickaxe, add_wear, build_pet, coin_color, PetLook, Stage, COIN, SPECIES};
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
    let path = std::env::args().nth(1).unwrap_or_else(|| "wear.bmp".into());
    let p = 4;
    let cell = 40 * p;
    // rows: none, shades, specs, party, hardhat, flower, crown, crown+shades, pickaxe up, pickaxe down
    let rows: Vec<Vec<&str>> = vec![vec![], vec!["shades"], vec!["specs"], vec!["party"], vec!["hardhat"], vec!["flower"], vec!["crown"], vec!["crown", "shades"], vec!["hardhat", "UP"], vec!["hardhat", "DOWN"]];
    let mut cv = Canvas::new(cell * (SPECIES.len() as i32 + 1), cell * rows.len() as i32);
    for (i, sp) in SPECIES.iter().enumerate() {
        for (r, wear) in rows.iter().enumerate() {
            let stage = if r % 3 == 2 { Stage::Child } else { Stage::Adult };
            let look = PetLook { stage, palette: "gold", mood: "normal", frame: 1, facing: if r >= 8 { 1 } else { 0 }, squash: false, blink: false, species: sp };
            let mut g = build_pet(&look);
            let w: Vec<String> = wear.iter().filter(|w| w.chars().all(|c| c.is_lowercase())).map(|s| s.to_string()).collect();
            add_wear(&mut g, &look, &w);
            if wear.contains(&"UP") {
                add_pickaxe(&mut g, &look, (i % 4 + 1) as u8, true);
            }
            if wear.contains(&"DOWN") {
                add_pickaxe(&mut g, &look, (i % 4 + 1) as u8, false);
            }
            let (bx, by, _, _) = sprite_bounds(&g);
            cv.sprite(&g, i as i32 * cell + 4 * p - bx * p, r as i32 * cell + 6 * p - by * p, p);
        }
    }
    // the coin
    let x0 = SPECIES.len() as i32 * cell + 8 * p;
    for (y, row) in COIN.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if let Some(c) = coin_color(ch) {
                cv.rect(x0 + x as i32 * p, 8 * p + y as i32 * p, x0 + x as i32 * p + p, 8 * p + y as i32 * p + p, c);
            }
        }
    }
    save_bmp(&cv, &path);
    println!("wrote {path}");
}
