"""Author Espiochum's game clips on the cast, headless.

    blender -b OUTDIR/cast.blend --python tools/mesh/game_clips.py -- OUTDIR

Every clip is in place: the game moves the actor, the clip moves the body.
Locomotion clips loop over exactly one stride (a step with each foot), so the
game picks the frame from its stride counter and feet do not slide: Gate
Crasher's stride is STRIDE + STRIDE_GROW x speed (0.6 + 0.6 v metres).
Paired clips are authored at the game's own offsets: a held soldier stands
0.55 m ahead of Big Chum facing her way; a carried body is drawn at her
origin; a body on the mule at the mule's origin. Hands close on what they
hold (finger poses). Writes OUTDIR/game.blend, actions named MODEL__CLIP, and
OUTDIR/game_clips.json: each clip's model, frame count, loop, and pairing.
"""
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Euler, Matrix, Vector

sys.path.insert(0, str(Path(__file__).parent))
import clips as C  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
OUT = Path(args[0])
FPS = 30
R = math.radians
OB = bpy.data.objects
RIGS = {"chum": OB["bigchum_rig"], "soldier": OB["veteran_rig"], "grace": OB["grace_rig"], "mule": OB["mule_rig"]}
ACTOR = {"chum": "bigchum", "soldier": "veteran", "grace": "grace"}
META = {}

# ---- poses (bone: XYZ degrees; ".*" mirrors to the right side) -------------------------
GRIP = {f"finger{f}-{k}.*": (a, 0, 0) for f in (2, 3, 4, 5) for k, a in ((1, 55), (2, 70), (3, 45))}
GRIP.update({"finger1-2.*": (25, 0, 0), "finger1-3.*": (30, 0, 0)})
RELAX = {f"finger{f}-{k}.*": (a, 0, 0) for f in (2, 3, 4, 5) for k, a in ((1, 12), (2, 18), (3, 10))}
STAND = {**RELAX, "upperarm01.*": (0, 0, -38), "lowerarm01.*": (12, 0, 0)}
CROUCH = {**RELAX, "spine05": (16, 0, 0), "spine03": (12, 0, 0), "neck01": (-16, 0, 0), "upperarm01.*": (30, 0, -36),
          "lowerarm01.*": (60, 0, 0), "upperleg01.*": (-68, 0, -6), "lowerleg01.*": (105, 0, 0), "foot.*": (-30, 0, 0)}
PRONE = {**RELAX, "spine04": (-10, 0, 0), "spine02": (-6, 0, 0), "neck01": (-45, 0, 0), "head": (-15, 0, 0),
         "upperarm01.*": (120, 0, -50), "lowerarm01.*": (90, 0, 0),
         "upperleg01.*": (5, 0, -6), "lowerleg01.*": (10, 0, 0), "foot.*": (40, 0, 0)}
LIE = {**RELAX, "upperarm01.*": (0, 0, -24), "lowerarm01.*": (25, 0, 0), "upperleg01.*": (-8, 0, -4),
       "lowerleg01.*": (14, 0, 0), "foot.*": (25, 0, 0), "head": (0, 0, 18)}
BOUND = {**RELAX, "upperarm01.*": (-30, 0, -45), "lowerarm01.*": (95, 0, 0), "upperleg01.*": (-10, 0, 0),
         "lowerleg01.*": (25, 0, 0), "head": (0, 0, 10)}
HANDS_UP = {**RELAX, "spine03": (-4, 0, 0), "upperarm01.*": (15, 0, 100), "lowerarm01.*": (75, 0, 0)}
HELD = {**GRIP, "spine04": (-14, 0, 0), "neck01": (-16, 0, 0), "upperarm01.*": (80, 0, -32), "lowerarm01.*": (100, 0, 0),
        "upperleg01.*": (-10, 0, 0), "lowerleg01.*": (16, 0, 0)}
