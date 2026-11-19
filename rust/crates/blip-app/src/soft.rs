//! A small CPU renderer for egui: fills egui's triangles into a pixel buffer, so Blip's windows
//! need no GPU, no OpenGL and no Metal - the same code draws them on every computer.
//!
//! egui hands over triangles with premultiplied colours and a texture each (the font atlas, or
//! images such as the pet). Each triangle is filled with the usual edge-function test and the
//! top-left rule (so two triangles sharing an edge never both paint it), colours and texture
//! coordinates are interpolated, the texture is sampled bilinearly and blended "over".

use std::collections::HashMap;

use egui::epaint::textures::TexturesDelta;
use egui::epaint::{ClippedPrimitive, ImageData, Primitive, TextureId, Vertex};

struct Tex {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>, // premultiplied RGBA
    nearest: bool,
}

impl Tex {
    #[inline]
    fn at(&self, x: usize, y: usize) -> [f32; 4] {
        let p = self.px[y * self.w + x];
        [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
    }

    /// Premultiplied RGBA (0..255) at texture coordinate (u, v).
    #[inline]
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.nearest {
            let x = ((u * self.w as f32) as isize).clamp(0, self.w as isize - 1) as usize;
            let y = ((v * self.h as f32) as isize).clamp(0, self.h as isize - 1) as usize;
            return self.at(x, y);
        }
        let fx = (u * self.w as f32 - 0.5).clamp(0.0, (self.w - 1) as f32);
        let fy = (v * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (fx as usize, fy as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let (a, b, c, d) = (self.at(x0, y0), self.at(x1, y0), self.at(x0, y1), self.at(x1, y1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * tx;
            let bot = c[i] + (d[i] - c[i]) * tx;
            out[i] = top + (bot - top) * ty;
        }
        out
    }
}

#[derive(Default)]
pub struct Painter {
    textures: HashMap<TextureId, Tex>,
}

impl Painter {
    pub fn update_textures(&mut self, delta: &TexturesDelta) {
        for (id, deltas) in &delta.set {
            for d in deltas {
                let ImageData::Color(img) = &d.image;
                let [w, h] = img.size;
                let px: Vec<[u8; 4]> = img.pixels.iter().map(|c| c.to_array()).collect();
                let nearest = d.options.magnification == egui::TextureFilter::Nearest;
                match d.pos {
                    None => {
                        self.textures.insert(*id, Tex { w, h, px, nearest });
                    }
                    Some([x0, y0]) => {
                        if let Some(t) = self.textures.get_mut(id) {
                            for y in 0..h {
                                for x in 0..w {
                                    if x0 + x < t.w && y0 + y < t.h {
                                        t.px[(y0 + y) * t.w + x0 + x] = px[y * w + x];
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn free_textures(&mut self, delta: &TexturesDelta) {
        for id in &delta.free {
            self.textures.remove(id);
        }
    }

    /// Draw the primitives into `buf` (0x00RRGGBB, `w` x `h`), which already holds the background.
    pub fn paint(&self, prims: &[ClippedPrimitive], ppp: f32, buf: &mut [u32], w: usize, h: usize) {
        for cp in prims {
            let Primitive::Mesh(mesh) = &cp.primitive else { continue };
            let Some(tex) = self.textures.get(&mesh.texture_id) else { continue };
            let c = cp.clip_rect;
            let clip = (
                ((c.min.x * ppp).round().max(0.0) as i32),
                ((c.min.y * ppp).round().max(0.0) as i32),
                ((c.max.x * ppp).round().min(w as f32) as i32),
                ((c.max.y * ppp).round().min(h as f32) as i32),
            );
            if clip.0 >= clip.2 || clip.1 >= clip.3 {
                continue;
            }
            for tri in mesh.indices.chunks_exact(3) {
                let v = [&mesh.vertices[tri[0] as usize], &mesh.vertices[tri[1] as usize], &mesh.vertices[tri[2] as usize]];
                fill(v, ppp, tex, clip, buf, w);
            }
        }
    }
}

#[inline]
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Whether pixels exactly on edge a->b belong to this triangle (the top-left rule).
#[inline]
fn owns_edge(ax: f32, ay: f32, bx: f32, by: f32) -> bool {
    let (dx, dy) = (bx - ax, by - ay);
    dy > 0.0 || (dy == 0.0 && dx < 0.0)
}

fn fill(v: [&Vertex; 3], ppp: f32, tex: &Tex, clip: (i32, i32, i32, i32), buf: &mut [u32], w: usize) {
    let mut p = [(v[0].pos.x * ppp, v[0].pos.y * ppp), (v[1].pos.x * ppp, v[1].pos.y * ppp), (v[2].pos.x * ppp, v[2].pos.y * ppp)];
    let mut vv = v;
    let mut area = edge(p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1);
    if area.abs() < 1e-6 {
        return;
    }
    if area < 0.0 {
        p.swap(1, 2);
        vv.swap(1, 2);
        area = -area;
    }
    let x0 = (p[0].0.min(p[1].0).min(p[2].0).floor() as i32).max(clip.0);
    let x1 = (p[0].0.max(p[1].0).max(p[2].0).ceil() as i32).min(clip.2);
    let y0 = (p[0].1.min(p[1].1).min(p[2].1).floor() as i32).max(clip.1);
    let y1 = (p[0].1.max(p[1].1).max(p[2].1).ceil() as i32).min(clip.3);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    // weight of vertex i comes from the edge opposite it
    let edges = [(1usize, 2usize), (2, 0), (0, 1)];
    let own: [bool; 3] = std::array::from_fn(|i| {
        let (a, b) = edges[i];
        owns_edge(p[a].0, p[a].1, p[b].0, p[b].1)
    });
    // per-pixel steps of each edge function
    let dx: [f32; 3] = std::array::from_fn(|i| {
        let (a, b) = edges[i];
        -(p[b].1 - p[a].1)
    });
    let dy: [f32; 3] = std::array::from_fn(|i| {
        let (a, b) = edges[i];
        p[b].0 - p[a].0
    });
    let col: [[f32; 4]; 3] = std::array::from_fn(|i| {
        let c = vv[i].color.to_array();
        [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]
    });
    let inv = 1.0 / area;
    let same_uv = vv[0].uv == vv[1].uv && vv[1].uv == vv[2].uv;
    let same_col = vv[0].color == vv[1].color && vv[1].color == vv[2].color;
    // a flat triangle (most of egui: rectangles, lines): one colour for every pixel
    let flat = if same_uv && same_col {
        let t = tex.sample(vv[0].uv.x, vv[0].uv.y);
        let c = col[0];
        let s = [c[0] * t[0] / 255.0, c[1] * t[1] / 255.0, c[2] * t[2] / 255.0, c[3] * t[3] / 255.0];
        if s[3] < 0.5 {
            return;
        }
        Some(s)
    } else {
        None
    };
    let tsame = if same_uv { Some(tex.sample(vv[0].uv.x, vv[0].uv.y)) } else { None };

    for y in y0..y1 {
        let py = y as f32 + 0.5;
        let px0 = x0 as f32 + 0.5;
        let mut e: [f32; 3] = std::array::from_fn(|i| {
            let (a, b) = edges[i];
            edge(p[a].0, p[a].1, p[b].0, p[b].1, px0, py)
        });
        let row = y as usize * w;
        for x in x0..x1 {
            let inside = (0..3).all(|i| e[i] > 0.0 || (e[i] == 0.0 && own[i]));
            if inside {
                let src = if let Some(s) = flat {
                    s
                } else {
                    let (l0, l1, l2) = (e[0] * inv, e[1] * inv, e[2] * inv);
                    let c: [f32; 4] = std::array::from_fn(|k| col[0][k] * l0 + col[1][k] * l1 + col[2][k] * l2);
                    let t = match tsame {
                        Some(t) => t,
                        None => {
                            let u = vv[0].uv.x * l0 + vv[1].uv.x * l1 + vv[2].uv.x * l2;
                            let v = vv[0].uv.y * l0 + vv[1].uv.y * l1 + vv[2].uv.y * l2;
                            tex.sample(u, v)
                        }
                    };
                    [c[0] * t[0] / 255.0, c[1] * t[1] / 255.0, c[2] * t[2] / 255.0, c[3] * t[3] / 255.0]
                };
                if src[3] > 0.0 || src[0] > 0.0 || src[1] > 0.0 || src[2] > 0.0 {
                    let d = &mut buf[row + x as usize];
                    let k = 1.0 - src[3] / 255.0;
                    let (dr, dg, db) = (((*d >> 16) & 255) as f32, ((*d >> 8) & 255) as f32, (*d & 255) as f32);
                    let r = (src[0] + dr * k).min(255.0) as u32;
                    let g = (src[1] + dg * k).min(255.0) as u32;
                    let b = (src[2] + db * k).min(255.0) as u32;
                    *d = (r << 16) | (g << 8) | b;
                }
            }
            for i in 0..3 {
                e[i] += dx[i];
            }
        }
    }
    let _ = dy;
}
