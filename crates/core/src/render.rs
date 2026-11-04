//! What the page draws: one instance array and one draw list a frame
//! (CW-5), and a static array uploaded once. Long Time Comin's renderer
//! contract (one 3×4 matrix, a colour and one more value per shape, grouped
//! so each (mesh, pass) is one instanced draw), with two additions:
//!
//! - **Depth layers.** One 24-bit depth range from 0.4 m to 16 km cannot
//!   tell apart two surfaces 160 m apart at the horizon. The frame is drawn
//!   in two ranges instead, the far one first (112 m to 16 km), then the
//!   depth buffer is cleared and the near one drawn (0.4 m to 128 m). A
//!   shape drawn in the 16 m both share is drawn in both, and the near
//!   layer's pixels win there, so the seam has no crack. `depth_step` and
//!   its tests give the numbers (CW-18).
//! - **An origin.** Positions reach the page relative to a point near the
//!   camera (`ORIGIN_CELL` grid), so the floats the GPU uses keep a tenth of
//!   a millimetre everywhere in the 4 km square. The static array is rebuilt
//!   when the camera crosses into another cell (`Scene::version`).
//!
//! Passes, in draw order: `Sky` (the dome round the eye, unlit, no depth),
//! `Solid` (lit), `Dither` (lit, and each pixel kept or dropped by the
//! ordered dither for M2's detail hand-over), `Ink` (the outline: the same
//! shape grown and drawn with its front faces culled, which leaves a rim),
//! `Shade` (soft round shadows, blended), `Glow` (halos, added). The page
//! sets the GL state each pass needs from `pass_states()`, so it knows no
//! pass by name.
//!
//! Instance values (16 Q16 integers; the shim hands the page floats): the
//! three columns of the shape's
//! linear part with the translation's x, y, z in each fourth slot, then the
//! colour and one value whose meaning is the pass's: Solid, the glow (0 lit,
//! `ONE` unlit); Dither and Ink, the dither fade (`look::keeps`; `ONE` keeps
//! every pixel); Glow and Shade, the strength.
//!
//! Extension (M1–M7): a module draws by pushing shapes, in world
//! coordinates, into the `Shapes` it is handed: `World::draw` for things that
//! move (drawn at `alpha` between the last two ticks), `World::scene` for
//! things that do not (a `Group` with a centre and radius, so whole groups
//! are left out of a layer they cannot reach). Push in a fixed order:
//! the instance array must be the same for the same world (CW-18).

use std::collections::BTreeMap;

use crate::fx::{self, Fx, ONE};
use crate::geom::{v3, Affine, Mat4, V3};
use crate::look::Rgb;
use crate::prim::{Mesh, MeshId};

pub const INSTANCE_VALUES: usize = 16;
/// buffer, mesh, byte offset, count, pass, layer
pub const DRAW_VALUES: usize = 6;
pub const BYTES_PER_VALUE: usize = 4;
pub const STATIC_BUFFER: u32 = 0;
pub const DYNAMIC_BUFFER: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pass {
    Sky = 0,
    Solid = 1,
    Dither = 2,
    Ink = 3,
    Shade = 4,
    Glow = 5,
    /// T4: a map's surface: lit as a solid, with the material's texture
    /// (tiles, brick, planks, mottle) and a dark rim at each face's edge,
    /// its material's number in the value
    Surface = 6,
}

impl Pass {
    /// Draw order within a layer: every opaque pass before the blended ones.
    pub const ORDER: [Pass; 7] = [
        Pass::Sky,
        Pass::Solid,
        Pass::Surface,
        Pass::Dither,
        Pass::Ink,
        Pass::Shade,
        Pass::Glow,
    ];

    /// Drawn as an opaque, lit surface that writes depth.
    pub fn opaque(self) -> bool {
        matches!(self, Pass::Solid | Pass::Dither | Pass::Surface)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    Far = 0,
    Near = 1,
    /// M1 (R3.4): what lies within `EYE_LAYER_FAR` of the eye, drawn last
    /// with its own near plane, so the player's own arms, sword and legs are
    /// whole in first person while the near layer keeps CHUMWORLD's planes
    Eye = 2,
}

impl Layer {
    /// Draw order: far first, then the depth buffer is cleared for the
    /// near, then again for the eye's.
    pub const ORDER: [Layer; 3] = [Layer::Far, Layer::Near, Layer::Eye];

    /// The layer's near and far planes.
    pub fn planes(self) -> (Fx, Fx) {
        match self {
            Layer::Far => (FAR_LAYER_NEAR, FAR),
            Layer::Near => (NEAR, NEAR_LAYER_FAR),
            Layer::Eye => (EYE_NEAR, EYE_LAYER_FAR),
        }
    }

