"""Opus voice codec through ctypes (libopus ships inside the Windows and Mac builds).

20 ms of 16 kHz mono speech: 640 bytes raw, about 40-60 bytes as Opus. Calls through a
server use it; without libopus Blip falls back to raw audio.
"""
import ctypes
import ctypes.util
import os
import sys

RATE, CHANNELS, FRAME = 16000, 1, 320
APPLICATION_VOIP = 2048
MAX_PACKET = 400

_lib = None
_tried = False

NAMES = ["opus.dll", "libopus-0.dll", "libopus-0.x64.dll", "libopus.0.dylib", "libopus.dylib", "libopus.so.0"]


def _dirs():
    here = os.path.dirname(os.path.abspath(__file__))
    out = []
    if os.environ.get("BLIP_OPUS"):
        out.append(os.environ["BLIP_OPUS"])
    base = getattr(sys, "_MEIPASS", None)
    if base:
        out += [base, os.path.join(base, "opus")]
    out += [os.path.join(here, "_opus"), os.path.join(os.path.dirname(here), "vendor")]
    return out


def load():
    """The libopus handle, or None."""
    global _lib, _tried
    if _tried:
        return _lib
    _tried = True
    cands = []
    for d in _dirs():
        if os.path.isfile(d):
            cands.append(d)
        else:
            cands += [os.path.join(d, n) for n in NAMES]
    found = ctypes.util.find_library("opus")
    if found:
        cands.append(found)
    for c in cands:
        if c and (os.path.isfile(c) or c == found):
            try:
                lib = ctypes.CDLL(c)
                lib.opus_encoder_create.restype = ctypes.c_void_p
                lib.opus_encoder_create.argtypes = [ctypes.c_int32, ctypes.c_int, ctypes.c_int,
                                                    ctypes.POINTER(ctypes.c_int)]
                lib.opus_encode.restype = ctypes.c_int32
                lib.opus_encode.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int,
                                            ctypes.c_char_p, ctypes.c_int32]
                lib.opus_encoder_destroy.argtypes = [ctypes.c_void_p]
                lib.opus_decoder_create.restype = ctypes.c_void_p
                lib.opus_decoder_create.argtypes = [ctypes.c_int32, ctypes.c_int, ctypes.POINTER(ctypes.c_int)]
                lib.opus_decode.restype = ctypes.c_int
                lib.opus_decode.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int32,
                                            ctypes.c_char_p, ctypes.c_int, ctypes.c_int]
                lib.opus_decoder_destroy.argtypes = [ctypes.c_void_p]
                _lib = lib
                return lib
            except (OSError, AttributeError):
                continue
    return None


def available():
    return load() is not None


class Encoder:
    def __init__(self):
        lib = load()
        if not lib:
            raise RuntimeError("libopus not found")
        err = ctypes.c_int(0)
        self.lib, self.h = lib, lib.opus_encoder_create(RATE, CHANNELS, APPLICATION_VOIP, ctypes.byref(err))
        if err.value != 0 or not self.h:
            raise RuntimeError(f"opus encoder error {err.value}")
        self.out = ctypes.create_string_buffer(MAX_PACKET)

    def encode(self, pcm):
        """640 bytes of int16 PCM (one 20 ms frame) -> Opus packet bytes."""
        n = self.lib.opus_encode(self.h, pcm, FRAME, self.out, MAX_PACKET)
        if n < 0:
            raise RuntimeError(f"opus encode error {n}")
        return self.out.raw[:n]

    def close(self):
        if self.h:
            self.lib.opus_encoder_destroy(self.h)
            self.h = None


class Decoder:
    def __init__(self):
        lib = load()
        if not lib:
            raise RuntimeError("libopus not found")
        err = ctypes.c_int(0)
        self.lib, self.h = lib, lib.opus_decoder_create(RATE, CHANNELS, ctypes.byref(err))
        if err.value != 0 or not self.h:
            raise RuntimeError(f"opus decoder error {err.value}")
        self.out = ctypes.create_string_buffer(FRAME * 2 * 6)

    def decode(self, packet):
        """Opus packet -> 640 bytes of PCM. None/empty packet = fill a lost frame."""
        data = packet or None
        n = self.lib.opus_decode(self.h, data, len(packet) if packet else 0, self.out,
                                 FRAME * 6 if packet else FRAME, 0)
        if n < 0:
            return None
        return self.out.raw[:n * 2]

    def close(self):
        if self.h:
            self.lib.opus_decoder_destroy(self.h)
            self.h = None