HOLD = {**GRIP, "spine04": (6, 0, 0), "upperarm01.R": (88, 0, -5), "lowerarm01.R": (122, 0, 0),
        "upperarm01.L": (70, 0, -10), "lowerarm01.L": (100, 0, 0),
        "upperleg01.*": (-20, 0, 0), "lowerleg01.*": (32, 0, 0), "foot.*": (-12, 0, 0)}
DRAPED = {**RELAX, "spine04": (55, 0, 0), "spine02": (20, 0, 0), "neck01": (20, 0, 0), "upperarm01.*": (70, 0, -20),
          "lowerarm01.*": (10, 0, 0), "upperleg01.*": (-60, 0, 0), "lowerleg01.*": (20, 0, 0)}
CARRY = {**GRIP, "spine04": (10, 0, 0), "upperarm01.R": (60, 0, -10), "lowerarm01.R": (100, 0, 0),
         "upperarm01.L": (0, 0, -38), "lowerarm01.L": (20, 0, 0)}
HANG = {**GRIP, "upperarm01.*": (150, 0, -48), "lowerarm01.*": (8, 0, 0), "upperleg01.*": (-4, 0, 0),
        "lowerleg01.*": (8, 0, 0), "foot.*": (0, 0, 0)}
RIDE = {**GRIP, "spine04": (12, 0, 0), "upperleg01.*": (-66, 0, -40), "lowerleg01.*": (85, 0, 0), "foot.*": (-10, 0, 0),
        "upperarm01.*": (45, 0, -30), "lowerarm01.*": (70, 0, 0)}
CAGED = {**RELAX, "spine04": (14, 0, 0), "neck01": (20, 0, 0), "upperarm01.*": (35, 0, -30), "lowerarm01.*": (70, 0, 0),
         "upperleg01.*": (-84, 0, -14), "lowerleg01.*": (128, 0, 0), "foot.*": (-35, 0, 0)}
DIVE = {**RELAX, "spine04": (-10, 0, 0), "upperarm01.*": (145, 0, -50), "lowerarm01.*": (10, 0, 0),
        "upperleg01.*": (10, 0, 0), "lowerleg01.*": (20, 0, 0), "foot.*": (40, 0, 0)}

ALL_BONES = None


def bones_of(rig):
    return [b.name for b in rig.pose.bones if not b.name.startswith(("oris", "levator", "risorius", "temporalis",
                                                                       "orbicularis", "oculi", "tongue", "special"))]


def key(model, frame, pose, loc=(0, 0, 0), rot=(0, 0, 0)):
    rig = RIGS[model]
    p = C.mirror(pose)
    for name in bones_of(rig):
        pb = rig.pose.bones[name]
        pb.rotation_mode = "XYZ"
        pb.rotation_euler = [R(a) for a in p.get(name, (0, 0, 0))]
        pb.keyframe_insert("rotation_euler", frame=frame)
    rig.location = loc
    rig.rotation_euler = [R(a) for a in rot]
    rig.keyframe_insert("location", frame=frame)
    rig.keyframe_insert("rotation_euler", frame=frame)


def begin(model, name, frames, loop, pair=None):
    rig = RIGS[model]
    rig.animation_data_create()
    act = bpy.data.actions.new(f"{model}__{name}")
    act.use_fake_user = True
    rig.animation_data.action = act
    META[f"{model}__{name}"] = {"model": model, "clip": name, "frames": frames, "loop": loop, "pair": pair}
    return act


def finish(model, cyclic):
    rig = RIGS[model]
    for fc in getattr(rig.animation_data.action, "fcurves", []):
        for kp in fc.keyframe_points:
            kp.interpolation = "BEZIER"
            kp.handle_left_type = kp.handle_right_type = "AUTO_CLAMPED"
        if cyclic:
            fc.modifiers.new("CYCLES")


def stride_frames(speed):
    return max(12, round((0.6 + 0.6 * speed) / speed * FPS))


