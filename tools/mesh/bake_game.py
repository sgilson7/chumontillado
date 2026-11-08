"""Bake Espiochum's cast for the browser, headless.

    blender -b OUTDIR/game.blend --python tools/mesh/bake_game.py -- OUTDIR GAME_REPO

For each model (chum, soldier, grace, mule): every mesh that belongs to it,
shape keys and modifiers applied, decimated, in the game's frame (y up,
heading zero faces -z: game (x, y, z) = Blender (-x, z, y)); skin weights
onto a reduced bone set (four per vertex); gear parented to a bone is
skinned rigidly to it; each part's colour as a texture (its own image, or
baked from its procedural material) with a tint. Every clip is sampled to
bone matrices, written as half floats for a texture the shader reads.

Writes GAME_REPO/web/mesh/<model>.bin, <model>_anim.bin, tex/*.png|jpg and
GAME_REPO/data/mesh/<model>.json (the manifest: every offset, count and
size), and GAME_REPO/data/clips.json (the clip table the core reads).
"""
import json
import math
import struct
import sys
from pathlib import Path

import bpy
from mathutils import Matrix, Vector

args = sys.argv[sys.argv.index("--") + 1:]
OUT = Path(args[0])
GAME = Path(args[1])
WEB = GAME / "web" / "mesh"
TEX = WEB / "tex"
DATA = GAME / "data" / "mesh"
for d in (WEB, TEX, DATA):
    d.mkdir(parents=True, exist_ok=True)
META = json.loads((OUT / "game_clips.json").read_text())
SHEETS = {m: json.loads((Path(__file__).parent / f).read_text())
          for m, f in (("chum", "bigchum.json"), ("soldier", "veteran.json"), ("grace", "grace.json"))}
OB = bpy.data.objects
MODELS = {"chum": ("bigchum", "bigchum_rig", 6000), "soldier": ("veteran", "veteran_rig", 6000),
          "grace": ("grace", "grace_rig", 6000), "mule": ("mule", "mule_rig", 40000)}
# faces do not animate in game, so the mouth stays shut: no teeth or tongue
SKIP = ("teeth", "tongue")
A = Matrix(((-1, 0, 0, 0), (0, 0, 1, 0), (0, 1, 0, 0), (0, 0, 0, 1)))   # Blender to game
AI = A.inverted()
DROP = ("oris", "levator", "risorius", "temporalis", "orbicularis", "oculi", "tongue", "special", "breast",
        "toe2", "toe3", "toe4", "toe5", "toe1-2", "metacarpal")
TEX_PX = 1024


def say(*a):
    print("bake:", *a, flush=True)


def lin_to_srgb(c):
    return [(12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055) for x in c]


def rest(rig, on):
    """Put a rig in its rest pose (no action, no IK) or back."""
    if on:
        rig["_act"] = rig.animation_data.action.name if rig.animation_data and rig.animation_data.action else ""
        if rig.animation_data:
            rig.animation_data.action = None
        rig.location, rig.rotation_euler = (0, 0, 0), (0, 0, 0)
        for pb in rig.pose.bones:
            pb.rotation_mode = "XYZ"
            pb.rotation_euler = (0, 0, 0)
            pb.location = (0, 0, 0)
            for c in pb.constraints:
                c.mute = True
    bpy.context.view_layer.update()


def kept_bones(rig):
    return [b.name for b in rig.data.bones if not b.name.startswith(DROP)]


def owner_bone(rig, name, kept):
    b = rig.data.bones.get(name)
    while b is not None and b.name not in kept:
        b = b.parent
    return b.name if b else kept[0]


def parts_of(prefix, rig):
    out = []
    for ob in OB:
        if ob.type != "MESH" or not ob.name.startswith(prefix) or ob.hide_render:
            continue
        if ob.name.startswith(("mule_hair",)) or any(k in ob.name for k in SKIP):
            continue
        skinned = any(m.type == "ARMATURE" and m.object == rig for m in ob.modifiers)
        top = ob
        while top.parent is not None and top.parent != rig:
            top = top.parent
        rigid = top.parent == rig and top.parent_type == "BONE"
        if skinned or rigid:
            out.append((ob, skinned, top.parent_bone if rigid else None))
    return out


