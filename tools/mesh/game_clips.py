"""Author Chumontillado's clips on one lifelike skin's rig, inside the bake.

Copied from Style Lab's tools/mesh/game_clips.py at 5f18c50 (CS-2) and
changed for Chumontillado: imported by bake_game.py, which opens one skin's
costumed .blend; no saved .blend, no paired clips, no rifle.

Every clip is in place: the core moves the figure, the clip moves the body.
The body is split at the boundary bone (`BOUNDARY`, spine04):

- **Lower clips** pose the boundary and everything not above it: the legs,
  the hips and the lower back. stand, walk, run, air, slide, duck, hang. The
  gaits loop over exactly one turn of the painted figure's stride (13° a
  tick, so 360 / 13 = 27.7 ticks), so the core picks the frame from the same
  phase `skins.rs` swings the painted legs with, and a frame is a tick.
- **Upper clips** pose only what hangs from the boundary (the chest, the
  arms, the head) with the boundary at rest, so the page draws an upper
  bone as the lower clip's boundary times the upper clip's bone. aim is
  sampled over pitch (a frame each 15°, -90° to 90°); low, raise, slash and
  thrust follow the painted weapon's pose (`hands::pose`) for a reload, a
  raise, a cut and a thrust, each sampled over how far through it is.

Hands reach their places by IK: the right hand to the weapon's grip (the
painted figure's `GRIP` from this body's own right shoulder, in the aim's
frame), the left to `FORE`; each wrist turned to the weapon's frame.
"""
import math

import bpy
from mathutils import Matrix, Vector

R = math.radians
BOUNDARY = "spine04"
# the painted figure's hands (skins.rs, cm, in the aim's frame from the
# right shoulder: x right, y up, z back)
GRIP = Vector((-4, -14, -28)) / 100
FORE = Vector((-20, -10, -36)) / 100
# hands.rs: a cut turns about a point this far back of the grip, leans out
# of its plane and carries the hands this far across
CUT_PIVOT = 0.35
CUT_LIFT = R(20)
CUT_ACROSS = 0.20
PITCHES = list(range(-90, 91, 15))
# a cut winds back at most 45° + 20° (weapons::WIND) and follows through as far
CUTS = list(range(-70, 71, 10))
STEPS = 6
STRIDE = 28   # frames (ticks) in one turn of the stride, 360 / 13 = 27.7

GRIP_FINGERS = {f"finger{f}-{k}.*": (a, 0, 0) for f in (2, 3, 4, 5) for k, a in ((1, 55), (2, 70), (3, 45))}
GRIP_FINGERS.update({"finger1-2.*": (25, 0, 0), "finger1-3.*": (30, 0, 0)})


def mirror(pose):
    """name.* sets .L as given and .R with Y and Z negated (Style Lab's clips.py)."""
    out = {}
    for k, v in pose.items():
        if k.endswith(".*"):
            out[k[:-2] + ".L"] = v
            out[k[:-2] + ".R"] = (v[0], -v[1], -v[2])
        else:
            out[k] = v
    return out


