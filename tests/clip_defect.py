#!/usr/bin/env python3
"""The clip check must fail on a clip that clips (T34). Copied from
Espiochum's tests/clip_defect.py (CS-2) and changed for Chumontillado's
lifelike skins. Two planted defects, each in a scratch copy of the baked data:

  1. every bone of the Hidalgo's stand lowered 30 cm: his feet go into the
     floor;
  2. the Hidalgo's right forearm and hand in the level aim moved 25 cm back
     and 15 cm across: into his chest.

Each copy must make scripts/check_clips.py exit 1 with the matching message;
the untouched data must pass. Exits 1 if any of that is not so.
"""
import json
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check_clips.py"
SKIN = "hidalgo"


def copy_data(dst):
    for rel in ("data/mesh", "data/skins", "data/clip_limits.json"):
        src, target = ROOT / rel, dst / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        (shutil.copytree if src.is_dir() else shutil.copy)(src, target)
    (dst / "assets" / "mesh").mkdir(parents=True)
    for f in (ROOT / "assets" / "mesh").glob("*_anim.bin"):
        shutil.copy(f, dst / "assets" / "mesh" / f.name)


def run(root):
    r = subprocess.run([sys.executable, str(CHECK), str(root)], capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def nudge(root, clip_kind, clip_name, frame, bones, move):
    m = json.loads((root / "data/mesh" / f"{SKIN}.json").read_text())
    c = json.loads((root / "data/skins" / f"{SKIN}.json").read_text())
    clip = next(x for x in c[clip_kind] if x["name"] == clip_name)
    names = [b["name"] for b in m["bones"]]
    anim = root / "assets" / m["anim"]
    raw = bytearray(anim.read_bytes())
    row = clip["row"] + frame
    for b in bones(names):
        j = names.index(b) if isinstance(b, str) else b
        for axis, d in enumerate(move):
            at = (((row * m["joints"] + j) * 3 + axis) * 4 + 3) * 2   # the translation of row `axis`
            struct.pack_into("<e", raw, at, struct.unpack_from("<e", raw, at)[0] + d)
    anim.write_bytes(bytes(raw))


def main():
    code, out = run(ROOT)
    if code != 0:
        print("clip defect: the shipped clips already fail the check:\n" + out[-800:])
        return 1
    bad = 0
    plants = [
        ("the stand lowered into the floor", "below the floor",
         lambda t: nudge(t, "lower", "stand", 0, lambda n: range(len(n)), (0.0, -0.30, 0.0))),
        ("the aim's right forearm in the chest", "into spine",
         lambda t: nudge(t, "upper", "aim", 6, lambda n: [x for x in n if x.startswith(("lowerarm01.R", "lowerarm02.R", "wrist.R"))],
                         (-0.15, 0.0, 0.25))),
    ]
    for what, says, plant in plants:
        with tempfile.TemporaryDirectory() as t:
            t = Path(t)
            copy_data(t)
            plant(t)
            code, out = run(t)
            if code == 0 or says not in out:
                print(f"clip defect: {what} was not caught:\n{out[-600:]}")
                bad += 1
            else:
                print(f"clip defect: {what}: caught")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
