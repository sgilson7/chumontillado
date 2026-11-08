"""Bake one of Chumontillado's lifelike skins for the browser, headless.

    blender -b STYLE_LAB/notes/costumes/final/<id>.blend --python tools/mesh/bake_game.py -- <id> GAME_REPO

Copied from Style Lab's tools/mesh/bake_game.py at 5f18c50 (CS-2) and
changed for Chumontillado. For the skin `<id>` (its body `<id>`, rig
`<id>_rig`, costume pieces `<id>.<piece>`):

- **Mesh.** The body with its masks applied (no faces under the costume), and
  every costume piece with its modifiers but subdivision applied, each
  decimated to a share of `BUDGET` triangles by its area; teeth and tongue
  left out. Brows, lashes, a hair mesh and the eyes keep their own pictures.
- **One picture** for the body and costume (`ATLAS` px): their UVs packed
  together (the body's islands at `SKIN_DENSITY` times the costume's texel
  density, for the face), then baked from the full-detail originals, the
  body with its particle hair (beards, brows, cropped hair land in the skin)
  and the costume's procedural colours. Style Lab's particle hair becomes
  part of the skin picture, not cards (SECOND-ORDER row).
- **Floor.** Pieces named in `FLOOR` (the Infanta's farthingale and its
  bands, the Friar's habit below the belt) have their hem weighted to an extra joint that never moves from the
  feet, so the hoops fold down when she ducks and hang when she jumps.
- **Clips** from game_clips.py, every frame's skin matrices (game frame) as
  half floats, one row a frame: the lower clips, then the upper.
- **For the core** (data/skins.json, one entry a skin): the clip table; for
  each lower frame the boundary bone's skin matrix and its forward tilt;
  for each upper frame the hold frame (where the weapon sits, from the right
  wrist as it ended up); and each piece's reach (furthest from the upright
  line, highest) over every lower frame with every upper frame.

Game (x, y, z) = Blender (-x, z, y); heading zero faces -z.
Writes GAME_REPO/assets/mesh/<id>.bin, <id>_anim.bin, tex/<id>_*.jpg|png,
GAME_REPO/data/mesh/<id>.json (the page's manifest) and
GAME_REPO/data/skins/<id>.json (the core's part, joined by scripts/join_skins.py).
"""
import json
import math
import struct
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).parent))
import game_clips as GC  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
ID = args[0]
GAME = Path(args[1])
WEB = GAME / "assets" / "mesh"
TEX = WEB / "tex"
DATA = GAME / "data" / "mesh"
CORE = GAME / "data" / "skins"
for d in (WEB, TEX, DATA, CORE):
    d.mkdir(parents=True, exist_ok=True)
