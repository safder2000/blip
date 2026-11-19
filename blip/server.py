"""Blip server: lets pets reach each other over the internet (or across office networks).

    python -m blip.server                 # listens on 0.0.0.0:47620 (or $PORT if set)
    python -m blip.server --port 8080

Pets connect with a WebSocket (works behind Cloudflare Tunnel, Render, Railway, nginx, Caddy...)
or, for older pets, plain TCP lines on the same port. Opening the address in a browser shows a
small status page.

The server can't read anything. Each team's pets encrypt every message and every bit of call
audio with a key made from their team code, and the team code never reaches the server (it
lives after the '#' in the invite link). The server only sees a random-looking room id, which
pet a message is for, and how big it is. It forwards; that's all.

Standard library only: copy blip/server.py and blip/ws.py anywhere with Python 3.8+.
"""
import argparse
import json
import os
import socket
import threading
import time

try:
    from . import ws
except ImportError:           # run as a loose script next to ws.py
    import ws

DEFAULT_PORT = 47620
MAX_FRAME = 64 * 1024
MAX_CLIENTS = 5000
MAX_PER_ROOM = 1000
RATE = 250                 # messages per second per connection (voice is 50/s)
BURST = 500
VERSION = "1"


class Client:
    def __init__(self, server, sock, addr, wsconn=None):
        self.server, self.sock, self.addr, self.ws = server, sock, addr, wsconn
        self.room, self.id = None, None
        self.lock = threading.Lock()
        self.tokens, self.t = float(BURST), time.monotonic()
        self.alive = True

    def send(self, text):
        try:
            if self.ws:
                self.ws.send(text)
            else:
                with self.lock:
                    self.sock.sendall(text.encode("utf-8") + b"\n")
        except (ws.Closed, OSError):
            self.alive = False

    def allow(self):
        now = time.monotonic()
        self.tokens = min(BURST, self.tokens + (now - self.t) * RATE)
        self.t = now
        if self.tokens < 1:
            return False
        self.tokens -= 1
        return True


