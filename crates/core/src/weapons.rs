//! The weapons and what each does. A hitscan gun's shot is a ray from the
//! eye along the aim, stopped by the first wall, hitting the first body in
//! front of it (or every body, for the Pincel, which pierces). M3 adds the
//! projectile guns, whose shots fly through the ticks in `world.rs` and
//! meet bodies through `meets` and bursts through `blast_damage`, and the
//! melee weapons, whose swings hit through `in_arc`.
//!
//! The numbers are the journal's Roster page (2026-11-04), in one table,
//! `WEAPONS`, so tuning by play is one edit. Damage is out of 100 health; a
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
pub enum Weapon {
    Pistol = 0,
    Shotgun = 1,
    MachinePistol = 2,
    Rifle = 3,
    Pincel = 4,
    // M3: the projectile guns
    Drum = 5,
    Trabuco = 6,
    Toro = 7,
    Campana = 8,
    Collage = 9,
    // M3: the melee weapons
    Katana = 10,
    Navaja = 11,
    Guadana = 12,
    // asked for on 2026-11-04 (T16): the high-jumping knife
    Punal = 13,
}

pub const COUNT: usize = 14;
pub const ALL: [Weapon; COUNT] = [
    Weapon::Pistol,
    Weapon::Shotgun,
    Weapon::MachinePistol,
    Weapon::Rifle,
    Weapon::Pincel,
    Weapon::Drum,
    Weapon::Trabuco,
    Weapon::Toro,
    Weapon::Campana,
    Weapon::Collage,
    Weapon::Katana,
    Weapon::Navaja,
    Weapon::Guadana,
    Weapon::Punal,
];
/// The guns take the number keys 1 to 0 in this order; the melee key takes
/// the four melee weapons in turn.
pub const MELEE: [Weapon; 4] = [Weapon::Katana, Weapon::Navaja, Weapon::Guadana, Weapon::Punal];

/// How a weapon reaches what it hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// a ray from the eye, there at once
    Hitscan,
    /// a body that flies, falls and bounces through the ticks (M3)
    Projectile(Flight),
    /// an arc in front of the eye during a swing's active ticks (CS-Q7: A)
    Melee(Swing),
}

/// A projectile's flight. Speeds in m/s, gravity in m/s².
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flight {
    pub speed: Fx,
    pub gravity: Fx,
    /// up added to the launch, for a lob (the Campana)
    pub lift: Fx,
    /// how many walls it bounces off before the next one stops it
    pub bounces: i32,
    /// the speed a bounce keeps, percent
    pub keep_pct: i32,
    /// ticks until it bursts where it is (the drum's fuse), or until it is
    /// gone; 0 for neither
    pub fuse: i32,
    /// how far from its centre it meets a body (m)
    pub size: Fx,
    /// its burst on meeting a body or a wall that stops it, or at its fuse
    pub blast: Option<Blast>,
    /// ticks a nail that meets a body with a wall close behind it holds the
    /// body there (the Trabuco)
    pub pins: i32,
}

/// A burst: damage that falls off with distance, and a push away from it.
/// Within `inner` a body takes `near`; from there to `outer` it falls in a
/// straight line to `far`; beyond `outer`, nothing. The push falls the
/// same way from `push` to nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blast {
    pub inner: Fx,
    pub outer: Fx,
    pub near: i32,
    pub far: i32,
    pub push: Fx,
}

/// The second button's use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alt {
    None,
    /// the katana: flung along the aim, hitting what the arc meets
    Lunge,
    /// the guadaña: a line to a wall within reach that pulls you to it
    Hook,
}

/// A melee weapon's swing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    /// how far past the body's edge it reaches (m)
    pub reach: Fx,
    /// the arc each side of the aim (angle units)
    pub half_arc: i32,
    /// the ticks of a swing during which it hits: from `from` to `until`
    pub from: i32,
    pub until: i32,
    /// speed added along the aim on the ground plane by each swing (m/s)
    pub carry: Fx,
    /// damage from behind, or 0 for none extra (the navaja)
    pub behind: i32,
    pub alt: Alt,
    /// the run speed while held, percent (the navaja's 10% faster)
    pub run_pct: i32,
    /// wall jumps a flight while held
    pub wall_jumps: i32,
    /// a jump's speed while held, percent (the puñal's higher jump)
    pub jump_pct: i32,
    /// T23: with no flick it thrusts straight ahead, in `half_arc` of the
    /// aim (the knives), rather than cut level
    pub thrusts: bool,
}

