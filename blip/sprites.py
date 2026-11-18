"""Pixel art: the pets, their props, and a 5x7 pixel font with icons. Plain data + functions."""

GW, GH = 32, 28          # pet sprite grid, in cells

COL = {
    "out": "#2b2340", "white": "#ffffff", "cheek": "#ff8fab", "tear": "#5ec8ff",
    "red": "#e5484d", "heart": "#ff4d6d", "z": "#6b7fd7", "spark": "#ffd23f",
    "bowl": "#c0703a", "berry": "#c2185b", "leaf": "#4caf50",
    "cookie": "#e0a458", "chip": "#5a3a1a", "pill": "#ff6b6b",
    "egg": "#fff3d6", "egg2": "#efd9a8", "spot": "#f4a6c0", "poop": "#8b5a2b",
    "alert": "#ff4d6d", "phone": "#4cc38a",
}
# body, shade, antenna tip
PALETTES = {
    "baby": ("#9be8c8", "#6cc9a5", "#ff8fab"),
    "child": ("#8fd3f0", "#5fb0d6", "#ff8fab"),
    "gold": ("#ffd166", "#e9a93c", "#ff5d73"),
    "sky": ("#8fb4ff", "#6688e0", "#ffd166"),
    "plum": ("#c3a6e8", "#9a7cc8", "#7ee0c3"),
    "sick": ("#c4dc9a", "#9cba70", "#9cba70"),
    "ghost": ("#eef1ff", "#c9d0f0", "#c9d0f0"),
    "mint": ("#9be8c8", "#6cc9a5", "#ff8fab"),
    "peach": ("#ffc29e", "#f09a6c", "#ff6f91"),
    "pink": ("#ffb8d6", "#ec8bb4", "#7ee0c3"),
    "snow": ("#f6f6fb", "#d6d6e6", "#ff8fab"),
    "cocoa": ("#d6ae86", "#ab805a", "#ffd166"),
    "lime": ("#cdea7a", "#a3c654", "#ff8fab"),
}
SPECIES = {"blip": "Blip", "kitty": "Kitty", "bunny": "Bunny", "bear": "Bear", "pup": "Pup",
           "chick": "Chick", "bot": "Bot"}
COLORS = ["auto", "mint", "sky", "gold", "plum", "peach", "pink", "snow", "cocoa", "lime"]

