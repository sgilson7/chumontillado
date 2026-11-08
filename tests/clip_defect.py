#!/usr/bin/env python3
"""The clip check must fail on a clip that clips. Two planted defects, each in
a scratch copy of the baked data:

  1. one frame of Big Chum's run lowered 30 cm: her feet go into the ground;
  2. the held soldier moved onto Big Chum (the pair offset set to zero): one
     body inside the other.

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


def copy_data(dst):
    for rel in ("data/mesh", "data/clip_limits.json", "web/mesh"):
        src = ROOT / rel
        target = dst / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        if src.is_dir():
            shutil.copytree(src, target, ignore=shutil.ignore_patterns("tex"))
        else:
            shutil.copy(src, target)


def run(root):
    r = subprocess.run([sys.executable, str(CHECK), str(root)], capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr


def main():
    bad = 0
    code, out = run(ROOT)
    if code != 0:
        print("clip defect: the shipped clips already fail the check:\n" + out[-800:])
        return 1
    with tempfile.TemporaryDirectory() as t:
        t = Path(t)
        copy_data(t)
        m = json.loads((t / "data/mesh/chum.json").read_text())
        clip = next(c for c in m["clips"] if c["name"] == "run")
        anim = t / "web" / m["anim"]
        raw = bytearray(anim.read_bytes())
        width = m["anim_width"]
        row = clip["row"] + clip["frames"] // 2
        for bone in range(len(m["bones"])):
            # the y row's translation (row 1, column 3) of this bone's matrix
            at = ((row * width + bone * 3 + 1) * 4 + 3) * 2
            v = struct.unpack_from("<e", raw, at)[0]
            struct.pack_into("<e", raw, at, v - 0.30)
        anim.write_bytes(bytes(raw))
        code, out = run(t)
        if code == 1 and "below the ground" in out and "chum__run" in out:
            print("clip defect: caught a run lowered into the ground")
        else:
            print("clip defect: MISSED a run lowered into the ground", out[-400:])
            bad += 1
    with tempfile.TemporaryDirectory() as t:
        t = Path(t)
        copy_data(t)
        p = t / "data/mesh/chum.json"
        m = json.loads(p.read_text())
        for c in m["clips"]:
            if c["name"] == "hold":
                c["pair"][1] = [0, 0, 0]
        p.write_text(json.dumps(m))
        code, out = run(t)
        if code == 1 and "with soldier__held" in out:
            print("clip defect: caught a held soldier inside Big Chum")
        else:
            print("clip defect: MISSED a held soldier inside Big Chum", out[-400:])
            bad += 1
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
