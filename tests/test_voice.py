import math
import os
import struct
import time
import unittest

from blip import opus, voice


def tone(i, hz=440):
    return struct.pack("<320h", *[int(9000 * math.sin(2 * math.pi * hz * (i * 320 + n) / 16000)) for n in range(320)])


def wait(cond, secs=4):
    end = time.time() + secs
    while time.time() < end:
        if cond():
            return True
        time.sleep(0.02)
    return False


class VoiceTests(unittest.TestCase):
    def test_seal_roundtrip_and_tamper(self):
        keys = voice.session_keys(b"k" * 32, "call1", voice.new_nonce())
        pcm = os.urandom(640)
        pkt = voice.seal(keys, 0, 7, pcm)
        self.assertNotIn(pcm[:32], pkt)              # encrypted on the wire
        self.assertEqual(voice.open_packet(keys, pkt), (0, 7, pcm))
        bad = bytearray(pkt)
        bad[20] ^= 1
        self.assertIsNone(voice.open_packet(keys, bytes(bad)))
        other = voice.session_keys(b"k" * 32, "call2", voice.new_nonce())
        self.assertIsNone(voice.open_packet(other, pkt))

    def test_two_sessions_talk(self):
        nonce = voice.new_nonce()
        keys = voice.session_keys(b"office" * 6, "c", nonce)
        callee = voice.VoiceSession(keys, 1, None, audio=False).start()
        caller = voice.VoiceSession(keys, 0, ("127.0.0.1", callee.port), audio=False).start()
        try:
            frames = [os.urandom(voice.FRAME * 2) for _ in range(5)]
            for f in frames:
                caller.send_frame(f)
            self.assertTrue(wait(lambda: len(callee.rx) == 5))
            self.assertEqual(list(callee.rx), frames)
            self.assertEqual(callee.peer[1], caller.port)      # learned the caller's address
            back = os.urandom(voice.FRAME * 2)
            callee.send_frame(back)
            self.assertTrue(wait(lambda: len(caller.rx) == 1))
            self.assertEqual(caller.rx[0], back)
            # a stranger with the wrong key is ignored
            stranger = voice.VoiceSession(voice.session_keys(b"x" * 32, "c", nonce), 0,
                                          ("127.0.0.1", callee.port), audio=False).start()
            stranger.send_frame(os.urandom(voice.FRAME * 2))
            time.sleep(0.3)
            self.assertEqual(len(callee.rx), 5)
            stranger.stop()
        finally:
            caller.stop()
            callee.stop()


@unittest.skipUnless(opus.available(), "libopus not found")
class OpusCallTests(unittest.TestCase):
    def test_opus_session_through_a_relay_function(self):
        keys = voice.session_keys(b"t" * 32, "c", voice.new_nonce())
        sent = []
        callee = voice.VoiceSession(keys, 1, audio=False, transport=sent.append, codec="opus").start()
        caller = voice.VoiceSession(keys, 0, audio=False,
                                    transport=lambda pkt: (sent.append(pkt), callee.feed(pkt)), codec="opus").start()
        try:
            for i in range(20):
                caller.send_frame(tone(i))
            self.assertTrue(wait(lambda: len(callee.rx) >= 12))
            self.assertTrue(all(len(f) == voice.FRAME * 2 for f in callee.rx))
            self.assertGreater(callee.level_out, 0.2)              # it's the tone, not silence
            self.assertLess(max(len(p) for p in sent), 200)         # compressed (raw would be 655 bytes)
        finally:
            caller.stop()
            callee.stop()

    def test_lost_frames_are_filled(self):
        keys = voice.session_keys(b"t" * 32, "c", voice.new_nonce())
        callee = voice.VoiceSession(keys, 1, audio=False, transport=lambda p: None, codec="opus").start()
        caller = voice.VoiceSession(keys, 0, audio=False, transport=lambda p: None, codec="opus").start()
        try:
            pkts = []
            caller.transport = pkts.append
            for i in range(6):
                caller.send_frame(tone(i))
            for i, p in enumerate(pkts):
                if i != 3:                                           # frame 3 never arrives
                    callee.feed(p)
            self.assertEqual(len(callee.rx), 6)
        finally:
            caller.stop()
            callee.stop()

    def test_call_through_a_real_server(self):
        from blip.net import Net
        from blip.server import Server
        from tests.test_net import Inbox, free_udp_port, wait as nwait
        import uuid
        srv = Server(port=0, host="127.0.0.1", log=None).start()
        link = f"127.0.0.1:{srv.port}#team=callteam1"
        nets = []
        try:
            for name in ("Ann", "Ben"):
                n = Net(uuid.uuid4().hex, lambda name=name: {"name": name, "voice": True, "opus": True},
                        "office", Inbox(), udp_port=free_udp_port(), tcp_port=0, announce_port=free_udp_port(),
                        server=link, manual_peers=())
                self.assertTrue(n.start())
                nets.append(n)
            a, b = nets
            self.assertTrue(nwait(lambda: a.peer(b.id) and b.peer(a.id), 10))
            self.assertTrue(a.peer(b.id)["opus"])
            nonce = voice.new_nonce()
            ka = voice.session_keys(a.key_for(b.id), "call9", nonce)
            kb = voice.session_keys(b.key_for(a.id), "call9", nonce)
            self.assertEqual(ka, kb)
            caller = voice.VoiceSession(ka, 0, audio=False, transport=lambda p: a.server_voice(b.id, p),
                                        codec="opus").start()
            callee = voice.VoiceSession(kb, 1, audio=False, transport=lambda p: b.server_voice(a.id, p),
                                        codec="opus").start()
            a.on_voice = lambda src, pkt: caller.feed(pkt)
            b.on_voice = lambda src, pkt: callee.feed(pkt)
            for i in range(25):
                caller.send_frame(tone(i))
                callee.send_frame(tone(i, 660))
                time.sleep(0.02)
            self.assertTrue(wait(lambda: len(callee.rx) >= 10 and len(caller.rx) >= 10, 5))
            caller.stop()
            callee.stop()
        finally:
            for n in nets:
                n.stop()
            srv.stop()


if __name__ == "__main__":
    unittest.main()
