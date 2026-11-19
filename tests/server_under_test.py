"""The Blip server the tests talk to: the Python one by default, or any other server program
(the Rust one) when BLIP_SERVER_BIN points at it, so the same tests prove both are compatible:

    BLIP_SERVER_BIN=rust/target/release/blip-server.exe python -m unittest tests.test_net tests.test_voice
"""
import json
import os
import socket
import subprocess
import time
import urllib.request

from blip.server import Server


def _free_tcp_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


class External:
    """A server program started as a separate process. No `tap`: we can't watch inside it."""

    def __init__(self, path):
        self.port = _free_tcp_port()
        self.proc = subprocess.Popen([os.path.abspath(path), "--port", str(self.port)], stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL)
        self.tap = None
        end = time.time() + 10
        while time.time() < end:
            try:
                self.stats()
                return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError(f"{path} didn't start")

    def stats(self):
        with urllib.request.urlopen(f"http://127.0.0.1:{self.port}/health", timeout=3) as r:
            return json.loads(r.read())

    def stop(self):
        self.proc.kill()
        self.proc.wait(5)


def start():
    path = os.environ.get("BLIP_SERVER_BIN")
    return External(path) if path else Server(port=0, host="127.0.0.1", log=None).start()


def external():
    return bool(os.environ.get("BLIP_SERVER_BIN"))
