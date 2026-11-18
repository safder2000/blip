"""Optional relay for offices where pets can't see each other by broadcast
(different floors/VLANs, Wi-Fi client isolation).

Run it on any always-on machine everyone can reach:
    python -m blip.relay            # listens on 0.0.0.0:47620
    python -m blip.relay --port 5000
Then put "host:47620" in each pet's Settings > Relay.

The relay never sees the passphrase. It only forwards signed envelopes: a hello goes to
everyone, anything addressed to someone goes to that pet. Pets check every signature.
"""
import argparse
import json
import socket
import threading

from .net import MAX_FRAME, RELAY_PORT


class Relay:
    def __init__(self, port=RELAY_PORT, host="0.0.0.0"):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.sock.bind((host, port))
        self.sock.listen(64)
        self.port = self.sock.getsockname()[1]
        self.clients = {}     # conn -> node id (or None until its first hello)
        self.lock = threading.Lock()
        self.running = True

    def serve_forever(self):
        while self.running:
            try:
                conn, _ = self.sock.accept()
            except OSError:
                return
            with self.lock:
                self.clients[conn] = None
            threading.Thread(target=self._client, args=(conn,), daemon=True).start()

    def start(self):
        threading.Thread(target=self.serve_forever, daemon=True).start()
        return self

    def stop(self):
        self.running = False
        self.sock.close()
        with self.lock:
            for c in list(self.clients):
                try:
                    c.close()
                except OSError:
                    pass

    def _send(self, conn, line):
        try:
            conn.sendall(line + b"\n")
        except OSError:
            pass

    def _client(self, conn):
        buf = b""
        try:
            while self.running:
                chunk = conn.recv(8192)
                if not chunk:
                    break
                buf += chunk
                if len(buf) > MAX_FRAME * 4:
                    break
                while b"\n" in buf:
                    line, buf = buf.split(b"\n", 1)
                    self._route(conn, line)
        except OSError:
            pass
        finally:
            with self.lock:
                self.clients.pop(conn, None)
            try:
                conn.close()
            except OSError:
                pass

    def _route(self, conn, line):
        if len(line) > MAX_FRAME:
            return
        try:
            d = json.loads(line.decode("utf-8"))
            src, to = d.get("from"), d.get("to")
        except (ValueError, UnicodeDecodeError, AttributeError):
            return
        with self.lock:
            if src and isinstance(src, str):
                self.clients[conn] = src
            targets = [c for c, nid in self.clients.items()
                       if c is not conn and (to is None or nid == to)]
        for c in targets:
            self._send(c, line)


def main():
    ap = argparse.ArgumentParser(description="Blip relay")
    ap.add_argument("--port", type=int, default=RELAY_PORT)
    ap.add_argument("--host", default="0.0.0.0")
    a = ap.parse_args()
    r = Relay(a.port, a.host)
    print(f"Blip relay listening on {a.host}:{r.port}")
    try:
        r.serve_forever()
    except KeyboardInterrupt:
        r.stop()


if __name__ == "__main__":
    main()
