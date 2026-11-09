//! T37: gravity that turns. On the Cubo every body has its own frame, one of
//! the 24 right-angle turns of the world, and its own "up" is the frame's y.
//! The mover is the same Source mover as everywhere: it runs in the body's
//! own frame against the map turned into that frame (`Map::turned_back`), so
//! the floor it stands on is always its y. Anything else that needs a body's
//! up, its eye or its view takes it through the frame.
//!
//! Sam: "a map that is a cube, that when you touch a wall / start to
//! approach it, your gravity shifts to walk on that wall, with the intent
//! being you lose no momentum as you transition from wall to wall".
//! Approaching a face ahead (a concave corner) and rolling off an edge (a
//! convex one) each turn the frame a quarter turn about the corner's line,
//! and the body's speed and view turn with it, so a run along the floor
//! into a wall goes on up the wall at the same speed. Out in the open,
//! outside the cube, a body falls toward the face it is over, keeping its
//! speed and where it looks.

use crate::fx::{self, Fx, ONE};
use crate::geom::{facing, right_of, v3, yaw_of, Affine, V3};
use crate::input::Input;
use crate::map::{block, Brush, Map, Shape};
use crate::movement::{Body, Mover, MINS};

/// A right-angle turn of the world: where it carries x, y and z, each one
/// of the six axis directions. It carries a body's own frame into the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

impl Turn {
    pub const IDENTITY: Turn = Turn {
        x: v3(ONE, 0, 0),
        y: v3(0, ONE, 0),
        z: v3(0, 0, ONE),
    };

    /// The world point or direction for one in this frame.
    pub fn apply(&self, v: V3) -> V3 {
        self.x.scale(v.x).add(self.y.scale(v.y)).add(self.z.scale(v.z))
    }

    /// This frame's point or direction for one in the world.
    pub fn back(&self, v: V3) -> V3 {
        v3(self.x.dot(v), self.y.dot(v), self.z.dot(v))
    }

    /// This turn after `inner`: a frame within a frame.
    pub fn then(&self, inner: &Turn) -> Turn {
        Turn {
            x: self.apply(inner.x),
            y: self.apply(inner.y),
            z: self.apply(inner.z),
        }
    }

    /// The frame's up, in the world.
    pub fn up(&self) -> V3 {
        self.y
    }

    pub fn affine(&self) -> Affine {
        Affine {
            c0: self.x,
            c1: self.y,
            c2: self.z,
            t: V3::ZERO,
        }
    }

    /// The quarter turn about `a × b` that carries axis `a` onto axis `b`,
    /// which stands square to it.
    pub fn quarter(a: V3, b: V3) -> Turn {
        let axis = a.cross(b);
        let f = |v: V3| axis.cross(v).add(axis.scale(axis.dot(v)));
        Turn {
            x: f(v3(ONE, 0, 0)),
            y: f(v3(0, ONE, 0)),
            z: f(v3(0, 0, ONE)),
        }
    }

    /// The half turn about axis `a`.
    pub fn half(a: V3) -> Turn {
        let f = |v: V3| a.scale(2 * a.dot(v)).sub(v);
        Turn {
            x: f(v3(ONE, 0, 0)),
            y: f(v3(0, ONE, 0)),
            z: f(v3(0, 0, ONE)),
        }
    }

    /// Which of `ALL` this is.
    pub fn index(&self) -> u8 {
        all().iter().position(|t| t == self).expect("a right-angle turn") as u8
    }
}

/// The six axis directions.
pub const AXES: [V3; 6] = [
    v3(ONE, 0, 0),
    v3(-ONE, 0, 0),
    v3(0, ONE, 0),
    v3(0, -ONE, 0),
    v3(0, 0, ONE),
    v3(0, 0, -ONE),
];

/// The 24 right-angle turns, the identity first.
pub fn all() -> Vec<Turn> {
    let mut out = vec![Turn::IDENTITY];
    for x in AXES {
        for y in AXES {
            if x.dot(y) != 0 {
                continue;
            }
            let t = Turn { x, y, z: x.cross(y) };
            if t != Turn::IDENTITY {
                out.push(t);
            }
        }
    }
    out
}

/// The axis nearest a direction.
pub fn axis_of(d: V3) -> V3 {
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    if ax >= ay && ax >= az {
        v3(ONE * d.x.signum(), 0, 0)
    } else if ay >= az {
        v3(0, ONE * d.y.signum(), 0)
    } else {
        v3(0, 0, ONE * d.z.signum())
    }
}

