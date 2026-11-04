//! The mesh table: the primitive shapes everything is drawn from, and the
//! meshes the core builds for the world (`MeshTable`). Each is a triangle
//! list of (position, normal, colour) in Q16. A primitive is centred on the
//! origin and inside the unit cube, so an instance's affine carries its size,
//! and its colour is white, so the instance's colour is the shape's.
//! Ported from Long Time Comin's `mesh.rs`, with a capsule, a ground
//! triangle and a flat square added, and the round shapes split into smooth
//! (`Sphere`, `Capsule`) and faceted (`Blob`, `Cone`, `Cyl`), the faceted
//! look of Protection of the Spheres' rocks and trees.
//!
//! The page uploads every mesh once (the shim's `meshes` and `mesh`), the
//! primitives in the order of `Mesh::ALL` and then the world's, and an
//! instance names its mesh by number (`MeshId`).
//!
//! A world mesh is for many triangles drawn as one: the test field's ground
//! rings now, M2's terrain next. A software renderer (SwiftShader) drew 3
//! frames a second when the ground was 17 500 instances of one triangle and
//! 60 when they were four meshes (SECOND-ORDER.md R2.16). Its colours are
//! baked per vertex, multiplied by the instance's.
//!
//! Extension (M3, `mesh.rs` and `models.rs`): a new shape is a new variant
//! here, appended to `Mesh::ALL` (ids are positions, so append, never
//! insert), with an arm in `vertices` that builds it, usually by calling a
//! builder in `crate::mesh`. Models are arrangements of these shapes, pushed
//! as instances; they are not meshes.

use crate::fx::{self, ONE};
use crate::geom::{v3, V3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mesh {
    Cube = 0,
    /// half of a flat square, facing up: the ground's facets
    Tri = 1,
    /// a flat square facing up: shadows, decals
    Square = 2,
    /// a smooth globe: heads, the sky dome
    Sphere = 3,
    /// a faceted low globe: rocks, hills, crowns
    Blob = 4,
    /// a faceted upright cylinder: posts, trunks
    Cyl = 5,
    /// a faceted upright cone: pines, hats
    Cone = 6,
    /// a smooth capsule standing on end: bodies, limbs
    Capsule = 7,
    Octa = 8,
    // M3 (`mesh.rs`): the faceted shapes the characters and props are built from
    /// a ball of 80 flat faces: heads, bodies, joints (Protection of the Spheres)
    Facet = 9,
    /// a low smooth globe: eyes, catch-lights, blush, buttons
    Dot = 10,
    /// a triangular prism, ridge along z: roofs, tents
    Wedge = 11,
    /// a half cylinder along z, flat side down: lids, canopies
    HalfCyl = 12,
    /// an eight-sided column narrowing upward: skirts, coats, trunks
    Taper = 13,
    /// a ring of sixteen sides: a well's wall, a wheel's rim
    Tube = 14,
    /// four sides to an apex: roofs, caps
    Pyramid = 15,
}

impl Mesh {
    pub const ALL: [Mesh; 16] = [
        Mesh::Cube,
        Mesh::Tri,
        Mesh::Square,
        Mesh::Sphere,
        Mesh::Blob,
        Mesh::Cyl,
        Mesh::Cone,
        Mesh::Capsule,
        Mesh::Octa,
        Mesh::Facet,
        Mesh::Dot,
        Mesh::Wedge,
        Mesh::HalfCyl,
        Mesh::Taper,
        Mesh::Tube,
        Mesh::Pyramid,
    ];

    /// Shapes whose faces are flat: one of them can lie in another's plane,
    /// so the flicker scan (`flicker.rs`) reads their faces.
    pub fn flat_faced(self) -> bool {
        matches!(
            self,
            Mesh::Cube
                | Mesh::Tri
                | Mesh::Square
                | Mesh::Cyl
                | Mesh::Octa
                | Mesh::Facet
                | Mesh::Wedge
                | Mesh::HalfCyl
                | Mesh::Taper
                | Mesh::Tube
                | Mesh::Pyramid
        )
    }
}

/// Values per vertex: position xyz, normal xyz, colour rgb.
pub const VERTEX_VALUES: usize = 9;

const WHITE: [i32; 3] = [ONE; 3];

/// A mesh by number: a primitive's place in `Mesh::ALL`, or a world mesh's
/// after them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId(pub u32);

impl From<Mesh> for MeshId {
    fn from(m: Mesh) -> MeshId {
        MeshId(m as u32)
    }
}

impl MeshId {
    /// The primitive this names, if it names one.
    pub fn prim(self) -> Option<Mesh> {
        Mesh::ALL.get(self.0 as usize).copied()
    }
}

/// Every mesh the page uploads: the primitives, then the meshes the world
/// built (`World::scene` adds them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MeshTable {
    pub world: Vec<Vec<i32>>,
}

