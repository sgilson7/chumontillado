//! Sweeping a player's box through the map: Source's CM_ClipBoxToBrush
//! (from Quake 2's), as CSSurf's `trace.rs` ports it, in Q16.16 (R1.3).
//! Each brush plane is pushed out by the box corner that meets it first, so
//! a box sweep becomes a point sweep against bigger planes. Every brush
//! carries its own box's planes as bevels (`map.rs`), so a box cannot snag
//! on a ramp's sharp ridge.

use crate::fx::{self, Fx, ONE};
use crate::geom::{v3, V3};
use crate::map::Map;

/// A trace stops this far short of a plane (Source's DIST_EPSILON is 1/32
/// of an inch; this is 1/1024 m, about the same).
pub const DIST_EPSILON: Fx = ONE / 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trace {
    /// how far along the sweep the box got, 0 to `ONE`
    pub fraction: Fx,
    pub end: V3,
    /// the face hit (zero when nothing was)
    pub normal: V3,
    /// the box began inside a brush
    pub start_solid: bool,
    /// and never got out of it
    pub all_solid: bool,
}

impl Trace {
    pub fn hit(&self) -> bool {
        self.fraction < ONE
    }
}

/// Space a tracer reuses between traces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scratch {
    near: Vec<u32>,
}

/// Sweeps the box `mins`..`maxs` (relative to the origin) from `start` to
/// `end` through `map`.
pub fn trace_box(map: &Map, scratch: &mut Scratch, start: V3, end: V3, mins: V3, maxs: V3) -> Trace {
    let mut t = Trace {
        fraction: ONE,
        end,
        normal: V3::ZERO,
        start_solid: false,
        all_solid: false,
    };
    let pad = v3(ONE, ONE, ONE);
    let lo = v3(start.x.min(end.x), start.y.min(end.y), start.z.min(end.z))
        .add(mins)
        .sub(pad);
    let hi = v3(start.x.max(end.x), start.y.max(end.y), start.z.max(end.z))
        .add(maxs)
        .add(pad);
    map.brushes_near(lo, hi, &mut scratch.near);
    for &i in &scratch.near {
        clip_box_to_brush(&mut t, &map.brushes[i as usize].planes, start, end, mins, maxs);
        if t.all_solid {
            break;
        }
    }
    if t.fraction < ONE {
        t.end = start.add(end.sub(start).scale(t.fraction));
    }
    t
}

fn clip_box_to_brush(t: &mut Trace, planes: &[crate::map::Plane], start: V3, end: V3, mins: V3, maxs: V3) {
    let mut enter = -ONE;
    let mut leave = ONE;
    let mut clip: Option<V3> = None;
    let mut start_out = false;
    let mut get_out = false;
    for p in planes {
        // the corner of the box that reaches furthest against this plane
        let ofs = v3(
            if p.n.x < 0 { maxs.x } else { mins.x },
            if p.n.y < 0 { maxs.y } else { mins.y },
            if p.n.z < 0 { maxs.z } else { mins.z },
        );
        let dist = p.d - ofs.dot(p.n);
        let d1 = start.dot(p.n) - dist;
        let d2 = end.dot(p.n) - dist;
        if d2 > 0 {
            get_out = true;
        }
        if d1 > 0 {
            start_out = true;
        }
        // wholly in front of this face: the sweep misses the brush
        if d1 > 0 && (d2 >= DIST_EPSILON || d2 >= d1) {
            return;
        }
        // wholly behind it
        if d1 <= 0 && d2 <= 0 {
            continue;
        }
        if d1 > d2 {
            // entering
            let f = fx::div(d1 - DIST_EPSILON, d1 - d2).max(0);
            if f > enter {
                enter = f;
                clip = Some(p.n);
            }
        } else {
            // leaving
            let f = fx::div(d1 + DIST_EPSILON, d1 - d2).min(ONE);
            if f < leave {
                leave = f;
            }
        }
    }
    if !start_out {
        t.start_solid = true;
        if !get_out {
            t.all_solid = true;
            t.fraction = 0;
        }
        return;
    }
    if enter < leave && enter > -ONE && enter < t.fraction {
        if let Some(n) = clip {
            t.fraction = enter.max(0);
            t.normal = n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::int;
    use crate::map::practice;

    const MINS: V3 = v3(-ONE * 32 / 100, 0, -ONE * 32 / 100);
    const MAXS: V3 = v3(ONE * 32 / 100, ONE * 185 / 100, ONE * 32 / 100);

    #[test]
    fn falling_onto_the_floor_stops_on_it_facing_up() {
        let m = practice();
        let mut s = Scratch::default();
        let a = v3(int(8), int(5), int(16));
        let t = trace_box(&m, &mut s, a, a.sub(v3(0, int(10), 0)), MINS, MAXS);
        assert!(t.hit());
        assert_eq!(t.normal, v3(0, ONE, 0));
        // stopped a hair above the floor's top (y = 0)
        assert!(t.end.y >= 0 && t.end.y < ONE / 200, "{}", t.end.y);
        assert!(!t.start_solid);
    }

    #[test]
    fn a_box_inside_a_wall_is_all_solid_and_one_beside_it_is_not() {
        let m = practice();
        let mut s = Scratch::default();
        let inside = v3(int(-32) - ONE / 2, int(3), 0);
        let t = trace_box(
            &m,
            &mut s,
            inside,
            inside.add(v3(ONE / 10, 0, 0)),
            v3(-ONE / 10, 0, -ONE / 10),
            v3(ONE / 10, ONE / 10, ONE / 10),
        );
        assert!(t.start_solid && t.all_solid);
        let free = v3(int(-20), int(9), int(-30));
        let t = trace_box(&m, &mut s, free, free, MINS, MAXS);
        assert!(!t.start_solid, "{t:?}");
    }

    #[test]
    fn running_at_a_wall_stops_short_of_it_and_reports_its_face() {
        let m = practice();
        let mut s = Scratch::default();
        let a = v3(int(28), ONE / 100, int(-10));
        let t = trace_box(&m, &mut s, a, a.add(v3(int(10), 0, 0)), MINS, MAXS);
        assert_eq!(t.normal, v3(-ONE, 0, 0));
        // the wall's face is at x = 32, the box reaches 0.32 past its origin
        let stop = int(32) - ONE * 32 / 100;
        assert!(t.end.x <= stop && t.end.x > stop - ONE / 100, "{}", t.end.x);
    }
}
