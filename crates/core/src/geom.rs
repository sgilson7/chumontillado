//! Vectors, rotations, affine transforms and the projection, all in Q16.16
//! metres (CW-2). Ported from Long Time Comin's `geom.rs`, whose projection
//! is Iron Vector's (i64 so a far plane kilometres away does not overflow).
//!
//! Conventions:
//!
//! - y is up and the world is right-handed: x runs east, z runs south. The
//!   walkable square's map has north up with its origin at the south-west
//!   corner; a map point (x, y) is the world point (x, 0, 4 km − y), as
//!   PLAN.md reads the journal's map.
//! - A heading (`yaw`) of zero faces −z, north, as an OpenGL camera does.
//!   Positive yaw turns left (counter-clockwise seen from above); positive
//!   pitch looks up. Angles are `fx::TURN` units to a circle.

use crate::fx::{self, ONE};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct V3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub const fn v3(x: i32, y: i32, z: i32) -> V3 {
    V3 { x, y, z }
}

// Named methods rather than operator traits: every call site reads as the
// fixed-point operation it is, and none needs a trait import.
#[allow(clippy::should_implement_trait, clippy::len_without_is_empty)]
impl V3 {
    pub const ZERO: V3 = v3(0, 0, 0);
    pub const UP: V3 = v3(0, ONE, 0);

    pub fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn scale(self, s: i32) -> V3 {
        v3(fx::mul(self.x, s), fx::mul(self.y, s), fx::mul(self.z, s))
    }
    pub fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
    pub fn dot(self, o: V3) -> i32 {
        ((self.x as i64 * o.x as i64 + self.y as i64 * o.y as i64 + self.z as i64 * o.z as i64)
            >> fx::FRAC_BITS) as i32
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            fx::mul(self.y, o.z) - fx::mul(self.z, o.y),
            fx::mul(self.z, o.x) - fx::mul(self.x, o.z),
            fx::mul(self.x, o.y) - fx::mul(self.y, o.x),
        )
    }
    /// The length, in u64 so any three Q16 components square and add
    /// without overflow (3 × 2⁶² is under 2⁶⁴).
    pub fn len(self) -> i32 {
        let (x, y, z) = (
            self.x.unsigned_abs() as u64,
            self.y.unsigned_abs() as u64,
            self.z.unsigned_abs() as u64,
        );
        fx::isqrt_u64(x * x + y * y + z * z).min(i32::MAX as u64) as i32
    }
    pub fn len_xz(self) -> i32 {
        v3(self.x, 0, self.z).len()
    }
    /// The unit vector, or zero for a zero vector.
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l == 0 {
            return V3::ZERO;
        }
        v3(fx::div(self.x, l), fx::div(self.y, l), fx::div(self.z, l))
    }
    /// The vector shortened to at most `max` long.
    pub fn clamp_len(self, max: i32) -> V3 {
        let l = self.len();
        if l <= max || l == 0 {
            return self;
        }
        self.scale(fx::div(max, l))
    }
    pub fn lerp(self, o: V3, t: i32) -> V3 {
        v3(
            fx::lerp(self.x, o.x, t),
            fx::lerp(self.y, o.y, t),
            fx::lerp(self.z, o.z, t),
        )
    }
    pub fn dist(self, o: V3) -> i32 {
        self.sub(o).len()
    }
}

/// The unit vector a heading and pitch face.
pub fn facing(yaw: i32, pitch: i32) -> V3 {
    let cp = fx::cos(pitch);
    v3(
        -fx::mul(fx::sin(yaw), cp),
        fx::sin(pitch),
        -fx::mul(fx::cos(yaw), cp),
    )
}

/// The flat unit vector to the right of a heading.
pub fn right_of(yaw: i32) -> V3 {
    v3(fx::cos(yaw), 0, -fx::sin(yaw))
}

/// The heading of a flat direction (y ignored); 0 for no direction.
pub fn yaw_of(d: V3) -> i32 {
    fx::atan2(-d.x, -d.z)
}