/// The heading and pitch in a frame that face world direction `f`.
pub fn aim_toward(t: &Turn, f: V3) -> (i32, i32) {
    let l = t.back(f);
    let pitch =
        fx::wrap(fx::atan2(l.y, l.len_xz())).clamp(crate::camera::PITCH_MIN, crate::camera::PITCH_MAX);
    (yaw_of(l), pitch)
}

/// The world direction a heading and pitch face in a frame.
pub fn look(t: &Turn, yaw: i32, pitch: i32) -> V3 {
    t.apply(facing(yaw, pitch))
}

impl Map {
    /// T37: the map as a body in frame `t` meets it: every brush carried
    /// back into the frame. Only boxes are built on a map whose gravity
    /// turns, and a box turned square is a box.
    pub fn turned_back(&self, t: &Turn) -> Map {
        let brushes = self
            .brushes
            .iter()
            .map(|b| {
                let Shape::Block { mins, maxs } = b.shape else {
                    panic!("a map whose gravity turns is built of boxes")
                };
                let (a, c) = (t.back(mins), t.back(maxs));
                let lo = v3(a.x.min(c.x), a.y.min(c.y), a.z.min(c.z));
                let hi = v3(a.x.max(c.x), a.y.max(c.y), a.z.max(c.z));
                Brush {
                    surface: b.surface,
                    ..block(lo, hi, b.surface)
                }
            })
            .collect();
        Map {
            brushes,
            name: self.name,
            cube: self.cube,
            ..Map::default()
        }
    }
}

/// T37: a cube's middle and its half width inside (m), for the open-air
/// rule: outside it, a body falls toward the face it is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cube {
    pub centre: V3,
    pub half: Fx,
}

impl Cube {
    /// Whether `p` lies outside the cube's outer faces, `wall` thick.
    pub fn outside(&self, p: V3, wall: Fx) -> bool {
        let d = p.sub(self.centre);
        let r = self.half + wall;
        d.x.abs() > r || d.y.abs() > r || d.z.abs() > r
    }

    /// The up of the outer face `p` is over: the axis it lies farthest along.
    pub fn up_over(&self, p: V3) -> V3 {
        axis_of(p.sub(self.centre))
    }
}

/// T37: how far ahead of a moving body a face turns its gravity (m), and
/// how slow a body may move and still be carried by it (m/s); how far ahead
/// of the keys a face it presses against does (m); and how many ticks the
/// view takes to turn with it.
pub const APPROACH: Fx = ONE;
pub const APPROACH_SPEED: Fx = ONE / 2;
pub const TOUCH: Fx = ONE / 10;
pub const TURN_TICKS: i32 = 12;

/// A turn of a body's gravity, all in its frame before the turn: the turn
/// that carries its old up onto its new, where its feet now stand, and
/// whether its speed and view turn with it (a corner it runs round) or stay
/// as they are in the world (the open air, or a face above it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shift {
    pub turn: Turn,
    pub origin: V3,
    pub carry: bool,
}

/// The turn that carries up (+y) onto `n`: a quarter turn about the line
/// across, or, onto down, a half turn about the axis nearest `ahead`.
fn onto(n: V3, ahead: V3) -> Turn {
    if n.y == -ONE {
        Turn::half(axis_of(v3(ahead.x, 0, ahead.z)))
    } else {
        Turn::quarter(v3(0, ONE, 0), n)
    }
}

