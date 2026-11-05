//! `mesh`: owned by M3 Models (task A) in PLAN.md.
//!
//! Faceted primitives beyond `prim.rs`'s basic set, for the characters and
//! props in `models.rs`. Each is a variant appended to `prim::Mesh` and
//! `Mesh::ALL`, with an arm in `prim::vertices` that calls a builder here.
//! Like the primitives, each is centred on the origin, inside the unit cube,
//! white, and wound counter-clockwise seen from outside.
//!
//! - `facet`: Protection of the Spheres' ball of 80 flat faces (its
//!   `Mesh::Facet`): an icosahedron with each face cut in four and pushed out
//!   to the sphere. Heads, bodies, joints, rocks and crowns.
//! - `dot`: a low smooth globe (60 triangles) for the marks on a face: eyes,
//!   catch-lights, blush, buttons. Smooth, so a dot reads round however
//!   small it is drawn.
//! - `wedge`: a triangular prism, ridge along z: roofs, tents, arrow tips.
//! - `half_cyl`: a half cylinder lying along z, flat side down: a chest's
//!   lid, a cart's canopy.
//! - `taper`: an eight-sided column whose top is `TAPER_PCT` of its base:
//!   skirts, coat tails, trunks, hat crowns.
//! - `tube`: a ring of sixteen sides with a hole of `TUBE_HOLE_PCT`: a
//!   well's wall, a wheel's rim, a key's bow.
//! - `pyramid`: four sides to an apex: a shrine's roof, a post's cap.
//! - T21: `round`, `rod`, `bell`: a box, a cylinder and a taper with their
//!   edges rounded and their normals smooth, so the skins read as modelled
//!   forms rather than blocks. Each is built from a flat shape whose
//!   surface is pushed out from an inner core by the rounding radius,
//!   the normal being the direction pushed.
//!
//! How a module joins the world (state, step, draw, scene, screen, act):
//! the extension contract in `world.rs`.

use crate::fx::{self, ONE};
use crate::geom::{v3, V3};
use crate::prim::{push_coloured, push_smooth};

const H: i32 = ONE / 2;
const WHITE: [i32; 3] = [ONE; 3];

/// The taper's top radius, percent of its base.
pub const TAPER_PCT: i32 = 60;
/// The tube's hole, percent of its outer radius.
pub const TUBE_HOLE_PCT: i32 = 72;

fn tri(o: &mut Vec<i32>, a: V3, b: V3, c: V3) {
    push_coloured(o, a, b, c, WHITE);
}

fn quad(o: &mut Vec<i32>, a: V3, b: V3, c: V3, d: V3) {
    tri(o, a, b, c);
    tri(o, a, c, d);
}

/// A point on a circle of radius `r` at height `y`, `j` of `n` round,
/// counter-clockwise seen from above starting at +x.
fn rim(j: i32, n: i32, r: i32, y: i32) -> V3 {
    let a = fx::TURN * j / n;
    v3(fx::mul(fx::cos(a), r), y, fx::mul(-fx::sin(a), r))
}

/// The ball of 80 flat faces.
pub fn facet() -> Vec<i32> {
    // the golden ratio in Q16 (1.618 034 × 65 536 = 106 039.5)
    const PHI: i32 = 106_039;
    let ico = [
        v3(-ONE, PHI, 0),
        v3(ONE, PHI, 0),
        v3(-ONE, -PHI, 0),
        v3(ONE, -PHI, 0),
        v3(0, -ONE, PHI),
        v3(0, ONE, PHI),
        v3(0, -ONE, -PHI),
        v3(0, ONE, -PHI),
        v3(PHI, 0, -ONE),
        v3(PHI, 0, ONE),
        v3(-PHI, 0, -ONE),
        v3(-PHI, 0, ONE),
    ];
    const FACES: [[usize; 3]; 20] = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    let out = |p: V3| p.norm().scale(H);
    let mid = |a: V3, b: V3| out(v3((a.x + b.x) / 2, (a.y + b.y) / 2, (a.z + b.z) / 2));
    let mut o = Vec::new();
    let mut face = |a: V3, b: V3, c: V3| {
        // wound so the face looks away from the centre
        let n = b.sub(a).cross(c.sub(a));
        if n.dot(a.add(b).add(c)) >= 0 {
            tri(&mut o, a, b, c);
        } else {
            tri(&mut o, a, c, b);
        }
    };
    for f in FACES {
        let (a, b, c) = (out(ico[f[0]]), out(ico[f[1]]), out(ico[f[2]]));
        let (ab, bc, ca) = (mid(a, b), mid(b, c), mid(c, a));
        face(a, ab, ca);
        face(ab, b, bc);
        face(ca, bc, c);
        face(ab, bc, ca);
    }
    o
}

