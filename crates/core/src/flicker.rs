//! Flicker (T3, CW-18): two surfaces drawn in one plane, facing one way,
//! fight for the depth buffer, and which of them shows changes from frame to
//! frame as the camera moves (z-fighting). This finds them in the static
//! array the page receives. Ported from Long Time Comin's `flicker.rs`,
//! with three changes: it reads `render::Scene` (both depth layers),
//! `Seen::apart` uses the depth step of the layer that draws a distance, and
//! faces in one plane are swept in order along one axis, so ground made of
//! tens of thousands of coplanar triangles scans in a moment.
//!
//! A face is one triangle of a flat-sided mesh (`prim::MeshTable::flat_faced`)
//! drawn as an opaque solid, placed. Two faces fight when they face the same
//! way, overlap by more than `MIN_AREA`, differ in colour, value or pass
//! (same-coloured faces in one plane fight invisibly), and their planes are
//! within `PLANE_TOL` of each other, or further apart than that but too near
//! for the depth buffer to tell apart from the nearest place a camera comes.
//! And only where it can be seen: no closed box presses on the fight from in
//! front (a crate's bottom on the floor), and a face looking down has room
//! under it for a camera.
//!
//! Ink, halos and shadows are left out: the vertex shader draws every ink
//! hull a few depth steps back and every halo and shadow a few forward
//! (`uBias` in `data/shaders/scene.vert`), so a tie with a solid always goes
//! the solid's way, or the halo's.
//!
//! The other flicker tests: depth precision at every ring distance and the
//! dithered hand-over (`render.rs`, `look.rs`), stable instance order
//! (`game.rs`), and identical pixels on two still frames (the browser check,
//! `tests/browser/interactions.py`).

use std::collections::BTreeMap;

use crate::fx::{Fx, ONE};
use crate::geom::{v3, Affine, V3};
use crate::look::Rgb;
use crate::prim::{self, Mesh, MeshId, MeshTable};
use crate::render::{self, Pass, Scene, INSTANCE_VALUES};

/// Planes nearer than this are one plane wherever they are seen from (two
/// millimetres: the rounding of shapes placed to lie in one plane).
pub const PLANE_TOL: i64 = (ONE / 500) as i64;
/// Overlaps smaller than this (a 10 cm square) are let be.
pub const MIN_AREA: i64 = (ONE as i64) * (ONE as i64) / 100;
/// The faces' normals within about a degree of each other.
const SAME_WAY: i64 = (ONE as i64) - (ONE as i64) / 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inst {
    pub mesh: MeshId,
    pub at: Affine,
    pub rgb: Rgb,
    pub value: Fx,
    pub pass: Pass,
}

/// The instances of a static array, back in world coordinates.
pub fn instances(scene: &Scene) -> Vec<Inst> {
    let o = scene.origin;
    let mut out = Vec::new();
    for run in scene.runs() {
        let first = run.offset as usize / render::BYTES_PER_VALUE;
        for k in 0..run.count as usize {
            let v = &scene.values[first + k * INSTANCE_VALUES..first + (k + 1) * INSTANCE_VALUES];
            out.push(Inst {
                mesh: run.mesh,
                at: Affine {
                    c0: v3(v[0], v[1], v[2]),
                    c1: v3(v[4], v[5], v[6]),
                    c2: v3(v[8], v[9], v[10]),
                    t: v3(v[3], v[7], v[11]).add(o),
                },
                rgb: [v[12], v[13], v[14]],
                value: v[15],
                pass: run.pass,
            });
        }
    }
    out
}

type P3 = [i64; 3];
type P2 = [i64; 2];

#[derive(Debug, Clone)]
struct Face {
    inst: usize,
    /// the way it is drawn to, a unit vector in Q16
    n: P3,
    /// its plane's distance from the origin along `n`, Q16
    d: i64,
    pts: [P3; 3],
}

fn isqrt(v: u128) -> u128 {
    if v < 2 {
        return v;
    }
    let mut x = v;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}

