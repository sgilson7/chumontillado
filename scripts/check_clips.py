#!/usr/bin/env python3
"""No clip clips: rebuild every baked clip's bone capsules and measure.

For each model in data/mesh/*.json, for every frame of every clip, each kept
bone becomes a capsule (its rest head and tail moved by that frame's skin
matrix, with the manifest's radius). The check fails when, deeper than the
limits in data/clip_limits.json:

  - ground: any capsule's lowest point is below the ground (y < 0);
  - self: an arm or leg capsule passes into its own body's torso capsules
    further than the two already overlap in the rest pose (capsules are
    rough near joints, so the bind pose is the baseline; bones next to
    each other in the chain are skipped);
  - pair: a capsule of one body passes into a capsule of another body drawn
    with it (a held soldier ahead of Big Chum, a body on her shoulder or on
    the mule, Big Chum on the mule), at the offsets the core uses.

Pure Python, reading the same files the page draws, so CI runs it without
Blender. tests/clip_defect.py plants a defect and requires this to fail.

    python3 scripts/check_clips.py [repo]
Prints one line per failure (model clip frame: what, depth) and exits 1, or
prints a summary and exits 0.
"""
import json
import math
import struct
import sys
from pathlib import Path

ROOT = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[1]
LIMITS = json.loads((ROOT / "data" / "clip_limits.json").read_text())
LIMBS = ("upperarm", "lowerarm", "wrist", "upperleg", "lowerleg", "foot")
TORSO = ("spine01", "spine02", "spine03", "pelvis")


def load(model):
    m = json.loads((ROOT / "data" / "mesh" / f"{model}.json").read_text())
    raw = (ROOT / "web" / m["anim"]).read_bytes()
    n = len(raw) // 2
    vals = struct.unpack(f"<{n}e", raw)
    return m, vals


def mat(vals, width, row, bone):
    base = (row * width + bone * 3) * 4
    r = vals[base:base + 12]
    return (r[0:4], r[4:8], r[8:12])


def xf(m, p):
    return tuple(m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3] for i in range(3))


def capsules(model, frame_row, offset=(0.0, 0.0, 0.0)):
    m, vals = MODELS[model]
    out = []
    for i, b in enumerate(m["bones"]):
        if b["radius"] <= 0.015:
            continue
        mm = mat(vals, m["anim_width"], frame_row, i)
        h = xf(mm, b["head"])
        t = xf(mm, b["tail"])
        h = (h[0] + offset[0], h[1] + offset[1], h[2] + offset[2])
        t = (t[0] + offset[0], t[1] + offset[1], t[2] + offset[2])
        out.append((b["name"], h, t, b["radius"], b["parent"], i))
    return out


def seg_dist(p1, q1, p2, q2):
    """Closest distance between segments p1q1 and p2q2."""
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
    c1 = [p1[i] + d1[i] * s for i in range(3)]
    c2 = [p2[i] + d2[i] * t for i in range(3)]
    return math.dist(c1, c2)


def related(a, b, bones):
    """True for a bone and its parent or grandparent: joints touch."""
    for x, y in ((a, b), (b, a)):
        p = bones[x]["parent"]
        for _ in range(2):
            if p == y:
                return True
            if p < 0:
                break
            p = bones[p]["parent"]
    return False


def ok_contact(clip, n1, n2):
    for rule in LIMITS.get("contacts", {}).get(clip, []):
        if n1.startswith(tuple(rule[0])) and n2.startswith(tuple(rule[1])):
            return True
        if n2.startswith(tuple(rule[0])) and n1.startswith(tuple(rule[1])):
            return True
    return False


MODELS = {}
for f in sorted((ROOT / "data" / "mesh").glob("*.json")):
    MODELS[f.stem] = load(f.stem)


def rest_overlap(model):
    """Each limb-torso pair's overlap in the rest pose: the baseline."""
    m, _ = MODELS[model]
    out = {}
    bones = m["bones"]
    for i, a in enumerate(bones):
        if not a["name"].startswith(LIMBS):
            continue
        for j, b in enumerate(bones):
            if b["name"].startswith(TORSO):
                d = a["radius"] + b["radius"] - seg_dist(a["head"], a["tail"], b["head"], b["tail"])
                out[(i, j)] = max(0.0, d)
    return out


def main():
    fails = []
    checked = 0
    ground_lim, self_lim, pair_lim = LIMITS["ground"], LIMITS["self"], LIMITS["pair"]
    for model, (m, _) in MODELS.items():
        bones = m["bones"]
        base = rest_overlap(model)
        for c in m["clips"]:
            name = f"{model}__{c['name']}"
            exempt_ground = name in LIMITS.get("ground_exempt", [])
            for k in range(c["frames"]):
                row = c["row"] + k
                caps = capsules(model, row)
                checked += 1
                for bname, h, t, r, _, _ in caps:
                    low = min(h[1], t[1]) - r
                    if not exempt_ground and low < -ground_lim:
                        fails.append(f"{name} {k}: {bname} is {-low:.3f} m below the ground")
                if model != "mule":
                    torso = [x for x in caps if x[0].startswith(TORSO)]
                    for limb in [x for x in caps if x[0].startswith(LIMBS)]:
                        for tb in torso:
                            if related(limb[5], tb[5], bones) or ok_contact(name, limb[0], tb[0]):
                                continue
                            depth = limb[3] + tb[3] - seg_dist(limb[1], limb[2], tb[1], tb[2])
                            depth -= base.get((limb[5], tb[5]), 0.0)
                            if depth > self_lim:
                                fails.append(f"{name} {k}: {limb[0]} {depth:.3f} m into {tb[0]}")
                pairs = []
                if c.get("pair"):
                    pairs.append(c["pair"])
                pairs += [[a, [0, 0, 0]] for a in c.get("also", [])]
                for other, off in pairs:
                    om, oc = other.split("__")
                    if om not in MODELS:
                        continue
                    oclip = next((x for x in MODELS[om][0]["clips"] if x["name"] == oc), None)
                    if oclip is None:
                        fails.append(f"{name}: paired clip {other} is missing")
                        continue
                    orow = oclip["row"] + k % oclip["frames"]
                    # offsets are authored in Blender (x, y, z); the game frame is (-x, z, y)
                    o = (-off[0], off[2], off[1])
                    theirs = capsules(om, orow, o)
                    for a in caps:
                        if a[0].startswith(("finger", "eye", "jaw")):
                            continue
                        for b in theirs:
                            if b[0].startswith(("finger", "eye", "jaw")) or ok_contact(name, a[0], b[0]):
                                continue
                            depth = a[3] + b[3] - seg_dist(a[1], a[2], b[1], b[2])
                            if depth > pair_lim:
                                fails.append(f"{name} {k} with {other}: {a[0]} {depth:.3f} m into {b[0]}")
    for line in fails[:60]:
        print(line)
    if fails:
        print(f"clip check: {len(fails)} failure(s) in {checked} frames ({len(fails) - min(len(fails), 60)} more not shown)", file=sys.stderr)
        return 1
    print(f"clip check: {checked} frames across {sum(len(m['clips']) for m, _ in MODELS.values())} clips, nothing clips")
    return 0


if __name__ == "__main__":
    sys.exit(main())