def colour_source(ob, model):
    """(texture image or None, tint linear rgb, alpha test, needs bake)"""
    m = ob.active_material
    if m is None or not m.use_nodes:
        return None, [0.6, 0.6, 0.6], False, False
    nodes = m.node_tree.nodes
    bsdf = next((n for n in nodes if n.type == "BSDF_PRINCIPLED"), None)
    img = next((n.image for n in nodes if n.type == "TEX_IMAGE" and n.image), None)
    base = bsdf.inputs["Base Color"] if bsdf else None
    tint = [1.0, 1.0, 1.0]
    mix = next((n for n in nodes if n.type == "MIX" and n.blend_type == "MULTIPLY" and not n.inputs[7].is_linked), None)
    if mix is not None:
        tint = list(mix.inputs[7].default_value)[:3]
    procedural = base is not None and base.is_linked and img is None
    alpha = bool(img and img.depth == 32 and any(k in ob.name for k in ("hair", "brow", "lash", "ponytail", "short")))
    if base is not None and not base.is_linked:
        return None, list(base.default_value)[:3], False, False
    if ob.name.endswith(("_skin",)) or ob.name in ("bigchum", "veteran", "grace"):
        return "skin", [1.0, 1.0, 1.0], False, False
    if any(k in (ob.active_material.name if ob.active_material else "") for k in ("camo",)) or procedural:
        return None, [1.0, 1.0, 1.0], False, True
    if mix is None and any(n.type == "MIX" for n in nodes) and img is not None:
        # a camouflage tree that kept the stock print for shading: bake it
        return None, [1.0, 1.0, 1.0], False, True
    return img, tint, alpha, False


def save_image(img, name, alpha):
    im = img.copy()
    if max(im.size) > TEX_PX:
        im.scale(min(TEX_PX, im.size[0]), min(TEX_PX, im.size[1]))
    ext = "png" if alpha else "jpg"
    im.file_format = "PNG" if alpha else "JPEG"
    im.filepath_raw = str(TEX / f"{name}.{ext}")
    im.save()
    bpy.data.images.remove(im)
    return f"{name}.{ext}"