/// A low smooth globe: 6 bands of 10, each vertex's normal its own direction.
pub fn dot() -> Vec<i32> {
    let (lat, lon) = (6, 10);
    let at = |i: i32, j: i32| {
        let a = fx::HALF_TURN * i / lat - fx::QUARTER_TURN;
        let b = fx::TURN * j / lon;
        let r = fx::cos(a);
        v3(
            fx::mul(fx::mul(r, fx::cos(b)), H),
            fx::mul(fx::sin(a), H),
            fx::mul(fx::mul(r, -fx::sin(b)), H),
        )
    };
    let mut o = Vec::new();
    let mut smooth = |ps: [V3; 3]| {
        for p in ps {
            let n = p.norm();
            o.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, ONE, ONE, ONE]);
        }
    };
    for i in 0..lat {
        for j in 0..lon {
            let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
            if i > 0 {
                smooth([a, b, c]);
            }
            if i < lat - 1 {
                smooth([a, c, d]);
            }
        }
    }
    o
}

/// A triangular prism: its base the bottom face, its ridge along z at the top.
pub fn wedge() -> Vec<i32> {
    let mut o = Vec::new();
    let (bl, br, top) = (|z| v3(-H, -H, z), |z| v3(H, -H, z), |z| v3(0, H, z));
    // the two gable ends
    tri(&mut o, bl(H), br(H), top(H));
    tri(&mut o, br(-H), bl(-H), top(-H));
    // the two slopes and the floor
    quad(&mut o, br(H), br(-H), top(-H), top(H));
    quad(&mut o, bl(-H), bl(H), top(H), top(-H));
    quad(&mut o, bl(-H), br(-H), br(H), bl(H));
    o
}

/// A half cylinder along z, flat side down, its arch reaching the top.
pub fn half_cyl() -> Vec<i32> {
    const SIDES: i32 = 8;
    let mut o = Vec::new();
    // the arch from +x over the top to −x, its height stretched to fill the cube
    let arch = |j: i32, z: i32| {
        let a = fx::HALF_TURN * j / SIDES;
        v3(fx::mul(fx::cos(a), H), -H + fx::mul(fx::sin(a), ONE), z)
    };
    for j in 0..SIDES {
        let (a, b, c, d) = (arch(j, H), arch(j, -H), arch(j + 1, -H), arch(j + 1, H));
        quad(&mut o, a, b, c, d);
        // the end caps, fanned from the middle of the flat edge
        tri(&mut o, v3(0, -H, H), arch(j, H), arch(j + 1, H));
        tri(&mut o, v3(0, -H, -H), arch(j + 1, -H), arch(j, -H));
    }
    quad(&mut o, v3(-H, -H, H), v3(-H, -H, -H), v3(H, -H, -H), v3(H, -H, H));
    o
}

/// An eight-sided column narrowing to `TAPER_PCT` at its top.
pub fn taper() -> Vec<i32> {
    const SIDES: i32 = 8;
    let top = H * TAPER_PCT / 100;
    let mut o = Vec::new();
    for j in 0..SIDES {
        let (a, b) = (rim(j, SIDES, H, -H), rim(j + 1, SIDES, H, -H));
        let (c, d) = (rim(j + 1, SIDES, top, H), rim(j, SIDES, top, H));
        quad(&mut o, a, b, c, d);
        tri(&mut o, b, a, v3(0, -H, 0));
        tri(&mut o, d, c, v3(0, H, 0));
    }
    o
}