/// A 3×3 linear part (columns) and a translation: what one drawn shape needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Affine {
    pub c0: V3,
    pub c1: V3,
    pub c2: V3,
    pub t: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        c0: v3(ONE, 0, 0),
        c1: v3(0, ONE, 0),
        c2: v3(0, 0, ONE),
        t: V3::ZERO,
    };

    pub fn translate(t: V3) -> Affine {
        Affine {
            t,
            ..Affine::IDENTITY
        }
    }
    pub fn scale(s: V3) -> Affine {
        Affine {
            c0: v3(s.x, 0, 0),
            c1: v3(0, s.y, 0),
            c2: v3(0, 0, s.z),
            t: V3::ZERO,
        }
    }
    /// A turn about the vertical: what faced heading 0 faces heading `a`.
    pub fn rot_y(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(c, 0, -s),
            c1: v3(0, ONE, 0),
            c2: v3(s, 0, c),
            t: V3::ZERO,
        }
    }
    pub fn rot_x(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(ONE, 0, 0),
            c1: v3(0, c, s),
            c2: v3(0, -s, c),
            t: V3::ZERO,
        }
    }
    pub fn rot_z(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(c, s, 0),
            c1: v3(-s, c, 0),
            c2: v3(0, 0, ONE),
            t: V3::ZERO,
        }
    }
    /// A basis whose −z axis points along `dir` (a unit vector), for things
    /// that face where they fly.
    pub fn looking(dir: V3) -> Affine {
        let back = dir.neg();
        let helper = if back.y.abs() > ONE - ONE / 16 {
            v3(ONE, 0, 0)
        } else {
            V3::UP
        };
        let x = helper.cross(back).norm();
        let y = back.cross(x);
        Affine {
            c0: x,
            c1: y,
            c2: back,
            t: V3::ZERO,
        }
    }
    pub fn apply(&self, p: V3) -> V3 {
        self.apply_dir(p).add(self.t)
    }
    pub fn apply_dir(&self, d: V3) -> V3 {
        self.c0.scale(d.x).add(self.c1.scale(d.y)).add(self.c2.scale(d.z))
    }
    /// `self * o`: apply `o` first, then `self`.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            c0: self.apply_dir(o.c0),
            c1: self.apply_dir(o.c1),
            c2: self.apply_dir(o.c2),
            t: self.apply(o.t),
        }
    }
    /// The same shape moved by `d`.
    pub fn moved(&self, d: V3) -> Affine {
        Affine {
            t: self.t.add(d),
            ..*self
        }
    }
    /// Half the sum of the columns' lengths: a radius that holds the unit
    /// shape (every mesh fits in the unit cube about its origin).
    pub fn reach(&self) -> i32 {
        (self.c0.len() / 2)
            .saturating_add(self.c1.len() / 2)
            .saturating_add(self.c2.len() / 2)
    }
}

/// A 4×4 matrix, column-major, as WebGL reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mat4(pub [i32; 16]);

