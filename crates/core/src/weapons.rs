//! The guns (M2: the five hitscan ones) and what a shot does: a ray from the
//! eye along the aim, stopped by the first wall, hitting the first body in
//! front of it (or every body, for the Pincel, which pierces).
//!
//! The numbers are the journal's Roster page (2026-11-04), in one table,
//! `GUNS`, so tuning by play is one edit. Damage is out of 100 health; a
//! head takes `HEAD_PCT` of a body's.
//!
//! Bodies are hit as a vertical cylinder for the body and a ball for the
//! head, inside the shared envelope (CS-Q12: A); every skin is hit the same.
//! The ray tests use 64-bit integers: across 200 m a squared length
//! overflows Q16.16.

use crate::fx::{self, int, Fx, ONE};
use crate::geom::{facing, v3, V3};
use crate::map::Map;
use crate::trace::{trace_box, Scratch};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gun {
    Pistol = 0,
    Shotgun = 1,
    MachinePistol = 2,
    Rifle = 3,
    Pincel = 4,
}

pub const GUN_COUNT: usize = 5;
pub const ALL: [Gun; GUN_COUNT] = [
    Gun::Pistol,
    Gun::Shotgun,
    Gun::MachinePistol,
    Gun::Rifle,
    Gun::Pincel,
];

/// A gun's numbers. Rates are in ticks (60 a second).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// the strings' key for its name
    pub name: &'static str,
    pub damage: i32,
    /// rays a shot casts (the shotgun's pellets)
    pub pellets: i32,
    /// how far a pellet or a shot strays from the aim, at most (angle units)
    pub spread: i32,
    /// ticks between shots
    pub interval: i32,
    /// fires while the button is held
    pub auto: bool,
    pub magazine: i32,
    pub reload: i32,
    /// how far a shot reaches (m)
    pub range: Fx,
    /// the Pincel: ticks to charge to full, and its damage then; it fires
    /// on release and passes through bodies
    pub charge: i32,
    pub charged_damage: i32,
    pub pierce: bool,
    /// a push back along the aim when fired in the air (m/s): the
    /// shotgun's jump
    pub kick: Fx,
}

pub const GUNS: [Stats; GUN_COUNT] = [
    Stats {
        name: "gun_pistol",
        damage: 25,
        pellets: 1,
        spread: 0,
        interval: 15,
        auto: false,
        magazine: 8,
        reload: 60,
        range: int(120),
        charge: 0,
        charged_damage: 0,
        pierce: false,
        kick: 0,
    },
    Stats {
        name: "gun_shotgun",
        damage: 12,
        pellets: 8,
        spread: fx::deg(5),
        interval: 55,
        auto: false,
        magazine: 8,
        reload: 90,
        range: int(40),
        charge: 0,
        charged_damage: 0,
        pierce: false,
        kick: int(6),
    },
    Stats {
        name: "gun_machine_pistol",
        damage: 11,
        pellets: 1,
        spread: fx::deg(2),
        interval: 5,
        auto: true,
        magazine: 30,
        reload: 75,
        range: int(80),
        charge: 0,
        charged_damage: 0,
        pierce: false,
        kick: 0,
    },
    Stats {
        name: "gun_rifle",
        damage: 70,
        pellets: 1,
        spread: 0,
        interval: 60,
        auto: false,
        magazine: 5,
        reload: 120,
        range: int(200),
        charge: 0,
        charged_damage: 0,
        pierce: false,
        kick: 0,
    },
    Stats {
        name: "gun_pincel",
        damage: 40,
        pellets: 1,
        spread: 0,
        interval: 30,
        auto: false,
        magazine: 4,
        reload: 120,
        range: int(200),
        charge: 60,
        charged_damage: 100,
        pierce: true,
        kick: 0,
    },
];

/// A head takes twice a body's damage.
pub const HEAD_PCT: i32 = 200;

impl Gun {
    pub fn stats(self) -> &'static Stats {
        &GUNS[self as usize]
    }

    pub fn from_index(i: usize) -> Gun {
        ALL[i % GUN_COUNT]
    }
}