/// T37: whether a body, just moved in its own frame on a map whose gravity
/// turns, turns its gravity, and how. In order:
///
/// - a face ahead of its motion within `APPROACH`, or of its keys within
///   `TOUCH`, that is not its floor: that face becomes its floor. A wall
///   carries the body round the corner, its centre kept; a face overhead
///   takes it as it is.
/// - off the edge of its floor without a jump: it rolls round the edge
///   onto the face below it, carried.
/// - out in the open past the cube's outer faces: it falls toward the face
///   it is over, as it is.
pub fn shift(
    m: &mut Mover,
    b: &Body,
    was_on_ground: bool,
    input: &Input,
    yaw: i32,
    cube: &Cube,
) -> Option<Shift> {
    let high = b.maxs().y;
    let centre = b.origin.add(v3(0, high / 2, 0));
    let ahead = facing(yaw, 0);
    // a face ahead
    let (x, z) = input.walk_axes();
    let wish = ahead.scale(int_of(-z)).add(right_of(yaw).scale(int_of(x))).norm();
    let mut looks = Vec::with_capacity(2);
    if b.velocity.len() > APPROACH_SPEED {
        looks.push(b.velocity.norm().scale(APPROACH));
    }
    if wish != V3::ZERO {
        looks.push(wish.scale(TOUCH));
    }
    for d in looks {
        let t = m.trace(b, b.origin, b.origin.add(d));
        let n = t.normal;
        let square = [n.x, n.y, n.z].iter().filter(|c| c.abs() == ONE).count() == 1;
        if t.hit() && square && n.y != ONE {
            let half = high / 2;
            let origin = centre.sub(n.scale(half));
            return Some(Shift {
                turn: onto(n, ahead),
                origin,
                carry: n.y == 0,
            });
        }
    }
    // off an edge: the face it ran off, behind it and just below its feet
    if was_on_ground && !b.on_ground && b.velocity.y <= 0 {
        let flat = v3(b.velocity.x, 0, b.velocity.z);
        if flat.len() > 0 {
            let from = b.origin.sub(v3(0, ONE / 20, 0));
            let back = from.sub(flat.norm().scale(ONE + ONE / 2));
            let t = crate::trace::trace_box(m.map, &mut m.scratch, from, back, V3::ZERO, V3::ZERO);
            let n = t.normal;
            if t.hit() && n.y == 0 && (n.x.abs() == ONE || n.z.abs() == ONE) {
                // how far past the face the body is, measured from its feet
                let past = n.dot(b.origin.sub(t.end));
                let origin = b.origin.sub(n.scale(past)).sub(v3(0, past, 0));
                return Some(Shift {
                    turn: Turn::quarter(v3(0, ONE, 0), n),
                    origin,
                    carry: true,
                });
            }
        }
    }
    // the open air
    if !b.on_ground && cube.outside(centre, int_of(crate::maps::CUBO_WALL)) {
        let n = cube.up_over(centre);
        if n.y != ONE {
            return Some(Shift {
                turn: onto(n, ahead),
                origin: centre.sub(n.scale(high / 2)),
                carry: false,
            });
        }
    }
    None
}

fn int_of(k: i32) -> Fx {
    k * ONE
}

/// Whether a body box at `origin` (in the frame `m` was turned into) is
/// clear of every brush.
pub fn clear(m: &mut Mover, b: &Body, origin: V3) -> bool {
    let t = crate::trace::trace_box(m.map, &mut m.scratch, origin, origin, MINS, b.maxs());
    !t.start_solid
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::int;

    #[test]
    fn there_are_24_turns_each_its_own_and_each_undone_by_back() {
        let all = all();
        assert_eq!(all.len(), 24);
        let p = v3(int(1), int(2), int(3));
        for (k, t) in all.iter().enumerate() {
            assert_eq!(t.index() as usize, k);
            assert_eq!(t.back(t.apply(p)), p);
            assert_eq!(t.x.cross(t.y), t.z, "{t:?} keeps its hand");
        }
    }

    #[test]
    fn a_quarter_turn_carries_one_axis_onto_the_next_and_that_onto_the_first_reversed() {
        // floor up, wall ahead along +x: up goes to −x, forward to up
        let q = Turn::quarter(v3(0, ONE, 0), v3(-ONE, 0, 0));
        assert_eq!(q.apply(v3(0, ONE, 0)), v3(-ONE, 0, 0));
        assert_eq!(q.apply(v3(ONE, 0, 0)), v3(0, ONE, 0));
        assert_eq!(q.apply(v3(0, 0, ONE)), v3(0, 0, ONE));
        let h = Turn::half(v3(0, 0, ONE));
        assert_eq!(h.apply(v3(0, ONE, 0)), v3(0, -ONE, 0));
        assert!(all().contains(&q) && all().contains(&h));
    }

    #[test]
    fn a_view_turned_into_another_frame_faces_the_same_way() {
        for t in all() {
            let f = look(&Turn::IDENTITY, fx::deg(30), fx::deg(20));
            let (yaw, pitch) = aim_toward(&t, f);
            let g = look(&t, yaw, pitch);
            assert!(f.sub(g).len() < ONE / 100, "{t:?}: {f:?} against {g:?}");
        }
    }
}
