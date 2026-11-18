import os
import time
import unittest

from blip import voice


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


if __name__ == "__main__":
    unittest.main()