/// What a body offers a shot: its feet, its height to the top of the
/// body's cylinder, its head's centre and the envelope's radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub feet: V3,
    pub body_top: Fx,
    pub head: V3,
}

/// The body's radius (the envelope's, 0.32 m) and the head's (0.16 m).
pub const BODY_R: Fx = ONE * 32 / 100;
pub const HEAD_R: Fx = ONE * 16 / 100;

impl Target {
    /// A standing or crouched body with its feet at `feet` and its eye
    /// `eye` above them.
    pub fn of(feet: V3, eye: Fx) -> Target {
        Target {
            feet,
            body_top: eye - HEAD_R,
            head: feet.add(v3(0, eye, 0)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// which target, by its place in the list given
    pub who: usize,
    /// how far along the ray (m)
    pub t: Fx,
    pub head: bool,
}

fn dot64(a: V3, b: V3) -> i64 {
    (a.x as i64 * b.x as i64 + a.y as i64 * b.y as i64 + a.z as i64 * b.z as i64) >> fx::SHIFT
}

/// Where along the unit ray from `o` along `d` it first meets the ball at
/// `c` of radius `r`, if it does ahead of `o`.
pub fn ray_ball(o: V3, d: V3, c: V3, r: Fx) -> Option<Fx> {
    let oc = c.sub(o);
    let along = dot64(oc, d);
    let off2 = dot64(oc, oc) - ((along * along) >> fx::SHIFT);
    let r2 = (r as i64 * r as i64) >> fx::SHIFT;
    if off2 > r2 {
        return None;
    }
    let half = fx::isqrt_u64(((r2 - off2) << fx::SHIFT) as u64) as i64;
    let t = along - half;
    (t >= 0).then_some(t as Fx)
}

/// Where the ray first meets the upright cylinder of radius `r` round the
/// line through `c`, between heights `lo` and `hi`, if it does ahead of `o`.
pub fn ray_column(o: V3, d: V3, c: V3, r: Fx, lo: Fx, hi: Fx) -> Option<Fx> {
    let (ox, oz) = ((o.x - c.x) as i64, (o.z - c.z) as i64);
    let (dx, dz) = (d.x as i64, d.z as i64);
    let a = (dx * dx + dz * dz) >> fx::SHIFT;
    if a == 0 {
        return None;
    }
    let b = (ox * dx + oz * dz) >> fx::SHIFT;
    let cc = ((ox * ox + oz * oz) >> fx::SHIFT) - ((r as i64 * r as i64) >> fx::SHIFT);
    let disc = ((b * b) >> fx::SHIFT) - ((a * cc) >> fx::SHIFT);
    if disc < 0 {
        return None;
    }
    let s = fx::isqrt_u64((disc << fx::SHIFT) as u64) as i64;
    let t = ((-b - s) << fx::SHIFT) / a;
    if t < 0 {
        return None;
    }
    let y = o.y as i64 + ((d.y as i64 * t) >> fx::SHIFT);
    (y >= lo as i64 && y <= hi as i64).then_some(t as Fx)
}

/// How far the ray goes before a wall stops it, up to `range`.
pub fn wall_distance(map: &Map, scratch: &mut Scratch, o: V3, d: V3, range: Fx) -> Fx {
    let end = o.add(d.scale(range));
    let t = trace_box(map, scratch, o, end, V3::ZERO, V3::ZERO);
    fx::mul(t.fraction, range)
}

/// One ray: the bodies it hits before the wall, nearest first (only the
/// nearest unless `pierce`), and where it stops.
pub fn cast(
    map: &Map,
    scratch: &mut Scratch,
    o: V3,
    d: V3,
    range: Fx,
    targets: &[Option<Target>],
    pierce: bool,
) -> (Vec<Hit>, Fx) {
    let wall = wall_distance(map, scratch, o, d, range);
    let mut hits: Vec<Hit> = Vec::new();
    for (who, t) in targets.iter().enumerate() {
        let Some(t) = t else { continue };
        let head = ray_ball(o, d, t.head, HEAD_R);
        let body = ray_column(o, d, t.feet, BODY_R, t.feet.y, t.feet.y + t.body_top);
        let hit = match (head, body) {
            (Some(h), Some(b)) if b < h => Some((b, false)),
            (Some(h), _) => Some((h, true)),
            (None, Some(b)) => Some((b, false)),
            (None, None) => None,
        };
        if let Some((at, head)) = hit {
            if at < wall {
                hits.push(Hit { who, t: at, head });
            }
        }
    }
    hits.sort_by_key(|h| (h.t, h.who));
    if !pierce {
        hits.truncate(1);
    }
    let stop = if pierce || hits.is_empty() {
        wall
    } else {
        hits[0].t
    };
    (hits, stop)
}

/// The directions a shot's pellets fly: the aim, and for a spread gun,
/// the rest on a ring round it (the shotgun's fixed pattern, so every
/// browser and every shot agree), or one stray for an automatic, chosen by
/// `salt`.
pub fn pellets(g: &Stats, yaw: i32, pitch: i32, salt: u32) -> Vec<V3> {
    if g.spread == 0 {
        return vec![facing(yaw, pitch)];
    }
    if g.pellets == 1 {
        // a stray within the spread, from the salt: the same on every browser
        let a = (salt.wrapping_mul(2_654_435_761) >> 16) as i32 % fx::TURN;
        let r = ((salt.wrapping_mul(40_503) >> 8) % 1000) as i32 * g.spread / 1000;
        return vec![facing(
            yaw + fx::mul(fx::cos(a), r),
            pitch + fx::mul(fx::sin(a), r),
        )];
    }
    let mut out = vec![facing(yaw, pitch)];
    let ring = g.pellets - 1;
    for k in 0..ring {
        let a = fx::TURN * k / ring;
        out.push(facing(
            yaw + fx::mul(fx::cos(a), g.spread),
            pitch + fx::mul(fx::sin(a), g.spread),
        ));
    }
    out
}

/// A gun's damage for a hit: the Pincel's from its charge (`charged`
/// ticks, linear to full), and twice for a head.
pub fn damage(g: &Stats, charged: i32, head: bool) -> i32 {
    let base = if g.charge > 0 {
        g.damage + (g.charged_damage - g.damage) * charged.clamp(0, g.charge) / g.charge
    } else {
        g.damage
    };
    if head {
        base * HEAD_PCT / 100
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::practice;

    fn near(a: Fx, b: Fx, within: Fx) -> bool {
        (a - b).abs() <= within
    }

    #[test]
    fn a_ray_meets_a_ball_at_its_near_side_and_misses_one_off_to_the_side() {
        // from the origin along −z to a ball 10 m off of radius 0.16
        let t = ray_ball(V3::ZERO, v3(0, 0, -ONE), v3(0, 0, -int(10)), HEAD_R).unwrap();
        assert!(near(t, int(10) - HEAD_R, 8), "{}", fx::decimal(t, 4));
        assert_eq!(
            ray_ball(V3::ZERO, v3(0, 0, -ONE), v3(ONE / 5, 0, -int(10)), HEAD_R),
            None
        );
        // behind the ray
        assert_eq!(
            ray_ball(V3::ZERO, v3(0, 0, -ONE), v3(0, 0, int(10)), HEAD_R),
            None
        );
    }

    #[test]
    fn a_ray_meets_a_column_between_its_heights_only() {
        let d = v3(0, 0, -ONE);
        // a body 20 m off, from the feet to 1.49 m: a ray at 1 m meets its
        // near side 0.32 m short
        let t = ray_column(v3(0, ONE, 0), d, v3(0, 0, -int(20)), BODY_R, 0, ONE * 149 / 100).unwrap();
        assert!(near(t, int(20) - BODY_R, 8), "{}", fx::decimal(t, 4));
        // a ray at 2 m passes over it
        assert_eq!(
            ray_column(
                v3(0, int(2), 0),
                d,
                v3(0, 0, -int(20)),
                BODY_R,
                0,
                ONE * 149 / 100
            ),
            None
        );
    }

    #[test]
    fn a_shot_hits_the_nearest_body_before_the_wall_and_a_head_counts_as_one() {
        let m = practice();
        let mut s = Scratch::default();
        // from the spawn's eye, north along the open floor
        let o = v3(int(8), ONE * 165 / 100, int(16));
        let d = v3(0, 0, -ONE);
        let near_one = Target::of(v3(int(8), 0, int(6)), ONE * 165 / 100);
        let far_one = Target::of(v3(int(8), 0, -int(2)), ONE * 165 / 100);
        let (hits, stop) = cast(
            &m,
            &mut s,
            o,
            d,
            int(100),
            &[Some(far_one), Some(near_one)],
            false,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(
            (hits[0].who, hits[0].head),
            (1, true),
            "the eye-high ray meets the near one's head"
        );
        assert!(near(stop, int(10) - HEAD_R, 8));
        // piercing, both, nearest first; stopped by the north wall at z = −32
        let (hits, stop) = cast(&m, &mut s, o, d, int(100), &[Some(far_one), Some(near_one)], true);
        assert_eq!(hits.iter().map(|h| h.who).collect::<Vec<_>>(), vec![1, 0]);
        assert!(near(stop, int(48), ONE / 100), "{}", fx::decimal(stop, 3));
    }

    #[test]
    fn a_wall_between_stops_the_shot() {
        let m = practice();
        let mut s = Scratch::default();
        // the block at (10, −12) is 2 m high; behind it, a target
        let o = v3(int(10), ONE, int(-6));
        let d = v3(0, 0, -ONE);
        let behind = Target::of(v3(int(10), 0, int(-20)), ONE * 165 / 100);
        let (hits, stop) = cast(&m, &mut s, o, d, int(100), &[Some(behind)], false);
        assert!(hits.is_empty());
        // the block's face at z = −11
        assert!(near(stop, int(5), ONE / 100), "{}", fx::decimal(stop, 3));
    }

    #[test]
    fn damage_is_the_rosters_with_a_head_doubled_and_the_pincel_charged() {
        let pistol = Gun::Pistol.stats();
        assert_eq!((damage(pistol, 0, false), damage(pistol, 0, true)), (25, 50));
        let rifle = Gun::Rifle.stats();
        assert_eq!(damage(rifle, 0, true), 140, "a rifle headshot kills");
        let pincel = Gun::Pincel.stats();
        // 40 uncharged, 100 at a full second, 70 at half
        assert_eq!(
            (
                damage(pincel, 0, false),
                damage(pincel, 30, false),
                damage(pincel, 60, false)
            ),
            (40, 70, 100)
        );
        assert_eq!(damage(pincel, 600, false), 100);
    }

    #[test]
    fn the_shotgun_throws_eight_pellets_on_a_fixed_ring_and_the_pistol_one_true() {
        let shotgun = Gun::Shotgun.stats();
        let a = pellets(shotgun, 0, 0, 1);
        let b = pellets(shotgun, 0, 0, 99);
        assert_eq!(a.len(), 8);
        assert_eq!(a, b, "the same pattern whatever the salt");
        // each ring pellet 5° off the aim: its cosine with the aim ≈ 0.996
        for p in &a[1..] {
            let c = p.dot(a[0]);
            assert!(c < ONE && c > ONE * 995 / 1000, "{c}");
        }
        assert_eq!(pellets(Gun::Pistol.stats(), 0, 0, 7), vec![facing(0, 0)]);
    }

    #[test]
    fn the_numbers_are_the_rosters() {
        // the Roster page: pistol 4/s, 8 rounds; shotgun 1.1/s, 8; machine
        // pistol 12/s, 30; rifle 1/s, 5; Pincel 4
        let rate = |g: Gun| g.stats().interval;
        assert_eq!(
            (
                rate(Gun::Pistol),
                rate(Gun::Shotgun),
                rate(Gun::MachinePistol),
                rate(Gun::Rifle)
            ),
            (15, 55, 5, 60)
        );
        let mag = |g: Gun| g.stats().magazine;
        assert_eq!(ALL.map(mag), [8, 8, 30, 5, 4]);
    }
}
