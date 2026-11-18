"""Voice calls between pets on the office network.

16 kHz mono, 20 ms frames, straight from one PC to the other over UDP (nothing leaves the
office network). Each call gets its own keys, derived from the office group key and a random
nonce; every packet is encrypted (BLAKE2b keystream) and authenticated (BLAKE2b MAC), so
other machines can neither listen in nor inject audio.
"""
import collections
import hashlib
import hmac
import os
import socket
import struct
import threading
import time

try:
    import sounddevice as sd
except Exception:      # optional: no audio library, no voice calls
    sd = None

RATE = 16000
FRAME = 320                    # samples per packet (20 ms)
MAGIC = b"BV1"
HEADER = struct.Struct(">3sBI")   # magic, direction, sequence
TAG = 8
PREBUFFER = 3                  # frames to collect before playback starts
MAX_BUFFER = 12
SILENCE = bytes(FRAME * 2)
IDLE_TIMEOUT = 15.0


def available():
    """True if this PC can do voice: audio library present and a mic + speaker exist."""
    if sd is None:
        return False
    try:
        sd.query_devices(kind="input")
        sd.query_devices(kind="output")
        return True
    except Exception:
        return False


def new_nonce():
    return os.urandom(16).hex()


def session_keys(group_key, call_id, nonce_hex):
    base = hmac.new(group_key, b"blip-voice|" + call_id.encode() + b"|" + bytes.fromhex(nonce_hex),
                    hashlib.sha256).digest()
    return (hmac.new(base, b"enc", hashlib.sha256).digest(),
            hmac.new(base, b"mac", hashlib.sha256).digest())


def _keystream(key, direction, seq, n):
    out = bytearray()
    block = 0
    while len(out) < n:
        out += hashlib.blake2b(struct.pack(">BIH", direction, seq, block), key=key, digest_size=64).digest()
        block += 1
    return bytes(out[:n])


def seal(keys, direction, seq, pcm):
    enc, mac = keys
    head = HEADER.pack(MAGIC, direction, seq)
    ks = _keystream(enc, direction, seq, len(pcm))
    ct = bytes(a ^ b for a, b in zip(pcm, ks))
    tag = hashlib.blake2b(head + ct, key=mac, digest_size=TAG).digest()
    return head + ct + tag


def open_packet(keys, packet):
    """(direction, seq, pcm) or None if the packet isn't ours or was tampered with."""
    enc, mac = keys
    if len(packet) < HEADER.size + TAG:
        return None
    head, ct, tag = packet[:HEADER.size], packet[HEADER.size:-TAG], packet[-TAG:]
    magic, direction, seq = HEADER.unpack(head)
    if magic != MAGIC:
        return None
    if not hmac.compare_digest(tag, hashlib.blake2b(head + ct, key=mac, digest_size=TAG).digest()):
        return None
    ks = _keystream(enc, direction, seq, len(ct))
    return direction, seq, bytes(a ^ b for a, b in zip(ct, ks))


def level(pcm):
    """Rough loudness 0..1 of an int16 frame."""
    if not pcm:
        return 0.0
    n = len(pcm) // 2
    vals = struct.unpack(f"<{n}h", pcm[:n * 2])
    return min(1.0, (sum(abs(v) for v in vals[::8]) / max(1, len(vals[::8]))) / 6000.0)


class VoiceSession:
    """One side of a call. role 0 = caller, 1 = the person who answered."""

    def __init__(self, keys, role, peer=None, on_end=None, audio=True):
        self.keys, self.role, self.peer = keys, role, peer
        self.on_end = on_end
        self.audio = audio and sd is not None
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("0.0.0.0", 0))
        self.port = self.sock.getsockname()[1]
        self.seq = 0
        self.rx = collections.deque()
        self.playing = False
        self.last_seq = -1
        self.last_rx = time.time()
        self.started = time.time()
        self.muted = False
        self.running = False
        self.level_in = self.level_out = 0.0
        self.received = 0
        self.inp = self.out = None
        self.error = None
        self.lock = threading.Lock()

    # ------------------------------------------------ lifecycle
    def start(self):
        self.running = True
        threading.Thread(target=self._recv_loop, daemon=True).start()
        threading.Thread(target=self._keepalive, daemon=True).start()
        if self.audio:
            try:
                self.out = sd.RawOutputStream(samplerate=RATE, channels=1, dtype="int16", blocksize=FRAME,
                                              callback=self._out_cb)
                self.inp = sd.RawInputStream(samplerate=RATE, channels=1, dtype="int16", blocksize=FRAME,
                                             callback=self._in_cb)
                self.out.start()
                self.inp.start()
            except Exception as e:      # no mic permission, device busy...
                self.error = str(e)
        return self

    def stop(self):
        if not self.running:
            return
        self.running = False
        for s in (self.inp, self.out):
            try:
                if s:
                    s.stop()
                    s.close()
            except Exception:
                pass
        try:
            self.sock.close()
        except OSError:
            pass

    def seconds(self):
        return time.time() - self.started

    # ------------------------------------------------ sending
    def send_frame(self, pcm):
        peer = self.peer
        if not peer or not self.running:
            return
        with self.lock:
            seq = self.seq
            self.seq += 1
        try:
            self.sock.sendto(seal(self.keys, self.role, seq, pcm), peer)
        except OSError:
            pass

    def _in_cb(self, data, frames, t, status):
        pcm = bytes(data)
        self.level_in = level(pcm)
        self.send_frame(SILENCE if self.muted else pcm)

    def _keepalive(self):
        """Keep the line open (and let the other side learn our address) even without a mic."""
        while self.running:
            if self.inp is None:
                self.send_frame(SILENCE[:64])
            if time.time() - self.last_rx > IDLE_TIMEOUT and self.received:
                self._end("lost")
                return
            time.sleep(0.5)

    # ------------------------------------------------ receiving
    def _recv_loop(self):
        while self.running:
            try:
                data, addr = self.sock.recvfrom(4096)
            except OSError:
                return
            got = open_packet(self.keys, data)
            if not got:
                continue
            direction, seq, pcm = got
            if direction == self.role:
                continue                   # our own direction reflected back: ignore
            self.last_rx = time.time()
            self.received += 1
            if self.peer is None:
                self.peer = addr           # learn where to send, from the first genuine packet
            if seq <= self.last_seq or len(pcm) < 2:
                continue
            self.last_seq = seq
            if len(pcm) == FRAME * 2:
                self.level_out = level(pcm)
                self.rx.append(pcm)
                while len(self.rx) > MAX_BUFFER:
                    self.rx.popleft()

    def _out_cb(self, outdata, frames, t, status):
        need = frames * 2
        if not self.playing and len(self.rx) >= PREBUFFER:
            self.playing = True
        chunk = b""
        if self.playing:
            try:
                chunk = self.rx.popleft()
            except IndexError:
                self.playing = False       # ran dry: build the buffer up again
        outdata[:] = (chunk + SILENCE)[:need].ljust(need, b"\0")

    def _end(self, why):
        self.stop()
        if self.on_end:
            self.on_end(why)