def gait(model, name, speed, swing, knee, lean, arm, arm_bend, bob, base=None, hands=RELAX, pair=None, lift=0.0):
    """One stride in place: contact, down, passing, up for each foot."""
    n = stride_frames(speed)
    begin(model, name, n, True, pair)
    base = base or {}
    for k in range(8):
        f = 1 + n * k / 8
        s = 1 if k < 4 else -1
        ph = k % 4
        lead = [(-swing, 8), (-swing * 0.3, 25), (swing * 0.5, 15), (swing * 0.85, knee * 0.4)][ph]
        trail = [(swing * 0.8, knee * 0.4), (swing * 0.1, knee), (-swing * 0.55, knee * 1.05), (-swing * 0.95, knee * 0.5)][ph]
        L, Rt = (lead, trail) if s > 0 else (trail, lead)
        a = [arm, arm * 0.4, -arm * 0.3, -arm * 0.8][ph] * s
        pose = {**hands, **base, "spine05": (lean * 0.4, 0, 0), "spine03": (lean * 0.6, s * 5, 0), "neck01": (-lean * 0.5, 0, 0),
                "upperleg01.L": (L[0], 0, 0), "upperleg01.R": (Rt[0], 0, 0), "lowerleg01.L": (L[1], 0, 0),
                "lowerleg01.R": (Rt[1], 0, 0), "foot.L": (-L[1] * 0.3, 0, 0), "foot.R": (-Rt[1] * 0.3, 0, 0)}
        if arm:
            pose.update({"upperarm01.L": (a, 0, -34), "upperarm01.R": (-a, 0, 34), "lowerarm01.*": (arm_bend, 0, 0)})
        z = [0.0, -bob, 0.0, bob][ph] + lift
        key(model, f, pose, (0, 0, z))
    key(model, n + 1, {**hands, **base, "spine05": (lean * 0.4, 0, 0), "spine03": (lean * 0.6, 5, 0), "neck01": (-lean * 0.5, 0, 0),
                       "upperleg01.L": (-swing, 0, 0), "upperleg01.R": (swing * 0.8, 0, 0), "lowerleg01.L": (8, 0, 0),
                       "lowerleg01.R": (knee * 0.4, 0, 0), "foot.L": (-2.4, 0, 0), "foot.R": (-knee * 0.12, 0, 0),
                       **({"upperarm01.L": (arm, 0, -34), "upperarm01.R": (-arm, 0, 34), "lowerarm01.*": (arm_bend, 0, 0)} if arm else {})},
        (0, 0, lift))
    finish(model, True)