/// A ring of sixteen sides, open through the middle.
pub fn tube() -> Vec<i32> {
    const SIDES: i32 = 16;
    let inner = H * TUBE_HOLE_PCT / 100;
    let mut o = Vec::new();
    for j in 0..SIDES {
        let (a, b) = (rim(j, SIDES, H, -H), rim(j + 1, SIDES, H, -H));
        let (c, d) = (rim(j + 1, SIDES, H, H), rim(j, SIDES, H, H));
        quad(&mut o, a, b, c, d);
        let (ia, ib) = (rim(j, SIDES, inner, -H), rim(j + 1, SIDES, inner, -H));
        let (ic, id) = (rim(j + 1, SIDES, inner, H), rim(j, SIDES, inner, H));
        // the inner wall faces the axis
        quad(&mut o, ib, ia, id, ic);
        // the top and bottom rings
        quad(&mut o, d, c, ic, id);
        quad(&mut o, b, a, ia, ib);
    }
    o
}

/// A square pyramid: the base the bottom face, the apex at the top.
pub fn pyramid() -> Vec<i32> {
    let mut o = Vec::new();
    let apex = v3(0, H, 0);
    let c = [v3(-H, -H, H), v3(H, -H, H), v3(H, -H, -H), v3(-H, -H, -H)];
    for k in 0..4 {
        tri(&mut o, c[k], c[(k + 1) % 4], apex);
    }
    quad(&mut o, c[3], c[2], c[1], c[0]);
    o
}

/// T21: the rounding radius of the smooth shapes' edges, percent of the
/// unit cube's side: the box's and the rod's, and the bell's, smaller, as
/// a bell is drawn large (a skirt, a habit) and its hem should stay a hem.
pub const ROUND_PCT: i32 = 20;
pub const BELL_ROUND_PCT: i32 = 6;
/// The smooth columns' sides, and the steps round each rounded edge.
pub const SMOOTH_SIDES: i32 = 32;
const ARC_STEPS: i32 = 4;

/// The values across one axis of a rounded face: the rounding band at each
/// end in `ARC_STEPS`, the flat between.
fn band_steps(r: i32) -> Vec<i32> {
    let mut v: Vec<i32> = (0..=ARC_STEPS).map(|k| -H + r * k / ARC_STEPS).collect();
    v.extend((0..=ARC_STEPS).map(|k| H - r + r * k / ARC_STEPS));
    v
}

/// A box with rounded edges: each point of a face grid is pulled in to the
/// inner core (the box less `r` each way) and pushed out again by `r`
/// along the way it was pulled, which is its normal.
pub fn round() -> Vec<i32> {
    let r = ONE * ROUND_PCT / 100;
    let core = H - r;
    let steps = band_steps(r);
    let mut o = Vec::new();
    let at = |c: V3| -> (V3, V3) {
        let inner = v3(
            c.x.clamp(-core, core),
            c.y.clamp(-core, core),
            c.z.clamp(-core, core),
        );
        let n = c.sub(inner).norm();
        (inner.add(n.scale(r)), n)
    };
    // each face: its normal axis, and the two axes across it, ordered so
    // (u × v) points out
    let faces: [(V3, V3, V3); 6] = [
        (v3(H, 0, 0), v3(0, 0, -ONE), v3(0, ONE, 0)),
        (v3(-H, 0, 0), v3(0, 0, ONE), v3(0, ONE, 0)),
        (v3(0, H, 0), v3(ONE, 0, 0), v3(0, 0, -ONE)),
        (v3(0, -H, 0), v3(ONE, 0, 0), v3(0, 0, ONE)),
        (v3(0, 0, H), v3(ONE, 0, 0), v3(0, ONE, 0)),
        (v3(0, 0, -H), v3(-ONE, 0, 0), v3(0, ONE, 0)),
    ];
    for (mid, u, v) in faces {
        let pt = |a: i32, b: i32| at(mid.add(u.scale(a)).add(v.scale(b)));
        for i in 0..steps.len() - 1 {
            for j in 0..steps.len() - 1 {
                let (a0, a1, b0, b1) = (steps[i], steps[i + 1], steps[j], steps[j + 1]);
                let (p, q, s, t) = (pt(a0, b0), pt(a1, b0), pt(a1, b1), pt(a0, b1));
                push_smooth(&mut o, [p, q, s]);
                push_smooth(&mut o, [p, s, t]);
            }
        }
    }
    o
}