class Clips:
    def __init__(self, rig, skin):
        self.rig = rig
        self.skin = skin
        self.lower = []    # (name, loop, [pose a frame]); each poses the rig
        self.upper = []    # (name, [pose a frame]); each poses the rig and returns the hold frame
        self.bones = [b.name for b in rig.data.bones]
        above = set()
        stack = [rig.data.bones[BOUNDARY]]
        while stack:
            b = stack.pop()
            for ch in b.children:
                above.add(ch.name)
                stack.append(ch)
        self.above = above
        self.empties = {}

    # ---- posing -------------------------------------------------------------------
    def clear(self):
        rig = self.rig
        for t, pole, ik in getattr(self, "feet", {}).values():
            ik.mute = True
        rig.animation_data_create()
        rig.animation_data.action = None
        rig.location, rig.rotation_euler = (0, 0, 0), (0, 0, 0)
        for pb in rig.pose.bones:
            pb.rotation_mode = "XYZ"
            pb.rotation_euler = (0, 0, 0)
            pb.location = (0, 0, 0)
            pb.scale = (1, 1, 1)
        self.hands(False)

    def pose(self, pose, only):
        for name, rot in mirror(pose).items():
            if name in self.rig.pose.bones and only(name):
                pb = self.rig.pose.bones[name]
                pb.rotation_mode = "XYZ"
                pb.rotation_euler = [R(a) for a in rot]

    # ---- lower clips: keyframed actions, sampled a tick a frame --------------------
    def lower_clip(self, name, frames, loop, keys, feet=None):
        """keys: [(frame, pose, rig z or (rig y, rig z))], frames counted
        from 1. With `feet` (each foot's way forward, m) the legs reach the
        floor by IK, wherever the hips are."""
        rig = self.rig
        self.clear()
        act = bpy.data.actions.new(f"{self.skin}__{name}")
        rig.animation_data.action = act
        below = [n for n in self.bones if n not in self.above]
        for f, pose, z in keys:
            for n in below:
                rig.pose.bones[n].rotation_euler = (0, 0, 0)
            self.pose(pose, lambda n: n not in self.above)
            for n in below:
                rig.pose.bones[n].keyframe_insert("rotation_euler", frame=f)
            y, z = z if isinstance(z, tuple) else (0.0, z)
            rig.location = (0, y, z)
            rig.keyframe_insert("location", frame=f)
        for fc in getattr(act, "fcurves", []):
            for kp in fc.keyframe_points:
                kp.interpolation = "BEZIER"
                kp.handle_left_type = kp.handle_right_type = "AUTO_CLAMPED"
            if loop:
                fc.modifiers.new("CYCLES")
        rig.animation_data.action = None

        def at(k):
            def go():
                self.clear()
                rig.animation_data.action = act
                if feet:
                    self.legs(feet)
                bpy.context.scene.frame_set(1 + k)
            return go
        self.lower.append((name, loop, [at(k) for k in range(frames)]))

    def still(self, name, pose, z=0.0, frames=2, feet=None):
        self.lower_clip(name, frames, True, [(1, pose, z), (frames + 1, pose, z)], feet)

    def setup_legs(self):
        rig = self.rig
        sc = bpy.context.scene.collection
        self.feet = {}
        for side in ("L", "R"):
            t = bpy.data.objects.new(f"foot_{side}", None)
            pole = bpy.data.objects.new(f"knee_{side}", None)
            sc.objects.link(t)
            sc.objects.link(pole)
            ik = rig.pose.bones[f"lowerleg02.{side}"].constraints.new("IK")
            ik.name, ik.target, ik.chain_count = "game_leg", t, 4
            ik.pole_target, ik.pole_angle = pole, R(-90)
            ik.mute = True
            self.feet[side] = (t, pole, ik)

    def legs(self, ahead):
        """Each ankle where it stands at rest, `ahead` (L, R) metres forward,
        the knees pointing forward and a little out."""
        for side, a in zip(("L", "R"), ahead):
            t, pole, ik = self.feet[side]
            # from the rest skeleton, the rig at the origin (its object may
            # still stand where the last frame left it)
            ankle = self.rig.data.bones[f"foot.{side}"].head_local
            t.matrix_world = Matrix.Translation(ankle + Vector((0, -a, 0)))
            knee = self.rig.data.bones[f"lowerleg01.{side}"].head_local
            out = 0.15 if side == "L" else -0.15
            pole.matrix_world = Matrix.Translation(knee + Vector((out, -0.8, 0)))
            ik.mute = False


    def gait(self, name, swing, knee, lean, bob, lift=0.0):
        """One turn of the stride in place: contact, down, passing, up for each foot."""
        n = STRIDE
        keys = []
        for k in range(9):
            s = 1 if (k % 8) < 4 else -1
            ph = k % 4
            lead = [(-swing, 8), (-swing * 0.3, 25), (swing * 0.5, 15), (swing * 0.85, knee * 0.4)][ph]
            trail = [(swing * 0.8, knee * 0.4), (swing * 0.1, knee), (-swing * 0.55, knee * 1.05), (-swing * 0.95, knee * 0.5)][ph]
            L, Rt = (lead, trail) if s > 0 else (trail, lead)
            pose = {"spine05": (lean * 0.5, 0, 0), BOUNDARY: (lean * 0.5, s * 3, 0),
                    "upperleg01.L": (L[0], 0, 0), "upperleg01.R": (Rt[0], 0, 0),
                    "lowerleg01.L": (L[1], 0, 0), "lowerleg01.R": (Rt[1], 0, 0),
                    "foot.L": (-L[1] * 0.3, 0, 0), "foot.R": (-Rt[1] * 0.3, 0, 0)}
            z = [0.0, -bob, 0.0, bob][ph] + lift
            keys.append((1 + n * k / 8, pose, z))
        self.lower_clip(name, n, True, keys)

    # ---- upper clips: posed directly, hands by IK ---------------------------------
    def setup_hands(self):
        rig = self.rig
        sc = bpy.context.scene.collection
        for side in ("L", "R"):
            ik_t = bpy.data.objects.new(f"ik_{side}", None)
            rot_t = bpy.data.objects.new(f"rot_{side}", None)
            pole = bpy.data.objects.new(f"pole_{side}", None)
            for o in (ik_t, rot_t, pole):
                sc.objects.link(o)
            ik = rig.pose.bones[f"lowerarm02.{side}"].constraints.new("IK")
            ik.name, ik.target, ik.chain_count = "game_ik", ik_t, 4
            ik.pole_target, ik.pole_angle = pole, R(-90)
            cr = rig.pose.bones[f"wrist.{side}"].constraints.new("COPY_ROTATION")
            cr.name, cr.target = "game_rot", rot_t
            self.empties[side] = (ik_t, rot_t, pole, ik, cr)
        self.hands(False)

    def hands(self, on):
        for _, _, _, ik, cr in self.empties.values():
            ik.mute = cr.mute = not on

    def world_of(self, name):
        return self.rig.matrix_world @ self.rig.pose.bones[name].matrix

    def rest_world(self, name):
        return self.rig.matrix_world @ self.rig.data.bones[name].matrix_local

    @staticmethod
    def aim_frame(origin, pitch):
        """The aim's frame at `origin`, in Blender coordinates, as skins.rs's
        `Affine::looking(aim)`: x the figure's right, y up, z back. The
        figure faces -y, its right is -x; pitch up is positive."""
        fwd = Vector((0, -math.cos(pitch), math.sin(pitch)))
        right = Vector((-1, 0, 0))
        up = right.cross(fwd).normalized()
        if up.z < 0:
            up = -up
        back = -fwd
        m = Matrix.Identity(4)
        for i in range(3):
            m[i][0], m[i][1], m[i][2], m[i][3] = right[i], up[i], back[i], origin[i]
        return m

    def calibrate(self):
        """Each wrist's frame against the weapon's: the right wrist along the
        grip, 35° down from the aim, thumb up; the left along the forend.
        The grip lies 7 cm past the wrist joint along the hand."""
        self.clear()
        bpy.context.view_layer.update()
        self.offset = {}
        fwd, up, right = Vector((0, -1, 0)), Vector((0, 0, 1)), Vector((-1, 0, 0))
        for side in ("R", "L"):
            rest = self.rest_world(f"wrist.{side}")
            y_r = (rest.to_3x3() @ Vector((0, 1, 0))).normalized()
            thumb = self.rest_world(f"finger1-1.{side}").translation - rest.translation
            t_r = (thumb - y_r * thumb.dot(y_r)).normalized()
            basis_r = Matrix((y_r, t_r, y_r.cross(t_r))).transposed()
            if side == "R":
                y_d = (fwd * math.cos(R(35)) - up * math.sin(R(35))).normalized()
                t_d = up * 0.7 + fwd * 0.3 - right * 0.2
            else:
                y_d = (fwd * 0.8 + right * 0.6 - up * 0.1).normalized()
                t_d = up * 0.6 + fwd * 0.5
            t_d = (t_d - y_d * t_d.dot(y_d)).normalized()
            basis_d = Matrix((y_d, t_d, y_d.cross(t_d))).transposed()
            rot = basis_d @ basis_r.inverted() @ rest.to_3x3().normalized()
            hand = rot.to_4x4()
            hand.translation = -(rot @ Vector((0, 0.07, 0)))
            # wrist (world) = weapon frame @ offset, the aim frame at pitch 0 being
            # the identity's turn here
            self.offset[side] = self.aim_frame(Vector((0, 0, 0)), 0.0).inverted() @ hand

    def place(self, side, frame):
        ik_t, rot_t, pole, ik, cr = self.empties[side]
        w = frame @ self.offset[side]
        ik_t.matrix_world = Matrix.Translation(w.translation)
        rot_t.matrix_world = w.to_3x3().normalized().to_4x4()
        sh = self.rest_world(f"upperarm01.{side}").translation
        pole.matrix_world = Matrix.Translation(sh + Vector((-0.35 if side == "R" else 0.35, 0.15, -0.45)))

    def upper_pose(self, pitch, hand, two_handed=True):
        """Pose the chest and head for `pitch` (radians), then the hands: the
        right on the weapon at `hand(grip frame)`, the left on its forend or
        at the side. Returns the weapon's hold frame (the grip, unmoved)."""
        self.clear()
        # the body and the hands follow the pitch to 45° up or down; past
        # that the weapon turns in the hand, so a raised arm or a bowed head
        # (and its hat) stays inside the hit envelope
        body_pitch = max(R(-45), min(R(45), pitch))
        lean = -math.degrees(body_pitch)
        # the head leans a sixth of the pitch at most, so a brim stays in
        pose = {"spine03": (lean * 0.12, 0, 0), "spine02": (lean * 0.12, 0, 0), "spine01": (lean * 0.1, 0, 0),
                "neck01": (lean * 0.08, 0, 0), "head": (lean * 0.08, 0, 0), **GRIP_FINGERS}
        if not two_handed:
            pose.update({"upperarm01.L": (0, 0, -40), "lowerarm01.L": (20, 0, 0)})
        self.pose(pose, lambda n: n in self.above)
        bpy.context.view_layer.update()
        sh = self.world_of("upperarm01.R").translation
        aim = self.aim_frame(sh, pitch)
        # aimed up the hands come in, by half at 90° (a third at 45°), so they
        # stay under the envelope's top; the weapon points along the aim.
        low = self.aim_frame(sh, body_pitch)
        near = 1.0 - 0.5 * max(0.0, math.sin(body_pitch))

        def held(at):
            m = aim @ Matrix.Translation(at)
            m.translation = (low @ Matrix.Translation(Vector((at.x, at.y, at.z * near)))).translation
            return m
        grip = held(GRIP)
        self.place("R", hand(grip))
        self.place("L", hand(held(FORE)))
        self.hands(True)
        if not two_handed:
            self.empties["L"][3].mute = self.empties["L"][4].mute = True
        bpy.context.view_layer.update()
        return grip


