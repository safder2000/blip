import socket
import threading
import time
import unittest
import uuid

from blip.net import Net, check, key_from_passphrase, seal
from blip.relay import Relay


def free_udp_port():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def wait(cond, secs=6):
    end = time.time() + secs
    while time.time() < end:
        if cond():
            return True
        time.sleep(0.05)
    return False


class Inbox:
    def __init__(self):
        self.events = []
        self.lock = threading.Lock()

    def __call__(self, kind, d):
        with self.lock:
            self.events.append((kind, d))

    def of(self, kind):
        with self.lock:
            return [d for k, d in self.events if k == kind]


def make(name, passphrase, udp, announce, relay="", manual=("127.0.0.1",)):
    """udp=None picks a free port, retrying if another process grabs it first (busy CI machines)."""
    for attempt in range(8):
        port = free_udp_port() if udp is None else udp
        box = Inbox()
        n = Net(uuid.uuid4().hex, lambda: {"name": name, "status": "available", "ver": "t"}, passphrase, box,
                udp_port=port, tcp_port=0, announce_port=announce, relay=relay, manual_peers=manual)
        if n.start():
            return n, box
        if udp is not None:
            break
        time.sleep(0.1)
    raise AssertionError(n.error)


class SealTests(unittest.TestCase):
    def test_tamper_and_wrong_key(self):
        k = key_from_passphrase("a")
        d = seal(k, {"type": "msg", "body": {"text": "hi"}})
        self.assertTrue(check(k, d))
        self.assertFalse(check(key_from_passphrase("b"), d))
        d2 = dict(d, body={"text": "evil"})
        self.assertFalse(check(k, d2))


class LanTests(unittest.TestCase):
    def setUp(self):
        self.pa, self.pb = free_udp_port(), free_udp_port()
        self.nets = []

    def tearDown(self):
        for n in self.nets:
            n.stop()

    def test_discover_and_message(self):
        a, abox = make("Ann", "office", self.pa, self.pb)
        b, bbox = make("Ben", "office", self.pb, self.pa)
        self.nets += [a, b]
        self.assertTrue(wait(lambda: any(p["name"] == "Ben" for p in a.online())), "A never saw B")
        self.assertTrue(wait(lambda: any(p["name"] == "Ann" for p in b.online())), "B never saw A")
        res = []
        a.send(b.id, "call", {"call_id": "c1"}, res.append)
        self.assertTrue(wait(lambda: bbox.of("call")))
        self.assertTrue(wait(lambda: res == [True]))
        call = bbox.of("call")[0]
        self.assertEqual(call["from"], a.id)
        self.assertEqual(call["body"]["call_id"], "c1")
        b.send(a.id, "call_reply", {"call_id": "c1", "text": "On my way!"})
        self.assertTrue(wait(lambda: abox.of("call_reply")))
        self.assertEqual(abox.of("call_reply")[0]["body"]["text"], "On my way!")

    def test_different_passphrase_is_invisible(self):
        a, _ = make("Ann", "office", self.pa, self.pb)
        b, bbox = make("Eve", "other", self.pb, self.pa)
        self.nets += [a, b]
        time.sleep(1.0)
        self.assertEqual(a.online(), [])
        self.assertEqual(b.online(), [])

    def test_replay_is_dropped(self):
        a, abox = make("Ann", "office", self.pa, self.pb)
        self.nets.append(a)
        env = seal(a.key, {"v": 1, "type": "msg", "from": "x" * 32, "to": None, "ts": time.time(),
                           "nonce": "n1", "body": {"text": "hi"}})
        a._handle(dict(env), "127.0.0.1", "lan")
        a._handle(dict(env), "127.0.0.1", "lan")
        self.assertEqual(len(abox.of("msg")), 1)
        old = seal(a.key, dict(env, nonce="n2", ts=time.time() - 3600))
        a._handle(old, "127.0.0.1", "lan")
        self.assertEqual(len(abox.of("msg")), 1)


class RelayTests(unittest.TestCase):
    def test_peers_meet_through_relay(self):
        relay = Relay(port=0, host="127.0.0.1").start()
        addr = f"127.0.0.1:{relay.port}"
        dead = free_udp_port()   # nobody listens here, so no LAN path between the two
        a, abox = make("Ann", "office", None, dead, relay=addr, manual=())
        b, bbox = make("Ben", "office", None, dead, relay=addr, manual=())
        try:
            self.assertTrue(wait(lambda: any(p["name"] == "Ben" for p in a.online()), 10))
            self.assertTrue(wait(lambda: any(p["name"] == "Ann" for p in b.online()), 10))
            self.assertEqual(a.peer(b.id)["via"], "relay")
            res = []
            a.send(b.id, "msg", {"text": "lunch?"}, res.append)
            self.assertTrue(wait(lambda: bbox.of("msg")))
            self.assertEqual(bbox.of("msg")[0]["body"]["text"], "lunch?")
            self.assertEqual(res, [True])
        finally:
            a.stop()
            b.stop()
            relay.stop()


if __name__ == "__main__":
    unittest.main()


class PeerSharingTests(unittest.TestCase):
    def test_learns_only_private_ipv4(self):
        n = Net("me", lambda: {}, "x", lambda *a: None)
        n._learn(["10.1.2.3", "192.168.5.9", "8.8.8.8", "127.0.0.1", "not-an-ip", "fe80::1", 42])
        self.assertEqual(n.learned, {"10.1.2.3", "192.168.5.9"})
        n._learn("10.0.0.1")          # not a list: ignored
        self.assertEqual(len(n.learned), 2)
