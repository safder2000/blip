"""Just enough WebSocket (RFC 6455) for Blip: the server side for blip.server, the client side
for pets. Standard library only, so the server runs anywhere Python does.

A WebSocket is what lets Blip go through Cloudflare Tunnel, Render, Railway, nginx/Caddy and
other web front doors; plain TCP would not.
"""
import base64
import hashlib
import os
import socket
import ssl
import struct
import threading

GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
MAX_MESSAGE = 256 * 1024


class Closed(Exception):
    pass


def accept_key(key):
    return base64.b64encode(hashlib.sha1((key + GUID).encode()).digest()).decode()


def read_exact(sock, n):
    buf = bytearray()
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise Closed()
        buf += chunk
    return bytes(buf)


def read_http_head(sock, first=b"", limit=16384):
    """Read up to the blank line ending an HTTP head. Returns (head_text, leftover_bytes)."""
    buf = bytearray(first)
    while b"\r\n\r\n" not in buf:
        if len(buf) > limit:
            raise Closed()
        chunk = sock.recv(4096)
        if not chunk:
            raise Closed()
        buf += chunk
    head, _, rest = bytes(buf).partition(b"\r\n\r\n")
    return head.decode("latin-1"), rest


def parse_headers(head):
    lines = head.split("\r\n")
    hdrs = {}
    for line in lines[1:]:
        k, _, v = line.partition(":")
        hdrs[k.strip().lower()] = v.strip()
    return lines[0], hdrs


class Conn:
    """A WebSocket connection over an already-connected socket. Thread-safe send."""

    def __init__(self, sock, client, leftover=b""):
        self.sock, self.client = sock, client      # clients mask what they send
        self.buf = bytearray(leftover)
        self.lock = threading.Lock()
        self.closed = False

    def _read(self, n):
        while len(self.buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise Closed()
            self.buf += chunk
        out = bytes(self.buf[:n])
        del self.buf[:n]
        return out

    def _frame(self, opcode, payload):
        n = len(payload)
        head = bytearray([0x80 | opcode])
        mask_bit = 0x80 if self.client else 0
        if n < 126:
            head.append(mask_bit | n)
        elif n < 65536:
            head.append(mask_bit | 126)
            head += struct.pack(">H", n)
        else:
            head.append(mask_bit | 127)
            head += struct.pack(">Q", n)
        if self.client:
            mask = os.urandom(4)
            head += mask
            payload = bytes(b ^ mask[i & 3] for i, b in enumerate(payload)) if n < 4096 else _xor_mask(payload, mask)
        return bytes(head) + payload

    def send(self, data, opcode=None):
        if isinstance(data, str):
            data, op = data.encode("utf-8"), 1
        else:
            op = 2
        frame = self._frame(opcode or op, data)
        with self.lock:
            if self.closed:
                raise Closed()
            try:
                self.sock.sendall(frame)
            except OSError:
                self.closed = True
                raise Closed()

    def recv(self):
        """Next text/binary message (str or bytes). Answers pings. Raises Closed."""
        parts, kind = [], None
        while True:
            b0, b1 = self._read(2)
            fin, opcode = b0 & 0x80, b0 & 0x0F
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack(">H", self._read(2))[0]
            elif n == 127:
                n = struct.unpack(">Q", self._read(8))[0]
            if n > MAX_MESSAGE:
                raise Closed()
            mask = self._read(4) if b1 & 0x80 else None
            payload = self._read(n)
            if mask:
                payload = _xor_mask(payload, mask)
            if opcode == 8:
                try:
                    self.send(b"", opcode=8)
                except Closed:
                    pass
                raise Closed()
            if opcode == 9:
                try:
                    self.send(payload, opcode=10)
                except Closed:
                    pass
                continue
            if opcode == 10:
                continue
            if opcode in (1, 2):
                kind, parts = opcode, [payload]
            elif opcode == 0 and kind:
                parts.append(payload)
            else:
                continue
            if sum(len(p) for p in parts) > MAX_MESSAGE:
                raise Closed()
            if fin:
                data = b"".join(parts)
                return data.decode("utf-8", "replace") if kind == 1 else data

    def close(self):
        if not self.closed:
            try:
                self.send(b"", opcode=8)
            except Closed:
                pass
        self.closed = True
        try:
            self.sock.close()
        except OSError:
            pass


def _xor_mask(payload, mask):
    n = len(payload)
    m = (mask * (n // 4 + 1))[:n]
    return (int.from_bytes(payload, "big") ^ int.from_bytes(m, "big")).to_bytes(n, "big") if n else b""


def server_handshake(sock, head, leftover):
    """Finish an HTTP Upgrade we already read. Returns a Conn, or None if it wasn't a WebSocket."""
    _, hdrs = parse_headers(head)
    key = hdrs.get("sec-websocket-key")
    if "websocket" not in hdrs.get("upgrade", "").lower() or not key:
        return None
    sock.sendall(("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                  f"Sec-WebSocket-Accept: {accept_key(key)}\r\n\r\n").encode())
    return Conn(sock, client=False, leftover=leftover)


def connect(url, timeout=8, ssl_context=None):
    """Open ws:// or wss:// url. Returns a Conn (blocking reads)."""
    scheme, _, rest = url.partition("://")
    hostport, slash, path = rest.partition("/")
    path = "/" + path if slash else "/"
    secure = scheme == "wss"
    host, _, port = hostport.rpartition(":") if hostport.count(":") == 1 else (hostport, "", "")
    host = host or hostport
    port = int(port) if port else (443 if secure else 80)
    raw = socket.create_connection((host, port), timeout=timeout)
    try:
        if secure:
            ctx = ssl_context or ssl.create_default_context()
            raw = ctx.wrap_socket(raw, server_hostname=host)
        key = base64.b64encode(os.urandom(16)).decode()
        host_hdr = host if port in (80, 443) else f"{host}:{port}"
        raw.sendall((f"GET {path} HTTP/1.1\r\nHost: {host_hdr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                     f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n"
                     "User-Agent: Blip\r\n\r\n").encode())
        head, leftover = read_http_head(raw)
        status, hdrs = parse_headers(head)
        if " 101 " not in status + " " or hdrs.get("sec-websocket-accept") != accept_key(key):
            raise OSError(f"not a Blip server ({status.strip()[:60]})")
        raw.settimeout(None)
        return Conn(raw, client=True, leftover=leftover)
    except Exception:
        raw.close()
        raise