    /// The layer a surface `d` from the eye is depth-tested in (the near
    /// one wherever it reaches).
    pub fn at(d: Fx) -> Layer {
        if d <= NEAR_LAYER_FAR {
            Layer::Near
        } else {
            Layer::Far
        }
    }
}

/// The near plane, 0.4 m (Iron Vector's 40 cm): as far out as it can be, since
/// the depth buffer's steps grow with the square of distance over it. The
/// camera sits 2.6 m behind the head, so nothing the player looks at comes
/// nearer.
pub const NEAR: Fx = ONE * 2 / 5;
/// M1 (R3.4): the eye layer's planes, 5 cm to 3 m. At 3 m its depth step is
/// 3 × 2.95 / (0.05 × 2²⁴) = 0.01 mm, finer than the near layer's anywhere.
pub const EYE_NEAR: Fx = ONE / 20;
pub const EYE_LAYER_FAR: Fx = fx::int(3);
/// The near layer's far plane.
pub const NEAR_LAYER_FAR: Fx = fx::int(128);
/// The far layer's near plane: 16 m inside the near layer's far plane, the
/// band both draw.
pub const FAR_LAYER_NEAR: Fx = fx::int(112);
/// The far plane: the 16 km horizon (2¹⁴ m, the outer edge of the terrain
/// rings in PLAN.md).
pub const FAR: Fx = fx::int(16_384);

/// The grid the origin snaps to. Within one cell the floats the GPU uses for
/// a position near the camera keep better than a tenth of a millimetre.
pub const ORIGIN_CELL: Fx = fx::int(256);

/// The ink line is this thick on a small shape, and grows with a shape's
/// size so a tree far off still has one (Long Time Comin').
const INK_MIN: Fx = ONE * 3 / 100;
const INK_PER_METRE: Fx = ONE / 160;
const INK_MAX: Fx = ONE / 5;
/// and is never thinner than this (a centimetre), nor thicker than a third of
/// the shape's thinnest side unless that is thinner still
const INK_THINNEST: Fx = ONE / 100;

/// One shape to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub mesh: MeshId,
    /// in world coordinates
    pub at: Affine,
    pub rgb: Rgb,
    /// the pass's own value (see the module doc)
    pub value: Fx,
    pub pass: Pass,
}

/// Shapes in the order they were pushed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shapes {
    pub list: Vec<Shape>,
}

impl Shapes {
    pub fn push(&mut self, s: Shape) {
        self.list.push(s);
    }

    /// A lit solid. `glow` 0 is fully lit; `ONE` shows its colour unlit.
    pub fn solid(&mut self, mesh: impl Into<MeshId>, at: Affine, rgb: Rgb, glow: Fx) {
        self.push(Shape {
            mesh: mesh.into(),
            at,
            rgb,
            value: glow,
            pass: Pass::Solid,
        });
    }

    /// T4: a map's surface in material `material` (its row in the look's
    /// grit table), with an ink outline.
    pub fn surface(&mut self, mesh: impl Into<MeshId>, at: Affine, rgb: Rgb, material: i32, ink: Rgb) {
        let mesh = mesh.into();
        self.push(Shape {
            mesh,
            at,
            rgb,
            value: material * ONE,
            pass: Pass::Surface,
        });
        self.outline(mesh, &at, ink, ONE);
    }

    /// A lit solid with an ink outline in `ink`.
    pub fn inked(&mut self, mesh: impl Into<MeshId>, at: Affine, rgb: Rgb, ink: Rgb) {
        let mesh = mesh.into();
        self.solid(mesh, at, rgb, 0);
        self.outline(mesh, &at, ink, ONE);
    }

    /// The outline alone: the shape grown by the ink's width, `fade` as the
    /// dither's (`ONE`: whole).
    pub fn outline(&mut self, mesh: impl Into<MeshId>, at: &Affine, ink: Rgb, fade: Fx) {
        self.push(Shape {
            mesh: mesh.into(),
            at: inflate(at),
            rgb: ink,
            value: fade,
            pass: Pass::Ink,
        });
    }

    /// A solid drawn on the pixels the ordered dither keeps at `fade`
    /// (`look::keeps`; `look::hand_over` gives a pair of fades): the detail
    /// hand-over M2 draws its seams with.
    pub fn dithered(&mut self, mesh: impl Into<MeshId>, at: Affine, rgb: Rgb, fade: Fx) {
        self.push(Shape {
            mesh: mesh.into(),
            at,
            rgb,
            value: fade,
            pass: Pass::Dither,
        });
    }