impl MeshTable {
    /// Adds a world mesh (a triangle list of `VERTEX_VALUES` a vertex) and
    /// returns its number.
    pub fn add(&mut self, vertices: Vec<i32>) -> MeshId {
        self.world.push(vertices);
        MeshId((Mesh::ALL.len() + self.world.len() - 1) as u32)
    }

    pub fn count(&self) -> usize {
        Mesh::ALL.len() + self.world.len()
    }

    /// A mesh's vertex values; none for a number past the end.
    pub fn vertices(&self, id: MeshId) -> Vec<i32> {
        match id.prim() {
            Some(m) => vertices(m),
            None => self
                .world
                .get(id.0 as usize - Mesh::ALL.len())
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Whether a mesh's faces are flat, so one can lie in another's plane:
    /// the flat primitives, and every world mesh (ground and terrain are
    /// flat triangles).
    pub fn flat_faced(&self, id: MeshId) -> bool {
        id.prim().map(Mesh::flat_faced).unwrap_or(true)
    }
}

/// A flat triangle of a world mesh with its own colour, counter-clockwise
/// seen from the side it faces.
pub fn push_coloured(out: &mut Vec<i32>, a: V3, b: V3, c: V3, rgb: [i32; 3]) {
    // the edges made unit first: a world triangle's edges are long enough
    // that their Q16 cross product would overflow
    let n = b.sub(a).norm().cross(c.sub(a).norm()).norm();
    for p in [a, b, c] {
        out.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z, rgb[0], rgb[1], rgb[2]]);
    }
}

/// The capsule's radius as a share of its height.
pub const CAPSULE_RADIUS: i32 = ONE * 18 / 100;

const H: i32 = ONE / 2;

fn push_flat(out: &mut Vec<i32>, a: V3, b: V3, c: V3) {
    push_coloured(out, a, b, c, WHITE);
}

fn push_quad(out: &mut Vec<i32>, a: V3, b: V3, c: V3, d: V3) {
    push_flat(out, a, b, c);
    push_flat(out, a, c, d);
}

/// A triangle whose normals are given per vertex (smooth shading).
fn push_smooth(out: &mut Vec<i32>, p: [(V3, V3); 3]) {
    for (q, n) in p {
        out.extend_from_slice(&[q.x, q.y, q.z, n.x, n.y, n.z, WHITE[0], WHITE[1], WHITE[2]]);
    }
}

/// A point on a globe of radius `H`: `i` of `lat` bands from the south pole,
/// `j` of `lon` round.
fn globe(i: i32, j: i32, lat: i32, lon: i32) -> V3 {
    let a = fx::HALF_TURN * i / lat - fx::QUARTER_TURN;
    let b = fx::TURN * j / lon;
    let r = fx::cos(a);
    v3(
        fx::mul(fx::mul(r, fx::cos(b)), H),
        fx::mul(fx::sin(a), H),
        fx::mul(fx::mul(r, -fx::sin(b)), H),
    )
}

