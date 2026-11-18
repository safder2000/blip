"""Render assets/blip.ico (and a PNG preview) from the pet sprite. Needs Pillow."""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from blip.sprites import GH, GW, build_pet  # noqa: E402


def render(size):
    g = build_pet("adult", "gold", "happy", 1)
    xs = [x for x, _ in g]
    ys = [y for _, y in g]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    w, h = x1 - x0 + 1, y1 - y0 + 1
    cell = max(1, size // max(w, h))
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    ox = (size - w * cell) // 2
    oy = (size - h * cell) // 2
    for (x, y), c in g.items():
        d.rectangle([ox + (x - x0) * cell, oy + (y - y0) * cell,
                     ox + (x - x0 + 1) * cell - 1, oy + (y - y0 + 1) * cell - 1], fill=c)
    return img


def main():
    out = ROOT / "assets"
    out.mkdir(exist_ok=True)
    sizes = [16, 24, 32, 48, 64, 128, 256]
    big = render(256)
    big.save(out / "blip.ico", sizes=[(s, s) for s in sizes],
             append_images=[render(s) for s in sizes if s != 256])
    render(512).save(out / "blip.png")
    print("wrote", out / "blip.ico")


if __name__ == "__main__":
    main()
