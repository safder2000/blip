//! Turning a sprite into window pixels.
//!
//! The buffer is `w * h` pixels of premultiplied ARGB (`0xAARRGGBB`): sprite cells are fully opaque,
//! everything else fully transparent, so the desktop shows through and clicks there go to whatever
//! is behind the pet.

use crate::body::Geo;
use crate::sprites::Sprite;

pub fn render(g: &Sprite, geo: &Geo, buf: &mut Vec<u32>) {
    let (w, h, p) = (geo.w, geo.h, geo.p);
    buf.clear();
    buf.resize((w * h) as usize, 0);
    for (&(cx, cy), &c) in g {
        let x0 = geo.ox + cx * p;
        let y0 = geo.bub_h + cy * p;
        for y in y0.max(0)..(y0 + p).min(h) {
            let row = (y * w) as usize;
            for x in x0.max(0)..(x0 + p).min(w) {
                buf[row + x as usize] = 0xff00_0000 | c;
            }
        }
    }
}

/// True if the window pixel (x, y) is part of the pet (not see-through).
pub fn opaque(buf: &[u32], geo: &Geo, x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= geo.w || y >= geo.h {
        return false;
    }
    buf[(y * geo.w + x) as usize] >> 24 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprites::{build_pet, PetLook, Stage};

    #[test]
    fn pixels_land_where_cells_are() {
        let geo = Geo::new(1.0);
        let g = build_pet(&PetLook {
            stage: Stage::Adult,
            palette: "gold",
            mood: "normal",
            frame: 0,
            facing: 0,
            squash: false,
            blink: false,
            species: "bunny",
        });
        let mut buf = Vec::new();
        render(&g, &geo, &mut buf);
        assert_eq!(buf.len(), (geo.w * geo.h) as usize);
        // the bottom-middle of the body is opaque, the window corners are see-through
        assert!(opaque(&buf, &geo, geo.w / 2, geo.h - geo.p * 2));
        assert!(!opaque(&buf, &geo, 0, 0));
        assert!(!opaque(&buf, &geo, geo.w - 1, geo.h - 1));
        let painted = buf.iter().filter(|px| **px >> 24 != 0).count();
        assert_eq!(painted, g.iter().filter(|((_, y), _)| geo.bub_h + y * geo.p >= 0).count() * (geo.p * geo.p) as usize);
    }
}