/// A weapon's numbers. Rates are in ticks (60 a second).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// the strings' key for its name
    pub name: &'static str,
    pub kind: Kind,
    /// a hit's damage: a ray's, a projectile's direct one, a swing's
    pub damage: i32,
    /// rays or projectiles a shot sends (the shotgun's and Trabuco's)
    pub pellets: i32,
    /// how far a pellet or a shot strays from the aim, at most (angle units)
    pub spread: i32,
    /// ticks between shots or swings
    pub interval: i32,
    /// fires while the button is held
    pub auto: bool,
    /// rounds; 0 for a melee weapon, which needs none
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

/// A gun that fires at once: its kind and the fields a hitscan gun leaves
/// unused, so the table below states only what differs.
const HITSCAN: Stats = Stats {
    name: "",
    kind: Kind::Hitscan,
    damage: 0,
    pellets: 1,
    spread: 0,
    interval: 0,
    auto: false,
    magazine: 0,
    reload: 0,
    range: int(200),
    charge: 0,
    charged_damage: 0,
    pierce: false,
    kick: 0,
};

const FLIGHT: Flight = Flight {
    speed: 0,
    gravity: 0,
    lift: 0,
    bounces: 0,
    keep_pct: 0,
    fuse: 0,
    size: ONE / 10,
    blast: None,
    pins: 0,
};

const SWING: Swing = Swing {
    reach: 0,
    half_arc: 0,
    from: 2,
    until: 8,
    carry: 0,
    behind: 0,
    alt: Alt::None,
    run_pct: 100,
    wall_jumps: 3,
    jump_pct: 100,
    thrusts: false,
};

/// The beat every semi-automatic gun's and melee weapon's interval, and
/// every reload, is a whole or half of: 30 ticks, 120 beats a minute
/// (CS-Q20: A; the journal's Feel page, claim 2). One edit changes the
/// tempo. The automatics (machine pistol, Collage) fire a stream, not taps,
/// and keep their rates.
pub const BEAT: i32 = 30;

/// Self-damage from your own burst, percent of what another body would take
/// (CS-Q16: A).
pub const SELF_PCT: i32 = 30;
/// A katana swing turns projectiles in its first ticks (CS-Q15: A).
pub const PARRY: i32 = 6;
/// The katana's lunge: the speed it flings you along the aim (m/s), the
/// ticks it hits for, and the ticks before the next.
pub const LUNGE_SPEED: Fx = int(16);
/// The lunge lifts you off the ground (m/s up), past the speed at which a
/// body leaves it (`movement::TUNING.non_jump_velocity`, 3.5), so friction
/// does not eat the fling: 0.44 s in the air at 16 m/s is about 7 m, near
/// the Roster page's 8.
pub const LUNGE_LIFT: Fx = int(4);
/// A lunge's damage (the Roster page).
pub const LUNGE_DAMAGE: i32 = 100;
pub const LUNGE_TICKS: i32 = 18;
pub const LUNGE_INTERVAL: i32 = BEAT * 3;
/// The guadaña's hook: how far it reaches, how hard it pulls (m/s²), and
/// the ticks it holds at most.
pub const HOOK_REACH: Fx = int(12);
pub const HOOK_PULL: Fx = int(40);
pub const HOOK_TICKS: i32 = 150;
/// The hook lifts you off the ground as it catches (m/s up), past the
/// speed at which a body leaves it, or the floor holds you and the pull
/// only drags you along it.
pub const HOOK_LIFT: Fx = int(4);
/// A nail pins a body when a wall is this close behind it (m).
pub const PIN_BEHIND: Fx = ONE * 15 / 10;