/// A surface turned about y from a profile of (radius, height, normal
/// across, normal up), bottom to top, `SMOOTH_SIDES` round.
fn revolve(profile: &[(i32, i32, i32, i32)]) -> Vec<i32> {
    let mut o = Vec::new();
    let at = |k: usize, j: i32| -> (V3, V3) {
        let (r, y, nr, ny) = profile[k];
        let b = fx::TURN * j / SMOOTH_SIDES;
        let (c, s) = (fx::cos(b), -fx::sin(b));
        (
            v3(fx::mul(r, c), y, fx::mul(r, s)),
            v3(fx::mul(nr, c), ny, fx::mul(nr, s)),
        )
    };
    for k in 0..profile.len() - 1 {
        for j in 0..SMOOTH_SIDES {
            let (a, b, c, d) = (at(k, j), at(k, j + 1), at(k + 1, j + 1), at(k + 1, j));
            if profile[k].0 > 0 {
                push_smooth(&mut o, [a, b, c]);
            }
            if profile[k + 1].0 > 0 {
                push_smooth(&mut o, [a, c, d]);
            }
        }
    }
    o
}

/// The profile of a column of radius `H` at its foot and `top_pct` of that
/// at its head, its rims rounded by `round_pct` of the cube's side.
fn column(top_pct: i32, round_pct: i32) -> Vec<i32> {
    let r = ONE * round_pct / 100;
    let (foot, head) = (H, H * top_pct / 100);
    // the side's outward normal, tilted up as the column narrows
    let side = fx::wrap(fx::atan2(foot - head, ONE));
    let mut p = vec![(0, -H, 0, -ONE)];
    let arc = |p: &mut Vec<(i32, i32, i32, i32)>, centre: (i32, i32), from: i32, to: i32| {
        for k in 0..=ARC_STEPS {
            let a = from + (to - from) * k / ARC_STEPS;
            let (nr, ny) = (fx::cos(a), fx::sin(a));
            p.push((centre.0 + fx::mul(nr, r), centre.1 + fx::mul(ny, r), nr, ny));
        }
    };
    arc(&mut p, (foot - r, -H + r), -fx::QUARTER_TURN, side);
    arc(&mut p, (head - r, H - r), side, fx::QUARTER_TURN);
    p.push((0, H, 0, ONE));
    revolve(&p)
}

/// A cylinder with rounded rims.
pub fn rod() -> Vec<i32> {
    column(100, ROUND_PCT)
}