OB = bpy.data.objects
A = Matrix(((-1, 0, 0, 0), (0, 0, 1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))   # Blender to game
AI = A.inverted()
SKIP = ("teeth", "tongue")
DROP = ("oris", "levator", "risorius", "temporalis", "orbicularis", "oculi", "tongue", "special", "breast",
        "toe", "metacarpal", "eye", "jaw", "neck02", "neck03")
BODY_TRIS = 4500
BUDGET = 8500          # the costume's triangles
SIDE_TRIS = 1500       # each brow, lash, hair or eye mesh at most
ATLAS = 2048
SKIN_DENSITY = 2.2
SIDE_PX = 512
FLOOR = {"infanta": ("farthingale", "band_"), "fraile": ("habit_skirt",)}
WAIST = {"infanta": 1.0, "fraile": 0.95}


def say(*a):
    print("bake:", ID, *a, flush=True)


def lin_to_srgb(c):
    return [(12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055) for x in c]


def select(obs, active):
    bpy.ops.object.select_all(action="DESELECT")
    for o in obs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = active


def tris_of(ob):
    return sum(len(p.vertices) - 2 for p in ob.data.polygons)


def area_of(ob):
    return sum(p.area for p in ob.data.polygons)


rig = OB[f"{ID}_rig"]
body = OB[ID]
rig.data.pose_position = "REST"
bpy.context.view_layer.update()
kept = [b.name for b in rig.data.bones if not b.name.startswith(DROP)
        and not (b.name.startswith("finger") and b.name.endswith(("-3.L", "-3.R")))]
index = {n: i for i, n in enumerate(kept)}
FLOOR_JOINT = len(kept)


def owner_bone(name):
    b = rig.data.bones.get(name)
    while b is not None and b.name not in index:
        b = b.parent
    return b.name if b else kept[0]


def copy_of(ob, keep_subsurf):
    """A copy in the rest pose with shape keys mixed in and every modifier but
    the armature applied (subdivision only when asked); vertex groups kept."""
    cp = ob.copy()
    cp.data = ob.data.copy()
    bpy.context.scene.collection.objects.link(cp)
    cp.parent = None
    cp.matrix_world = ob.matrix_world.copy()
    select([cp], cp)
    if cp.data.shape_keys:
        bpy.ops.object.shape_key_remove(all=True, apply_mix=True)
    for m in list(cp.modifiers):
        if m.type in ("ARMATURE", "PARTICLE_SYSTEM") or (m.type == "SUBSURF" and not keep_subsurf):
            cp.modifiers.remove(m)
            continue
        try:
            bpy.ops.object.modifier_apply(modifier=m.name)
        except RuntimeError:
            cp.modifiers.remove(m)
    return cp


def decimate(cp, target):
    n = tris_of(cp)
    if n > target:
        select([cp], cp)
        d = cp.modifiers.new("dec", "DECIMATE")
        d.ratio = max(target / n, 0.002)
        bpy.ops.object.modifier_apply(modifier="dec")


def image_of(ob):
    m = ob.active_material
    if m is None or not m.use_nodes:
        return None, [0.6, 0.6, 0.6]
    nodes = m.node_tree.nodes
    img = next((n.image for n in nodes if n.type == "TEX_IMAGE" and n.image), None)
    bsdf = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
    tint = [1.0, 1.0, 1.0]
    mix = next((n for n in nodes if n.type == "MIX" and n.blend_type == "MULTIPLY" and not n.inputs[7].is_linked), None)
    if mix is not None:
        tint = list(mix.inputs[7].default_value)[:3]
    if img is None and bsdf is not None and not bsdf.inputs["Base Color"].is_linked:
        tint = list(bsdf.inputs["Base Color"].default_value)[:3]
    return img, tint


def save_image(img, name, alpha, px):
    im = img.copy()
    if max(im.size) > px:
        im.scale(min(px, im.size[0]), min(px, im.size[1]))
    ext = "png" if alpha else "jpg"
    im.file_format = "PNG" if alpha else "JPEG"
    im.filepath_raw = str(TEX / f"{name}.{ext}")
    im.save()
    bpy.data.images.remove(im)
    return f"{name}.{ext}"


# ---- the pieces ------------------------------------------------------------------------
pieces = []      # (original, kind) kind: body, costume, side
for ob in OB:
    if ob.type != "MESH" or ob.hide_render or not (ob == body or ob.name.startswith(ID + ".")):
        continue
    if any(k in ob.name for k in SKIP):
        continue
    if not any(m.type == "ARMATURE" and m.object == rig for m in ob.modifiers) and ob.parent != rig:
        continue
    img, _ = image_of(ob)
    if ob == body:
        kind = "body"
    elif img is not None:
        kind = "side"
    else:
        kind = "costume"
    pieces.append((ob, kind))
say("pieces", [(o.name, k) for o, k in pieces])

copies = {}
for ob, kind in pieces:
    copies[ob.name] = copy_of(ob, keep_subsurf=False)
costume = [ob for ob, k in pieces if k == "costume"]
total_area = sum(area_of(copies[o.name]) for o in costume) or 1.0
for ob, kind in pieces:
    cp = copies[ob.name]
    if kind == "body":
        decimate(cp, BODY_TRIS)
    elif kind == "side":
        decimate(cp, SIDE_TRIS)
    else:
        share = max(60, int(BUDGET * area_of(cp) / total_area))
        decimate(cp, share)

# the floor's weight: the hem on the floor joint, fading to none at the waist
floor_pieces = FLOOR.get(ID, ())
for ob, kind in pieces:
    if kind == "costume" and any(f in ob.name for f in floor_pieces):
        cp = copies[ob.name]
        g = cp.vertex_groups.new(name="__floor")
        for v in cp.data.vertices:
            z = (cp.matrix_world @ v.co).z
            w = max(0.0, min(1.0, 1.0 - z / WAIST[ID])) ** 0.7
            if w > 0:
                g.add([v.index], w, "REPLACE")

# ---- one picture for the body and the costume ------------------------------------------
tbody = copies[body.name]
tcost = [copies[o.name] for o in costume]
# a piece number on every costume vertex, for the reach
for k, cp in enumerate(tcost):
    a = cp.data.attributes.new("piece", "INT", "POINT")
    a.data.foreach_set("value", [k + 1] * len(cp.data.vertices))
if tcost:
    select(tcost, tcost[0])
    bpy.ops.object.join()
    tc = tcost[0]
    tc.name = f"{ID}__costume"
    for uv in list(tc.data.uv_layers):
        tc.data.uv_layers.remove(uv)
    tc.data.uv_layers.new(name="UVMap")
    select([tc], tc)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.0)
    bpy.ops.object.mode_set(mode="OBJECT")
