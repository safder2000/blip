"""Tiny chiptune sound effects, generated as square-wave WAVs on first run."""
import math
import struct
import sys
import wave
from pathlib import Path

RATE = 22050
SFX_VERSION = 1

# (frequency Hz or 0 for a rest, seconds)
TUNES = {
    "ring": [(1319, .07), (0, .02), (1568, .07), (0, .02)] * 5 + [(0, .7)],
    "alarm": [(1760, .11), (0, .05)] * 4 + [(0, .45)],
    "ding": [(1047, .07), (1568, .16)],
    "done": [(784, .09), (988, .09), (1175, .09), (1568, .22)],
    "pop": [(660, .04), (990, .06)],
    "sad": [(523, .12), (466, .12), (392, .25)],
}


def _samples(notes, vol=0.22):
    out = bytearray()
    for freq, dur in notes:
        n = int(RATE * dur)
        for i in range(n):
            if freq <= 0:
                v = 0.0
            else:
                env = min(1.0, i / 60.0, (n - i) / 200.0)   # click-free edges
                v = (1.0 if math.sin(2 * math.pi * freq * i / RATE) >= 0 else -1.0) * vol * env
            out += struct.pack("<h", int(v * 32767))
    return bytes(out)


def write_wav(path, notes):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(_samples(notes))


class Sound:
    def __init__(self, folder, enabled=lambda: True):
        self.dir = Path(folder) / f"sfx{SFX_VERSION}"
        self.enabled = enabled
        self.looping = None
        try:
            self.dir.mkdir(parents=True, exist_ok=True)
            for name, notes in TUNES.items():
                p = self.dir / f"{name}.wav"
                if not p.exists():
                    write_wav(p, notes)
        except OSError:
            pass

    def play(self, name, loop=False):
        if sys.platform != "win32" or not self.enabled():
            return
        import winsound
        flags = winsound.SND_FILENAME | winsound.SND_ASYNC | winsound.SND_NODEFAULT
        if loop:
            flags |= winsound.SND_LOOP
        try:
            winsound.PlaySound(str(self.dir / f"{name}.wav"), flags)
            self.looping = name if loop else None
        except RuntimeError:
            pass

    def stop(self):
        self.looping = None
        if sys.platform == "win32":
            import winsound
            try:
                winsound.PlaySound(None, 0)
            except RuntimeError:
                pass