    /// A soft halo, added to what is behind it.
    pub fn glow(&mut self, mesh: impl Into<MeshId>, at: Affine, rgb: Rgb, strength: Fx) {
        self.push(Shape {
            mesh: mesh.into(),
            at,
            rgb,
            value: strength,
            pass: Pass::Glow,
        });
    }

    /// A soft round shadow on flat ground: a `Square` this wide, darkening
    /// toward `rgb` by `strength` at its middle.
    pub fn shadow(&mut self, centre: V3, width: Fx, rgb: Rgb, strength: Fx) {
        self.push(Shape {
            mesh: Mesh::Square.into(),
            at: Affine::translate(centre).then(&Affine::scale(v3(width, ONE, width))),
            rgb,
            value: strength,
            pass: Pass::Shade,
        });
    }

    /// The sky dome round the eye. Drawn first in the far layer, behind
    /// everything; the shader colours it by direction.
    pub fn sky(&mut self, eye: V3) {
        self.push(Shape {
            mesh: Mesh::Sphere.into(),
            at: Affine::translate(eye).then(&Affine::scale(v3(fx::int(100), fx::int(100), fx::int(100)))),
            rgb: [ONE; 3],
            value: 0,
            pass: Pass::Sky,
        });
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// M7 (R9.91): a thin thing (a blade, a wing, a board) seen edge-on is kept
/// at least this wide for each metre it is from the eye: 1.6 mm, a pixel
/// and a half at 1080 lines with the 60° view, so a blade edge-on at 20 m
/// does not vanish and come back as it turns.
pub const KEEP_VISIBLE_PER_M: Fx = ONE * 16 / 10_000;
/// A shape counts as thin when its thinnest side is under a quarter of its
/// longest (a third for an outline, whose sides the ink has grown) and its
/// longest is at least this: smaller things (an eye, a stud) are left alone.
pub const KEEP_VISIBLE_LONGEST: Fx = ONE * 12 / 100;

/// Widens each thin lit shape, and its outline, to `KEEP_VISIBLE_PER_M` of
/// its distance from `eye` across its thinnest side (R9.91). An outline is
/// kept half again as wide as the least a solid is widened to, so the ink's
/// rim stays round a widened blade and no two faces meet.
pub fn keep_visible(shapes: &mut [Shape], eye: V3) {
    for s in shapes.iter_mut() {
        let ink = match s.pass {
            Pass::Solid | Pass::Dither | Pass::Surface => false,
            Pass::Ink => true,
            _ => continue,
        };
        let a = &mut s.at;
        let lens = [a.c0.len(), a.c1.len(), a.c2.len()];
        let longest = lens.into_iter().max().unwrap_or(0);
        let (k, thin) = lens
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|(_, l)| *l)
            .unwrap_or((0, 0));
        let share = if ink { 3 } else { 4 };
        if longest < KEEP_VISIBLE_LONGEST || thin == 0 || thin * share >= longest {
            continue;
        }
        let least = fx::mul(a.t.dist(eye), KEEP_VISIBLE_PER_M);
        let least = if ink { least * 3 / 2 } else { least };
        if thin >= least {
            continue;
        }
        let col = match k {
            0 => &mut a.c0,
            1 => &mut a.c1,
            _ => &mut a.c2,
        };
        *col = col.scale(fx::div(least, thin));
    }
}

/// The same shape grown by the ink's width on every side.
pub fn inflate(a: &Affine) -> Affine {
    let lens = [a.c0.len(), a.c1.len(), a.c2.len()];
    let biggest = lens.into_iter().max().unwrap_or(0);
    let smallest = lens.into_iter().filter(|&l| l > 0).min().unwrap_or(0);
    // a long thin shape (a rail, a pole) keeps a line in proportion to its
    // thickness, not its length
    let width = (INK_MIN + fx::mul(biggest, INK_PER_METRE))
        .min(INK_MAX)
        .min((smallest / 3).max(INK_THINNEST));
    inflate_by(a, width)
}

/// The same shape grown by `t` on every side.
pub fn inflate_by(a: &Affine, t: Fx) -> Affine {
    let grow = |c: V3| {
        let l = c.len();
        if l == 0 {
            c
        } else {
            c.scale(fx::div(l + 2 * t, l))
        }
    };
    Affine {
        c0: grow(a.c0),
        c1: grow(a.c1),
        c2: grow(a.c2),
        t: a.t,
    }
}

/// A shape's 16 instance values, its position made relative to `origin`.
fn values(s: &Shape, origin: V3) -> [i32; INSTANCE_VALUES] {
    let a = &s.at;
    let t = a.t.sub(origin);
    [
        a.c0.x, a.c0.y, a.c0.z, t.x, a.c1.x, a.c1.y, a.c1.z, t.y, a.c2.x, a.c2.y, a.c2.z, t.z, s.rgb[0],
        s.rgb[1], s.rgb[2], s.value,
    ]
}

/// A run of instances of one mesh in one pass, at a byte offset into its
/// buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub pass: Pass,
    pub mesh: MeshId,
    pub offset: u32,
    pub count: u32,
}