fn place(a: &Affine, p: (i32, i32, i32)) -> P3 {
    let m = |c: i32, k: i32| (c as i64 * k as i64) >> 16;
    [
        m(a.c0.x, p.0) + m(a.c1.x, p.1) + m(a.c2.x, p.2) + a.t.x as i64,
        m(a.c0.y, p.0) + m(a.c1.y, p.1) + m(a.c2.y, p.2) + a.t.y as i64,
        m(a.c0.z, p.0) + m(a.c1.z, p.1) + m(a.c2.z, p.2) + a.t.z as i64,
    ]
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: P3, b: P3) -> [i128; 3] {
    let (a, b) = (a.map(|x| x as i128), b.map(|x| x as i128));
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Drawn as an opaque solid: not ink, a halo, a shadow or the sky.
fn solid(s: &Inst) -> bool {
    s.pass.opaque()
}

/// A closed solid that hides what is pressed against it.
fn closed(s: &Inst) -> bool {
    solid(s)
        && matches!(
            s.mesh.prim(),
            Some(Mesh::Cube | Mesh::Cyl | Mesh::Sphere | Mesh::Blob | Mesh::Capsule)
        )
}

/// A triangle smaller than this (twice its area, Q32: half a square
/// centimetre) is a sliver.
const SLIVER: i128 = (ONE as i128) * (ONE as i128) / 10_000;

fn faces(insts: &[Inst], table: &MeshTable) -> Vec<Face> {
    let mut meshes: BTreeMap<MeshId, Vec<i32>> = BTreeMap::new();
    let mut out = Vec::new();
    for (i, s) in insts.iter().enumerate() {
        if !table.flat_faced(s.mesh) || !solid(s) {
            continue;
        }
        let vs = meshes.entry(s.mesh).or_insert_with(|| table.vertices(s.mesh));
        let stride = prim::VERTEX_VALUES;
        for t in vs.chunks(stride * 3) {
            let p = |k: usize| place(&s.at, (t[k * stride], t[k * stride + 1], t[k * stride + 2]));
            let pts = [p(0), p(1), p(2)];
            let c = cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]));
            let len = isqrt((c[0] * c[0] + c[1] * c[1] + c[2] * c[2]) as u128) as i128;
            if len < SLIVER {
                continue;
            }
            let n = c.map(|x| (x * ONE as i128 / len) as i64);
            let d = (n[0] as i128 * pts[0][0] as i128
                + n[1] as i128 * pts[0][1] as i128
                + n[2] as i128 * pts[0][2] as i128)
                >> 16;
            out.push(Face {
                inst: i,
                n,
                d: d as i64,
                pts,
            });
        }
    }
    out
}

/// Steps of the depth buffer two planes need between them to be told apart.
const DEPTH_STEPS: i64 = render::DEPTH_STEPS as i64;
/// How far in front of a fight to look for a box it is pressed against.
const PROBE: i64 = (ONE / 200) as i64;
/// A face looking into a box this near is in a crevice no camera sees into.
const CREVICE: i64 = (ONE * 6 / 100) as i64;
/// A face looking down this near a floor has no room under it for a camera.
const UNDER: i64 = (ONE * 3 / 10) as i64;

/// Is `p` inside the shape `s` (its columns at right angles)?
fn inside(s: &Inst, p: P3) -> bool {
    let a = &s.at;
    let rel = [p[0] - a.t.x as i64, p[1] - a.t.y as i64, p[2] - a.t.z as i64];
    let mut local = [0i128; 3];
    for (k, c) in [a.c0, a.c1, a.c2].iter().enumerate() {
        let c = [c.x as i128, c.y as i128, c.z as i128];
        let len2 = c[0] * c[0] + c[1] * c[1] + c[2] * c[2];
        if len2 == 0 {
            return false;
        }
        let dot = c[0] * rel[0] as i128 + c[1] * rel[1] as i128 + c[2] * rel[2] as i128;
        local[k] = dot * ONE as i128 / len2;
    }
    let half = (ONE / 2) as i128;
    let quarter = half * half;
    match s.mesh.prim() {
        Some(Mesh::Cube) => local.iter().all(|l| l.abs() < half),
        Some(Mesh::Cyl) => local[1].abs() < half && local[0] * local[0] + local[2] * local[2] < quarter,
        _ => local.iter().map(|l| l * l).sum::<i128>() < quarter,
    }
}