class Server:
    def __init__(self, port=DEFAULT_PORT, host="0.0.0.0", log=print):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind((host, port))
        self.sock.listen(128)
        self.port = self.sock.getsockname()[1]
        self.rooms = {}            # room -> {id: Client}
        self.clients = set()
        self.lock = threading.Lock()
        self.running = True
        self.log = log or (lambda *a: None)
        self.started = time.time()
        self.forwarded = 0
        self.tap = None            # tests: called with every routed frame

    # ------------------------------------------------ lifecycle
    def start(self):
        threading.Thread(target=self.serve_forever, daemon=True).start()
        return self

    def serve_forever(self):
        while self.running:
            try:
                conn, addr = self.sock.accept()
            except OSError:
                return
            with self.lock:
                full = len(self.clients) >= MAX_CLIENTS
            if full:
                conn.close()
                continue
            threading.Thread(target=self._serve, args=(conn, addr), daemon=True).start()

    def stop(self):
        self.running = False
        try:
            self.sock.close()
        except OSError:
            pass
        with self.lock:
            clients = list(self.clients)
        for c in clients:
            try:
                (c.ws.close() if c.ws else c.sock.close())
            except OSError:
                pass

    def stats(self):
        with self.lock:
            return {"pets": sum(len(r) for r in self.rooms.values()), "teams": len(self.rooms),
                    "connections": len(self.clients)}

    # ------------------------------------------------ connections
    def _serve(self, conn, addr):
        conn.settimeout(30)
        client = None
        try:
            first = conn.recv(4096)
            if not first:
                return
            if first[:4] in (b"GET ", b"HEAD"):
                head, rest = ws.read_http_head(conn, first)
                wsconn = ws.server_handshake(conn, head, rest)
                if wsconn is None:
                    self._http(conn, head)
                    return
                conn.settimeout(None)
                client = Client(self, conn, addr, wsconn)
                self._add(client)
                while self.running and client.alive:
                    msg = wsconn.recv()
                    if isinstance(msg, str):
                        self._route(client, msg)
            else:                                  # plain TCP: one JSON message per line
                conn.settimeout(None)
                client = Client(self, conn, addr)
                self._add(client)
                buf = first
                while self.running and client.alive:
                    while b"\n" in buf:
                        line, buf = buf.split(b"\n", 1)
                        self._route(client, line.decode("utf-8", "replace"))
                    if len(buf) > MAX_FRAME:
                        return
                    chunk = conn.recv(65536)
                    if not chunk:
                        return
                    buf += chunk
        except (ws.Closed, OSError, ValueError):
            pass
        finally:
            if client:
                self._remove(client)
            try:
                conn.close()
            except OSError:
                pass

    def _add(self, c):
        with self.lock:
            self.clients.add(c)

    def _remove(self, c):
        with self.lock:
            self.clients.discard(c)
            room = self.rooms.get(c.room)
            if room is not None and room.get(c.id) is c:
                del room[c.id]
                if not room:
                    del self.rooms[c.room]

    def _route(self, c, text):
        if not text or len(text) > MAX_FRAME or not c.allow():
            return
        try:
            d = json.loads(text)
            src, to = d.get("from"), d.get("to")
            room = d.get("room") or "legacy"       # pets before v0.7 had no rooms
        except (ValueError, AttributeError):
            return
        if not isinstance(src, str) or not isinstance(room, str) or len(src) > 64 or len(room) > 64:
            return
        if to is not None and not isinstance(to, str):
            return
        with self.lock:
            if c.room != room or c.id != src:      # (re)register this connection
                old = self.rooms.get(c.room)
                if old is not None and old.get(c.id) is c:
                    del old[c.id]
                    if not old:
                        del self.rooms[c.room]
                members = self.rooms.setdefault(room, {})
                if len(members) >= MAX_PER_ROOM and src not in members:
                    return
                c.room, c.id = room, src
                members[src] = c
            members = self.rooms.get(room, {})
            targets = [m for pid, m in members.items() if m is not c and (to is None or pid == to)]
            self.forwarded += len(targets)
        if self.tap:
            self.tap(text)
        for t in targets:
            t.send(text)

    # ------------------------------------------------ status page
    def _http(self, conn, head):
        line, hdrs = ws.parse_headers(head)
        path = line.split(" ")[1] if " " in line else "/"
        st = self.stats()
        if path.startswith("/health"):
            body = json.dumps(dict(st, ok=True, version=VERSION, uptime=int(time.time() - self.started)))
            ctype = "application/json"
        else:
            host = hdrs.get("host", "this-server")
            scheme = "https" if hdrs.get("x-forwarded-proto") == "https" or hdrs.get("cf-visitor", "").find(
                "https") >= 0 else "http"
            body = PAGE.replace("{pets}", str(st["pets"])).replace("{teams}", str(st["teams"])) \
                .replace("{addr}", f"{scheme}://{host}")
            ctype = "text/html; charset=utf-8"
        data = body.encode("utf-8")
        conn.sendall((f"HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {len(data)}\r\n"
                      "Cache-Control: no-store\r\nConnection: close\r\n\r\n").encode() + data)


PAGE = """<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Blip server</title><style>
body{font:16px/1.5 system-ui,sans-serif;background:#191919;color:#efefef;max-width:560px;margin:48px auto;padding:0 16px}
h1{font-size:22px}code{background:#262626;padding:3px 7px;border-radius:4px;word-break:break-all}
.m{color:#9c9c9c}button{font:inherit;background:#8a9bef;color:#fff;border:0;border-radius:6px;padding:8px 14px;cursor:pointer}
#link{display:block;margin:10px 0;padding:10px;background:#262626;border-radius:6px}</style></head><body>
<h1>Blip server is running</h1>
<p class="m">{pets} pets online in {teams} teams.</p>
<p>To connect, enter this address in Blip &rarr; Settings &rarr; Office &rarr; Online server:</p>
<code>{addr}</code>
<p>Everyone who enters it is on one shared team. Want a team of your own on this server?
Make a private link and give it only to your teammates.</p>
<button onclick="mk()">Make a private team link</button><code id="link" hidden></code>
<p class="m">The part after <code>#</code> is your team's secret. Browsers and Blip never send it to this server,
so the server can't read your messages or listen to calls.</p>
<script>function mk(){var a=new Uint8Array(12);crypto.getRandomValues(a);
var t=Array.from(a,function(b){return b.toString(16).padStart(2,'0')}).join('');
var e=document.getElementById('link');e.hidden=false;e.textContent=location.origin+'/#team='+t;}</script>
</body></html>"""


def main():
    ap = argparse.ArgumentParser(description="Blip server")
    ap.add_argument("--port", type=int, default=int(os.environ.get("PORT") or DEFAULT_PORT))
    ap.add_argument("--host", default="0.0.0.0")
    a = ap.parse_args()
    s = Server(a.port, a.host)
    print(f"Blip server listening on {a.host}:{s.port}", flush=True)
    try:
        s.serve_forever()
    except KeyboardInterrupt:
        s.stop()


if __name__ == "__main__":
    main()