/// Writes shapes into `out` grouped by (pass, mesh) in draw order, keeping
/// the push order within each, and returns the runs.
fn pack<'a>(shapes: impl Iterator<Item = &'a Shape>, origin: V3, out: &mut Vec<i32>) -> Vec<Run> {
    let mut lists: BTreeMap<(Pass, MeshId), Vec<&Shape>> = BTreeMap::new();
    for s in shapes {
        lists.entry((s.pass, s.mesh)).or_default().push(s);
    }
    let mut runs = Vec::new();
    for ((pass, mesh), list) in lists {
        let offset = (out.len() * BYTES_PER_VALUE) as u32;
        for s in &list {
            out.extend_from_slice(&values(s, origin));
        }
        runs.push(Run {
            pass,
            mesh,
            offset,
            count: list.len() as u32,
        });
    }
    runs
}

/// Things that do not move, gathered in a place, for the static array.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Group {
    /// what it is, for the flicker scan's report
    pub name: String,
    pub centre: V3,
    /// holds every shape in the group
    pub radius: Fx,
    pub shapes: Shapes,
}

impl Group {
    pub fn new(name: &str, centre: V3, radius: Fx) -> Group {
        Group {
            name: name.to_string(),
            centre,
            radius,
            shapes: Shapes::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    centre: V3,
    radius: Fx,
    runs: Vec<Run>,
}

/// The static array: every group's shapes relative to one origin.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scene {
    pub origin: V3,
    pub values: Vec<i32>,
    groups: Vec<Placed>,
    /// changes whenever `values` does, so the page uploads it again
    pub version: u32,
}

impl Scene {
    pub fn build(groups: &[Group], origin: V3, version: u32) -> Scene {
        let mut values = Vec::new();
        let placed = groups
            .iter()
            .map(|g| Placed {
                centre: g.centre,
                radius: g.radius,
                runs: pack(g.shapes.list.iter(), origin, &mut values),
            })
            .collect();
        Scene {
            origin,
            values,
            groups: placed,
            version,
        }
    }

    pub fn instance_count(&self) -> usize {
        self.values.len() / INSTANCE_VALUES
    }

    /// Every run in the static array, group by group: what the flicker scan
    /// reads, as the page receives it.
    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.groups.iter().flat_map(|g| g.runs.iter())
    }
}

/// The cell corner the origin snaps to for an eye.
pub fn origin_for(eye: V3) -> V3 {
    let snap = |c: Fx| c.div_euclid(ORIGIN_CELL) * ORIGIN_CELL;
    v3(snap(eye.x), 0, snap(eye.z))
}

/// Where the camera is, where it looks, and its vertical field of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub eye: V3,
    pub fwd: V3,
    pub fov: i32,
    /// the view's roll about `fwd`, angle units, positive to the right
    /// (the eye's tilt with a cut or a blow, `feel::tilt_at`); 0 is level
    pub roll: i32,
}

impl Camera {
    /// The layer's projection times the camera's turn, with no translation:
    /// the shader subtracts the eye from each position first (camera-relative
    /// drawing). A matrix that also carried the translation was rounded to
    /// Q16 entry by entry, and that rounding times a position hundreds of
    /// metres from the origin moved the body a pixel back and forth on
    /// screen while the view turned (SECOND-ORDER.md R2.8). Without the
    /// translation, the rounding is an angle: under a hundredth of a pixel.
    pub fn matrix(&self, layer: Layer, aspect: Fx) -> Mat4 {
        let (n, f) = layer.planes();
        Mat4::perspective(self.fov, aspect, n, f).mul(&Mat4::look(V3::ZERO, self.fwd, self.up()))
    }

    /// Up for the view: the world's up, rolled about the view's line by
    /// `roll` toward the view's right; with no roll, the world's up exactly.
    pub fn up(&self) -> V3 {
        if self.roll == 0 {
            return V3::UP;
        }
        let right = self.fwd.cross(V3::UP).norm();
        V3::UP
            .scale(fx::cos(self.roll))
            .add(right.scale(fx::sin(self.roll)))
    }

    /// Which layers a sphere is drawn in: none behind the camera or past the
    /// far plane, near where it reaches inside the near layer's far plane,
    /// far where it reaches past the far layer's near plane.
    pub fn layers(&self, centre: V3, radius: Fx) -> (bool, bool) {
        let to = centre.sub(self.eye);
        let d = to.len();
        if to.dot(self.fwd) < -radius || d.saturating_sub(radius) > FAR {
            return (false, false);
        }
        let near = d.saturating_sub(radius) < NEAR_LAYER_FAR;
        let far = d.saturating_add(radius) > FAR_LAYER_NEAR;
        (near, far)
    }

