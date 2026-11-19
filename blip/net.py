"""Office network: pets find each other with UDP broadcast and talk over TCP.

Every packet is a JSON envelope signed with HMAC-SHA256 using a key derived from the
office passphrase, so only pets that share the passphrase see or hear each other.
Pets can also meet through a Blip server (blip/server.py) over the internet: everything sent
that way is encrypted end to end with the team code from the invite link.
"""
import hashlib
import hmac
import base64
import ipaddress
import json
import socket
import sys
import threading
import time
import uuid

from . import link as _link
from .plat import ssl_context

UDP_PORT = 47610
TCP_PORT = 47611
RELAY_PORT = 47620
MAX_FRAME = 32 * 1024
HELLO_EVERY = 15
OFFLINE_AFTER = 50
MAX_SKEW = 600
MAX_SHARED = 64
DEFAULT_GROUP = "blip-office"


def key_from_passphrase(passphrase):
    return hashlib.sha256(("blip-office:" + passphrase).encode("utf-8")).digest()


def _canon(d):
    return json.dumps({k: v for k, v in d.items() if k != "sig"}, sort_keys=True,
                      separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def seal(key, d):
    d = dict(d)
    d["sig"] = hmac.new(key, _canon(d), hashlib.sha256).hexdigest()
    return d


def check(key, d):
    sig = d.get("sig")
    if not isinstance(sig, str):
        return False
    return hmac.compare_digest(sig, hmac.new(key, _canon(d), hashlib.sha256).hexdigest())


def local_ips():
    ips = set()
    if sys.platform != "darwin":    # on macOS this lookup can stall for seconds
        try:
            for info in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
                ips.add(info[4][0])
        except OSError:
            pass
    try:  # the address of the default route (connect() on UDP sends nothing)
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("10.255.255.255", 1))
        ips.add(s.getsockname()[0])
        s.close()
    except OSError:
        pass
    return sorted(ip for ip in ips if not ip.startswith(("127.", "169.254.")))


def broadcast_addrs():
    out = {"255.255.255.255"}
    for ip in local_ips():
        a, b, c, _ = ip.split(".")
        out.add(f"{a}.{b}.{c}.255")
    return sorted(out)


