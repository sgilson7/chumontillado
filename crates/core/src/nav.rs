//! T19: where a bot can walk on a map, and the way round its walls. The
//! map's floor is cut into half-metre cells; a cell is open when a standing
//! player there would touch nothing but the floor, with a margin. A bot
//! finds its way to a goal over open cells (a breadth-first search out from
//! the goal) and heads for the furthest cell on that way it can reach in a
//! straight line, so it takes corridors and doorways instead of pressing
//! into the wall between it and the goal.

use crate::fx::{self, Fx, ONE};
use crate::geom::{v3, V3};
use crate::map::{Map, Surface};

/// A cell's side (m).
pub const CELL: Fx = ONE / 2;
/// Clear of anything by this much at the cell's middle (m): the player's
/// half width, its corner, and some to spare.
pub const CLEAR: Fx = ONE * 55 / 100;
/// The heights a cell is tested at (m): over a low step, the waist, the
/// head.
const HEIGHTS: [Fx; 3] = [ONE / 2, ONE * 11 / 10, ONE * 17 / 10];
/// Cells along the way a bot looks ahead for one it can reach straight.
pub const AHEAD: usize = 16;
/// How far from a closed cell to look for an open one (cells).
const NEAREST: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nav {
    /// the corner cell's (x, z), in cells
    x0: i32,
    z0: i32,
    w: i32,
    h: i32,
    open: Vec<bool>,
}

impl Nav {
    /// The open cells of `map`'s floor, which is its first brush.
    pub fn of(map: &Map) -> Nav {
        let floor = &map.brushes[0];
        let cell = |v: Fx| fx::floor(v * 2);
        let (x0, z0) = (cell(floor.mins.x), cell(floor.mins.z));
        let (w, h) = (cell(floor.maxs.x) - x0, cell(floor.maxs.z) - z0);
        let mut open = vec![true; (w * h) as usize];
        for br in map.brushes.iter().filter(|b| b.surface != Surface::Floor) {
            // only the cells near the brush's box, and only if it comes
            // down to head height
            if br.mins.y > HEIGHTS[2] + CLEAR {
                continue;
            }
            let lo = |v: Fx| cell(v - CLEAR) - 1;
            let (cx0, cz0) = (lo(br.mins.x) - x0, lo(br.mins.z) - z0);
            let (cx1, cz1) = (cell(br.maxs.x + CLEAR) + 1 - x0, cell(br.maxs.z + CLEAR) + 1 - z0);
            for cz in cz0.max(0)..cz1.min(h) {
                for cx in cx0.max(0)..cx1.min(w) {
                    let k = (cz * w + cx) as usize;
                    if !open[k] {
                        continue;
                    }
                    let mid = Nav::middle_of(x0 + cx, z0 + cz);
                    if HEIGHTS
                        .iter()
                        .any(|&y| crate::maps::distance_to(br, v3(mid.x, y, mid.z)) < CLEAR)
                    {
                        open[k] = false;
                    }
                }
            }
        }
        Nav { x0, z0, w, h, open }
    }

    fn middle_of(cx: i32, cz: i32) -> V3 {
        v3(cx * CELL + CELL / 2, 0, cz * CELL + CELL / 2)
    }

    /// The cell (index) a place is in, if it is on the map.
    fn cell_of(&self, p: V3) -> Option<(i32, i32)> {
        let (cx, cz) = (fx::floor(p.x * 2) - self.x0, fx::floor(p.z * 2) - self.z0);
        ((0..self.w).contains(&cx) && (0..self.h).contains(&cz)).then_some((cx, cz))
    }

    pub fn is_open(&self, p: V3) -> bool {
        self.cell_of(p)
            .is_some_and(|(cx, cz)| self.open[(cz * self.w + cx) as usize])
    }