def hold_still(model, name, pose, frames=60, loc=(0, 0, 0), rot=(0, 0, 0), breathe=3.0, pair=None, loop=True):
    begin(model, name, frames, loop, pair)
    for i, f in enumerate((1, frames // 2 + 1, frames + 1)):
        b = breathe if i == 1 else 0.0
        key(model, f, {**pose, "spine02": (pose.get("spine02", (0, 0, 0))[0] + b, 0, 0)}, loc, rot)
    finish(model, loop)


def lying_rot():
    return (-90, 0, 0)


# ---- the soldier's rifle: on the chest, hands on it by IK -------------------------------
C.bind("veteran")
rifle = C.make_rifle()
C.hold_rifle(rifle)
rifle.name = "veteran_rifle"
for child in list(rifle.children):
    child.name = "veteran_rifle_" + child.name.replace("prop_rifle_", "")
flash = OB.get("veteran_rifle_prop_flash") or next((o for o in OB if o.name.endswith("flash")), None)
if flash:
    bpy.data.objects.remove(flash, do_unlink=True)
IK = [RIGS["soldier"].pose.bones[f"lowerarm02.{s}"].constraints["rifle_ik"] for s in ("L", "R")]


def armed(on, frame):
    for ik in IK:
        ik.influence = 1.0 if on else 0.0
        ik.keyframe_insert("influence", frame=frame)


def rifle_low(frame, deg):
    """The rifle stays at one ready position on the chest: the bake draws it
    rigid on that bone, so a tilt keyed on the rifle would part it from the
    hands."""


# ---- Big Chum ---------------------------------------------------------------------------------
hold_still("chum", "idle", STAND)
gait("chum", "run", 4.0, 34, 95, 10, 40, 80, 0.035, lift=0.045)
gait("chum", "sprint", 6.8, 52, 115, 18, 62, 95, 0.05, lift=0.06)
gait("chum", "crouch_walk", 2.0, 26, 60, 0, 0, 0, 0.015, base={k: v for k, v in CROUCH.items() if not k.startswith(("upperleg", "lowerleg", "foot"))})
hold_still("chum", "crouch_idle", CROUCH, loc=(0, 0, -0.36))
for fr in (1, 31, 61):
    pass
hold_still("chum", "prone_idle", PRONE, loc=(0, 0, 0.22), rot=(90, 0, 0), breathe=1.5)
# crawl: one stride in place on elbows and knees
n = stride_frames(0.8)
begin("chum", "crawl", n, True)
for k in range(5):
    s = 1 if k % 2 else -1
    key("chum", 1 + n * k / 4, {**PRONE, "upperarm01.L": (120 + 18 * s, 0, -50), "upperarm01.R": (120 - 18 * s, 0, 50),
                                "lowerleg01.L": (10 + 55 * (s > 0), 0, 0), "lowerleg01.R": (10 + 55 * (s < 0), 0, 0)},
        (0, 0, 0.22), (90, 0, 0))
finish("chum", True)
gait("chum", "carry_walk", 2.4, 26, 70, 8, 0, 0, 0.02, base=CARRY, hands=GRIP, pair=["soldier__draped", [0, 0, 0]])
hold_still("chum", "carry_idle", CARRY, pair=["soldier__draped", [0, 0, 0]])
hold_still("chum", "hold", HOLD, pair=["soldier__held", [0, -0.55, 0]])
hold_still("chum", "hang", HANG, loc=(0, 0, 0.06), breathe=0)
hold_still("chum", "ride", RIDE, loc=(0, 0.05, 0.78), breathe=1.0)
begin("chum", "dive", 20, False)
key("chum", 1, DIVE, (0, 0, 0.2), (40, 0, 0))
key("chum", 10, DIVE, (0, 0, 0.45), (80, 0, 0))
key("chum", 20, {**PRONE}, (0, 0, 0.22), (90, 0, 0))
finish("chum", False)
# aiming: both hands on the pistol by IK at eye level ahead
C.bind("bigchum")
aim_t = bpy.data.objects.new("game_aim", None)
bpy.context.scene.collection.objects.link(aim_t)
aim_t.parent, aim_t.parent_type, aim_t.parent_bone = RIGS["chum"], "BONE", "spine01"
bpy.context.view_layer.update()
aim_t.matrix_world = Matrix.Translation(C.CHEST + Vector((-0.02, -0.46, 0.06)))
AIM_IK = []
for side in ("L", "R"):
    ik = RIGS["chum"].pose.bones[f"lowerarm02.{side}"].constraints.new("IK")
    ik.name, ik.target, ik.chain_count = "game_aim", aim_t, 4
    AIM_IK.append(ik)


def chum_aim(on, frame):
    for ik in AIM_IK:
        ik.influence = 1.0 if on else 0.0
        ik.keyframe_insert("influence", frame=frame)


for name, pose, loc in (("aim", {**STAND, **GRIP}, (0, 0, 0)), ("crouch_aim", {**{k: v for k, v in CROUCH.items() if not k.startswith(("upperarm", "lowerarm"))}, **GRIP, "upperarm01.*": (0, 0, -38), "lowerarm01.*": (12, 0, 0), "spine05": (0, 0, 0), "spine03": (-4, 0, 0), "neck01": (-4, 0, 0)}, (0, 0, -0.36))):
    begin("chum", name, 30, True)
    chum_aim(True, 1)
    key("chum", 1, pose, loc)
    key("chum", 31, pose, loc)
    finish("chum", True)
for name in [n for n in META if n.startswith("chum__") and n not in ("chum__aim", "chum__crouch_aim")]:
    pass
# every other chum clip has the aim IK off
for act_name in [n for n in META if n.startswith("chum__") and META[n]["clip"] not in ("aim", "crouch_aim")]:
    RIGS["chum"].animation_data.action = bpy.data.actions[act_name]
    chum_aim(False, 1)

# ---- the soldier ----------------------------------------------------------------------------------
SOLDIER_HANDS = {**GRIP}
hold_still("soldier", "idle", STAND)
armed(True, 1)
rifle_low(1, -35)
gait("soldier", "walk", 1.4, 20, 45, 2, 0, 0, 0.012, hands=GRIP)
armed(True, 1)
rifle_low(1, -35)
gait("soldier", "run", 4.0, 34, 95, 10, 0, 0, 0.035, hands=GRIP, lift=0.045)
armed(True, 1)
rifle_low(1, -25)
hold_still("soldier", "aim", {**GRIP, "spine03": (6, 0, 0), "neck01": (8, 0, 0), "upperleg01.L": (-18, 0, 0),
                              "upperleg01.R": (12, 0, 0), "lowerleg01.*": (18, 0, 0)}, frames=30)
armed(True, 1)
rifle_low(1, 0)
for name, pose, kw in (("held", HELD, {}), ("handsup", HANDS_UP, {}), ("crouch_idle", CROUCH, {"loc": (0, 0, -0.36)}),
                       ("lying", LIE, {"loc": (0, 0, 0.19), "rot": lying_rot(), "breathe": 1.5}),
                       ("bound", BOUND, {"loc": (0, 0, 0.20), "rot": lying_rot(), "breathe": 1.0}),
                       ("draped", DRAPED, {"loc": (0, 0.97, 1.42), "rot": (90, 0, 180), "breathe": 0.5}),
                       ("on_mule", DRAPED, {"loc": (0.97, 0.0, 1.55), "rot": (90, 0, 90), "breathe": 0.5})):
    hold_still("soldier", name, pose, **kw)
    armed(False, 1)
# kicking a sleeping buddy awake
begin("soldier", "kick", 24, True)
armed(True, 1)
rifle_low(1, -35)
key("soldier", 1, {**GRIP, "upperleg01.R": (-10, 0, 0)})
key("soldier", 9, {**GRIP, "spine04": (-6, 0, 0), "upperleg01.R": (-55, 0, 0), "lowerleg01.R": (20, 0, 0)})
key("soldier", 13, {**GRIP, "upperleg01.R": (-20, 0, 0), "lowerleg01.R": (40, 0, 0)})
key("soldier", 25, {**GRIP, "upperleg01.R": (-10, 0, 0)})
finish("soldier", True)

# ---- Corporal Grace -----------------------------------------------------------------------------
hold_still("grace", "caged", CAGED, loc=(0, 0, -0.38))
hold_still("grace", "lying", LIE, loc=(0, 0, 0.19), rot=lying_rot(), breathe=1.5)
hold_still("grace", "draped", DRAPED, loc=(0, 0.97, 1.42), rot=(90, 0, 180), breathe=0.5)
hold_still("grace", "on_mule", DRAPED, loc=(0.97, 0.0, 1.55), rot=(90, 0, 90), breathe=0.5)
META["chum__carry_walk"]["also"] = ["grace__draped"]
META["chum__carry_idle"]["also"] = ["grace__draped"]

# ---- the Memphis Mule -----------------------------------------------------------------------
MULE = RIGS["mule"]
GAITS = {"walk": ({"LH": 0.0, "LF": 0.25, "RH": 0.5, "RF": 0.75}, 0.62), "trot": ({"LF": 0.0, "RH": 0.0, "RF": 0.5, "LH": 0.5}, 0.45),
         "gallop": ({"LH": 0.0, "RH": 0.12, "LF": 0.38, "RF": 0.5}, 0.35)}


def mule_key(f, bones, z=0.0):
    for b in MULE.pose.bones:
        b.rotation_mode = "XYZ"
        b.rotation_euler = [R(a) for a in bones.get(b.name, (0, 0, 0))]
        b.keyframe_insert("rotation_euler", frame=f)
    MULE.location = (0, 0, z)
    MULE.rotation_euler = (0, 0, 0)
    MULE.keyframe_insert("location", frame=f)
    MULE.keyframe_insert("rotation_euler", frame=f)


def mule_gait(name, kind, speed, swing, fold, head, bob, stride=(1.0, 0.5)):
    phases, duty = GAITS[kind]
    n = max(12, round((stride[0] + stride[1] * speed) / speed * FPS))
    begin("mule", name, n, True)
    for k in range(0, n + 1):
        t = k / n
        bones = {}
        for leg, ph in phases.items():
            part, side = {"LF": ("front", "L"), "RF": ("front", "R"), "LH": ("hind", "L"), "RH": ("hind", "R")}[leg]
            p = (t - ph) % 1.0
            if p < duty:
                a, lo = -swing + 2 * swing * (p / duty), 0.0
            else:
                q = (p - duty) / (1 - duty)
                a, lo = swing - 2 * swing * q, math.sin(math.pi * q) * fold
            up, mid = ("upper", "fore") if part == "front" else ("thigh", "gaskin")
            bones[f"{part}_{up}.{side}"] = (a, 0, 0)
            bones[f"{part}_{mid}.{side}"] = (-lo * 0.4 if part == "front" else lo * 0.3, 0, 0)
            bones[f"{part}_cannon.{side}"] = (lo if part == "front" else -lo * 0.6, 0, 0)
        nod = math.sin(2 * math.pi * t * (2 if kind == "walk" else 1))
        bones.update({"neck1": (head + nod * 4, 0, 0), "head": (-nod * 3, 0, 0), "tail1": (8 + nod * 4, 0, nod * 5),
                      "ear.L": (nod * 6, 0, 0), "ear.R": (-nod * 6, 0, 0)})
        mule_key(1 + k, bones, bob * abs(math.sin(2 * math.pi * t)))
    finish("mule", True)


begin("mule", "idle", 90, True)
mule_key(1, {"neck1": (6, 0, 0), "tail1": (6, 0, 4)})
mule_key(30, {"neck1": (8, 0, 0), "ear.L": (-12, 0, 0), "tail1": (6, 0, -6)})
mule_key(60, {"neck1": (5, 0, 0), "ear.R": (-14, 0, 0), "tail1": (6, 0, 6)})
mule_key(91, {"neck1": (6, 0, 0), "tail1": (6, 0, 4)})
finish("mule", True)
mule_gait("walk", "walk", 1.8, 20, 50, 5, 0.02)
mule_gait("trot", "trot", 3.6, 28, 75, 2, 0.03)
mule_gait("charge", "gallop", 6.5, 38, 95, 22, 0.06)
for n in ("mule__walk", "mule__trot", "mule__idle"):
    META[n]["also"] = ["soldier__on_mule", "grace__on_mule"]
for n in ("chum__ride",):
    META[n]["pair"] = ["mule__walk", [0, 0, 0]]

for model, rig in RIGS.items():
    if rig.animation_data:
        rig.animation_data.action = None
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / "game.blend"))
(OUT / "game_clips.json").write_text(json.dumps(META, indent=1))
print("game_clips:", len(META), "clips", flush=True)
