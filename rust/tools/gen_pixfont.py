"""Generate crates/blip-pet/src/pixfont.rs (the 5x7 pixel font, icons and big icons) from blip/sprites.py.
    python rust/tools/gen_pixfont.py"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
from blip import sprites  # noqa: E402

out = ['//! The 5x7 pixel font, its icons and the bigger button icons.',
       '//! Generated from `blip/sprites.py` by `python rust/tools/gen_pixfont.py`; do not edit by hand.',
       '',
       '/// Rows of a 5x7 glyph, top to bottom; bit 0x10 is the leftmost pixel.',
       'pub fn glyph(c: char) -> [u8; 7] {',
       '    match c {']
for ch, rows in sorted(sprites.FONT.items()):
    if ord(ch) >= 0xE000 or ord(ch) < 0x20:
        continue
    lit = repr(ch) if ch != "'" else '\'\\\'\''
    if ch == "\\":
        lit = "'\\\\'"
    out.append(f"        {lit} => {list(rows)},")
out += ["        c if c.is_ascii_lowercase() => glyph(c.to_ascii_uppercase()),",
        f"        _ => {list(sprites.FONT['?'])},",
        "    }",
        "}",
        "",
        "/// A named icon (same names as the Python ICON table).",
        "pub fn icon(name: &str) -> [u8; 7] {",
        "    match name {"]
for name, ch in sorted(sprites.ICON.items()):
    out.append(f'        "{name}" => {list(sprites.FONT[ch])},')
out += [f"        _ => {list(sprites.FONT['?'])},", "    }", "}", "",
        "/// Bigger icons as rows of 0/1.", "pub fn big(name: &str) -> &'static [&'static str] {", "    match name {"]
for name, rows in sorted(sprites.BIG.items()):
    out.append(f'        "{name}" => &[{", ".join(repr(r).replace(chr(39), chr(34)) for r in rows)}],')
out += ["        _ => &[],", "    }", "}", "",
        "/// Width in pixels of pixel text drawn with cells of `px` (5 wide + 1 gap per character).",
        "pub fn text_width(text: &str, px: i32) -> i32 {",
        "    (text.chars().count() as i32 * 6 - 1).max(0) * px",
        "}", ""]
path = os.path.join(os.path.dirname(__file__), "..", "crates", "blip-pet", "src", "pixfont.rs")
with open(path, "w", encoding="utf-8", newline="\n") as f:
    f.write("\n".join(out))
print("wrote", os.path.normpath(path))