def bake_diffuse(ob, name):
    """Bake a part's base colour to a texture on its first UV map."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 4
    if not ob.data.uv_layers:
        bpy.context.view_layer.objects.active = ob
        bpy.ops.object.select_all(action="DESELECT")
        ob.select_set(True)
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.01)
        bpy.ops.object.mode_set(mode="OBJECT")
    img = bpy.data.images.new(f"bake_{name}", TEX_PX, TEX_PX)
    nt = ob.active_material.node_tree
    tn = nt.nodes.new("ShaderNodeTexImage")
    tn.image = img
    nt.nodes.active = tn
    bpy.ops.object.select_all(action="DESELECT")
    ob.select_set(True)
    bpy.context.view_layer.objects.active = ob
    scene.render.bake.use_pass_direct = scene.render.bake.use_pass_indirect = False
    scene.render.bake.use_pass_color = True
    scene.render.bake.margin = 24
    bpy.ops.object.bake(type="DIFFUSE", use_clear=True)
    nt.nodes.remove(tn)
    img.file_format = "JPEG"
    img.filepath_raw = str(TEX / f"{name}.jpg")
    img.save()
    return f"{name}.jpg"


def evaluated_copy(ob, target_tris):
    """A copy with shape keys mixed in and every modifier but the armature
    applied, then decimated toward target_tris."""
    cp = ob.copy()
    cp.data = ob.data.copy()
    bpy.context.scene.collection.objects.link(cp)
    cp.parent = None
    cp.matrix_world = ob.matrix_world.copy()
    bpy.context.view_layer.objects.active = cp
    bpy.ops.object.select_all(action="DESELECT")
    cp.select_set(True)
    if cp.data.shape_keys:
        bpy.ops.object.shape_key_remove(all=True, apply_mix=True)
    for m in list(cp.modifiers):
        if m.type in ("ARMATURE", "PARTICLE_SYSTEM"):
            cp.modifiers.remove(m)
            continue
        try:
            bpy.ops.object.modifier_apply(modifier=m.name)
        except RuntimeError:
            cp.modifiers.remove(m)
    if ob.name.startswith("mule"):
        # the skin modifier's mesh can face some polygons inward: make them all face out
        import bmesh
        bm = bmesh.new()
        bm.from_mesh(cp.data)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(cp.data)
        bm.free()
    tris = sum(len(p.vertices) - 2 for p in cp.data.polygons)
    if tris > target_tris:
        d = cp.modifiers.new("dec", "DECIMATE")
        d.ratio = target_tris / tris
        bpy.ops.object.modifier_apply(modifier="dec")
    cp.data.calc_loop_triangles()
    return cp


def capsule_radius(model, name):
    if model == "mule":
        if name.startswith(("torso", "hips")):
            return 0.30
        if name.startswith(("neck",)):
            return 0.16
        if name.startswith(("head",)):
            return 0.11
        if name.startswith(("ear", "jaw", "tail")):
            return 0.03
        return 0.06
    table = (("spine0", 0.13), ("pelvis", 0.12), ("neck", 0.06), ("head", 0.10), ("clavicle", 0.05), ("shoulder", 0.06),
             ("upperarm", 0.055), ("lowerarm", 0.045), ("wrist", 0.035), ("finger", 0.01), ("upperleg", 0.08),
             ("lowerleg", 0.06), ("foot", 0.045), ("toe", 0.03), ("root", 0.12), ("jaw", 0.04), ("eye", 0.012))
    return next((r for k, r in table if name.startswith(k)), 0.04)


def half(x):
    return struct.pack("<e", max(-65000.0, min(65000.0, x)))


def bake_model(model):
    prefix, rig_name, tris = MODELS[model]
    rig = OB[rig_name]
    rest(rig, True)
    kept = kept_bones(rig)
    index = {n: i for i, n in enumerate(kept)}
    parts = parts_of(prefix, rig)
    blob = bytearray()
    pmeta = []
    sheet = SHEETS.get(model, {})
    for ob, skinned, bone in parts:
        src, tint, alpha, needs_bake = colour_source(ob, model)
        tex = None
        safe = ob.name.replace(".", "_")
        if src == "skin":
            skin = None
            for d in (OUT / "tex", OUT.parent / "mesh" / "tex"):
                f = d / f"{prefix}_basecolor.png"
                if f.exists():
                    skin = bpy.data.images.load(str(f), check_existing=True)
                    break
            tex = save_image(skin, f"{model}_skin", False) if skin else None
        elif needs_bake:
            tex = bake_diffuse(ob, f"{model}_{safe}")
        elif src is not None:
            tex = save_image(src, f"{model}_{safe}", alpha)
        cp = evaluated_copy(ob, tris if skinned and ob.name == prefix else 1800)
        me = cp.data
        uv = me.uv_layers.active.data if me.uv_layers else None
        groups = {g.index: g.name for g in cp.vertex_groups}
        verts = []
        vmap = {}
        idx = []
        mw = cp.matrix_world
        nmat = mw.to_3x3().inverted().transposed()
        for tri in me.loop_triangles:
            for li, vi in zip(tri.loops, tri.vertices):
                v = me.vertices[vi]
                u = tuple(uv[li].uv) if uv else (0.0, 0.0)
                keyv = (vi, round(u[0], 4), round(u[1], 4))
                if keyv not in vmap:
                    p = A @ (mw @ v.co)
                    n = (A.to_3x3() @ (nmat @ (me.loops[li].normal if not tri.use_smooth else v.normal))).normalized()
                    if skinned:
                        ws = {}
                        for g in v.groups:
                            gname = groups.get(g.group)
                            if gname is None or g.weight <= 0.0 or gname not in rig.data.bones:
                                continue
                            ob_name = owner_bone(rig, gname, kept)
                            ws[ob_name] = ws.get(ob_name, 0.0) + g.weight
                        top = sorted(ws.items(), key=lambda kv: -kv[1])[:4] or [(kept[0], 1.0)]
                    else:
                        top = [(owner_bone(rig, bone, kept), 1.0)]
                    tot = sum(w for _, w in top)
                    j = [index[n] for n, _ in top] + [0] * (4 - len(top))
                    wq = [int(round(255 * w / tot)) for _, w in top] + [0] * (4 - len(top))
                    wq[0] += 255 - sum(wq)
                    vmap[keyv] = len(verts)
                    verts.append((p, n, u, j, wq))
                idx.append(vmap[keyv])
        bpy.data.objects.remove(cp, do_unlink=True)
        off = len(blob)
        for p, n, u, j, w in verts:
            blob += struct.pack("<3f", *p) + struct.pack("<3b", *(int(max(-127, min(127, c * 127))) for c in n)) + b"\0"
            blob += struct.pack("<2f", u[0], u[1]) + bytes(j) + bytes(w)
        stride = 12 + 4 + 8 + 4 + 4
        ioff = len(blob)
        big = len(verts) > 65535
        for i in idx:
            blob += struct.pack("<I" if big else "<H", i)
        while len(blob) % 4:
            blob += b"\0"
        pmeta.append({"name": ob.name, "vertices": len(verts), "offset": off, "stride": stride,
                      "indices": len(idx), "index_offset": ioff, "index_bytes": 4 if big else 2,
                      "texture": tex, "tint": lin_to_srgb(tint), "alpha_test": alpha,
                      "rifle": "rifle" in ob.name, "skinned": skinned})
        say(model, ob.name, len(verts), "vertices", len(idx) // 3, "triangles", tex or "flat")
    (WEB / f"{model}.bin").write_bytes(bytes(blob))
    # bones: rest heads and tails in the game frame, capsule radii
    bones = []
    for n in kept:
        b = rig.data.bones[n]
        h, t = A @ b.head_local, A @ b.tail_local
        par = b.parent
        while par is not None and par.name not in index:
            par = par.parent
        bones.append({"name": n, "head": list(h), "tail": list(t), "radius": capsule_radius(model, n),
                      "parent": index[par.name] if par else -1})
    # clips
    anim = bytearray()
    clips = []
    row = 0
    rest_inv = {n: (rig.data.bones[n].matrix_local).inverted() for n in kept}
    for act_name, meta in META.items():
        if meta["model"] != model:
            continue
        rest(rig, True)
        for pb in rig.pose.bones:
            for c in pb.constraints:
                c.mute = False
        rig.animation_data_create()
        rig.animation_data.action = bpy.data.actions[act_name]
        n = meta["frames"]
        for f in range(1, n + 1):
            bpy.context.scene.frame_set(f)
            for name in kept:
                pb = rig.pose.bones[name]
                m = A @ (rig.matrix_world @ pb.matrix @ rest_inv[name]) @ AI
                for r_ in range(3):
                    for c_ in range(4):
                        anim += half(m[r_][c_])
        clips.append({"name": meta["clip"], "frames": n, "row": row, "loop": meta["loop"], "pair": meta.get("pair"),
                      "also": meta.get("also", [])})
        row += n
        rig.animation_data.action = None
    (WEB / f"{model}_anim.bin").write_bytes(bytes(anim))
    manifest = {"_about": f"Espiochum's {model}, baked by style-lab/tools/mesh/bake_game.py. Vertex layout: position 3 x f32, normal 3 x i8 and a pad, uv 2 x f32, joints 4 x u8, weights 4 x u8 (sum 255). The animation file holds, for each frame of each clip in order, every bone's skin matrix as three rows of four half floats.",
                "model": model, "bin": f"mesh/{model}.bin", "anim": f"mesh/{model}_anim.bin", "fps": 30,
                "bones": bones, "parts": pmeta, "clips": clips, "anim_rows": row, "anim_width": len(kept) * 3,
                "attrib": {"position": [3, 0], "normal": [3, 12], "uv": [2, 16], "joints": [4, 24], "weights": [4, 28]}}
    (DATA / f"{model}.json").write_text(json.dumps(manifest))
    rest(rig, True)
    say(model, len(kept), "bones,", len(clips), "clips,", row, "frames,", len(blob), "bytes mesh,", len(anim), "bytes anim")
    return {"model": model, "clips": [{"name": c["name"], "frames": c["frames"], "loop": c["loop"], "row": c["row"]} for c in clips]}


table = [bake_model(m) for m in MODELS]
(GAME / "data" / "clips.json").write_text(json.dumps({
    "_about": "Every clip the page can draw, by model, in the order the core numbers them. Frames at 30 a second; a loop's last frame joins its first. Written by the bake; the core reads it to choose a clip and a frame.",
    "fps": 30, "models": table}, indent=1))
say("done")
