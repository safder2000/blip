//! Saving a window's own pixels as a PNG, for testing without a screen grab (BLIP_SHOT=dir).
//! A plain PNG writer: stored (uncompressed) deflate blocks, so no extra library.

use std::io::Write;
use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xffff_ffffu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

pub fn save_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(if i + 1 == blocks.len() { 1 } else { 0 });
        let n = b.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = vec![];
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    std::fs::File::create(path)?.write_all(&out)
}

/// Save an overlay canvas (premultiplied ARGB) if BLIP_SHOT is set: `<dir>/<name>.png`.
pub fn canvas(name: &str, cv: &blip_ui::Canvas) {
    let Some(dir) = std::env::var_os("BLIP_SHOT") else { return };
    let mut rgba = Vec::with_capacity(cv.px.len() * 4);
    for &p in &cv.px {
        let a = (p >> 24) & 255;
        let un = |c: u32| if a == 0 { 0 } else { ((c * 255 + a / 2) / a).min(255) as u8 };
        rgba.extend_from_slice(&[un((p >> 16) & 255), un((p >> 8) & 255), un(p & 255), a as u8]);
    }
    let _ = save_png(&std::path::Path::new(&dir).join(format!("{name}.png")), cv.w as u32, cv.h as u32, &rgba);
}

/// Like `canvas`, but at most every `secs` seconds per name.
pub fn canvas_every(name: &str, cv: &blip_ui::Canvas, secs: f64) {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static LAST: Mutex<Option<HashMap<String, f64>>> = Mutex::new(None);
    if std::env::var_os("BLIP_SHOT").is_none() {
        return;
    }
    let now = blip_core::now();
    {
        let mut g = LAST.lock().unwrap();
        let m = g.get_or_insert_with(HashMap::new);
        if m.get(name).map_or(false, |t| now - t < secs) {
            return;
        }
        m.insert(name.to_string(), now);
    }
    canvas(name, cv);
}
