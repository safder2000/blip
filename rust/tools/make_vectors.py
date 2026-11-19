"""Reference values from the Python Blip that the Rust code must reproduce byte for byte.
    python rust/tools/make_vectors.py   (from the repo root) -> rust/crates/blip-proto/tests/vectors.json"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
from blip import link, net, voice  # noqa: E402

TRICKY = ("lunch at the samosa place? ✨ café സഫ് \"q\" \\ / \n\t\x01\x1f\x7f "
          " \U0001F600 end")

key = net.key_from_passphrase("blip-office")
envelopes = [
    {"v": 1, "type": "hello", "from": "a" * 32, "to": None, "ts": 1795070000.1234565, "nonce": "n1",
     "body": {"name": "Safder", "status": "available", "voice": True, "opus": True, "port": 47611,
              "peers": ["192.168.1.7", "192.168.1.6"], "ver": "0.7.6"}},
    {"v": 1, "type": "msg", "from": "b", "to": "c", "ts": 1795070001.0, "nonce": "n2", "body": {"text": TRICKY}},
    {"v": 1, "type": "call", "from": "b", "to": None, "ts": 1e16, "nonce": "n3",
     "body": {"x": 1e-5, "y": 0.1, "z": -2.5, "w": 1e-4, "u": 123456789.0, "t": 1.5e300, "s": 2e-300,
              "big": 123456789012345678, "neg": -7, "list": [1, 2.0, "3", None, False, {"b": 1, "a": 2}]}},
    {"v": 1, "type": "msg", "from": "\U0001F600", "to": "d", "ts": 1795070002.5, "nonce": "n4", "body": {}},
]
out = {"group_key": key.hex(), "envelopes": []}
for e in envelopes:
    sealed = net.seal(key, e)
    out["envelopes"].append({"input": json.dumps(e, ensure_ascii=False), "canon": net._canon(e).decode("utf-8"),
                             "sig": sealed["sig"]})

teams = []
for code in ("abcd1234ef", "shared:https://blip.emilda.co", "team-സ"):
    tk = link.team_key(code)
    teams.append({"code": code, "key": tk.hex(), "room": link.room_id(tk),
                  "box": link.box(tk, ("hello " + code).encode("utf-8")), "plain": "hello " + code})
out["teams"] = teams
out["links"] = [{"text": t, "parsed": link.parse(t), "team_for": link.team_for(t, "")} for t in
                ("https://blip.emilda.co", "https://blip.emilda.co/#team=abcd1234", "blip.emilda.co",
                 "192.168.1.200:30620", "192.168.1.200", "http://nas.local:8080/", "ws://h:1/ws", "ftp://x", "",
                 "localhost:9000#team=xyz9", "https://Blip.Emilda.co/")]
vk = voice.session_keys(key, "call9", "00112233445566778899aabbccddeeff")
pcm = bytes(range(256)) * 2 + bytes(128)
out["voice"] = {"call_id": "call9", "nonce": "00112233445566778899aabbccddeeff", "enc": vk[0].hex(),
                "mac": vk[1].hex(), "dir": 1, "seq": 77, "pcm": pcm.hex(),
                "packet": voice.seal(vk, 1, 77, pcm).hex()}
path = os.path.join(os.path.dirname(__file__), "..", "crates", "blip-proto", "tests", "vectors.json")
with open(path, "w", encoding="utf-8") as f:
    json.dump(out, f, ensure_ascii=False, indent=1)
print("wrote", os.path.normpath(path))


# ---------------------------------------------------------------- sprites (for crates/blip-pet)
import hashlib  # noqa: E402
import itertools  # noqa: E402

from blip import sprites  # noqa: E402


def fp(g):
    s = "".join(f"{x},{y},{c.lstrip('#').lower()};" for (x, y), c in sorted(g.items()))
    return hashlib.sha256(s.encode()).hexdigest()[:16]


pals = list(sprites.PALETTES)
pets = []
combos = itertools.product(list(sprites.SPECIES), ["baby", "child", "adult"],
                           ["normal", "happy", "sad", "eat", "sleep", "sick", "held", "alert"],
                           range(4), (-1, 0, 1), (False, True), (False, True))
for i, (sp, st, mood, fr, facing, squash, blink) in enumerate(combos):
    pal = pals[i % len(pals)]
    pets.append([sp, st, pal, mood, fr, facing, squash, blink,
                 fp(sprites.build_pet(st, pal, mood, fr, facing, squash, blink, sp))])
eggs = [[fr, pr, fp(sprites.build_egg(fr, pr))] for fr in range(4) for pr in (0.1, 0.5, 0.7, 0.9)]
balls = []
for i, (ang, sq, face) in enumerate(itertools.product((0.0, 0.7, 2.5, 4.4, -1.3), (0.0, 0.25), (True, False))):
    pal = pals[i % len(pals)]
    balls.append([pal, ang, sq, face, fp(sprites.build_ball(sprites.PALETTES[pal], ang, face, sq))])
spath = os.path.join(os.path.dirname(__file__), "..", "crates", "blip-pet", "tests", "sprites.json")
with open(spath, "w", encoding="utf-8") as f:
    json.dump({"pets": pets, "eggs": eggs, "balls": balls}, f, separators=(",", ":"))
print("wrote", os.path.normpath(spath), len(pets), "pets")
