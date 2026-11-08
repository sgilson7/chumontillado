#!/usr/bin/env python3
"""No clip clips: rebuild every baked frame's bone capsules and measure (T34).

Copied from Espiochum's scripts/check_clips.py (CS-2) and changed for
Chumontillado's lifelike skins, whose body is split at a boundary bone: each
lower frame is checked with every upper frame on top of it (an upper bone is
the lower frame's boundary times its upper matrix, as the page draws it).

For each skin in data/mesh/*.json, each bone becomes a capsule (its rest head
and tail moved by the frame's skin matrix, with a radius by its name). The
check fails when, deeper than data/clip_limits.json allows:

  - ground: on a lower clip that stands on the floor (all but air and hang,
    whose feet are off it), a capsule's lowest point is below the floor;
  - self: an arm or leg capsule passes into the body's torso capsules
    further than the two already overlap in the rest pose (bones next to
    each other in the chain are skipped).

Pure Python, reading the files the page draws, so it runs without Blender.
tests/clip_defect.py plants a defect of each kind and requires this to fail.

    python3 scripts/check_clips.py [repo] [--all]   (--all prints every failure)
"""
import json
import math
import struct
import sys
from pathlib import Path

ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
ALL = "--all" in sys.argv
ROOT = Path(ARGS[0]).resolve() if ARGS else Path(__file__).resolve().parents[1]
LIMITS = json.loads((ROOT / "data" / "clip_limits.json").read_text())
LIMBS = ("upperarm", "lowerarm", "wrist", "upperleg", "lowerleg", "foot")
TORSO = ("spine01", "spine02", "spine03", "pelvis")
RADII = (("spine0", 0.13), ("pelvis", 0.12), ("neck", 0.06), ("head", 0.10), ("clavicle", 0.05), ("shoulder", 0.06),
         ("upperarm", 0.055), ("lowerarm", 0.045), ("wrist", 0.035), ("finger", 0.01), ("upperleg", 0.08),
         ("lowerleg", 0.06), ("foot", 0.045), ("root", 0.12))


def radius(name):
    return next((r for k, r in RADII if name.startswith(k)), 0.0)


def load(skin):
    m = json.loads((ROOT / "data" / "mesh" / f"{skin}.json").read_text())
    c = json.loads((ROOT / "data" / "skins" / f"{skin}.json").read_text())
    raw = (ROOT / "assets" / m["anim"]).read_bytes()
    vals = struct.unpack(f"<{len(raw) // 2}e", raw)
    return m, c, vals


def mat(vals, joints, row, j):
    base = (row * joints + j) * 12
    r = vals[base:base + 12]
    return (r[0:4], r[4:8], r[8:12])


def mul(a, b):
    """3x4 times 3x4, the second taken as 4x4 with a last row 0 0 0 1."""
    out = []
    for i in range(3):
        out.append(tuple(a[i][0] * b[0][k] + a[i][1] * b[1][k] + a[i][2] * b[2][k] + (a[i][3] if k == 3 else 0.0)
                         for k in range(4)))
    return tuple(out)


def xf(m, p):
    return tuple(m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3] for i in range(3))


def seg_dist(p1, q1, p2, q2):
    d1 = [q1[i] - p1[i] for i in range(3)]
    d2 = [q2[i] - p2[i] for i in range(3)]
    r = [p1[i] - p2[i] for i in range(3)]
    a = sum(x * x for x in d1)
    e = sum(x * x for x in d2)
    f = sum(d2[i] * r[i] for i in range(3))
    eps = 1e-9
    if a <= eps and e <= eps:
        return math.dist(p1, p2)
    if a <= eps:
        s, t = 0.0, min(max(f / e, 0.0), 1.0)
    else:
        c = sum(d1[i] * r[i] for i in range(3))
        if e <= eps:
            t, s = 0.0, min(max(-c / a, 0.0), 1.0)
        else:
            b = sum(d1[i] * d2[i] for i in range(3))
            den = a * e - b * b
            s = min(max((b * f - c * e) / den, 0.0), 1.0) if den > eps else 0.0
            t = (b * s + f) / e
            if t < 0.0:
                t, s = 0.0, min(max(-c / a, 0.0), 1.0)
            elif t > 1.0:
                t, s = 1.0, min(max((b - c) / a, 0.0), 1.0)
    return math.dist([p1[i] + d1[i] * s for i in range(3)], [p2[i] + d2[i] * t for i in range(3)])


def ok_contact(clip, n1, n2):
    """Bones data/clip_limits.json says are meant to touch in this clip."""
    for limbs, others in LIMITS.get("contacts", {}).get(clip, []):
        if n1.startswith(tuple(limbs)) and n2.startswith(tuple(others)):
            return True
    return False


def related(a, b, bones):
    for x, y in ((a, b), (b, a)):
        p = bones[x]["parent"]
        for _ in range(2):
            if p == y:
                return True
            if p < 0:
                break
            p = bones[p]["parent"]
    return False


def capsules(m, vals, lower_row, upper_row):
    bones, joints, above = m["bones"], m["joints"], set(m["above"])
    b = mat(vals, joints, lower_row, m["boundary"])
    out = []
    for i, bone in enumerate(bones):
        r = radius(bone["name"])
        if r <= 0.015:
            continue
        mm = mul(b, mat(vals, joints, upper_row, i)) if i in above else mat(vals, joints, lower_row, i)
        out.append((bone["name"], xf(mm, bone["head"]), xf(mm, bone["tail"]), r, i))
    return out


def main():
    fails, checked, clips = [], 0, 0
    for f in sorted((ROOT / "data" / "mesh").glob("*.json")):
        m, c, vals = load(f.stem)
        bones = m["bones"]
        base = {}
        for i, a in enumerate(bones):
            for j, t in enumerate(bones):
                if a["name"].startswith(LIMBS) and t["name"].startswith(TORSO):
                    d = radius(a["name"]) + radius(t["name"]) - seg_dist(a["head"], a["tail"], t["head"], t["tail"])
                    base[(i, j)] = max(0.0, d)
        clips += len(c["lower"]) + len(c["upper"])
        for lc in c["lower"]:
            on_floor = lc["name"] not in LIMITS["off_the_floor"]
            for k in range(lc["frames"]):
                for uc in c["upper"]:
                    for u in range(uc["frames"]):
                        name = f"{f.stem} {lc['name']} {k} + {uc['name']} {u}"
                        caps = capsules(m, vals, lc["row"] + k, uc["row"] + u)
                        checked += 1
                        if on_floor:
                            for bn, h, t, r, _ in caps:
                                low = min(h[1], t[1]) - r
                                if low < -LIMITS["ground"]:
                                    fails.append(f"{name}: {bn} is {-low:.3f} m below the floor")
                        torso = [x for x in caps if x[0].startswith(TORSO)]
                        for limb in [x for x in caps if x[0].startswith(LIMBS)]:
                            for tb in torso:
                                if related(limb[4], tb[4], bones) or ok_contact(uc["name"], limb[0], tb[0]):
                                    continue
                                depth = limb[3] + tb[3] - seg_dist(limb[1], limb[2], tb[1], tb[2]) - base.get((limb[4], tb[4]), 0.0)
                                if depth > LIMITS["self"]:
                                    fails.append(f"{name}: {limb[0]} {depth:.3f} m into {tb[0]}")
    for line in fails if ALL else fails[:40]:
        print(line)
    if fails:
        print(f"clip check: {len(fails)} failure(s) in {checked} frames", file=sys.stderr)
        return 1
    print(f"clip check: {checked} frames across {clips} clips, nothing clips")
    return 0


if __name__ == "__main__":
    sys.exit(main())