class Net:
    """on_event(kind, data) is called from worker threads:
    kind = 'peers' (data None) or a message type ('call', 'call_cancel', 'call_reply', 'msg') with the envelope."""

    def __init__(self, node_id, profile, passphrase, on_event, udp_port=UDP_PORT, tcp_port=TCP_PORT,
                 announce_port=None, server="", team="", manual_peers=()):
        self.id = node_id
        self.profile = profile          # callable -> dict(name, status, stage, palette, ver)
        self.key = key_from_passphrase(passphrase)
        self.on_event = on_event
        self.udp_port = udp_port
        self.announce_port = announce_port or udp_port
        self.want_tcp_port = tcp_port
        self.tcp_port = None
        info = _link.parse(server) if server else None
        self.server_url = info["url"] if info else ""
        team = team or (info or {}).get("team") or ""
        self.tkey = _link.team_key(team) if team else None
        self.room = _link.room_id(self.tkey) if team else None
        self.link = None
        self.on_voice = None           # (src, packet) for call audio arriving through the server
        self.manual = [p.strip() for p in manual_peers if p.strip()]
        self.peers = {}
        self.learned = set()           # IPs other pets told us about
        self.lock = threading.Lock()
        self.nonces = {}
        self.running = False
        self.udp = self.tcp = None
        self.wake = threading.Event()
        self.error = None

    # ------------------------------------------------ lifecycle
    def start(self):
        self.running = True
        try:
            self.tcp = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            try:
                self.tcp.bind(("0.0.0.0", self.want_tcp_port))
            except OSError:
                self.tcp.bind(("0.0.0.0", 0))
            self.tcp.listen(16)
            self.tcp_port = self.tcp.getsockname()[1]
            self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            self.udp.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.udp.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
            self.udp.bind(("0.0.0.0", self.udp_port))
        except OSError as e:
            self.error = str(e)
            self.running = False
            for sock in (getattr(self, "tcp", None), getattr(self, "udp", None)):
                try:
                    sock and sock.close()
                except OSError:
                    pass
            return False
        for fn in (self._udp_loop, self._tcp_loop, self._announce_loop):
            threading.Thread(target=fn, daemon=True).start()
        if self.server_url and self.tkey:
            self.link = _link.ServerLink(self.server_url, self._server_message,
                                         on_connect=lambda: self._server_send(self._hello()),
                                         ssl_context=ssl_context()).start()
        return True

    def stop(self):
        if not self.running:
            return
        try:
            self._send_udp_all(self._env("bye"))
        except Exception:
            pass
        self.running = False
        if self.link:
            self._server_send(self._env("bye"))
            self.link.stop()
        for s in (self.udp, self.tcp):
            try:
                if s:
                    s.close()
            except OSError:
                pass

    def announce_now(self):
        self.wake.set()

    # ------------------------------------------------ envelopes
    def _env(self, typ, to=None, body=None):
        return seal(self.key, {"v": 1, "type": typ, "from": self.id, "to": to, "ts": time.time(),
                               "nonce": uuid.uuid4().hex, "body": body or {}})

    def _hello(self, to=None):
        body = dict(self.profile())
        body["port"] = self.tcp_port
        with self.lock:   # share who we can see, so pets on other subnets get introduced
            body["peers"] = sorted({p["ip"] for p in self.peers.values()
                                    if p.get("via") == "lan" and p.get("ip")})[:MAX_SHARED]
        return self._env("hello", to, body)

    def _learn(self, ips):
        mine = set(local_ips())
        for ip in ips if isinstance(ips, list) else []:
            try:
                a = ipaddress.ip_address(str(ip))
            except ValueError:
                continue
            if a.version != 4 or not a.is_private or a.is_loopback or str(a) in mine:
                continue
            with self.lock:
                if str(a) not in self.learned and len(self.learned) < 256:
                    self.learned.add(str(a))
                    self.wake.set()

    def _accept(self, d, key=None):
        if not isinstance(d, dict) or not check(key or self.key, d):
            return False
        if d.get("from") == self.id or d.get("to") not in (None, self.id):
            return False
        try:
            if abs(time.time() - float(d.get("ts", 0))) > MAX_SKEW:
                return False
        except (TypeError, ValueError):
            return False
        n = d.get("nonce")
        now = time.time()
        with self.lock:
            if n in self.nonces:
                return False
            self.nonces[n] = now
            if len(self.nonces) > 2000:
                self.nonces = {k: t for k, t in self.nonces.items() if now - t < MAX_SKEW}
        return True

    def _handle(self, d, ip, via, key=None):
        if not self._accept(d, key):
            return
        typ, src, body = d.get("type"), d["from"], d.get("body") or {}
        if typ == "hello":
            with self.lock:
                new = src not in self.peers
                p = self.peers.setdefault(src, {"id": src})
                if via == "lan" or p.get("via") != "lan" or time.time() - p.get("seen", 0) > OFFLINE_AFTER:
                    p["via"] = via
                    if ip:
                        p["ip"] = ip
                p.update(name=str(body.get("name", "?"))[:24], status=body.get("status", "available"),
                         stage=body.get("stage"), palette=body.get("palette"), ver=body.get("ver"),
                         port=int(body.get("port") or TCP_PORT), seen=time.time(),
                         species=body.get("species"), voice=bool(body.get("voice")),
                         opus=bool(body.get("opus")))
            if via == "lan":
                self._learn(body.get("peers"))
            if new and d.get("to") is None:
                reply = self._hello(to=src)
                if via == "lan" and ip:
                    self._send_udp(reply, ip)
                else:
                    self._server_send(reply)
            self.on_event("peers", None)
            return
        if typ == "bye":
            with self.lock:
                self.peers.pop(src, None)
            self.on_event("peers", None)
            return
        with self.lock:
            if src in self.peers:
                self.peers[src]["seen"] = time.time()
        self.on_event(typ, d)

    # ------------------------------------------------ peers
    def online(self):
        now = time.time()
        with self.lock:
            return sorted((dict(p) for p in self.peers.values() if now - p.get("seen", 0) < OFFLINE_AFTER),
                          key=lambda p: p.get("name", "").lower())

    def peer(self, pid):
        with self.lock:
            p = self.peers.get(pid)
            return dict(p) if p else None

    # ------------------------------------------------ sending
    def send(self, pid, typ, body, done=None):
        """Send in the background; done(ok: bool) is called from a worker thread."""
        def work():
            ok = False
            p = self.peer(pid)
            env = self._env(typ, pid, body)
            if p and p.get("via") == "lan" and p.get("ip"):
                ok = self._send_tcp(p["ip"], p.get("port") or TCP_PORT, env)
            if not ok and self.link and self.link.connected():
                ok = self._server_send(env)
            if done:
                done(ok)
        threading.Thread(target=work, daemon=True).start()

    def _send_tcp(self, ip, port, env):
        try:
            with socket.create_connection((ip, port), timeout=4) as s:
                s.sendall(json.dumps(env).encode("utf-8") + b"\n")
                s.settimeout(4)
                ack = s.recv(64)
                return ack.startswith(b"ok")
        except OSError:
            return False

    def _send_udp(self, env, addr):
        try:
            self.udp.sendto(json.dumps(env).encode("utf-8"), (addr, self.announce_port))
        except OSError:
            pass

    def _send_udp_all(self, env):
        with self.lock:   # broadcast, plus direct hellos to every pet we know of (reaches other subnets)
            direct = self.learned | {p["ip"] for p in self.peers.values() if p.get("via") == "lan" and p.get("ip")}
        for a in broadcast_addrs() + sorted(set(self.manual) | direct):
            self._send_udp(env, a)

    # ------------------------------------------------ loops
    def _announce_loop(self):
        while self.running:
            self._send_udp_all(self._hello())
            if self.link and self.link.connected():
                self._server_send(self._hello())
            self.wake.wait(HELLO_EVERY)
            self.wake.clear()

    def _udp_loop(self):
        while self.running:
            try:
                data, (ip, _) = self.udp.recvfrom(MAX_FRAME)
            except OSError:
                if not self.running:
                    return
                continue
            try:
                self._handle(json.loads(data.decode("utf-8")), ip, "lan")
            except (ValueError, UnicodeDecodeError):
                pass

    def _tcp_loop(self):
        while self.running:
            try:
                conn, (ip, _) = self.tcp.accept()
            except OSError:
                if not self.running:
                    return
                continue
            threading.Thread(target=self._serve, args=(conn, ip), daemon=True).start()

    def _serve(self, conn, ip):
        with conn:
            conn.settimeout(5)
            buf = b""
            try:
                while b"\n" not in buf and len(buf) < MAX_FRAME:
                    chunk = conn.recv(4096)
                    if not chunk:
                        break
                    buf += chunk
                d = json.loads(buf.split(b"\n", 1)[0].decode("utf-8"))
            except (OSError, ValueError, UnicodeDecodeError):
                return
            ok = check(self.key, d) if isinstance(d, dict) else False
            try:
                conn.sendall(b"ok\n" if ok else b"no\n")
            except OSError:
                pass
            if ok:
                self._handle(d, ip, "lan")

    # ------------------------------------------------ online server
    def _server_send(self, env):
        """Send a signed envelope through the server, encrypted for the team."""
        link = self.link
        if not link or not link.connected():
            return False
        env = {k: v for k, v in env.items() if k != "sig"}
        if env.get("type") == "hello":           # office IPs mean nothing to other networks
            env["body"] = {k: v for k, v in (env.get("body") or {}).items() if k != "peers"}
        env = seal(self.tkey, env)                # team key: only teammates can check it
        data = json.dumps(env, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
        return link.send({"room": self.room, "from": self.id, "to": env.get("to"),
                          "box": _link.box(self.tkey, data)})

    def server_voice(self, pid, packet):
        """Call audio through the server (already encrypted by the voice session)."""
        link = self.link
        if link:
            link.send({"room": self.room, "from": self.id, "to": pid,
                       "v": base64.b64encode(packet).decode("ascii")})

    def _server_message(self, d):
        src = d.get("from")
        if "v" in d:
            if self.on_voice and isinstance(src, str):
                try:
                    self.on_voice(src, base64.b64decode(d["v"]))
                except (ValueError, TypeError):
                    pass
            return
        raw = _link.unbox(self.tkey, d.get("box") or "")
        if raw is None:
            return
        try:
            env = json.loads(raw.decode("utf-8"))
        except (ValueError, UnicodeDecodeError):
            return
        if isinstance(env, dict) and env.get("from") == src:
            self._handle(env, None, "server", key=self.tkey)

    def server_state(self):
        """('off'|'connecting'|'connected'|'error', detail)."""
        if not self.link:
            return "off", ""
        return self.link.state, self.link.error

    def key_for(self, pid):
        p = self.peer(pid)
        return self.tkey if p and p.get("via") == "server" else self.key