pub const WEAPONS: [Stats; COUNT] = [
    Stats {
        name: "gun_pistol",
        damage: 25,
        interval: BEAT / 2,
        magazine: 8,
        reload: BEAT * 2,
        range: int(120),
        ..HITSCAN
    },
    Stats {
        name: "gun_shotgun",
        damage: 12,
        pellets: 8,
        spread: fx::deg(5),
        interval: BEAT * 2,
        magazine: 8,
        reload: BEAT * 3,
        range: int(40),
        kick: int(6),
        ..HITSCAN
    },
    Stats {
        name: "gun_machine_pistol",
        damage: 11,
        spread: fx::deg(2),
        interval: 5,
        auto: true,
        magazine: 30,
        reload: BEAT * 3,
        range: int(80),
        ..HITSCAN
    },
    Stats {
        name: "gun_rifle",
        damage: 70,
        interval: BEAT * 2,
        magazine: 5,
        reload: BEAT * 4,
        ..HITSCAN
    },
    Stats {
        name: "gun_pincel",
        damage: 40,
        interval: BEAT,
        magazine: 4,
        reload: BEAT * 4,
        charge: 60,
        charged_damage: 100,
        pierce: true,
        ..HITSCAN
    },
    // M3: the projectile guns. The Roster page's damage and rates; the
    // speeds, fuses and pushes are mine (R7.1)
    Stats {
        name: "gun_drum",
        kind: Kind::Projectile(Flight {
            speed: int(24),
            gravity: int(18),
            // it bounces until its fuse
            bounces: 10_000,
            keep_pct: 55,
            fuse: 90,
            size: ONE * 12 / 100,
            blast: Some(Blast {
                inner: 0,
                outer: int(3),
                near: 60,
                far: 15,
                push: int(8),
            }),
            ..FLIGHT
        }),
        damage: 90,
        interval: BEAT * 2,
        magazine: 6,
        reload: BEAT * 4,
        ..HITSCAN
    },
    Stats {
        name: "gun_trabuco",
        kind: Kind::Projectile(Flight {
            speed: int(60),
            gravity: int(6),
            fuse: 120,
            size: ONE * 5 / 100,
            pins: 10,
            ..FLIGHT
        }),
        damage: 15,
        pellets: 7,
        spread: fx::deg(4),
        interval: BEAT * 2,
        magazine: 1,
        reload: BEAT * 2,
        ..HITSCAN
    },
    Stats {
        name: "gun_toro",
        kind: Kind::Projectile(Flight {
            speed: int(20),
            fuse: 600,
            size: ONE * 15 / 100,
            blast: Some(Blast {
                inner: 0,
                outer: int(4),
                near: 70,
                far: 20,
                push: int(10),
            }),
            ..FLIGHT
        }),
        damage: 100,
        interval: BEAT * 3,
        magazine: 3,
        reload: BEAT * 4,
        ..HITSCAN
    },
    Stats {
        name: "gun_campana",
        kind: Kind::Projectile(Flight {
            speed: int(16),
            gravity: int(18),
            lift: int(4),
            fuse: 600,
            size: ONE * 25 / 100,
            blast: Some(Blast {
                inner: ONE,
                outer: int(5),
                near: 120,
                far: 50,
                push: int(12),
            }),
            ..FLIGHT
        }),
        damage: 120,
        interval: BEAT * 3,
        magazine: 2,
        reload: BEAT * 4,
        ..HITSCAN
    },
    Stats {
        name: "gun_collage",
        kind: Kind::Projectile(Flight {
            speed: int(40),
            bounces: 1,
            keep_pct: 100,
            fuse: 180,
            size: ONE * 8 / 100,
            ..FLIGHT
        }),
        damage: 30,
        interval: 20,
        auto: true,
        magazine: 12,
        reload: BEAT * 3,
        ..HITSCAN
    },
    // M3: the melee weapons (CS-Q7: A)
    Stats {
        name: "melee_katana",
        kind: Kind::Melee(Swing {
            reach: ONE * 16 / 10,
            half_arc: fx::deg(50),
            carry: ONE,
            alt: Alt::Lunge,
            wall_jumps: 4,
            ..SWING
        }),
        damage: 50,
        interval: BEAT * 3 / 2,
        ..HITSCAN
    },
    Stats {
        name: "melee_navaja",
        kind: Kind::Melee(Swing {
            reach: ONE * 11 / 10,
            half_arc: fx::deg(35),
            from: 1,
            until: 5,
            behind: 150,
            run_pct: 110,
            thrusts: true,
            ..SWING
        }),
        damage: 35,
        interval: BEAT / 2,
        ..HITSCAN
    },
    Stats {
        name: "melee_guadana",
        kind: Kind::Melee(Swing {
            reach: ONE * 25 / 10,
            half_arc: fx::deg(65),
            from: 6,
            until: 14,
            alt: Alt::Hook,
            ..SWING
        }),
        damage: 70,
        interval: BEAT * 3,
        ..HITSCAN
    },
    // T16: the puñal, a short stabbing knife that, held, jumps half as fast
    // again: height goes with the square of the speed, so 1.5² = 2.25 times
    // a jump's 1.74 m, 3.92 m (R15.1)
    Stats {
        name: "melee_punal",
        kind: Kind::Melee(Swing {
            reach: ONE,
            half_arc: fx::deg(30),
            from: 1,
            until: 5,
            jump_pct: 150,
            thrusts: true,
            ..SWING
        }),
        damage: 30,
        interval: BEAT / 2,
        ..HITSCAN
    },
];