    /// M1: whether a sphere reaches the eye layer: not wholly behind the
    /// camera, and some of it within `EYE_LAYER_FAR`.
    pub fn eye_layer(&self, centre: V3, radius: Fx) -> bool {
        let to = centre.sub(self.eye);
        to.dot(self.fwd) >= -radius && to.len().saturating_sub(radius) < EYE_LAYER_FAR
    }

    /// Where a world point lands on screen, as Q16 shares of the width from
    /// the left and of the height from the top; `None` behind the camera.
    pub fn screen(&self, aspect: Fx, p: V3) -> Option<(Fx, Fx)> {
        let [x, y, _, w] = self.matrix(Layer::Near, aspect).project(p.sub(self.eye));
        if w <= ONE / 100 {
            return None;
        }
        Some(((fx::div(x, w) + ONE) / 2, (ONE - fx::div(y, w)) / 2))
    }
}

/// One frame for the page: the view values, the dynamic instance array and
/// the draw list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// the far layer's matrix, the near layer's, then the eye (relative to
    /// the origin): `numbers().view`
    pub view: Vec<i32>,
    pub instances: Vec<i32>,
    pub draws: Vec<u32>,
}

pub const VIEW_VALUES: usize = 16 + 16 + 16 + 3;

impl Frame {
    /// Builds the frame: for each layer, for each pass, the static groups'
    /// runs that reach the layer, then the moving shapes that do.
    pub fn build(scene: &Scene, moving: &Shapes, cam: &Camera, aspect: Fx) -> Frame {
        let origin = scene.origin;
        let mut view = Vec::with_capacity(VIEW_VALUES);
        for layer in Layer::ORDER {
            view.extend_from_slice(&cam.matrix(layer, aspect).0);
        }
        let e = cam.eye.sub(origin);
        view.extend([e.x, e.y, e.z]);

        let groups: Vec<(bool, bool, bool)> = scene
            .groups
            .iter()
            .map(|g| {
                let (n, f) = cam.layers(g.centre, g.radius);
                (n, f, cam.eye_layer(g.centre, g.radius))
            })
            .collect();
        // each moving shape's layers, the sky's always the far one alone
        let reach: Vec<(bool, bool, bool)> = moving
            .list
            .iter()
            .map(|s| {
                if s.pass == Pass::Sky {
                    (false, true, false)
                } else {
                    let (n, f) = cam.layers(s.at.t, s.at.reach());
                    (n, f, cam.eye_layer(s.at.t, s.at.reach()))
                }
            })
            .collect();

        let mut instances = Vec::new();
        let mut draws = Vec::new();
        for layer in Layer::ORDER {
            let pick = |r: &(bool, bool, bool)| match layer {
                Layer::Near => r.0,
                Layer::Far => r.1,
                Layer::Eye => r.2,
            };
            let mine = moving
                .list
                .iter()
                .zip(&reach)
                .filter(|(_, r)| pick(r))
                .map(|(s, _)| s);
            let moving_runs = pack(mine, origin, &mut instances);
            for pass in Pass::ORDER {
                for (g, r) in scene.groups.iter().zip(&groups) {
                    if !pick(r) {
                        continue;
                    }
                    for run in g.runs.iter().filter(|run| run.pass == pass) {
                        draws.extend([
                            STATIC_BUFFER,
                            run.mesh.0,
                            run.offset,
                            run.count,
                            pass as u32,
                            layer as u32,
                        ]);
                    }
                }
                for run in moving_runs.iter().filter(|run| run.pass == pass) {
                    draws.extend([
                        DYNAMIC_BUFFER,
                        run.mesh.0,
                        run.offset,
                        run.count,
                        pass as u32,
                        layer as u32,
                    ]);
                }
            }
        }
        Frame {
            view,
            instances,
            draws,
        }
    }
}