def author(c):
    """Every clip for one skin. The Infanta ducks with her hoops pinned to
    the floor (the bake weights their hem to it); the rest, the Friar too,
    squat hunched forward, low enough for the ducked hit envelope."""
    skin = c.skin
    # every lower pose keeps the body inside the hit envelope (CS-5): 0.48 m
    # from the upright line, under 1.93 m standing and 1.23 m ducked (the
    # ducked eye, 0.95 m, and the head's 0.28 m), so the stride is short, as
    # the painted legs' 16°, and nothing lifts the body
    c.still("stand", {"lowerleg01.*": (4, 0, 0), "upperleg01.*": (-2, 0, -2)})
    c.gait("walk", 10, 30, 2, 0.006)
    c.gait("run", 11, 55, 5, 0.010)
    c.still("air", {"upperleg01.*": (-40, 0, -4), "lowerleg01.*": (75, 0, 0), "foot.*": (-15, 0, 0), "spine05": (4, 0, 0)})
    # ducked: a deep squat, the hips low and back over the heels,
    # leaning forward 20°, the feet on the floor by IK
    c.setup_legs()
    hips = c.rig.data.bones["spine05"].head_local.z
    duck = {"spine05": (12, 0, 0), BOUNDARY: (8, 0, 0), "neck01": (-10, 0, 0), "upperleg01.*": (-100, 0, -8),
            "lowerleg01.*": (140, 0, 0), "foot.*": (-80, 0, 0)}
    # the hips at 37% of their standing height, and no lower than 0.38 m,
    # where the thighs of the shortest legs stay off the floor
    sink = (0.16, max(0.37 * hips, 0.38) - hips)
    if skin == "infanta":
        # under the farthingale, her hoops pinned to the floor fold down
        duck = {"upperleg01.*": (-95, 0, -8), "lowerleg01.*": (130, 0, 0), "foot.*": (-35, 0, 0)}
        sink = (0.0, -0.42)
    c.still("duck", duck, sink, feet=(0.02, 0.02))
    # sliding, the hips 6 cm higher, so the leg thrown forward clears the floor
    c.still("slide", duck, (sink[0], sink[1] + 0.06) if skin != "infanta" else sink, feet=(0.10, 0.0))
    c.still("hang", {"upperleg01.*": (-12, 0, -3), "lowerleg01.*": (24, 0, 0), "foot.*": (30, 0, 0)})

    c.setup_hands()
    c.calibrate()
    T = Matrix.Translation
    rot = Matrix.Rotation

    def sample(fn):
        return fn

    c.upper.append(("aim", [(lambda p=p: c.upper_pose(R(p), lambda f: f)) for p in PITCHES]))
    steps = [k / (STEPS - 1) for k in range(STEPS)]
    # hands.rs: a reload takes the weapon 10 cm down, rolls it 35° and tips it
    # up 10° at its lowest
    c.upper.append(("low", [(lambda t=t: c.upper_pose(0.0, lambda f: f @ T((0, -0.10 * t, 0)) @ rot(R(10 * t), 4, "X")
                                                       @ rot(R(35 * t), 4, "Z"))) for t in steps]))
    # hands.rs: raised from 22 cm below, tipped 30° down
    c.upper.append(("raise", [(lambda t=t: c.upper_pose(0.0, lambda f: f @ T((0, -0.22 * t, 0)) @ rot(R(-30 * t), 4, "X")))
                              for t in steps]))

    # hands.rs, a cut on the level (way 0): the grip carried across by
    # sin(at) x 20 cm, the weapon turned by -at about a point 35 cm back and
    # leant 20° out of the cut's plane; only the right hand holds it
    def cut(a):
        at = R(a)
        pivot = T((0, 0, CUT_PIVOT))
        # wound back, the right hand crosses less (8 cm, not 20) and is
        # carried 20 cm further out, so the arm stays clear of the chest
        back = max(0.0, -math.sin(at))
        across = CUT_ACROSS * math.sin(at) if at > 0 else -0.08 * back
        return lambda f: f @ T((across, 0, -0.20 * back)) @ pivot @ rot(-at, 4, "Y") @ rot(CUT_LIFT, 4, "X") @ pivot.inverted()
    c.upper.append(("slash", [(lambda a=a: c.upper_pose(0.0, cut(a), False)) for a in CUTS]))
    # hands.rs: a knife's thrust, 6 cm in, 2 up and 22 out at its height
    c.upper.append(("thrust", [(lambda t=t: c.upper_pose(0.0, lambda f: f @ T((-0.06 * t, 0.02 * t, -0.22 * t)), False))
                               for t in steps]))