else:
    tc = None
tbody.data.uv_layers.active.name = "UVMap"
targets = [tbody] + ([tc] if tc else [])
select(targets, tbody)
bpy.ops.object.mode_set(mode="EDIT")
bpy.ops.mesh.select_all(action="SELECT")
bpy.ops.uv.select_all(action="SELECT")
bpy.ops.uv.average_islands_scale()
bpy.ops.object.mode_set(mode="OBJECT")
uvl = tbody.data.uv_layers["UVMap"].data
for d in uvl:
    d.uv = d.uv * SKIN_DENSITY
select(targets, tbody)
bpy.ops.object.mode_set(mode="EDIT")
bpy.ops.mesh.select_all(action="SELECT")
bpy.ops.uv.select_all(action="SELECT")
bpy.ops.uv.pack_islands(rotate=True, margin=0.006)
bpy.ops.object.mode_set(mode="OBJECT")

scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.samples = 16
scene.cycles.device = "CPU"
atlas = bpy.data.images.new(f"{ID}_atlas", ATLAS, ATLAS)
bk = scene.render.bake
bk.use_pass_direct = bk.use_pass_indirect = False
bk.use_pass_color = True
bk.use_selected_to_active = True
bk.margin = 4


def target_material(ob):
    m = bpy.data.materials.new(f"{ob.name}_bake")
    m.use_nodes = True
    tn = m.node_tree.nodes.new("ShaderNodeTexImage")
    tn.image = atlas
    m.node_tree.nodes.active = tn
    ob.data.materials.clear()
    ob.data.materials.append(m)


# a metal's diffuse colour bakes black: bake its base colour, metal or not
# (in memory only; Style Lab's file is not saved)
for ob in costume:
    for slot in ob.material_slots:
        m = slot.material
        if m is None or not m.use_nodes:
            continue
        for n in m.node_tree.nodes:
            if n.type == "BSDF_PRINCIPLED":
                inp = n.inputs["Metallic"]
                for link in list(inp.links):
                    m.node_tree.links.remove(link)
                inp.default_value = 0.0