/// The two axes kept when the normal's largest axis is dropped.
fn kept(drop: usize) -> (usize, usize) {
    match drop {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    }
}

/// The triangle in the plane's 2-D projection, turned counter-clockwise.
fn flatten(f: &Face, drop: usize) -> Vec<P2> {
    let (u, v) = kept(drop);
    let mut p: Vec<P2> = f.pts.iter().map(|q| [q[u], q[v]]).collect();
    if area2(&p) < 0 {
        p.reverse();
    }
    p
}

fn area2(p: &[P2]) -> i128 {
    let mut s = 0i128;
    for k in 0..p.len() {
        let (a, b) = (p[k], p[(k + 1) % p.len()]);
        s += a[0] as i128 * b[1] as i128 - b[0] as i128 * a[1] as i128;
    }
    s
}

/// Clip the convex polygon `subject` by the convex, counter-clockwise `clip`.
fn clip(subject: Vec<P2>, clip: &[P2]) -> Vec<P2> {
    let mut out = subject;
    for k in 0..clip.len() {
        if out.is_empty() {
            break;
        }
        let (a, b) = (clip[k], clip[(k + 1) % clip.len()]);
        let side = |p: P2| -> i128 {
            (b[0] - a[0]) as i128 * (p[1] - a[1]) as i128 - (b[1] - a[1]) as i128 * (p[0] - a[0]) as i128
        };
        let input = std::mem::take(&mut out);
        for i in 0..input.len() {
            let (p, q) = (input[i], input[(i + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if sp >= 0 {
                out.push(p);
            }
            if (sp >= 0) != (sq >= 0) {
                let t = |x: i64, y: i64| x + ((y - x) as i128 * sp / (sp - sq)) as i64;
                out.push([t(p[0], q[0]), t(p[1], q[1])]);
            }
        }
    }
    out
}

/// The area each pair of instances shares in one plane, its largest piece,
/// where that is and the way it faces.
type Shared = BTreeMap<(usize, usize), (i64, i64, V3, [i64; 3])>;

/// One fight: two instances and the area they share in one plane (Q32 m²).
#[derive(Debug, Clone)]
pub struct Fight {
    pub n: [i64; 3],
    pub a: usize,
    pub b: usize,
    pub area: i64,
    /// a point in the shared area, Q16
    pub at: V3,
}

/// Where the static scene is seen from, for what a camera there can tell
/// apart.
#[derive(Debug, Clone)]
pub struct Seen {
    /// nothing looks up from under this
    pub bottom: Fx,
    /// where a camera can be; none: the planes are judged as if from near by
    pub eyes: Vec<V3>,
    /// past this from every eye nothing is drawn
    pub far: Fx,
}

impl Seen {
    pub fn near() -> Seen {
        Seen {
            bottom: i32::MIN,
            eyes: Vec::new(),
            far: 0,
        }
    }

    /// How far apart two parallel planes must be to be told apart at `d`
    /// from the lens, in the layer that draws there.
    pub fn apart(d: i64) -> i64 {
        DEPTH_STEPS * render::layered_depth_step(d.clamp(0, i32::MAX as i64) as Fx) as i64
    }

    /// M7 (R5.2): the widest gap the depth buffer cannot tell apart anywhere
    /// out to `far`. The step is not monotonic across the two layers (the
    /// near layer's at 128 m is wider than the far layer's out to about
    /// 1 km), so this is the worse of the near layer's at its farthest and
    /// the step at `far` itself.
    pub fn window(far: i64) -> i64 {
        let far = far.clamp(0, i32::MAX as i64);
        let near = Seen::apart(far.min(render::NEAR_LAYER_FAR as i64));
        near.max(Seen::apart(far))
    }

    /// The nearest a camera comes to `p`.
    fn reach(&self, p: P3) -> i64 {
        self.eyes
            .iter()
            .map(|e| {
                let d = [p[0] - e.x as i64, p[1] - e.y as i64, p[2] - e.z as i64];
                isqrt(
                    (d[0] as i128 * d[0] as i128 + d[1] as i128 * d[1] as i128 + d[2] as i128 * d[2] as i128)
                        as u128,
                ) as i64
            })
            .min()
            .unwrap_or(0)
    }
}

/// Every pair of instances that fight, with the area they share.
pub fn fights(insts: &[Inst], table: &MeshTable, seen: &Seen) -> Vec<Fight> {
    fights_over(insts, table, seen, MIN_AREA)
}

/// M3: the same, letting be only overlaps of `min_area` or less (a
/// costume's plates are smaller than `MIN_AREA`'s 10 cm square).
pub fn fights_over(insts: &[Inst], table: &MeshTable, seen: &Seen, min_area: i64) -> Vec<Fight> {
    let bottom = seen.bottom;
    // the widest gap the depth buffer cannot tell apart, short of the far plane
    let window = if seen.eyes.is_empty() {
        PLANE_TOL
    } else {
        PLANE_TOL.max(Seen::window(seen.far as i64))
    };
    let all = faces(insts, table);
    let boxes: Vec<usize> = (0..insts.len()).filter(|&k| closed(&insts[k])).collect();
    // faces facing one way, by plane
    let q = |x: i64| (x * 64 + (ONE as i64) / 2).div_euclid(ONE as i64);
    let mut buckets: BTreeMap<(i64, i64, i64), Vec<usize>> = BTreeMap::new();
    for (k, f) in all.iter().enumerate() {
        buckets
            .entry((q(f.n[0]), q(f.n[1]), q(f.n[2])))
            .or_default()
            .push(k);
    }
    let mut shared: Shared = BTreeMap::new();
    for list in buckets.values_mut() {
        list.sort_by_key(|&k| all[k].d);
        // runs of planes each within `window` of the next
        let mut start = 0;
        while start < list.len() {
            let mut end = start + 1;
            while end < list.len() && all[list[end]].d - all[list[end - 1]].d <= window {
                end += 1;
            }
            sweep(
                &all,
                &list[start..end],
                insts,
                seen,
                window,
                bottom,
                &boxes,
                &mut shared,
            );
            start = end;
        }
    }
    shared
        .into_iter()
        .filter(|(_, (area, _, _, _))| *area > min_area)
        .map(|((a, b), (area, _, at, n))| Fight { n, a, b, area, at })
        .collect()
}

/// The pairs in one run of near planes, swept in order along one axis of
/// their shared projection.
#[allow(clippy::too_many_arguments)]
fn sweep(
    all: &[Face],
    run: &[usize],
    insts: &[Inst],
    seen: &Seen,
    window: i64,
    bottom: Fx,
    boxes: &[usize],
    shared: &mut Shared,
) {
    if run.len() < 2 {
        return;
    }
    let f0 = &all[run[0]];
    let drop = (0..3).max_by_key(|&a| f0.n[a].abs()).unwrap_or(1);
    let flat: Vec<Vec<P2>> = run.iter().map(|&k| flatten(&all[k], drop)).collect();
    let lo = |p: &[P2], a: usize| p.iter().map(|q| q[a]).min().unwrap_or(0);
    let hi = |p: &[P2], a: usize| p.iter().map(|q| q[a]).max().unwrap_or(0);
    let mut order: Vec<usize> = (0..run.len()).collect();
    order.sort_by_key(|&i| (lo(&flat[i], 0), i));
    for (x, &i) in order.iter().enumerate() {
        let f = &all[run[i]];
        let pa = &flat[i];
        for &j in &order[x + 1..] {
            let pb = &flat[j];
            if lo(pb, 0) >= hi(pa, 0) {
                break;
            }
            if lo(pa, 1) >= hi(pb, 1) || lo(pb, 1) >= hi(pa, 1) {
                continue;
            }
            let g = &all[run[j]];
            let gap = (g.d - f.d).abs();
            if gap > window || f.inst == g.inst {
                continue;
            }
            let (si, sj) = (&insts[f.inst], &insts[g.inst]);
            if si.rgb == sj.rgb && si.value == sj.value && si.pass == sj.pass {
                continue;
            }
            let dot = (f.n[0] as i128 * g.n[0] as i128
                + f.n[1] as i128 * g.n[1] as i128
                + f.n[2] as i128 * g.n[2] as i128)
                >> 16;
            if (dot as i64) < SAME_WAY {
                continue;
            }
            let common = clip(pa.clone(), pb);
            if common.len() < 3 {
                continue;
            }
            let a2 = area2(&common);
            let area = (a2 * ONE as i128 / (2 * f.n[drop].abs().max(1) as i128)) as i64;
            if area <= 0 {
                continue;
            }
            let c = common.len() as i64;
            let mid2 = [
                common.iter().map(|p| p[0]).sum::<i64>() / c,
                common.iter().map(|p| p[1]).sum::<i64>() / c,
            ];
            let mut p = [0i64; 3];
            let (u, v) = kept(drop);
            p[u] = mid2[0];
            p[v] = mid2[1];
            let rest = (f.n[u] as i128 * p[u] as i128 + f.n[v] as i128 * p[v] as i128) >> 16;
            p[drop] = ((f.d as i128 - rest) * ONE as i128 / f.n[drop] as i128) as i64;
            // M7: how far apart they are where they overlap: `p` lies in
            // the first's plane; its height over the second's (two planes a
            // hair off parallel and far from the origin can have offsets
            // alike and still stand centimetres apart there, R9.46)
            let over = ((g.n[0] as i128 * p[0] as i128
                + g.n[1] as i128 * p[1] as i128
                + g.n[2] as i128 * p[2] as i128)
                >> 16) as i64
                - g.d;
            let gap = gap.max(over.abs());
            // planes apart: fighting only where a camera is too far off to
            // tell them apart, and short of the far plane
            if gap > PLANE_TOL {
                let d = seen.reach(p);
                if d >= seen.far as i64 || gap >= Seen::apart(d) {
                    continue;
                }
            }
            // a fight pressed against a box, in front of both faces, is never seen
            let front = [0, 1, 2].map(|a| p[a] + f.n[a] * (gap + PROBE) / ONE as i64);
            let deep = [0, 1, 2].map(|a| p[a] + f.n[a] * (gap + CREVICE) / ONE as i64);
            if boxes
                .iter()
                .any(|&k| inside(&insts[k], front) || inside(&insts[k], deep))
            {
                continue;
            }
            // a fight facing down at or under the lowest floor, or with a box
            // under it, is seen by no camera
            let down = f.n[1] < -(ONE as i64) * 9 / 10;
            if down && p[1] <= bottom as i64 + PLANE_TOL {
                continue;
            }
            if down
                && boxes
                    .iter()
                    .any(|&k| inside(&insts[k], [p[0], p[1] - UNDER, p[2]]))
            {
                continue;
            }
            let key = (f.inst.min(g.inst), f.inst.max(g.inst));
            let e = shared.entry(key).or_insert((0, 0, V3::ZERO, f.n));
            e.0 += area;
            if area > e.1 {
                e.1 = area;
                e.2 = v3(p[0] as i32, p[1] as i32, p[2] as i32);
                e.3 = f.n;
            }
        }
    }
}

/// One line per fight: what fights what, where, and how much.
pub fn describe(insts: &[Inst], f: &Fight) -> String {
    let cm = |x: i32| x as i64 * 100 / ONE as i64;
    let one = |s: &Inst| {
        let a = &s.at;
        format!(
            "{:?} {:?} at ({}, {}, {}) cm, size ({}, {}, {}) cm",
            s.mesh,
            s.pass,
            cm(a.t.x),
            cm(a.t.y),
            cm(a.t.z),
            cm(a.c0.len()),
            cm(a.c1.len()),
            cm(a.c2.len())
        )
    };
    format!(
        "{} vs {}: {} cm² at ({}, {}, {}) cm",
        one(&insts[f.a]),
        one(&insts[f.b]),
        (f.area as i128 * 10_000) >> 32,
        cm(f.at.x),
        cm(f.at.y),
        cm(f.at.z)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::int;
    use crate::render::Group;

    fn cube(at: V3, size: V3, rgb: Rgb) -> Inst {
        Inst {
            mesh: Mesh::Cube.into(),
            at: Affine::translate(at).then(&Affine::scale(size)),
            rgb,
            value: 0,
            pass: Pass::Solid,
        }
    }

    #[test]
    fn two_boxes_whose_tops_meet_in_one_plane_fight_and_a_lifted_one_does_not() {
        let floor = cube(v3(0, -ONE / 2, 0), v3(int(4), ONE, int(4)), [ONE, 0, 0]);
        // a rug flush with the floor's top
        let rug = cube(v3(0, -ONE / 20, 0), v3(int(2), ONE / 10, int(2)), [0, ONE, 0]);
        let f = fights(&[floor.clone(), rug], &MeshTable::default(), &Seen::near());
        assert_eq!(f.len(), 1, "a rug flush with the floor fights it: {f:?}");
        // the overlap is the rug's top, 2 m by 2 m
        assert!(
            (f[0].area - int(4) as i64 * ONE as i64).abs() < ONE as i64,
            "{f:?}"
        );
        let lifted = cube(
            v3(0, -ONE / 20 + ONE / 50, 0),
            v3(int(2), ONE / 10, int(2)),
            [0, ONE, 0],
        );
        assert!(
            fights(&[floor, lifted], &MeshTable::default(), &Seen::near()).is_empty(),
            "two centimetres up, it does not"
        );
    }

    #[test]
    fn a_band_in_a_walls_side_fights_and_one_colour_a_halo_or_a_crevice_does_not() {
        let wall = cube(v3(0, 0, 0), v3(int(2), int(2), int(2)), [ONE, 0, 0]);
        let band = cube(v3(0, 0, 0), v3(int(2), ONE / 5, int(3)), [0, 0, ONE]);
        assert_eq!(
            fights(
                &[wall.clone(), band.clone()],
                &MeshTable::default(),
                &Seen::near()
            )
            .len(),
            1
        );
        let halo = Inst {
            pass: Pass::Glow,
            ..band
        };
        assert!(
            fights(&[wall.clone(), halo], &MeshTable::default(), &Seen::near()).is_empty(),
            "the shader settles a halo"
        );
        let twin = cube(v3(0, ONE / 2, 0), v3(int(2), int(1), int(2)), [ONE, 0, 0]);
        assert!(
            fights(&[wall, twin], &MeshTable::default(), &Seen::near()).is_empty(),
            "one colour in one plane shows no fight"
        );
        // two crates' backs against a wall, in one plane, face into it
        let back = cube(v3(ONE / 2, int(2), 0), v3(ONE, int(4), int(9)), [0, ONE, 0]);
        let a = cube(v3(-ONE, ONE, 0), v3(int(2), int(2), int(2)), [ONE, ONE, 0]);
        let b = cube(v3(-ONE / 2, ONE * 3 / 4, 0), v3(ONE, ONE, ONE), [0, ONE, ONE]);
        assert!(
            !fights(&[a.clone(), b.clone()], &MeshTable::default(), &Seen::near()).is_empty(),
            "alone, their backs fight"
        );
        assert!(
            fights(&[back, a, b], &MeshTable::default(), &Seen::near()).is_empty(),
            "backs pressed on a wall are never seen"
        );
    }

    #[test]
    fn planes_apart_fight_only_where_the_depth_buffer_cannot_tell_them_apart() {
        // two squares 5 mm apart: told apart from 10 m, not from 10 km
        let sq = |y: i32, c: Rgb| Inst {
            mesh: Mesh::Square.into(),
            at: Affine::translate(v3(0, y, 0)).then(&Affine::scale(v3(int(50), ONE, int(50)))),
            rgb: c,
            value: 0,
            pass: Pass::Solid,
        };
        let pair = [sq(0, [ONE, 0, 0]), sq(ONE / 200, [0, ONE, 0])];
        let near = Seen {
            bottom: i32::MIN,
            eyes: vec![v3(0, int(10), 0)],
            far: render::FAR,
        };
        assert!(fights(&pair, &MeshTable::default(), &near).is_empty());
        let far = Seen {
            eyes: vec![v3(0, int(10_000), 0)],
            ..near
        };
        assert_eq!(fights(&pair, &MeshTable::default(), &far).len(), 1);
    }

    /// R5.2: two squares 5 mm apart seen from 100 m up, the scene drawn to
    /// 300 m. At 100 m the near layer cannot tell 5 mm apart (its step there
    /// times DEPTH_STEPS is over 5 mm), so they fight; the step at 300 m (the
    /// far layer's, finer) must not hide the pair from the scan.
    #[test]
    fn the_scan_looks_as_wide_as_the_worst_depth_step_short_of_its_far_plane() {
        assert!(
            Seen::apart(int(100) as i64) > (ONE / 200) as i64,
            "the premise: 5 mm is too near at 100 m"
        );
        assert!(
            Seen::apart(int(300) as i64) < (ONE / 200) as i64,
            "and not too near at 300 m"
        );
        let sq = |y: i32, c: Rgb| Inst {
            mesh: Mesh::Square.into(),
            at: Affine::translate(v3(0, y, 0)).then(&Affine::scale(v3(int(50), ONE, int(50)))),
            rgb: c,
            value: 0,
            pass: Pass::Solid,
        };
        let pair = [sq(0, [ONE, 0, 0]), sq(ONE / 200, [0, ONE, 0])];
        let seen = Seen {
            bottom: i32::MIN,
            eyes: vec![v3(0, int(100), 0)],
            far: int(300),
        };
        assert_eq!(fights(&pair, &MeshTable::default(), &seen).len(), 1);
        assert_eq!(Seen::window(int(300) as i64), Seen::apart(int(128) as i64));
        assert_eq!(Seen::window(int(50) as i64), Seen::apart(int(50) as i64));
    }

    #[test]
    fn the_static_array_read_back_is_the_shapes_pushed() {
        let mut g = Group::new("one", v3(int(300), 0, int(300)), int(3));
        let a = Affine::translate(v3(int(301), ONE, int(302))).then(&Affine::scale(v3(ONE, int(2), ONE)));
        g.shapes.solid(Mesh::Cube, a, [ONE, 0, 0], 0);
        let scene = Scene::build(&[g], render::origin_for(v3(int(300), 0, int(300))), 1);
        let i = instances(&scene);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].at, a);
        assert_eq!(i[0].pass, Pass::Solid);
    }

    /// The practice room as the page receives it (the static array: floor,
    /// walls, balcony, slope, ramps, shaft, blocks and stripes), seen from
    /// across the room: no two surfaces in one plane. (Chumworld Arena
    /// scanned its ring here.)
    #[test]
    fn the_static_scene_draws_no_two_surfaces_in_one_plane() {
        let g = crate::game::tests::game();
        let mut eyes = Vec::new();
        for dx in [-24, 0, 24] {
            for dz in [-24, 0, 24] {
                eyes.push(v3(int(dx), int(2), int(dz)));
            }
        }
        let insts = instances(&g.scene);
        let tris: usize = insts
            .iter()
            .map(|i| g.meshes.vertices(i.mesh).len() / (3 * prim::VERTEX_VALUES))
            .sum();
        // M1: about twenty boxes and three wedges, each drawn and inked
        assert!(tris > 400, "the scan reads the room's triangles: {tris}");
        let seen = Seen {
            bottom: -int(1),
            eyes,
            far: render::FAR,
        };
        let f: Vec<Fight> = fights(&insts, &g.meshes, &seen);
        let report: Vec<String> = f.iter().take(8).map(|x| describe(&insts, x)).collect();
        assert!(f.is_empty(), "{} pairs fight:\n{}", f.len(), report.join("\n"));
    }
}