/// The GL state each pass needs, for the page: the shader's mode flags (two
/// vec4s of 0 and 1), blending, depth and culling. GL names are strings the
/// page looks up on its context.
pub fn pass_states() -> serde_json::Value {
    let state = |p: Pass| {
        let flag = |b: bool| b as i32;
        let (blend, write, test, cull): (Option<[&str; 2]>, bool, bool, Option<&str>) = match p {
            Pass::Sky => (None, false, false, None),
            Pass::Solid | Pass::Dither | Pass::Surface => (None, true, true, Some("BACK")),
            Pass::Ink => (None, true, true, Some("FRONT")),
            Pass::Shade => (
                Some(["SRC_ALPHA", "ONE_MINUS_SRC_ALPHA"]),
                false,
                true,
                Some("BACK"),
            ),
            Pass::Glow => (Some(["ONE", "ONE"]), false, true, Some("BACK")),
        };
        serde_json::json!({
            "id": p as u32,
            // sky, ink, glow, dither; then shade
            "mode_a": [flag(p == Pass::Sky), flag(p == Pass::Ink), flag(p == Pass::Glow), flag(p == Pass::Dither)],
            // shade, surface (T4)
            "mode_b": [flag(p == Pass::Shade), flag(p == Pass::Surface), 0, 0],
            "blend": blend,
            "depth_write": write,
            "depth_test": test,
            "cull": cull,
        })
    };
    serde_json::Value::Array(Pass::ORDER.iter().map(|&p| state(p)).collect())
}

/// How far apart two surfaces must be, `d` from the eye, for one step of a
/// 24-bit depth buffer with planes `near` and `far` to tell them apart:
/// d²·(far − near) / (far·near·2²⁴). All in Q16 metres.
pub fn depth_step(d: Fx, near: Fx, far: Fx) -> Fx {
    let (d, n, f) = (d as i128, near as i128, far as i128);
    (d * d * (f - n) / (f * n * (1 << 24))) as Fx
}

/// The same at `d` in the layer that draws it.
pub fn layered_depth_step(d: Fx) -> Fx {
    let (n, f) = Layer::at(d).planes();
    depth_step(d, n, f)
}

/// Depth steps two surfaces must be apart to be told apart reliably.
pub const DEPTH_STEPS: i32 = 4;