# the originals in the rest pose with subdivision, the body with its hair
for t, sources, extrude, clear in [(tbody, [body], 0.07, True), (tc, costume, 0.015, False)]:
    if t is None:
        continue
    target_material(t)
    select(sources + [t], t)
    bk.cage_extrusion = extrude
    bk.max_ray_distance = extrude * 2.5
    bpy.ops.object.bake(type="DIFFUSE", use_clear=clear)
atlas.file_format = "JPEG"
atlas.filepath_raw = str(TEX / f"{ID}_atlas.jpg")
scene.render.image_settings.quality = 88
atlas.save()
if tc:
    pa = tbody.data.attributes.new("piece", "INT", "POINT")
    pa.data.foreach_set("value", [0] * len(tbody.data.vertices))
    select([tbody, tc], tbody)
    bpy.ops.object.join()
say("atlas baked")

# ---- vertices ---------------------------------------------------------------------------
piece_names = ["body"] + [o.name.split(".", 1)[1] for o in costume]


def vertices_of(cp):
    me = cp.data
    me.calc_loop_triangles()
    uv = me.uv_layers.active.data if me.uv_layers else None
    groups = {g.index: g.name for g in cp.vertex_groups}
    piece_attr = me.attributes.get("piece")
    verts, vmap, idx = [], {}, []
    mw = cp.matrix_world
    nmat = mw.to_3x3().inverted().transposed()
    for tri in me.loop_triangles:
        for li, vi in zip(tri.loops, tri.vertices):
            v = me.vertices[vi]
            u = tuple(uv[li].uv) if uv else (0.0, 0.0)
            keyv = (vi, round(u[0], 4), round(u[1], 4))
            if keyv not in vmap:
                p = A @ (mw @ v.co)
                n = (A.to_3x3() @ (nmat @ (me.corner_normals[li].vector if hasattr(me, "corner_normals") else v.normal))).normalized()
                ws = {}
                floor = 0.0
                for g in v.groups:
                    name = groups.get(g.group)
                    if name == "__floor":
                        floor = g.weight
                        continue
                    if name is None or g.weight <= 0.0 or name not in rig.data.bones:
                        continue
                    o = index[owner_bone(name)]
                    ws[o] = ws.get(o, 0.0) + g.weight
                tot = sum(ws.values()) or 1.0
                ws = {k: w / tot * (1.0 - floor) for k, w in ws.items()}
                if floor > 0:
                    ws[FLOOR_JOINT] = floor
                top = sorted(ws.items(), key=lambda kv: -kv[1])[:4] or [(index[owner_bone("spine05")], 1.0)]
                t = sum(w for _, w in top)
                j = [k for k, _ in top] + [0] * (4 - len(top))
                wq = [int(round(255 * w / t)) for _, w in top] + [0] * (4 - len(top))
                wq[0] += 255 - sum(wq)
                pc = piece_attr.data[vi].value if piece_attr else 0
                vmap[keyv] = len(verts)
                verts.append((tuple(p), tuple(n), u, j, wq, pc))
            idx.append(vmap[keyv])
    return verts, idx


blob = bytearray()
pmeta = []
all_verts = []
STRIDE = 32