    /// The open cell nearest `p` (by rings), if one is within `NEAREST`.
    fn nearest_open(&self, p: V3) -> Option<(i32, i32)> {
        let cx = (fx::floor(p.x * 2) - self.x0).clamp(0, self.w - 1);
        let cz = (fx::floor(p.z * 2) - self.z0).clamp(0, self.h - 1);
        for r in 0..=NEAREST {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    let (x, z) = (cx + dx, cz + dz);
                    if (0..self.w).contains(&x)
                        && (0..self.h).contains(&z)
                        && self.open[(z * self.w + x) as usize]
                    {
                        return Some((x, z));
                    }
                }
            }
        }
        None
    }

    /// Whether a straight walk from `a` to `b` crosses open cells only
    /// (tested every quarter cell).
    pub fn clear_between(&self, a: V3, b: V3) -> bool {
        let d = b.sub(a);
        let steps = (d.len_xz() * 8 / ONE).max(1);
        (0..=steps).all(|k| self.is_open(a.add(v3(d.x / steps * k, 0, d.z / steps * k))))
    }

    /// Where to head for, walking from `from` to `to`: the furthest point on
    /// the way round the walls that is in a straight line from `from`. None
    /// when there is no way (or no open cell near either end).
    pub fn toward(&self, from: V3, to: V3) -> Option<V3> {
        let start = self.nearest_open(from)?;
        let goal = self.nearest_open(to)?;
        let idx = |(x, z): (i32, i32)| (z * self.w + x) as usize;
        // steps from the goal, out to the start
        let mut steps = vec![i32::MAX; self.open.len()];
        let mut queue = std::collections::VecDeque::new();
        steps[idx(goal)] = 0;
        queue.push_back(goal);
        while let Some(c) = queue.pop_front() {
            if c == start {
                break;
            }
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let n = (c.0 + dx, c.1 + dz);
                if (0..self.w).contains(&n.0)
                    && (0..self.h).contains(&n.1)
                    && self.open[idx(n)]
                    && steps[idx(n)] == i32::MAX
                {
                    steps[idx(n)] = steps[idx(c)] + 1;
                    queue.push_back(n);
                }
            }
        }
        if steps[idx(start)] == i32::MAX {
            return None;
        }
        // down the steps from the start, keeping the furthest cell in a
        // straight line
        let mut at = start;
        let mut best = Nav::middle_of(self.x0 + start.0, self.z0 + start.1);
        for _ in 0..AHEAD {
            let next = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| (at.0 + dx, at.1 + dz))
                .filter(|n| (0..self.w).contains(&n.0) && (0..self.h).contains(&n.1))
                .min_by_key(|&n| steps[idx(n)])?;
            at = next;
            let mid = Nav::middle_of(self.x0 + at.0, self.z0 + at.1);
            if self.clear_between(from, mid) {
                best = mid;
            }
            if at == goal {
                return Some(if self.clear_between(from, to) { to } else { best });
            }
        }
        Some(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::int;

    #[test]
    fn a_wall_closes_its_cells_and_the_way_round_it_heads_for_its_end() {
        let m = crate::maps::galeria();
        let nav = Nav::of(&m);
        // every spawn stands on an open cell
        for &(at, _) in &m.spawns {
            assert!(nav.is_open(at), "{at:?}");
        }
        // inside a wall is closed
        let wall = &m.brushes[1];
        let mid = wall.mins.add(wall.maxs).scale(ONE / 2);
        assert!(!nav.is_open(v3(mid.x, 0, mid.z)));
        // from each spawn to every other there is a way, and the place to
        // head for is open and straight ahead of the start
        for &(a, _) in &m.spawns {
            for &(b, _) in &m.spawns {
                let next = nav.toward(a, b).expect("a way");
                assert!(nav.clear_between(a, next), "{a:?} to {b:?} by {next:?}");
            }
        }
    }

    #[test]
    fn on_open_floor_it_heads_straight_for_the_goal() {
        let m = crate::maps::plaza();
        let nav = Nav::of(&m);
        let (a, b) = (v3(int(8), 0, int(4)), v3(int(8), 0, int(-4)));
        assert_eq!(nav.toward(a, b), Some(b));
    }
}