HEART = [(1, 0), (3, 0), (0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (1, 2), (2, 2), (3, 2), (2, 3)]
ZED = [(0, 0), (1, 0), (2, 0), (1, 1), (0, 2), (1, 2), (2, 2)]
SPARK = [(1, 0), (0, 1), (1, 1), (2, 1), (1, 2)]
BANG = [(0, 0), (0, 1), (0, 2), (0, 4)]
NOTE = [(1, 0), (2, 0), (1, 1), (1, 2), (0, 3), (1, 3)]
FOOD = {
    "meal": [(2, 0, "leaf"), (1, 0, "berry"), (3, 0, "berry"),
             (0, 1, "berry"), (1, 1, "berry"), (2, 1, "berry"), (3, 1, "berry"), (4, 1, "berry"),
             (0, 2, "bowl"), (1, 2, "bowl"), (2, 2, "bowl"), (3, 2, "bowl"), (4, 2, "bowl"),
             (1, 3, "bowl"), (2, 3, "bowl"), (3, 3, "bowl")],
    "snack": [(1, 0, "cookie"), (2, 0, "cookie"), (0, 1, "cookie"), (1, 1, "chip"),
              (2, 1, "cookie"), (3, 1, "cookie"), (0, 2, "cookie"), (1, 2, "cookie"),
              (2, 2, "chip"), (3, 2, "cookie"), (1, 3, "cookie"), (2, 3, "cookie")],
    "pill": [(0, 2, "pill"), (1, 2, "pill"), (2, 2, "white"), (3, 2, "white"),
             (0, 3, "pill"), (1, 3, "pill"), (2, 3, "white"), (3, 3, "white")],
}
POOP = ["...oo...",
        "..obbo..",
        "..obbbo.",
        ".obbbbo.",
        "obbbbbbo",
        ".oooooo."]


# ---------------------------------------------------------------- shape helpers
def _ellipse(cx, cy, rx, ry):
    return {(x, y) for y in range(GH) for x in range(GW)
            if ((x + 0.5 - cx) / rx) ** 2 + ((y + 0.5 - cy) / ry) ** 2 <= 1.0}


def _rrect(cx, cy, rx, ry, corner=2):
    out = set()
    for y in range(GH):
        for x in range(GW):
            dx, dy = abs(x + 0.5 - cx), abs(y + 0.5 - cy)
            if dx <= rx and dy <= ry:
                ex, ey = dx - (rx - corner), dy - (ry - corner)
                if ex > 0 and ey > 0 and ex * ex + ey * ey > corner * corner:
                    continue
                out.add((x, y))
    return out


def _tri(apex_x, apex_y, base_cx, base_y, half):
    """Cells of a triangle from an apex down to a base row centred at base_cx."""
    out = set()
    h = max(1, base_y - apex_y)
    for i in range(h + 1):
        t = i / h
        mx = apex_x + (base_cx - apex_x) * t
        hw = half * t
        y = apex_y + i
        for x in range(int(mx - hw - 2), int(mx + hw + 2)):
            if abs(x + 0.5 - mx) <= hw + 0.5:
                out.add((x, y))
    return out


def _outline(g, shape):
    for x, y in list(shape):
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            p = (x + dx, y + dy)
            if p not in shape and 0 <= p[0] < GW and 0 <= p[1] < GH:
                g.setdefault(p, COL["out"])


def darker(hexcol, f=0.82):
    h = hexcol.lstrip("#")
    r, g, b = (int(h[i:i + 2], 16) for i in (0, 2, 4))
    return "#%02x%02x%02x" % (int(r * f), int(g * f), int(b * f))


# ---------------------------------------------------------------- egg
def build_egg(frame, progress):
    wob = 0
    if progress > 0.8:
        wob = (1, -1)[frame % 2]
    elif progress > 0.3:
        wob = (0, 1, 0, -1)[frame % 4]
    cx, rx, ry = GW / 2 + wob, 6.0, 7.5
    cy = (GH - 1) - ry
    shape = _ellipse(cx, cy, rx, ry)
    g = {p: (COL["egg2"] if p[1] + 0.5 > cy + ry * 0.5 else COL["egg"]) for p in shape}
    ci, cj = int(cx), int(cy)
    for dx, dy in ((-3, -3), (-2, -3), (2, 1), (3, 1), (2, 2), (-2, 3), (-1, 3)):
        if (ci + dx, cj + dy) in shape:
            g[(ci + dx, cj + dy)] = COL["spot"]
    hl = (int(cx - rx * 0.5), int(cy - ry * 0.5))
    if hl in shape:
        g[hl] = COL["white"]
    if progress > 0.6:
        for i, dx in enumerate(range(-4, 4)):
            p = (ci + dx, cj - 1 + (i % 2))
            if p in shape:
                g[p] = COL["out"]
    _outline(g, shape)
    return g


# ---------------------------------------------------------------- pets
def build_pet(stage, palette, mood, frame, facing=0, squash=False, blink=False, species="blip"):
    """Return {(x, y): colour} for the pet on a GW x GH grid, feet on the bottom row.

    mood: normal happy sad eat sleep sick held alert
    """
    rx, ry = {"baby": (5.0, 4.5), "child": (7.0, 6.0), "adult": (9.0, 7.5)}[stage]
    k = {"baby": 0, "child": 1, "adult": 2}[stage]
    if squash:
        rx, ry = rx + 0.7, ry - 0.8
    if species not in SPECIES:
        species = "blip"
    base, shade, tip = PALETTES[palette]
    O = COL["out"]
    cx = GW / 2
    cy = (GH - 1) - ry
    shape = _rrect(cx, cy, rx, ry, 2 + k) if species == "bot" else _ellipse(cx, cy, rx, ry)
    if palette == "ghost":  # wavy hem
        bottom = max(y for _, y in shape)
        shape = {(x, y) for x, y in shape if not (y == bottom and (x + frame) % 3 == 0)}
    top = min(y for _, y in shape)
    g = {p: (shade if p[1] + 0.5 > cy + ry * 0.45 else base) for p in shape}

    # ears and other bits that share the outline
    extra, inner = {}, {}
    if species == "kitty":
        h, half = 2 + k, 1.0 + 0.6 * k
        for s in (-1, 1):
            ecx = cx + s * rx * 0.55
            for p in _tri(ecx + s * 1.0, top - h, ecx, top + 1, half):
                extra[p] = base
            if k:
                for p in _tri(ecx + s * 1.0, top - h + 1, ecx, top + 1, half - 1.0):
                    if top - h < p[1] < top + 1:
                        inner[p] = COL["cheek"]
        if k:  # tail
            side = 1 if facing <= 0 else -1
            tx = int(cx + side * rx) - (0 if side > 0 else 1)
            for i in range(2 + k):
                extra[(tx + side * (i // 2), GH - 3 - i)] = shade
    elif species == "bunny":
        length, erx = 4 + 2 * k, 1.2 + 0.4 * k
        for s in (-1, 1):
            droop = 0.5 if (s > 0 and frame % 8 < 4) else 0.0
            ecx = cx + s * (max(1.6, rx * 0.35) + droop)
            for p in _ellipse(ecx, top - length / 2 + 1, erx, length / 2):
                extra[p] = base
                if abs(p[0] + 0.5 - ecx) < erx * 0.5 and p[1] < top and k:
                    inner[p] = COL["cheek"]
    elif species == "bear":
        r = 1.6 + 0.5 * k
        for s in (-1, 1):
            ecx = cx + s * rx * 0.72
            for p in _ellipse(ecx, top + 1, r, r):
                extra[p] = base
                if (p[0] + 0.5 - ecx) ** 2 + (p[1] + 0.5 - top - 1) ** 2 < (r * 0.55) ** 2:
                    inner[p] = shade
    elif species == "pup":
        erx, ery = 1.1 + 0.45 * k, 2.4 + 1.1 * k
        ear_col = darker(shade, 0.85)
        swing = 0.4 if frame % 4 in (1, 2) else 0.0
        for s in (-1, 1):
            ecx = cx + s * (rx * 0.78 + swing)
            for p in _ellipse(ecx, top + ery * 0.9, erx, ery):
                extra[p] = ear_col
    elif species == "chick":
        wrx, wry = 1.0 + 0.35 * k, 1.7 + 0.5 * k
        flap = -1 if frame % 4 == 0 else 0
        for s in (-1, 1):
            for p in _ellipse(cx + s * (rx - 0.6), cy + ry * 0.2 + flap, wrx, wry):
                extra[p] = shade
    full = set(shape) | set(extra)
    for p, c in extra.items():
        if p not in shape or species in ("pup", "chick"):
            g[p] = c
    if species == "bot":
        hx, hy = int(cx - rx + 1), int(cy - ry + 1)
    else:
        hx, hy = int(cx - rx * 0.55), int(cy - ry * 0.55)
    for p in ((hx, hy), (hx + 1, hy), (hx, hy + 1)):
        if p in shape and p not in extra:
            g[p] = COL["white"]
    for p, c in inner.items():
        if p in full:
            g[p] = c
    _outline(g, full)

    # antenna / tuft
    ax = 16 if facing > 0 else 15
    sway = (0, 1, 0, -1)[frame % 4]
    if species in ("blip", "bot"):
        if stage == "baby":
            g[(ax, top - 2)] = O
            g[(ax + 1, top - 3)] = O
        else:
            n = 2 if stage == "child" else 3
            for i in range(n):
                g[(ax + (sway if i == n - 1 else 0), top - 2 - i)] = O
            tx, ty = ax + sway, top - 2 - n
            tips = [(tx, ty)] if stage == "child" else [(tx, ty), (tx + 1, ty), (tx, ty - 1), (tx + 1, ty - 1)]
            for p in tips:
                g[p] = COL["alert"] if mood == "alert" and frame % 2 else tip
            for x, y in tips:
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    q = (x + dx, y + dy)
                    if q not in tips and 0 <= q[0] < GW and q[1] >= 0:
                        g.setdefault(q, O)
    elif species == "chick":
        for p in ((ax, top - 2), (ax - 1, top - 3), (ax + 1, top - 3), (ax + 1 + sway % 2, top - 4)):
            g[p] = O

    # face
    fc = O
    if species == "bot":
        fc = "#7df9ff"
        for x in range(int(cx - rx + 2), int(cx + rx - 1)):
            for y in range(int(cy - ry + 2 + (1 if k else 0)), int(cy + ry * 0.35) + 1):
                if (x, y) in shape:
                    g[(x, y)] = "#263049"
        for s in (-1, 1):
            bolt = (int(cx + s * rx) - (1 if s > 0 else 0), int(cy + ry * 0.55))
            if bolt in shape:
                g[bolt] = "#c9c9d6"

    def put(cells, col=None):
        col = col or fc
        for p in cells:
            if p in shape:
                g[p] = col

    f = 1 if facing > 0 else -1 if facing < 0 else 0
    d = max(3, int(rx * 0.45))
    ey = int(cy - ry * 0.15)
    eh = 1 if stage == "baby" else 2
    L, R = 16 - d + f, 15 + d + f
    big = stage == "adult" and species != "bot"
    my = ey + eh + (1 if stage == "baby" else 2)
    m = 15 + f

    if species == "bear":   # muzzle
        for p in _ellipse(16 + f, my + 0.8, 2.2 + 0.7 * k, 1.4 + 0.4 * k):
            if p in shape:
                g[p] = "#fff1df"

    for ex, side in ((L, -1), (R, 1)):
        if mood == "happy":
            put([(ex - 1, ey + 1), (ex, ey), (ex + 1, ey + 1)])
        elif mood == "sleep" or (blink and mood in ("normal", "sad")):
            put([(ex - 1, ey + eh - 1), (ex, ey + eh - 1), (ex + 1, ey + eh - 1)])
        elif mood == "sick":
            put([(ex - 1, ey - 1), (ex + 1, ey - 1), (ex, ey), (ex - 1, ey + 1), (ex + 1, ey + 1)])
        elif mood == "held":
            kk = -side
            put([(ex + kk, ey - 1), (ex, ey), (ex + kk, ey + 1)])
        elif big or mood == "alert":
            x0 = ex if side < 0 else ex - 1
            put([(x0, ey), (x0 + 1, ey), (x0, ey + 1), (x0 + 1, ey + 1)])
            if species != "bot":
                put([(x0 if side < 0 else x0 + 1, ey)], COL["white"])
        else:
            put([(ex, ey + i) for i in range(eh)])
    if mood == "sad":
        put([(L, ey + eh + frame % 3)], COL["tear"])

    if mood in ("normal", "happy", "eat", "alert") and species != "bot":
        cyk = ey + eh
        put([(L - (2 if big else 1), cyk), (R + (2 if big else 1), cyk)], COL["cheek"])

    if species == "chick":   # beak instead of a mouth
        orange, dark = "#ffa53a", "#e07b1a"
        put([(m, my), (m + 1, my)], orange)
        if mood in ("happy", "eat", "held", "alert") and frame % 2 == 0:
            put([(m, my + 1), (m + 1, my + 1)], COL["red"])
            put([(m, my + 2), (m + 1, my + 2)], dark)
        else:
            put([(m, my + 1), (m + 1, my + 1)], dark)
        return g
    if species in ("pup", "bear") and stage != "baby":   # nose
        put([(m, my - 1), (m + 1, my - 1)], O)
        my += 1
    if mood == "happy":
        put([(m - 1, my), (m + 2, my), (m, my + 1), (m + 1, my + 1)])
        put([(m, my), (m + 1, my)], COL["red"] if species != "bot" else fc)
        if species == "pup":
            put([(m + 1, my + 2)], "#ff7a93")
    elif mood == "sad":
        put([(m - 1, my + 1), (m, my), (m + 1, my), (m + 2, my + 1)])
    elif mood == "eat":
        if frame % 2 == 0:
            put([(m, my), (m + 1, my), (m, my + 1), (m + 1, my + 1)])
        else:
            put([(m, my), (m + 1, my)])
    elif mood == "sleep":
        put([(m, my)])
    elif mood == "sick":
        put([(m - 1, my + 1), (m, my), (m + 1, my + 1), (m + 2, my)])
    elif mood in ("held", "alert"):
        put([(m, my), (m + 1, my), (m, my + 1), (m + 1, my + 1)])
        if mood == "alert" and species != "bot":
            put([(m, my + 1), (m + 1, my + 1)], COL["red"])
    elif species == "kitty":
        put([(m - 1, my), (m, my + 1), (m + 1, my), (m + 2, my + 1)])
    else:
        put([(m - 1, my), (m, my + 1), (m + 1, my + 1), (m + 2, my)])
    return g


# ---------------------------------------------------------------- 5x7 pixel font
_F = {
    "0": "0E 11 13 15 19 11 0E", "1": "04 0C 04 04 04 04 0E", "2": "0E 11 01 02 04 08 1F",
    "3": "1F 02 04 02 01 11 0E", "4": "02 06 0A 12 1F 02 02", "5": "1F 10 1E 01 01 11 0E",
    "6": "06 08 10 1E 11 11 0E", "7": "1F 01 02 04 08 08 08", "8": "0E 11 11 0E 11 11 0E",
    "9": "0E 11 11 0F 01 02 0C",
    "A": "0E 11 11 11 1F 11 11", "B": "1E 11 11 1E 11 11 1E", "C": "0E 11 10 10 10 11 0E",
    "D": "1C 12 11 11 11 12 1C", "E": "1F 10 10 1E 10 10 1F", "F": "1F 10 10 1E 10 10 10",
    "G": "0E 11 10 17 11 11 0F", "H": "11 11 11 1F 11 11 11", "I": "0E 04 04 04 04 04 0E",
    "J": "07 02 02 02 02 12 0C", "K": "11 12 14 18 14 12 11", "L": "10 10 10 10 10 10 1F",
    "M": "11 1B 15 15 11 11 11", "N": "11 11 19 15 13 11 11", "O": "0E 11 11 11 11 11 0E",
    "P": "1E 11 11 1E 10 10 10", "Q": "0E 11 11 11 15 12 0D", "R": "1E 11 11 1E 14 12 11",
    "S": "0F 10 10 0E 01 01 1E", "T": "1F 04 04 04 04 04 04", "U": "11 11 11 11 11 11 0E",
    "V": "11 11 11 11 11 0A 04", "W": "11 11 11 15 15 15 0A", "X": "11 11 0A 04 0A 11 11",
    "Y": "11 11 11 0A 04 04 04", "Z": "1F 01 02 04 08 10 1F",
    " ": "00 00 00 00 00 00 00", ":": "00 0C 0C 00 0C 0C 00", ".": "00 00 00 00 00 0C 0C",
    "!": "04 04 04 04 04 00 04", "?": "0E 11 01 02 04 00 04", "-": "00 00 00 1F 00 00 00",
    "+": "00 04 04 1F 04 04 00", "/": "00 01 02 04 08 10 00", "'": "0C 04 08 00 00 00 00",
    ",": "00 00 00 00 0C 04 08", "(": "02 04 08 08 08 04 02", ")": "08 04 02 02 02 04 08",
    "%": "18 19 02 04 08 13 03", "#": "0A 0A 1F 0A 1F 0A 0A", ">": "08 04 02 01 02 04 08",
    "<": "02 04 08 10 08 04 02", "&": "0C 12 14 08 15 12 0D", "_": "00 00 00 00 00 00 1F",
    "=": "00 00 1F 00 1F 00 00", "@": "0E 11 17 15 17 10 0E",
    "\x01": "1F 11 0A 04 0A 11 1F",   # hourglass
    "\x02": "04 0E 0E 0E 1F 00 04",   # bell
    "\x03": "18 1C 0C 06 07 03 00",   # phone handset
    "\x04": "00 1F 1B 15 11 1F 00",   # envelope
    "\x05": "00 01 03 16 1C 08 00",   # check
    "\x06": "0E 04 0E 15 17 11 0E",   # stopwatch
    "\x07": "00 0A 1F 1F 0E 04 00",   # heart
    "\x08": "04 15 0E 1F 0E 15 04",   # sparkle / task
}
_EXTRA = {
    "meal": "0A 15 0A 00 1F 1F 0E", "snack": "0E 1B 1F 17 1F 0E 00", "play": "00 0E 17 1F 1F 1F 0E",
    "clean": "0C 12 12 0C 00 03 03", "pill": "06 0F 1F 1E 0C 00 00", "moon": "0E 1C 18 18 18 1C 0E",
    "gear": "04 15 0E 1B 0E 15 04", "people": "0A 0A 00 1B 1B 1B 00", "power": "04 15 15 11 11 0E 00",
    "paw": "0A 15 00 0E 1F 1F 0E", "walk": "18 18 00 03 03 00 18", "info": "04 00 0C 04 04 0E 00",
    "globe": "0E 15 1F 15 1F 15 0E", "dnd": "0E 11 11 1F 11 11 0E", "update": "04 04 15 0E 04 00 1F",
    "chevron": "00 08 04 02 04 08 00", "plus": "00 04 04 1F 04 04 00", "trash": "04 1F 00 1F 15 15 0E",
    "pause": "1B 1B 1B 1B 1B 1B 00", "playtri": "08 0C 0E 0F 0E 0C 08", "reset": "0D 13 17 10 10 11 0E",
    "x": "11 0A 04 0A 11 00 00", "open": "0F 03 05 09 10 00 00", "user": "0E 0E 0E 00 1F 1F 1F",
    "speaker": "02 06 1E 1E 1E 06 02", "mic": "0E 0E 0E 0E 11 0E 04", "micoff": "1E 0F 0E 06 13 0E 05",
    "hangup": "00 0E 1F 1B 11 00 00",
}
for _i, (_n, _v) in enumerate(_EXTRA.items()):
    _F[chr(0xE000 + _i)] = _v
FONT = {k: [int(b, 16) for b in v.split()] for k, v in _F.items()}
ICON = {"hourglass": "\x01", "bell": "\x02", "phone": "\x03", "mail": "\x04", "check": "\x05",
        "stopwatch": "\x06", "heart": "\x07", "task": "\x08"}
ICON.update({_n: chr(0xE000 + _i) for _i, _n in enumerate(_EXTRA)})


def glyph(ch):
    return FONT.get(ch) or FONT.get(ch.upper()) or FONT["?"]


def text_cells(text):
    """[(x, y)] cells for text, 6 cells per char (5 + gap), 7 rows."""
    out = []
    for i, ch in enumerate(text):
        for y, bits in enumerate(glyph(ch)):
            for x in range(5):
                if bits & (0x10 >> x):
                    out.append((i * 6 + x, y))
    return out


def text_width(text, px):
    return max(0, len(text) * 6 - 1) * px


def draw_text(canvas, x, y, text, px, color, tags=()):
    """Draw pixel text with its top-left at (x, y); returns the width in screen pixels."""
    for cx, cy in text_cells(text):
        canvas.create_rectangle(x + cx * px, y + cy * px, x + (cx + 1) * px, y + (cy + 1) * px,
                                fill=color, width=0, tags=tags)
    return text_width(text, px)


def draw_sprite(canvas, g, x, y, px, tags=()):
    """Draw a sprite dict with cell (0, 0) at (x, y)."""
    for (cx, cy), c in g.items():
        canvas.create_rectangle(x + cx * px, y + cy * px, x + (cx + 1) * px, y + (cy + 1) * px,
                                fill=c, width=0, tags=tags)