def emit(name, verts, idx, texture, tint, alpha):
    off = len(blob)
    for p, n, u, j, w, _ in verts:
        blob.extend(struct.pack("<3f", *p) + struct.pack("<3b", *(int(max(-127, min(127, c * 127))) for c in n)) + b"\0")
        # v from the top, as WebGL reads a picture's rows
        blob.extend(struct.pack("<2f", u[0], 1.0 - u[1]) + bytes(j) + bytes(w))
    ioff = len(blob)
    big = len(verts) > 65535
    for i in idx:
        blob.extend(struct.pack("<I" if big else "<H", i))
    while len(blob) % 4:
        blob.append(0)
    pmeta.append({"name": name, "vertices": len(verts), "offset": off, "stride": STRIDE, "indices": len(idx),
                  "index_offset": ioff, "index_bytes": 4 if big else 2, "texture": texture,
                  "tint": lin_to_srgb(tint), "alpha_test": alpha})
    say(name, len(verts), "vertices", len(idx) // 3, "triangles", texture)


verts, idx = vertices_of(tbody)
emit(f"{ID}", verts, idx, f"{ID}_atlas.jpg", [1, 1, 1], False)
all_verts += verts
for ob, kind in pieces:
    if kind != "side":
        continue
    cp = copies[ob.name]
    img, tint = image_of(ob)
    alpha = img.depth == 32 and not ob.name.endswith("high-poly")
    safe = ob.name.replace(".", "_").replace("-", "_")
    tex = save_image(img, f"{safe}", alpha, SIDE_PX)
    verts, idx = vertices_of(cp)
    for i, v in enumerate(verts):
        verts[i] = v[:5] + (-1,)
    emit(ob.name, verts, idx, tex, tint, alpha)
    all_verts += verts
(WEB / f"{ID}.bin").write_bytes(bytes(blob))

# ---- clips --------------------------------------------------------------------------------
rig.data.pose_position = "POSE"
clips = GC.Clips(rig, ID)
GC.author(clips)
rest_inv = {n: rig.data.bones[n].matrix_local.inverted() for n in kept}
above = [index[n] for n in kept if n in clips.above]
boundary = index[GC.BOUNDARY]

def skin_mats():
    bpy.context.view_layer.update()
    out = []
    for n in kept:
        pb = rig.pose.bones[n]
        out.append(A @ (rig.matrix_world @ pb.matrix @ rest_inv[n]) @ AI)
    out.append(Matrix.Identity(4))     # the floor joint
    return out


def half(x):
    return struct.pack("<e", max(-65000.0, min(65000.0, x)))


def flat(m):
    return [round(m[r][c], 5) for c in range(4) for r in range(3)]   # columns of 3, as the core's Affine


anim = bytearray()
rows = []
lower_table, upper_table = [], []
lower_mats, upper_mats = [], []
boundary_rows, tilt_rows, hold_rows = [], [], []
for name, loop, frames in clips.lower:
    lower_table.append({"name": name, "frames": len(frames), "row": len(rows), "loop": loop})
    for go in frames:
        go()
        ms = skin_mats()
        rows.append(ms)
        lower_mats.append(ms)
        b = ms[boundary]
        boundary_rows.append(flat(b))
        up = b.to_3x3() @ Vector((0, 1, 0))
        tilt_rows.append(round(math.degrees(math.atan2(-up.z, up.y)), 3))
for name, frames in clips.upper:
    upper_table.append({"name": name, "frames": len(frames), "row": len(rows), "loop": False})
    for go in frames:
        go()
        ms = skin_mats()
        rows.append(ms)
        upper_mats.append(ms)
        # the hold frame: where the right wrist ended up, as the weapon's
        # frame. Its own axes are already the weapon's (right, up, back), so
        # only the world it sits in changes to the game's
        w = clips.world_of("wrist.R") @ clips.offset["R"].inverted()
        hold_rows.append(flat(A @ w))
clips.clear()
bpy.context.view_layer.update()
for ms in rows:
    for m in ms:
        for r in range(3):
            for c in range(4):
                anim.extend(half(m[r][c]))
# the last row flags each joint drawn from the upper rows (above the boundary)
for j in range(len(kept) + 1):
    for c in range(12):
        anim.extend(half(1.0 if c == 0 and j in above else 0.0))
(WEB / f"{ID}_anim.bin").write_bytes(bytes(anim))
say(len(rows), "frames,", len(kept) + 1, "joints")

# ---- reach: every piece, every lower frame with every upper frame -------------------------
P = np.array([v[0] for v in all_verts], dtype=np.float64)
J = np.array([v[3] for v in all_verts], dtype=np.int64)
W = np.array([v[4] for v in all_verts], dtype=np.float64) / 255.0
PC = np.array([v[5] for v in all_verts], dtype=np.int64)
is_above = np.zeros(len(kept) + 1, dtype=bool)
is_above[above] = True
P1 = np.c_[P, np.ones(len(P))]


def stack(ms):
    return np.array([[list(m[r]) for r in range(3)] for m in ms])   # joints x 3 x 4


lower_np = [stack(ms) for ms in lower_mats]
upper_np = [stack(ms) for ms in upper_mats]
order = np.argsort(PC, kind="stable")
groups, starts = np.unique(PC[order], return_index=True)
tail4 = np.tile([[[0.0, 0.0, 0.0, 1.0]]], (int(is_above.sum()), 1, 1))
reach = {}
for c in lower_table:
  for u in upper_table:
    worst = {}
    for k in range(c["frames"]):
        L = lower_np[c["row"] + k]
        b4 = np.vstack([L[boundary], [0, 0, 0, 1]])
        for U in upper_np[u["row"] - upper_table[0]["row"]:u["row"] - upper_table[0]["row"] + u["frames"]]:
            M = L.copy()
            M[is_above] = (b4 @ np.concatenate([U[is_above], tail4], axis=1))[:, :3, :]
            pos = np.einsum("vkij,vj->vki", M[J], P1)
            pos = (pos * W[:, :, None]).sum(axis=1)[order]
            r = np.maximum.reduceat(np.hypot(pos[:, 0], pos[:, 2]), starts)
            h = np.maximum.reduceat(pos[:, 1], starts)
            lo = np.minimum.reduceat(pos[:, 1], starts)
            for g, a_, h_, l_ in zip(groups, r, h, lo):
                name = "side" if g < 0 else piece_names[g]
                o = worst.get(name, (0.0, 0.0, 9.0))
                worst[name] = (max(o[0], float(a_)), max(o[1], float(h_)), min(o[2], float(l_)))
    reach[c["name"] + "+" + u["name"]] = {n: [round(a_, 3), round(h_, 3), round(l_, 3)] for n, (a_, h_, l_) in sorted(worst.items())}
say("reach", json.dumps(reach))

manifest = {"_about": f"Chumontillado's lifelike {ID}, baked by tools/mesh/bake_game.py from Style Lab's notes/costumes/final/{ID}.blend. Vertex layout: position 3 x f32, normal 3 x i8 and a pad, uv 2 x f32 (v from the top), joints 4 x u8, weights 4 x u8 (sum 255). The animation file holds, for each frame in order, every joint's skin matrix as three rows of four half floats; the last joint is the floor and never moves; the last row (flags_row) holds 1 in each joint's first value for a joint above the boundary. Joints listed in `above` are drawn as the lower row's boundary joint times their upper row's matrix.",
            "model": ID, "bin": f"mesh/{ID}.bin", "anim": f"mesh/{ID}_anim.bin", "parts": pmeta,
            "anim_rows": len(rows) + 1, "flags_row": len(rows), "anim_width": (len(kept) + 1) * 3, "joints": len(kept) + 1,
            "boundary": boundary, "above": above,
            "attrib": {"position": [3, 0], "normal": [3, 12], "uv": [2, 16], "joints": [4, 24], "weights": [4, 28]},
            "bones": [{"name": n, "head": list(A @ rig.data.bones[n].head_local), "tail": list(A @ rig.data.bones[n].tail_local),
                       "parent": index[owner_bone(rig.data.bones[n].parent.name)] if rig.data.bones[n].parent else -1}
                      for n in kept]}
(DATA / f"{ID}.json").write_text(json.dumps(manifest))
(CORE / f"{ID}.json").write_text(json.dumps({
    "skin": ID, "lower": lower_table, "upper": upper_table, "boundary": boundary_rows, "tilt": tilt_rows,
    "hold": hold_rows, "reach": reach}, separators=(",", ":")))
say("done", len(blob), "bytes mesh,", len(anim), "bytes anim")
