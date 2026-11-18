"""Tiny chiptune sound effects, synthesised in code (no audio files ship with Blip).

Each effect is rendered once per volume step as a small WAV in the data folder and played
with winsound on Windows or afplay on macOS.
"""
import math
import struct
import subprocess
import sys
import threading
import wave
from pathlib import Path

RATE = 22050
SFX_VERSION = 2

# (frequency Hz or 0 for a rest, seconds)
TUNES = {
    "ring": [(1319, .07), (0, .02), (1568, .07), (0, .02)] * 5 + [(0, .7)],
    "alarm": [(1760, .11), (0, .05)] * 4 + [(0, .45)],
    "ding": [(1047, .07), (1568, .16)],
    "done": [(784, .09), (988, .09), (1175, .09), (1568, .22)],
    "pop": [(660, .04), (990, .06)],
    "sad": [(523, .12), (466, .12), (392, .25)],
    "sample": [(523, .07), (659, .07), (784, .07), (1047, .14)],
}


def amplitude(volume):
    """0..100 -> 0..0.4, on a curve that sounds even to the ear."""
    v = max(0, min(100, volume)) / 100.0
    return 0.4 * v * v


def _samples(notes, amp):
    out = bytearray()
    for freq, dur in notes:
        n = int(RATE * dur)
        for i in range(n):
            if freq <= 0 or amp <= 0:
                v = 0.0
            else:
                env = min(1.0, i / 60.0, (n - i) / 200.0)   # click-free edges
                v = (1.0 if math.sin(2 * math.pi * freq * i / RATE) >= 0 else -1.0) * amp * env
            out += struct.pack("<h", int(v * 32767))
    return bytes(out)


def write_wav(path, notes, amp=0.22):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(_samples(notes, amp))


class Sound:
    def __init__(self, folder, enabled=lambda: True, volume=lambda: 60):
        self.dir = Path(folder) / f"sfx{SFX_VERSION}"
        self.enabled = enabled
        self.volume = volume
        self.looping = None
        self._proc = None
        self._gen = 0
        try:
            self.dir.mkdir(parents=True, exist_ok=True)
        except OSError:
            pass

    def path(self, name, volume):
        step = int(round(max(0, min(100, volume)) / 5.0) * 5)
        p = self.dir / f"{name}_{step}.wav"
        if not p.exists():
            try:
                write_wav(p, TUNES[name], amplitude(step))
            except OSError:
                return None
        return p

    def play(self, name, loop=False, volume=None, force=False):
        if not force and not self.enabled():
            return
        vol = self.volume() if volume is None else volume
        if vol <= 0:
            self.stop()
            return
        p = self.path(name, vol)
        if p is None:
            return
        self.looping = name if loop else None
        if sys.platform == "win32":
            import winsound
            flags = winsound.SND_FILENAME | winsound.SND_ASYNC | winsound.SND_NODEFAULT
            if loop:
                flags |= winsound.SND_LOOP
            try:
                winsound.PlaySound(str(p), flags)
            except RuntimeError:
                pass
        elif sys.platform == "darwin":
            self._afplay(p, loop)

    def preview(self, volume):
        """Play a short sample at this volume, even if sounds are switched off."""
        self.play("sample", volume=volume, force=True)

    def _afplay(self, path, loop):
        self._kill()
        self._gen += 1
        gen = self._gen

        def run():
            while gen == self._gen:
                try:
                    self._proc = subprocess.Popen(["afplay", str(path)])
                    self._proc.wait()
                except OSError:
                    return
                if not loop:
                    return
        threading.Thread(target=run, daemon=True).start()

    def _kill(self):
        p, self._proc = self._proc, None
        if p and p.poll() is None:
            try:
                p.terminate()
            except OSError:
                pass

    def stop(self):
        self.looping = None
        self._gen += 1
        if sys.platform == "win32":
            import winsound
            try:
                winsound.PlaySound(None, 0)
            except RuntimeError:
                pass
        else:
            self._kill()