/// The terrain rings PLAN.md lays out (2 m cells to 256 m, then twice as
/// coarse with each doubling to 16 km), as (outer distance, cell), for the
/// depth test and for M2.
pub fn rings() -> Vec<(Fx, Fx)> {
    let mut out = vec![
        (fx::int(16), fx::int(2)),
        (fx::int(64), fx::int(2)),
        (fx::int(256), fx::int(2)),
    ];
    let (mut d, mut cell) = (fx::int(512), fx::int(4));
    loop {
        out.push((d, cell));
        if d >= FAR {
            return out;
        }
        d *= 2;
        cell *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{deg, int};
    use crate::geom::facing;

    fn cam() -> Camera {
        Camera {
            eye: v3(int(100), int(2), int(100)),
            fwd: facing(0, 0),
            fov: deg(60),
            roll: 0,
        }
    }

    /// M7 (R9.91): a blade 0.8 m long, 46 mm wide and 9 mm thick, inked,
    /// 20 m from the eye: kept 20 × 104 = 2 080 units thick (KEEP_VISIBLE_PER_M
    /// is 65 536 × 16 / 10 000 = 104 units a metre; 31.7 mm), its outline
    /// 3 120; at 2 m it is left as it was; so are a box and a small stud.
    #[test]
    fn a_thin_blade_far_off_keeps_a_width_and_its_outline_stays_wider() {
        let eye = v3(0, 0, 0);
        let blade = |at: Fx| {
            let mut s = Shapes::default();
            let a = Affine::translate(v3(0, 0, -at)).then(&Affine::scale(v3(
                ONE * 46 / 1000,
                ONE * 8 / 10,
                ONE * 9 / 1000,
            )));
            s.inked(Mesh::Cube, a, [ONE; 3], [0; 3]);
            s
        };
        let mut far = blade(int(20));
        let ink_before = far.list[1].at.c2.len();
        keep_visible(&mut far.list, eye);
        // to within the two units a rescale rounds away
        assert!(
            (far.list[0].at.c2.len() - 2080).abs() <= 2,
            "{}",
            far.list[0].at.c2.len()
        );
        assert!(
            (far.list[1].at.c2.len() - 3120).abs() <= 2,
            "{}",
            far.list[1].at.c2.len()
        );
        assert!(ink_before < 3120);
        assert_eq!(
            far.list[0].at.c0,
            blade(int(20)).list[0].at.c0,
            "only its thin side"
        );
        let mut near = blade(int(2));
        keep_visible(&mut near.list, eye);
        assert_eq!(near, blade(int(2)));
        let mut others = Shapes::default();
        others.solid(
            Mesh::Cube,
            Affine::translate(v3(0, 0, -int(30))).then(&Affine::scale(v3(ONE, ONE, ONE / 2))),
            [ONE; 3],
            0,
        );
        others.solid(
            Mesh::Cube,
            Affine::translate(v3(0, 0, -int(30))).then(&Affine::scale(v3(ONE / 10, ONE / 10, ONE / 200))),
            [ONE; 3],
            0,
        );
        others.glow(
            Mesh::Dot,
            Affine::translate(v3(0, 0, -int(30))).then(&Affine::scale(v3(ONE, ONE, ONE / 200))),
            [ONE; 3],
            ONE,
        );
        let before = others.clone();
        keep_visible(&mut others.list, eye);
        assert_eq!(others, before, "a box, a small stud and a glow are left alone");
    }

    #[test]
    fn an_inked_shape_is_drawn_twice_and_its_outline_is_evenly_larger() {
        let mut s = Shapes::default();
        let a = Affine::scale(v3(int(2), ONE, int(4)));
        s.inked(Mesh::Cube, a, [ONE; 3], [0; 3]);
        assert_eq!(s.len(), 2);
        let big = s.list[1].at;
        assert_eq!(s.list[1].pass, Pass::Ink);
        // a 4 m shape: 3 cm + 4 m / 160 = 5.5 cm a side, 11 cm on each axis
        let d0 = big.c0.len() - a.c0.len();
        let d2 = big.c2.len() - a.c2.len();
        let want = 2 * (ONE * 3 / 100 + int(4) / 160);
        assert!((d0 - want).abs() < 8 && (d2 - want).abs() < 8, "{d0} {d2} {want}");
    }

    #[test]
    fn draws_come_layer_by_layer_and_pass_by_pass_in_order() {
        let mut s = Shapes::default();
        let c = cam();
        // a glow, a solid with ink, and the sky, pushed out of order
        let near_box = Affine::translate(v3(int(100), int(1), int(90)));
        s.glow(Mesh::Sphere, near_box, [ONE; 3], ONE);
        s.inked(Mesh::Cube, near_box, [ONE; 3], [0; 3]);
        s.sky(c.eye);
        let f = Frame::build(&Scene::default(), &s, &c, ONE);
        let order: Vec<(u32, u32)> = f.draws.chunks(DRAW_VALUES).map(|d| (d[5], d[4])).collect();
        assert_eq!(
            order,
            vec![
                (Layer::Far as u32, Pass::Sky as u32),
                (Layer::Near as u32, Pass::Solid as u32),
                (Layer::Near as u32, Pass::Ink as u32),
                (Layer::Near as u32, Pass::Glow as u32),
            ]
        );
    }

    #[test]
    fn a_shape_is_drawn_in_each_layer_it_reaches_and_none_behind_the_eye() {
        let c = cam();
        let ahead = |m: i32| c.eye.add(c.fwd.scale(int(m)));
        assert_eq!(c.layers(ahead(10), ONE), (true, false));
        assert_eq!(c.layers(ahead(120), ONE), (true, true), "the shared band");
        assert_eq!(c.layers(ahead(2000), int(10)), (false, true));
        assert_eq!(
            c.layers(c.eye.sub(c.fwd.scale(int(10))), ONE),
            (false, false),
            "behind"
        );
        // a big thing behind whose edge reaches round the eye is still drawn
        assert_eq!(c.layers(c.eye.sub(c.fwd.scale(int(10))), int(50)), (true, false));
        assert_eq!(
            c.layers(ahead(17_000), int(100)),
            (false, false),
            "past the far plane"
        );
    }

    /// M1 (R3.4): a hand 0.3 m from the eye is drawn in the eye layer as
    /// well as the near one, which cuts it at 0.4 m; a foe 5 m off is not.
    #[test]
    fn what_lies_within_three_metres_of_the_eye_is_drawn_again_in_the_eye_layer() {
        let c = cam();
        let ahead = |cm: i32| c.eye.add(c.fwd.scale(cm * ONE / 100));
        assert!(c.eye_layer(ahead(30), ONE / 20));
        assert!(!c.eye_layer(ahead(500), ONE / 2));
        assert!(!c.eye_layer(c.eye.sub(c.fwd.scale(ONE)), ONE / 10), "behind");
        let mut s = Shapes::default();
        s.solid(
            Mesh::Cube,
            Affine::translate(ahead(30)).then(&Affine::scale(v3(ONE / 20, ONE / 20, ONE / 20))),
            [ONE; 3],
            0,
        );
        let f = Frame::build(&Scene::default(), &s, &c, ONE);
        let layers: Vec<u32> = f.draws.chunks(DRAW_VALUES).map(|d| d[5]).collect();
        assert_eq!(layers, vec![Layer::Near as u32, Layer::Eye as u32]);
        assert_eq!(f.view.len(), VIEW_VALUES);
    }

    #[test]
    fn positions_reach_the_page_relative_to_the_origin() {
        let mut g = Group::new("one box", v3(int(1000), 0, int(2000)), int(2));
        g.shapes.solid(
            Mesh::Cube,
            Affine::translate(v3(int(1001), ONE, int(2002))),
            [ONE; 3],
            0,
        );
        let origin = origin_for(v3(int(1003), int(2), int(2010)));
        // 1003 m is in the cell from 768 m; 2010 m in the cell from 1792 m
        assert_eq!(origin, v3(int(768), 0, int(1792)));
        let s = Scene::build(&[g], origin, 1);
        assert_eq!(
            (s.values[3], s.values[7], s.values[11]),
            (int(233), ONE, int(210))
        );
    }

    #[test]
    fn the_same_shapes_give_the_same_arrays_byte_for_byte() {
        let build = || {
            let mut s = Shapes::default();
            for k in 0..20 {
                let m = if k % 3 == 0 { Mesh::Cube } else { Mesh::Blob };
                s.inked(
                    m,
                    Affine::translate(v3(int(100 + k), 0, int(80))),
                    [ONE; 3],
                    [0; 3],
                );
            }
            s.sky(cam().eye);
            Frame::build(&Scene::default(), &s, &cam(), ONE)
        };
        assert_eq!(build(), build());
        assert!(!build().instances.is_empty());
    }

    #[test]
    fn the_depth_step_matches_the_hand_worked_formula() {
        // near 0.4 m, far 128 m, at 128 m: 128² × 127.6 / (128 × 0.4 × 2²⁴)
        // = 2 090 598 / 858 993 459 = 0.002 434 m, 159.5 Q16 steps
        let s = depth_step(int(128), NEAR, NEAR_LAYER_FAR);
        assert!((s - 159).abs() <= 1, "{s}");
        // one range from 0.4 m to 16 km, at 16 km: 16 384² × 16 383.6 /
        // (16 384 × 0.4 × 2²⁴) = 40 m
        let one = depth_step(FAR, NEAR, FAR);
        assert!((one - int(40)).abs() < int(1), "{}", one / ONE);
    }

    /// CW-18: at the outer edge of every ring, the depth buffer tells apart
    /// two surfaces a sixty-fourth of the ring's cell apart, with room for
    /// `DEPTH_STEPS` steps; in one range from the near plane to the horizon
    /// it could not.
    #[test]
    fn depth_precision_holds_at_every_ring_distance() {
        for (d, cell) in rings() {
            let need = DEPTH_STEPS * layered_depth_step(d);
            assert!(
                need < cell / 64,
                "at {} m: {} mm needed, {} mm allowed",
                d / ONE,
                need * 1000 / ONE,
                cell * 1000 / 64 / ONE
            );
        }
        let (d, cell) = *rings().last().unwrap();
        assert!(
            DEPTH_STEPS * depth_step(d, NEAR, FAR) > cell / 64,
            "one depth range would fail at the horizon"
        );
        assert_eq!(d, FAR, "the rings end at the far plane");
    }

    #[test]
    fn the_layers_overlap_so_the_seam_is_drawn_twice() {
        let (far_near, near_far) = (Layer::Far.planes().0, Layer::Near.planes().1);
        assert!(
            far_near < near_far,
            "the far layer starts before the near one ends"
        );
        assert_eq!(Layer::at(NEAR_LAYER_FAR), Layer::Near);
        assert_eq!(Layer::at(NEAR_LAYER_FAR + 1), Layer::Far);
    }

    #[test]
    fn every_pass_has_a_state_and_only_blended_passes_leave_depth_alone() {
        let states = pass_states();
        let list = states.as_array().unwrap();
        assert_eq!(list.len(), Pass::ORDER.len());
        for (s, p) in list.iter().zip(Pass::ORDER) {
            assert_eq!(s["id"], p as u32);
            let blended = !s["blend"].is_null();
            assert_eq!(s["depth_write"], !blended && p != Pass::Sky, "{p:?}");
        }
        // the page finds a pass by its id, not its place in the draw order
        let ink = list.iter().find(|s| s["id"] == Pass::Ink as u32).unwrap();
        assert_eq!(ink["cull"], "FRONT", "the inverted hull");
        // T4: a surface is drawn like a solid, with the surface flag on
        let surface = list.iter().find(|s| s["id"] == Pass::Surface as u32).unwrap();
        assert_eq!(
            (surface["cull"].as_str(), surface["mode_b"][1].as_i64()),
            (Some("BACK"), Some(1))
        );
    }

    #[test]
    fn the_shader_pushes_ink_back_and_halos_and_shadows_forward() {
        let vert = include_str!("../../../data/shaders/scene.vert");
        assert!(vert.contains(
            "gl_Position.z += (uModeA.y * uBias.x - (uModeA.z + uModeB.x) * uBias.y) * uQ * gl_Position.w;"
        ));
    }
}