/// A head takes twice a body's damage.
pub const HEAD_PCT: i32 = 200;

impl Weapon {
    pub fn stats(self) -> &'static Stats {
        &WEAPONS[self as usize]
    }

    pub fn from_index(i: usize) -> Weapon {
        ALL[i % COUNT]
    }

    pub fn swing(self) -> Option<Swing> {
        match self.stats().kind {
            Kind::Melee(s) => Some(s),
            _ => None,
        }
    }

    pub fn flight(self) -> Option<Flight> {
        match self.stats().kind {
            Kind::Projectile(f) => Some(f),
            _ => None,
        }
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

/// The body's radius and the head's: the shared hit envelope (CS-Q12),
/// 0.96 m across and a head 0.56 m across. T5 widened them from the Roster
/// page's 0.64 m and 0.32 m, under which a body at the arena's distances was
/// a sliver (R6.1). The movement box stays 0.64 m (`movement::HALF_WIDTH`).
pub const BODY_R: Fx = ONE * 48 / 100;
pub const HEAD_R: Fx = ONE * 28 / 100;

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

/// Where the unit ray first meets a body, grown by `grow` (a projectile's
/// size), and whether it is the head.
pub fn meets(o: V3, d: V3, t: &Target, grow: Fx) -> Option<(Fx, bool)> {
    let head = ray_ball(o, d, t.head, HEAD_R + grow);
    let body = ray_column(
        o,
        d,
        t.feet,
        BODY_R + grow,
        t.feet.y - grow,
        t.feet.y + t.body_top,
    );
    match (head, body) {
        (Some(h), Some(b)) if b < h => Some((b, false)),
        (Some(h), _) => Some((h, true)),
        (None, Some(b)) => Some((b, false)),
        (None, None) => None,
    }
}

/// The point of a body's upright line, from its feet to its head's centre,
/// nearest `p`.
pub fn nearest_on(t: &Target, p: V3) -> V3 {
    let top = t.head.y;
    t.feet.add(v3(0, p.y.clamp(t.feet.y, top) - t.feet.y, 0))
}

/// How far `p` is from a body's surface: from its upright line, less the
/// body's radius; 0 inside it.
pub fn gap_to(t: &Target, p: V3) -> Fx {
    (p.dist(nearest_on(t, p)) - BODY_R).max(0)
}

/// A burst's damage to a body whose surface is `gap` from its centre.
pub fn blast_damage(b: &Blast, gap: Fx) -> i32 {
    if gap > b.outer {
        return 0;
    }
    if gap <= b.inner {
        return b.near;
    }
    let along = fx::div(gap - b.inner, b.outer - b.inner);
    b.near - fx::round(fx::mul(int(b.near - b.far), along))
}

/// A burst's push on a body `gap` from its centre: `push` at the centre,
/// nothing at `outer`.
pub fn blast_push(b: &Blast, gap: Fx) -> Fx {
    if gap >= b.outer {
        return 0;
    }
    b.push - fx::mul(b.push, fx::div(gap, b.outer))
}

/// T23 (CS-Q27: A, CS-Q28: A): a melee swing is a cut, as a Chumworld
/// Arena blade's is. The way the view was turning as the button went down
/// picks it: `Player::flick` holds the turn of the last few ticks, across
/// and up, each tick's added to five sixths of the last. Within `CUT_SNAP`
/// of an axis it snaps to that axis (turning right a level cut from left to
/// right, down a chop, up a rising cut); between, a diagonal at 45°. A
/// flick under `FLICK` picks no cut: the katana and the guadaña then cut
/// level, the other way from their last; a knife thrusts.
pub const FLICK: i32 = fx::deg(3);
pub const CUT_SNAP: i32 = fx::deg(30);
/// The cuts' ways on the screen: the way the blade travels, 0 to the right,
/// a quarter turn up.
pub const CUT_RIGHT: i32 = 0;
pub const CUT_LEFT: i32 = -fx::HALF_TURN;
pub const CUT_CHOP: i32 = -fx::QUARTER_TURN;
/// A cut meets a body within this of its plane, beyond the angle the body
/// spans: the blade's width and the arm's give.
pub const BLADE: i32 = fx::deg(10);
/// After the strike a cut follows through `FOLLOW` past its end for
/// `FOLLOW_TICKS`, and comes back to rest over `RECOVER_TICKS` (drawn only:
/// it hits nothing then); before it, a wind-up pulls `WIND` back past the
/// start.
pub const FOLLOW: i32 = fx::deg(20);
pub const FOLLOW_TICKS: i32 = 4;
pub const RECOVER_TICKS: i32 = 8;
pub const WIND: i32 = fx::deg(20);

/// Whether a cut runs level, either way.
pub fn is_level(cut: i32) -> bool {
    cut == CUT_RIGHT || cut == CUT_LEFT
}

/// The cut a flick picks, if it is long enough: its way on the screen.
pub fn cut_of(flick: (i32, i32)) -> Option<i32> {
    let (across, up) = flick;
    if fx::length(across, up) < FLICK {
        return None;
    }
    let a = fx::wrap(fx::atan2(up, across));
    let axis = fx::wrap((a + fx::QUARTER_TURN / 2).div_euclid(fx::QUARTER_TURN) * fx::QUARTER_TURN);
    Some(if fx::wrap(a.wrapping_sub(axis)).abs() <= CUT_SNAP {
        axis
    } else {
        // between two axes: the diagonal between them
        let lower = fx::wrap(a.div_euclid(fx::QUARTER_TURN) * fx::QUARTER_TURN);
        fx::wrap(lower + fx::QUARTER_TURN / 2)
    })
}

/// Where a cut's blade is through its strike, from `-half_arc` at `from` to
/// `half_arc` at `until`: the angle in the cut's plane from the aim, toward
/// the way it travels.
pub fn sweep(s: &Swing, age: i32) -> i32 {
    let span = (s.until - s.from).max(1);
    let t = (age - s.from).clamp(0, span);
    -s.half_arc + 2 * s.half_arc / span * t
}

/// The aim's frame for a cut of way `cut`: the aim, the way the blade
/// travels, and the plane's normal.
pub fn cut_frame(yaw: i32, pitch: i32, cut: i32) -> (V3, V3, V3) {
    let f = facing(yaw, pitch);
    let right = crate::geom::right_of(yaw);
    let up = facing(yaw, pitch + fx::QUARTER_TURN);
    let s = right.scale(fx::cos(cut)).add(up.scale(fx::sin(cut)));
    (f, s, f.cross(s))
}

/// Points down a body's upright line a cut is tested at, each a ball of
/// the body's radius.
pub const CUT_SAMPLES: i32 = 5;

/// Whether a cut of way `cut` from `eye`, its blade swept this tick from
/// `was` to `now` in its plane, meets a body: some point down its line
/// within `reach` of the blade's length, within `BLADE` and the angle the
/// body spans of the plane, and between `was` and `now` give or take that
/// angle (or the body touching the eye).
pub fn in_cut(s: &Swing, eye: V3, frame: (V3, V3, V3), was: i32, now: i32, t: &Target) -> bool {
    let (f, along, n) = frame;
    (0..CUT_SAMPLES).any(|k| {
        let y = t.feet.y + fx::mul(t.head.y - t.feet.y, ONE * k / (CUT_SAMPLES - 1));
        let q = v3(t.feet.x, y, t.feet.z).sub(eye);
        let d = q.len();
        if d <= BODY_R {
            return true;
        }
        if d - BODY_R > s.reach {
            return false;
        }
        let spans = fx::atan2(BODY_R, d);
        let (a, b, c) = (q.dot(f), q.dot(along), q.dot(n));
        let out = fx::atan2(c.abs(), fx::length(a, b));
        let at = fx::wrap(fx::atan2(b, a));
        out <= spans + BLADE && at >= was.min(now) - spans && at <= was.max(now) + spans
    })
}

/// Whether a body is in a swing's arc from the eye along `aim`: its
/// surface within `reach`, and its nearest point within `half_arc` of the
/// aim (or touching the eye).
pub fn in_arc(s: &Swing, eye: V3, aim: V3, t: &Target) -> bool {
    let p = nearest_on(t, eye);
    let to = p.sub(eye);
    let d = p.dist(eye);
    if d - BODY_R > s.reach {
        return false;
    }
    if d <= BODY_R {
        return true;
    }
    // the body's edge widens the arc: the angle it spans each side
    let spans = fx::atan2(BODY_R, d);
    // |to × aim| and to · aim are |to| sin θ and |to| cos θ for a unit aim
    let off = fx::atan2(to.cross(aim).len(), to.dot(aim));
    off <= s.half_arc + spans
}

/// Whether `attacker` is behind a body facing `yaw`: more than 120° from the
/// way it faces, seen from the body.
pub fn from_behind(feet: V3, yaw: i32, attacker: V3) -> bool {
    let to = attacker.sub(feet);
    let ahead = facing(yaw, 0);
    let flat = v3(to.x, 0, to.z);
    let n = flat.len();
    n > 0 && fx::div(flat.dot(ahead), n) < -(ONE / 2)
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
        if let Some((at, head)) = meets(o, d, t, 0) {
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
        // from the origin along −z to a ball 10 m off of the head's radius,
        // 0.28 (T5); one 0.35 m to the side is missed
        let t = ray_ball(V3::ZERO, v3(0, 0, -ONE), v3(0, 0, -int(10)), HEAD_R).unwrap();
        assert!(near(t, int(10) - HEAD_R, 8), "{}", fx::decimal(t, 4));
        assert_eq!(
            ray_ball(V3::ZERO, v3(0, 0, -ONE), v3(ONE * 35 / 100, 0, -int(10)), HEAD_R),
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
    fn a_shot_four_tenths_of_a_metre_beside_a_body_hits_it_and_one_six_tenths_off_misses() {
        // T5: the body is 0.96 m across (radius 0.48)
        let m = practice();
        let mut s = Scratch::default();
        let d = v3(0, 0, -ONE);
        let body = [Some(Target::of(v3(int(8), 0, -int(2)), ONE * 165 / 100))];
        let mut at = |dx: Fx| {
            cast(
                &m,
                &mut s,
                v3(int(8) + dx, ONE, int(16)),
                d,
                int(100),
                &body,
                false,
            )
            .0
        };
        assert_eq!(at(ONE * 4 / 10).len(), 1);
        assert!(at(ONE * 6 / 10).is_empty());
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
        let pistol = Weapon::Pistol.stats();
        assert_eq!((damage(pistol, 0, false), damage(pistol, 0, true)), (25, 50));
        let rifle = Weapon::Rifle.stats();
        assert_eq!(damage(rifle, 0, true), 140, "a rifle headshot kills");
        let pincel = Weapon::Pincel.stats();
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
        let shotgun = Weapon::Shotgun.stats();
        let a = pellets(shotgun, 0, 0, 1);
        let b = pellets(shotgun, 0, 0, 99);
        assert_eq!(a.len(), 8);
        assert_eq!(a, b, "the same pattern whatever the salt");
        // each ring pellet 5° off the aim: its cosine with the aim ≈ 0.996
        for p in &a[1..] {
            let c = p.dot(a[0]);
            assert!(c < ONE && c > ONE * 995 / 1000, "{c}");
        }
        assert_eq!(pellets(Weapon::Pistol.stats(), 0, 0, 7), vec![facing(0, 0)]);
    }

    #[test]
    fn the_numbers_are_the_rosters() {
        // the Roster page: pistol 4/s, 8 rounds; shotgun 1.1/s, 8, on the
        // beat 1/s (CS-Q20); machine pistol 12/s, 30; rifle 1/s, 5; Pincel 4
        let rate = |g: Weapon| g.stats().interval;
        assert_eq!(
            (
                rate(Weapon::Pistol),
                rate(Weapon::Shotgun),
                rate(Weapon::MachinePistol),
                rate(Weapon::Rifle)
            ),
            (15, 60, 5, 60)
        );
        let mag = |g: Weapon| g.stats().magazine;
        assert_eq!(ALL.map(mag)[..5], [8, 8, 30, 5, 4]);
    }
    // ---- M3 ----------------------------------------------------------------

    #[test]
    fn a_bursts_damage_falls_in_a_line_from_near_to_far_and_stops_at_its_edge() {
        // the Toro: 70 at the centre, 20 at 4 m: at 2 m, 70 − 50 × ½ = 45
        let toro = Weapon::Toro.flight().unwrap().blast.unwrap();
        assert_eq!(
            [0, int(2), int(4), int(4) + ONE / 50].map(|g| blast_damage(&toro, g)),
            [70, 45, 20, 0]
        );
        // the Campana: 120 within 1 m, 50 at 5 m: at 3 m, 120 − 70 × ½ = 85
        let bell = Weapon::Campana.flight().unwrap().blast.unwrap();
        assert_eq!(
            [ONE / 2, ONE, int(3), int(5)].map(|g| blast_damage(&bell, g)),
            [120, 120, 85, 50]
        );
        // the push: the Toro's 10 m/s at the centre, 5 at 2 m, none at 4
        assert_eq!(
            [0, int(2), int(4)].map(|g| blast_push(&toro, g)),
            [int(10), int(5), 0]
        );
    }

    #[test]
    fn a_katana_swing_reaches_a_body_whose_edge_is_within_its_reach_and_in_its_arc() {
        let k = Weapon::Katana.swing().unwrap();
        let eye = v3(0, ONE * 165 / 100, 0);
        let north = v3(0, 0, -ONE);
        let at = |x: Fx, z: Fx| Target::of(v3(x, 0, z), ONE * 165 / 100);
        // 2 m off: its edge 2 − 0.48 = 1.52 m, within the 1.6
        assert!(in_arc(&k, eye, north, &at(0, -int(2))));
        // 2.2 m off: 1.72, past it
        assert!(!in_arc(&k, eye, north, &at(0, -(ONE * 22 / 10))));
        // 45° to the side at the same distance: within the 50° arc
        assert!(in_arc(&k, eye, north, &at(ONE * 14 / 10, -(ONE * 14 / 10))));
        // square to the side, and behind: no
        assert!(!in_arc(&k, eye, north, &at(int(2), 0)));
        assert!(!in_arc(&k, eye, north, &at(0, int(2))));
    }

    #[test]
    fn behind_is_more_than_a_hundred_and_twenty_degrees_from_where_a_body_faces() {
        // a body at the origin facing −z (heading 0)
        assert!(from_behind(V3::ZERO, 0, v3(0, 0, int(2))));
        assert!(
            from_behind(V3::ZERO, 0, v3(ONE, 0, int(2))),
            "27° off straight behind"
        );
        assert!(!from_behind(V3::ZERO, 0, v3(0, 0, -int(2))));
        assert!(!from_behind(V3::ZERO, 0, v3(int(2), 0, 0)), "square to the side");
    }

    #[test]
    fn a_projectile_meets_a_body_grown_by_its_size() {
        // along −z at 1 m high, 0.56 m beside a body's line: past the
        // body's 0.48, within 0.48 + the rocket's 0.15
        let t = Target::of(v3(0, 0, -int(10)), ONE * 165 / 100);
        let o = v3(ONE * 56 / 100, ONE, 0);
        assert_eq!(meets(o, v3(0, 0, -ONE), &t, 0), None);
        assert!(meets(o, v3(0, 0, -ONE), &t, ONE * 15 / 100).is_some());
    }

    #[test]
    fn every_tapped_interval_and_every_reload_is_a_whole_or_half_beat() {
        // CS-Q20: A. A beat is 30 ticks (120 a minute), so a half beat is
        // 15; the two automatics fire a stream and are left out
        for g in ALL {
            let st = g.stats();
            if !st.auto {
                assert_eq!(st.interval % 15, 0, "{:?} fires every {} ticks", g, st.interval);
            }
            assert_eq!(st.reload % 30, 0, "{:?} reloads in {} ticks", g, st.reload);
        }
        assert_eq!(BEAT, 30);
    }

    /// T23 (CS-Q27: A): the view's turn picks the cut, snapped to an axis
    /// within 30° of one, else the diagonal between; too small a turn, none.
    #[test]
    fn a_flick_picks_its_cut_snapped_to_an_axis_or_a_diagonal() {
        let d = fx::deg(6);
        assert_eq!(cut_of((d, 0)), Some(CUT_RIGHT));
        assert_eq!(cut_of((-d, 0)), Some(CUT_LEFT));
        assert_eq!(cut_of((0, -d)), Some(CUT_CHOP));
        assert_eq!(cut_of((0, d)), Some(fx::QUARTER_TURN), "a rising cut");
        // 22° up from level: a level cut; 45°: the diagonal
        assert_eq!(cut_of((d, d * 4 / 10)), Some(CUT_RIGHT));
        assert_eq!(cut_of((d, d)), Some(fx::QUARTER_TURN / 2));
        // 63° below level, 27° from straight down: a chop
        assert_eq!(cut_of((d / 2, -d)), Some(CUT_CHOP));
        assert_eq!(cut_of((FLICK / 2, 0)), None);
    }

    fn katana() -> Swing {
        match crate::weapons::Weapon::Katana.stats().kind {
            Kind::Melee(s) => s,
            _ => unreachable!(),
        }
    }

    /// T23 (CS-Q28: A): a cut meets what its arc passes, not what lies off
    /// its plane. From an eye 1.65 m up looking along −z, the katana's arc
    /// is 50° each side and reaches 1.6 m past a body's edge.
    #[test]
    fn a_cut_meets_what_its_arc_passes_and_not_what_lies_off_its_plane() {
        let s = katana();
        let eye = v3(0, ONE * 165 / 100, 0);
        let (yaw, pitch) = (0, 0);
        let sweep_all =
            |cut: i32, t: &Target| in_cut(&s, eye, cut_frame(yaw, pitch, cut), -s.half_arc, s.half_arc, t);
        // 1.8 m off, 40° to the right, standing level: the level cut meets
        // it; a chop's plane is 40° from it, past the 15° it spans and the
        // blade's 10°
        let (sn, cs) = (fx::sin(fx::deg(40)), fx::cos(fx::deg(40)));
        let side = Target::of(
            v3(fx::mul(int(18) / 10, sn), 0, -fx::mul(int(18) / 10, cs)),
            ONE * 165 / 100,
        );
        assert!(sweep_all(CUT_RIGHT, &side));
        assert!(!sweep_all(CUT_CHOP, &side));
        // 1.3 m ahead and 1.3 m below the floor, its head 45° down: the
        // chop meets it; the level cut's plane passes 45° over it
        let low = Target::of(v3(0, -int(13) / 10, -int(13) / 10), ONE * 165 / 100);
        assert!(sweep_all(CUT_CHOP, &low));
        assert!(!sweep_all(CUT_RIGHT, &low));
    }

    /// T23: a level cut to the right meets a body on its left early in the
    /// strike and one on its right late.
    #[test]
    fn a_cut_meets_a_body_where_its_blade_is_at_that_tick() {
        let s = katana();
        let eye = v3(0, ONE * 165 / 100, 0);
        let frame = cut_frame(0, 0, CUT_RIGHT);
        let first = |x: Fx| {
            let t = Target::of(v3(x, 0, -int(15) / 10), ONE * 165 / 100);
            (s.from..=s.until).find(|&age| in_cut(&s, eye, frame, sweep(&s, age - 1), sweep(&s, age), &t))
        };
        let (left, right) = (first(-int(1)).expect("met"), first(int(1)).expect("met"));
        assert!(left < right, "left at {left}, right at {right}");
        assert_eq!(sweep(&s, s.from), -s.half_arc);
        assert_eq!(sweep(&s, s.until), s.half_arc);
    }
}