/// Vertex data of a mesh, counter-clockwise from outside.
pub fn vertices(mesh: Mesh) -> Vec<i32> {
    let mut o = Vec::new();
    match mesh {
        Mesh::Cube => {
            let p = |x: i32, y: i32, z: i32| v3(x * H, y * H, z * H);
            push_quad(&mut o, p(1, -1, 1), p(1, -1, -1), p(1, 1, -1), p(1, 1, 1));
            push_quad(&mut o, p(-1, -1, -1), p(-1, -1, 1), p(-1, 1, 1), p(-1, 1, -1));
            push_quad(&mut o, p(-1, 1, 1), p(1, 1, 1), p(1, 1, -1), p(-1, 1, -1));
            push_quad(&mut o, p(-1, -1, -1), p(1, -1, -1), p(1, -1, 1), p(-1, -1, 1));
            push_quad(&mut o, p(-1, -1, 1), p(1, -1, 1), p(1, 1, 1), p(-1, 1, 1));
            push_quad(&mut o, p(1, -1, -1), p(-1, -1, -1), p(-1, 1, -1), p(1, 1, -1));
        }
        Mesh::Tri => {
            // the south-west, south-east and north-east corners; the other
            // half is this one turned half a turn
            push_flat(&mut o, v3(-H, 0, H), v3(H, 0, H), v3(H, 0, -H));
        }
        Mesh::Square => {
            push_quad(&mut o, v3(-H, 0, H), v3(H, 0, H), v3(H, 0, -H), v3(-H, 0, -H));
        }
        Mesh::Sphere => {
            let (lat, lon) = (16, 36);
            for i in 0..lat {
                for j in 0..lon {
                    let (a, b, c, d) = (
                        globe(i, j, lat, lon),
                        globe(i, j + 1, lat, lon),
                        globe(i + 1, j + 1, lat, lon),
                        globe(i + 1, j, lat, lon),
                    );
                    if i > 0 {
                        push_smooth(&mut o, [(a, a.norm()), (b, b.norm()), (c, c.norm())]);
                    }
                    if i < lat - 1 {
                        push_smooth(&mut o, [(a, a.norm()), (c, c.norm()), (d, d.norm())]);
                    }
                }
            }
        }
        Mesh::Blob => {
            let (lat, lon) = (5, 8);
            for i in 0..lat {
                for j in 0..lon {
                    let (a, b, c, d) = (
                        globe(i, j, lat, lon),
                        globe(i, j + 1, lat, lon),
                        globe(i + 1, j + 1, lat, lon),
                        globe(i + 1, j, lat, lon),
                    );
                    if i > 0 {
                        push_flat(&mut o, a, b, c);
                    }
                    if i < lat - 1 {
                        push_flat(&mut o, a, c, d);
                    }
                }
            }
        }
        Mesh::Cyl | Mesh::Cone => {
            let cone = mesh == Mesh::Cone;
            let sides = if cone { 8 } else { 10 };
            let rim = |j: i32, y: i32| {
                let a = fx::TURN * j / sides;
                v3(fx::mul(fx::cos(a), H), y, fx::mul(-fx::sin(a), H))
            };
            let apex = v3(0, H, 0);
            for j in 0..sides {
                let (a, b) = (rim(j, -H), rim(j + 1, -H));
                if cone {
                    push_flat(&mut o, a, b, apex);
                } else {
                    let (c, d) = (rim(j + 1, H), rim(j, H));
                    push_quad(&mut o, a, b, c, d);
                    push_flat(&mut o, d, c, apex);
                }
                push_flat(&mut o, b, a, v3(0, -H, 0));
            }
        }
        Mesh::Capsule => {
            // two half globes of radius r joined by a tube; normals point
            // out from the nearest point on the spine. 48 sides, so the
            // outline seen from 2.6 m stays round (with 20 its radius changed
            // by 1.2% as the camera went round, about a pixel)
            let r = CAPSULE_RADIUS;
            let spine = H - r;
            let (bands, lon) = (8, 48);
            // ring k of 2·bands + 1 from the bottom pole: its height and
            // radius, and the centre its normal points from
            let ring = |k: i32, j: i32| -> (V3, V3) {
                let (lat, centre) = if k <= bands {
                    (fx::QUARTER_TURN * k / bands - fx::QUARTER_TURN, -spine)
                } else {
                    (fx::QUARTER_TURN * (k - bands - 1) / bands, spine)
                };
                let lat = if k == bands + 1 { 0 } else { lat };
                let b = fx::TURN * j / lon;
                let n = v3(
                    fx::mul(fx::cos(lat), fx::cos(b)),
                    fx::sin(lat),
                    fx::mul(fx::cos(lat), -fx::sin(b)),
                );
                (v3(fx::mul(n.x, r), centre + fx::mul(n.y, r), fx::mul(n.z, r)), n)
            };
            let rings = 2 * bands + 2;
            for k in 0..rings - 1 {
                for j in 0..lon {
                    let (a, b, c, d) = (ring(k, j), ring(k, j + 1), ring(k + 1, j + 1), ring(k + 1, j));
                    if k > 0 {
                        push_smooth(&mut o, [a, b, c]);
                    }
                    if k < rings - 2 {
                        push_smooth(&mut o, [a, c, d]);
                    }
                }
            }
        }
        Mesh::Octa => {
            let (px, nx, py, ny, pz, nz) = (
                v3(H, 0, 0),
                v3(-H, 0, 0),
                v3(0, H, 0),
                v3(0, -H, 0),
                v3(0, 0, H),
                v3(0, 0, -H),
            );
            for (a, b) in [(px, pz), (pz, nx), (nx, nz), (nz, px)] {
                push_flat(&mut o, a, py, b);
                push_flat(&mut o, b, ny, a);
            }
        }
        Mesh::Facet => o = crate::mesh::facet(),
        Mesh::Dot => o = crate::mesh::dot(),
        Mesh::Wedge => o = crate::mesh::wedge(),
        Mesh::HalfCyl => o = crate::mesh::half_cyl(),
        Mesh::Taper => o = crate::mesh::taper(),
        Mesh::Tube => o = crate::mesh::tube(),
        Mesh::Pyramid => o = crate::mesh::pyramid(),
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(m: Mesh) -> Vec<[V3; 4]> {
        vertices(m)
            .chunks(3 * VERTEX_VALUES)
            .map(|t| {
                let v = |i: usize| v3(t[i], t[i + 1], t[i + 2]);
                // the winding's own normal, not the stored one
                let n = v(VERTEX_VALUES).sub(v(0)).cross(v(2 * VERTEX_VALUES).sub(v(0)));
                [v(0), v(VERTEX_VALUES), v(2 * VERTEX_VALUES), n]
            })
            .collect()
    }

    #[test]
    fn every_face_of_a_closed_mesh_winds_away_from_its_centre() {
        for (m, centre) in [
            (Mesh::Cube, V3::ZERO),
            (Mesh::Octa, V3::ZERO),
            (Mesh::Sphere, V3::ZERO),
            (Mesh::Blob, V3::ZERO),
            (Mesh::Cyl, V3::ZERO),
            (Mesh::Cone, v3(0, -H / 2, 0)),
            (Mesh::Capsule, V3::ZERO),
        ] {
            for [a, b, c, n] in tris(m) {
                let centroid = v3(
                    (a.x + b.x + c.x) / 3,
                    (a.y + b.y + c.y) / 3,
                    (a.z + b.z + c.z) / 3,
                );
                assert!(
                    centroid.sub(centre).dot(n) > 0,
                    "{m:?}: face at {centroid:?} winds inward"
                );
            }
        }
    }

    #[test]
    fn a_world_mesh_is_numbered_after_the_primitives_and_read_back() {
        let mut t = MeshTable::default();
        let mut tri = Vec::new();
        push_coloured(
            &mut tri,
            v3(0, 0, ONE),
            v3(ONE, 0, ONE),
            v3(ONE, 0, 0),
            [ONE, 0, 0],
        );
        let id = t.add(tri.clone());
        assert_eq!(
            id,
            MeshId(Mesh::ALL.len() as u32),
            "the first world mesh follows the primitives"
        );
        assert_eq!(t.vertices(id), tri);
        assert_eq!(t.vertices(MeshId(Mesh::Cube as u32)), vertices(Mesh::Cube));
        assert!(t.vertices(MeshId(99)).is_empty());
        assert_eq!(&tri[3..6], &[0, ONE, 0], "it faces up");
        assert_eq!(&tri[6..9], &[ONE, 0, 0], "with its own colour");
        assert_eq!(t.count(), Mesh::ALL.len() + 1);
    }

    #[test]
    fn every_mesh_fits_in_the_unit_cube() {
        for m in Mesh::ALL {
            for p in vertices(m).chunks(VERTEX_VALUES) {
                assert!(p[..3].iter().all(|c| c.abs() <= H), "{m:?}: {p:?}");
            }
        }
    }

    #[test]
    fn the_ground_shapes_face_up_and_the_cube_has_twelve_triangles() {
        assert_eq!(tris(Mesh::Cube).len(), 12);
        assert_eq!(tris(Mesh::Tri).len(), 1);
        for m in [Mesh::Tri, Mesh::Square] {
            for p in vertices(m).chunks(VERTEX_VALUES) {
                assert_eq!(&p[3..6], &[0, ONE, 0], "{m:?} faces up");
                assert_eq!(&p[6..], &[ONE; 3], "{m:?} is white");
            }
        }
    }

    #[test]
    fn the_capsule_is_round_ended_and_its_normals_point_out() {
        let vs = vertices(Mesh::Capsule);
        let top = vs.chunks(VERTEX_VALUES).map(|p| p[1]).max().unwrap();
        let widest = vs
            .chunks(VERTEX_VALUES)
            .map(|p| v3(p[0], 0, p[2]).len())
            .max()
            .unwrap();
        assert!((top - H).abs() <= 4, "the top pole is at half a unit: {top}");
        assert!((widest - CAPSULE_RADIUS).abs() <= 8, "{widest}");
        for p in vs.chunks(VERTEX_VALUES) {
            let n = v3(p[3], p[4], p[5]);
            assert!((n.len() - ONE).abs() < ONE / 100, "normal not unit: {n:?}");
            let out = v3(p[0], 0, p[2]);
            assert!(out.dot(n) >= -4, "a normal points into the capsule at {p:?}");
        }
    }
}