impl Mat4 {
    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut m = [0i32; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut s: i64 = 0;
                for k in 0..4 {
                    s += self.0[k * 4 + row] as i64 * o.0[col * 4 + k] as i64;
                }
                m[col * 4 + row] = (s >> fx::FRAC_BITS) as i32;
            }
        }
        Mat4(m)
    }

    /// Perspective projection. `fov` is the vertical field of view in angle
    /// units; `aspect` is width over height in Q16; `near` and `far` are in
    /// Q16 metres.
    pub fn perspective(fov: i32, aspect: i32, near: i32, far: i32) -> Mat4 {
        let f = fx::div(fx::cos(fov / 2), fx::sin(fov / 2));
        let mut m = [0i32; 16];
        m[0] = fx::div(f, aspect);
        m[5] = f;
        // in i64: at kilometre ranges far + near and 2·far·near overflow an
        // i32 (Iron Vector, geom.rs)
        let (n, fr) = (near as i64, far as i64);
        m[10] = (((fr + n) << fx::FRAC_BITS) / (n - fr)) as i32;
        m[11] = -ONE;
        m[14] = (((2 * fr * n) >> fx::FRAC_BITS << fx::FRAC_BITS) / (n - fr)) as i32;
        Mat4(m)
    }

    /// The view matrix of a camera at `eye` looking along `fwd` with `up`
    /// roughly up; both unit vectors.
    pub fn look(eye: V3, fwd: V3, up: V3) -> Mat4 {
        let s = fwd.cross(up).norm();
        let u = s.cross(fwd);
        Mat4([
            s.x,
            u.x,
            -fwd.x,
            0,
            s.y,
            u.y,
            -fwd.y,
            0,
            s.z,
            u.z,
            -fwd.z,
            0,
            -s.dot(eye),
            -u.dot(eye),
            fwd.dot(eye),
            ONE,
        ])
    }

    /// Clip-space coordinates `(x, y, z, w)` of a world point.
    pub fn project(&self, p: V3) -> [i32; 4] {
        let mut out = [0i32; 4];
        for (row, o) in out.iter_mut().enumerate() {
            let s = self.0[row] as i64 * p.x as i64
                + self.0[4 + row] as i64 * p.y as i64
                + self.0[8 + row] as i64 * p.z as i64
                + ((self.0[12 + row] as i64) << fx::FRAC_BITS);
            *o = (s >> fx::FRAC_BITS) as i32;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{deg, int, QUARTER_TURN};

    fn near(a: V3, b: V3, tol: i32) -> bool {
        (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
    }

    #[test]
    fn heading_zero_faces_north_and_right_is_east() {
        assert!(near(facing(0, 0), v3(0, 0, -ONE), 2));
        assert!(near(right_of(0), v3(ONE, 0, 0), 2));
        // a quarter turn left faces west (−x)
        assert!(near(facing(QUARTER_TURN, 0), v3(-ONE, 0, 0), 2));
        // right is forward × up
        let f = facing(deg(37), 0);
        assert!(near(f.cross(V3::UP), right_of(deg(37)), 4));
    }

    #[test]
    fn rotation_about_y_turns_forward_the_way_facing_does() {
        let r = Affine::rot_y(deg(70));
        assert!(near(r.apply_dir(v3(0, 0, -ONE)), facing(deg(70), 0), 3));
        let p = Affine::rot_x(deg(20));
        assert!(near(p.apply_dir(v3(0, 0, -ONE)), facing(0, deg(20)), 3));
    }

    #[test]
    fn yaw_of_inverts_facing() {
        for y in [deg(10), deg(100), deg(200), deg(300)] {
            let back = yaw_of(facing(y, 0));
            assert!(fx::wrap(back - y).abs() <= deg(1) / 2, "{y} {back}");
        }
    }

    #[test]
    fn a_point_ahead_of_the_camera_lands_in_the_clip_volume_and_one_behind_does_not() {
        let eye = v3(int(10), int(5), int(20));
        let fwd = facing(deg(30), 0);
        let vp = Mat4::perspective(deg(60), fx::ratio(16, 9), ONE * 2 / 5, int(128)).mul(&Mat4::look(
            eye,
            fwd,
            V3::UP,
        ));
        let ahead = eye.add(fwd.scale(int(50)));
        let [x, y, z, w] = vp.project(ahead);
        assert!(
            w > 0 && x.abs() < w / 64 && y.abs() < w / 64 && z.abs() < w,
            "{x} {y} {z} {w}"
        );
        let behind = eye.sub(fwd.scale(int(50)));
        assert!(vp.project(behind)[3] < 0);
    }

    #[test]
    fn a_far_plane_sixteen_kilometres_away_does_not_overflow() {
        let p = Mat4::perspective(deg(60), ONE, ONE * 2 / 5, int(16_000));
        // (far + near) / (near − far) = −16 000.4 / 15 999.6 = −1.000 05, and
        // 2·far·near / (near − far) = −12 800 / 15 999.6 = −0.800 02
        assert!((p.0[10] + ONE).abs() < 16, "{}", p.0[10]);
        assert!((p.0[14] + ONE * 4 / 5).abs() < 16, "{}", p.0[14]);
    }

    #[test]
    fn looking_points_minus_z_along_the_direction() {
        let d = v3(ONE / 2, ONE / 2, -46_341).norm();
        let b = Affine::looking(d);
        assert!(near(b.apply_dir(v3(0, 0, -ONE)), d, 8));
        let up = Affine::looking(V3::UP);
        assert!(near(up.apply_dir(v3(0, 0, -ONE)), V3::UP, 8));
    }

    #[test]
    fn reach_holds_a_box_from_its_centre_to_its_corner() {
        // a 2 × 4 × 6 m box: half of 2 + 4 + 6 = 6 m, above the corner's
        // √(1 + 4 + 9) = 3.74 m
        let a = Affine::scale(v3(int(2), int(4), int(6)));
        assert_eq!(a.reach(), int(6));
    }
}
