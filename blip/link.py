"""Connecting a pet to a Blip server (see blip/server.py): reading the link people paste,
end-to-end encryption with the team code, and the reconnecting WebSocket connection.

Links people can paste:
    https://blip.example.com/#team=3f9c...        invite link (server + team code)
    blip.example.com                              a domain: wss://blip.example.com/ws
    192.168.1.200:30620  or  http://host:port     an IP/port (plain ws://)
    ws://... or wss://...                         spelled out
"""
import base64
import hashlib
import hmac
import ipaddress
import json
import os
import re
import secrets
import threading
import time

from . import ws

DEFAULT_PORT = 47620


def new_team_code():
    return secrets.token_hex(12)


def parse(text):
    """{'url': ws(s)://.../ws, 'team': code-or-'', 'web': http(s)://host for invite links} or None."""
    text = (text or "").strip()
    if not text:
        return None
    team = ""
    m = re.search(r"[#&?]team=([A-Za-z0-9_-]{4,64})", text)
    if m:
        team = m.group(1)
    base = re.split(r"[#?]", text, maxsplit=1)[0].rstrip("/")
    scheme, sep, rest = base.partition("://")
    if not sep:
        scheme, rest = "", base
    hostport = rest.split("/", 1)[0]
    if not hostport:
        return None
    host, port = hostport, ""
    if hostport.count(":") == 1:
        host, port = hostport.split(":")
        if not port.isdigit():
            return None
    scheme = scheme.lower()
    if scheme in ("https", "wss", "blip"):
        secure = True
    elif scheme in ("http", "ws"):
        secure = False
    elif scheme:
        return None
    else:                     # bare address: IPs and host:port are plain, domains are https
        try:
            ipaddress.ip_address(host)
            is_ip = True
        except ValueError:
            is_ip = host == "localhost" or "." not in host
        secure = not (is_ip or port)
        if not secure and not port:
            port = str(DEFAULT_PORT)
    hp = f"{host}:{port}" if port else host
    return {"url": f"{'wss' if secure else 'ws'}://{hp}/ws", "team": team,
            "web": f"{'https' if secure else 'http'}://{hp}", "host": host}


def invite_link(info, team):
    """The link to share: the plain address for the server's shared team, address#team=... for a private one."""
    return f"{info['web']}/#team={team}" if team else info["web"]


def shared_team(info):
    """Team code for everyone who simply enters this server's address."""
    return "shared:" + info["web"].lower()


def team_for(server, team):
    """The team code a pet uses: its private team if it has one, else the server's shared team."""
    info = parse(server) if server else None
    if not info:
        return ""
    return team or info["team"] or shared_team(info)


# ------------------------------------------------------------------ team crypto
def team_key(code):
    return hashlib.sha256(b"blip-team:" + code.encode("utf-8")).digest()


def room_id(tkey):
    return hashlib.blake2b(tkey, digest_size=12, person=b"blip-room").hexdigest()


def _keys(tkey):
    return (hmac.new(tkey, b"box-enc", hashlib.sha256).digest(),
            hmac.new(tkey, b"box-mac", hashlib.sha256).digest())


def _stream(key, nonce, n):
    out = bytearray()
    i = 0
    while len(out) < n:
        out += hashlib.blake2b(nonce + i.to_bytes(4, "big"), key=key, digest_size=64).digest()
        i += 1
    return bytes(out[:n])


def _xor(a, b):
    n = len(a)
    return (int.from_bytes(a, "big") ^ int.from_bytes(b, "big")).to_bytes(n, "big") if n else b""


def box(tkey, data):
    """Encrypt + authenticate bytes; returns base64 text."""
    enc, mac = _keys(tkey)
    nonce = os.urandom(16)
    ct = _xor(data, _stream(enc, nonce, len(data)))
    tag = hashlib.blake2b(nonce + ct, key=mac, digest_size=16).digest()
    return base64.b64encode(nonce + ct + tag).decode("ascii")


def unbox(tkey, text):
    try:
        raw = base64.b64decode(text, validate=True)
    except (ValueError, TypeError):
        return None
    if len(raw) < 32:
        return None
    nonce, ct, tag = raw[:16], raw[16:-16], raw[-16:]
    enc, mac = _keys(tkey)
    if not hmac.compare_digest(tag, hashlib.blake2b(nonce + ct, key=mac, digest_size=16).digest()):
        return None
    return _xor(ct, _stream(enc, nonce, len(ct)))


# ------------------------------------------------------------------ connection
class ServerLink:
    """Keeps a WebSocket to the server open, reconnecting with backoff.
    on_message(dict) and on_connect() run on the link's thread."""

    def __init__(self, url, on_message, on_connect=None, ssl_context=None):
        self.url, self.on_message, self.on_connect = url, on_message, on_connect
        self.ssl_context = ssl_context
        self.conn = None
        self.running = False
        self.state, self.error = "connecting", ""
        self.since = 0.0

    def start(self):
        self.running = True
        threading.Thread(target=self._loop, daemon=True).start()
        return self

    def stop(self):
        self.running = False
        c, self.conn = self.conn, None
        if c:
            c.close()

    def connected(self):
        return self.conn is not None

    def send(self, d):
        c = self.conn
        if not c:
            return False
        try:
            c.send(json.dumps(d, separators=(",", ":")))
            return True
        except ws.Closed:
            return False

    def _loop(self):
        delay = 1.0
        while self.running:
            self.state = "connecting"
            try:
                c = ws.connect(self.url, timeout=10,
                               ssl_context=self.ssl_context if self.url.startswith("wss") else None)
            except Exception as e:      # DNS, refused, TLS, not a Blip server...
                self.state, self.error = "error", _short(e)
                self._sleep(delay)
                delay = min(30.0, delay * 2)
                continue
            self.conn, self.state, self.error, self.since = c, "connected", "", time.time()
            delay = 1.0
            if self.on_connect:
                try:
                    self.on_connect()
                except Exception:
                    pass
            try:
                while self.running:
                    msg = c.recv()
                    if isinstance(msg, str):
                        try:
                            d = json.loads(msg)
                        except ValueError:
                            continue
                        if isinstance(d, dict):
                            self.on_message(d)
            except (ws.Closed, OSError):
                pass
            self.conn = None
            c.close()
            if self.running:
                self.state, self.error = "error", "connection dropped"
                self._sleep(1.0)

    def _sleep(self, secs):
        end = time.time() + secs
        while self.running and time.time() < end:
            time.sleep(0.2)


def _short(e):
    s = str(e) or e.__class__.__name__
    if "getaddrinfo" in s or "Name or service" in s or "nodename" in s:
        return "can't find that address"
    if "refused" in s.lower():
        return "nothing is listening there"
    if "timed out" in s.lower():
        return "no answer (timed out)"
    if "CERTIFICATE" in s:
        return "certificate problem"
    return s[:80]