/// A column narrowing to `TAPER_PCT`, with rounded rims.
pub fn bell() -> Vec<i32> {
    column(TAPER_PCT, BELL_ROUND_PCT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prim::VERTEX_VALUES;

    fn tris(vs: &[i32]) -> Vec<[V3; 4]> {
        vs.chunks(3 * VERTEX_VALUES)
            .map(|t| {
                let v = |i: usize| v3(t[i], t[i + 1], t[i + 2]);
                let n = v(VERTEX_VALUES).sub(v(0)).cross(v(2 * VERTEX_VALUES).sub(v(0)));
                [v(0), v(VERTEX_VALUES), v(2 * VERTEX_VALUES), n]
            })
            .collect()
    }

    fn centroid(t: &[V3; 4]) -> V3 {
        v3(
            (t[0].x + t[1].x + t[2].x) / 3,
            (t[0].y + t[1].y + t[2].y) / 3,
            (t[0].z + t[1].z + t[2].z) / 3,
        )
    }

    /// Every closed new shape winds its faces away from a point inside it,
    /// as `prim`'s test asks of the primitives; the tube's inner wall faces
    /// its axis, away from the ring's own body.
    #[test]
    fn the_smooth_shapes_wind_outward_agree_with_their_normals_and_fill_the_cube() {
        for (name, vs) in [("round", round()), ("rod", rod()), ("bell", bell())] {
            let (mut lo, mut hi) = (V3::ZERO, V3::ZERO);
            for t in vs.chunks(3 * VERTEX_VALUES) {
                let v = |i: usize| v3(t[i], t[i + 1], t[i + 2]);
                let (a, b, c) = (v(0), v(VERTEX_VALUES), v(2 * VERTEX_VALUES));
                for q in [a, b, c] {
                    lo = v3(lo.x.min(q.x), lo.y.min(q.y), lo.z.min(q.z));
                    hi = v3(hi.x.max(q.x), hi.y.max(q.y), hi.z.max(q.z));
                }
                let n = b.sub(a).cross(c.sub(a));
                if n.len() == 0 {
                    continue;
                }
                // the stored normal at each corner leans the way the face does
                for k in 0..3 {
                    let o = k * VERTEX_VALUES + 3;
                    let stored = v3(t[o], t[o + 1], t[o + 2]);
                    assert!(
                        stored.dot(n.norm()) > 0,
                        "{name}: a face at {a:?} turns from its normal"
                    );
                }
            }
            // inside the unit cube, and reaching its faces within a rounding
            for (l, h) in [(lo.x, hi.x), (lo.y, hi.y), (lo.z, hi.z)] {
                assert!(l >= -H - 2 && h <= H + 2, "{name}: {l} {h}");
                assert!(h > H - ONE / 50 && l < -H + ONE / 50, "{name}: {l} {h}");
            }
        }
    }

    #[test]
    fn every_new_shape_winds_its_faces_outward() {
        for (name, vs, centre) in [
            ("facet", facet(), V3::ZERO),
            ("dot", dot(), V3::ZERO),
            // a point inside the prism and the half cylinder, low down
            ("wedge", wedge(), v3(0, -H / 2, 0)),
            ("half_cyl", half_cyl(), v3(0, -H / 2, 0)),
            ("taper", taper(), V3::ZERO),
            ("pyramid", pyramid(), v3(0, -H / 2, 0)),
        ] {
            for t in tris(&vs) {
                let c = centroid(&t);
                assert!(c.sub(centre).dot(t[3]) > 0, "{name}: face at {c:?} winds inward");
            }
        }
        // the tube: each face looks away from the middle of the wall it
        // belongs to, the point on the ring halfway between hole and rim
        let mid_r = H * (100 + TUBE_HOLE_PCT) / 200;
        for t in tris(&tube()) {
            let c = centroid(&t);
            let flat = v3(c.x, 0, c.z).norm();
            let wall = v3(fx::mul(flat.x, mid_r), 0, fx::mul(flat.z, mid_r));
            assert!(c.sub(wall).dot(t[3]) > 0, "tube: face at {c:?} winds inward");
        }
    }

    #[test]
    fn the_new_shapes_have_the_counts_and_sizes_they_say() {
        // 20 faces cut in four; 6 bands of 10 less the two pole rows' halves
        assert_eq!(tris(&facet()).len(), 80);
        assert_eq!(tris(&dot()).len(), 2 * 10 * 5);
        // two gables, two slopes and a floor of two each
        assert_eq!(tris(&wedge()).len(), 8);
        assert_eq!(tris(&pyramid()).len(), 6);
        // eight sides: a quad and a triangle at each end per side
        assert_eq!(tris(&taper()).len(), 32);
        assert_eq!(tris(&half_cyl()).len(), 8 * 4 + 2);
        assert_eq!(tris(&tube()).len(), 16 * 8);
        // every vertex of the facet ball lies on the sphere of radius ½
        for p in facet().chunks(VERTEX_VALUES) {
            let r = v3(p[0], p[1], p[2]).len();
            assert!((r - H).abs() <= 8, "{r}");
        }
        // the taper's top ring is 60% of its base: 0.3 of a unit from the axis
        let top = taper()
            .chunks(VERTEX_VALUES)
            .filter(|p| p[1] == H)
            .map(|p| v3(p[0], 0, p[2]).len())
            .max()
            .unwrap();
        assert!((top - ONE * 3 / 10).abs() <= 4, "{top}");
        // the tube's hole: 72% of ½, 0.36 of a unit
        let hole = tube()
            .chunks(VERTEX_VALUES)
            .map(|p| v3(p[0], 0, p[2]).len())
            .min()
            .unwrap();
        assert!((hole - ONE * 36 / 100).abs() <= 4, "{hole}");
    }
}
