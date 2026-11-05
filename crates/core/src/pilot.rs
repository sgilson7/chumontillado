//! The bot (M2): a seat whose input record is made from the world, so it
//! moves and shoots through exactly what a player's keys and mouse give
//! (CS-3). Every browser computes the same record from the same world, which
//! is what rollback needs (M4).
//!
//! It hops round a loop of waypoints. In a duel it also turns toward the
//! player at a limited rate with a small wobble, and once it has seen them
//! for a reaction time and its aim is close, it fires.
//!
//! T19: in a match it hunts the nearest other seat standing instead, and
//! finds its way there round the map's walls (`nav.rs`). One that has
//! stood still too long turns aside for a moment and finds its way again.
//!
//! T20 (CS-Q26: A): it fights the way a player does. Shot at, it dodges
//! sideways, sliding on the ground and kicking off walls in the air; low
//! on health, it falls back out of its shooter's sight for a while. It
//! aims ahead of a moving target, takes up weapons it lacks, and holds
//! the one that suits the distance. How well it does each is its skill, a
//! setting (Easy, Normal, Hard), the same for every bot in the match.

use crate::camera::LOOK_UNITS_PER_100_PX;
use crate::fx::{self, int, Fx, ONE};
use crate::geom::{v3, yaw_of, V3};
use crate::input::{Input, BACK, CROUCH, FIRE, FORWARD, JUMP, LEFT, MELEE, RIGHT, SLOTS};
use crate::weapons::{Kind, Weapon};

/// The bot's loop in the practice room, clear of the blocks.
pub const LOOP: [(i32, i32); 4] = [(8, -28), (16, -28), (16, -16), (6, -16)];
/// A waypoint is reached within this distance (m).
const REACHED: Fx = ONE * 15 / 10;
/// The most the bot turns in a tick, and the wobble on its aim (angle units).
pub const TURN_RATE: i32 = fx::deg(6);
pub const WOBBLE: i32 = fx::deg(6);
/// Ticks between first seeing the player and the first shot.
pub const REACTION: i32 = 40;
/// Sam, after T21: bots fire in bursts, `burst` ticks of every `BURST_CYCLE`
/// once their reaction is past.
pub const BURST_CYCLE: i32 = 120;
/// Its aim strays no further than this from the target (m), so a far
/// target is not out of its reach.
pub const STRAY: Fx = ONE;
/// Its aim's stray narrows by a quarter over this many ticks of keeping a
/// target in sight: a person who stays in its sight is found.
pub const SETTLE: i32 = 120;
/// A target out of sight this many ticks is forgotten: seen again, the bot
/// reacts and settles its aim anew.
pub const MEMORY: i32 = 30;
/// It fires when its aim is within this of the player's chest.
pub const FIRE_WITHIN: i32 = fx::deg(2);
/// It sees no further than this (m).
pub const SIGHT: Fx = int(50);
/// T19: ticks between finding its way again.
pub const REPLAN: i32 = 12;
/// T19: slower than this on the ground (m/s) for `STILL_FOR` ticks is
/// stuck; it then turns aside for `ASIDE` ticks.
pub const STILL: Fx = ONE;
pub const STILL_FOR: i32 = 40;
pub const ASIDE: i32 = 24;
/// T19: hunting, once it sees its quarry it circles it inside `CIRCLE` and
/// backs off inside `CLOSE` (m), rather than run onto it; it changes which
/// way it circles every `CIRCLE_TURN` ticks.
pub const CIRCLE: Fx = int(10);
pub const CLOSE: Fx = int(4);
pub const CIRCLE_TURN: u64 = 90;
/// T19: holding a blade, it runs in and swings once its quarry's chest is
/// within the blade's reach and this (m), with its aim this close.
pub const SWING_SLACK: Fx = ONE / 2;
pub const SWING_WITHIN: i32 = fx::deg(15);

/// T20: how far it lets a moving target's flight run ahead with a gun
/// that fires at once: the ticks its aim lags its turn (s).
pub const LAG: Fx = ONE * 4 / 60;
/// T20: ticks it dodges after a hit, and falls back after one when low.
pub const DODGE: i32 = 30;
pub const FALL_BACK: i32 = 150;
/// T20: ticks between its weapon switches, so it does not flicker between
/// two at a distance where both suit.
pub const SWITCH_REST: i32 = 45;
/// T20: out of a fight, it goes for a weapon it lacks lying within this
/// (m).
pub const DETOUR: Fx = int(14);
/// T20: a gun whose spread strays further than a body's half width at a
/// distance does not suit it; a projectile suits it while its flight is
/// under this (s).
pub const FLIGHT_SUITS: Fx = ONE / 2;
pub const HALF_BODY: Fx = ONE * 48 / 100;

/// T20 (CS-Q26: A): how good the bots are, a setting; Normal to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Skill {
    Easy,
    #[default]
    Normal,
    Hard,
}

pub const SKILLS: [Skill; 3] = [Skill::Easy, Skill::Normal, Skill::Hard];

/// What a skill changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Traits {
    /// ticks between first seeing a target and the first shot
    pub reaction: i32,
    /// the most it turns a tick, and the wobble on its aim (angle units)
    pub turn: i32,
    pub wobble: i32,
    /// it fires when its aim is within this of the target (angle units)
    pub fire_within: i32,
    /// ticks of every `BURST_CYCLE` it fires in
    pub burst: i32,
    /// how much of a moving target's lead it aims ahead, percent
    pub lead_pct: i32,
    /// it dodges when hit
    pub dodges: bool,
    /// it falls back when hit at or under this health
    pub fall_back_at: i32,
    /// it takes up weapons and switches by distance
    pub arms: bool,
}

impl Skill {
    pub fn traits(self) -> Traits {
        match self {
            Skill::Easy => Traits {
                reaction: 60,
                turn: fx::deg(4),
                wobble: fx::deg(10),
                fire_within: fx::deg(3),
                burst: 20,
                lead_pct: 0,
                dodges: false,
                fall_back_at: 0,
                arms: false,
            },
            Skill::Normal => Traits {
                reaction: REACTION,
                turn: TURN_RATE,
                wobble: WOBBLE,
                fire_within: FIRE_WITHIN,
                burst: 24,
                lead_pct: 70,
                dodges: true,
                fall_back_at: 30,
                arms: true,
            },
            Skill::Hard => Traits {
                reaction: 24,
                turn: fx::deg(9),
                wobble: fx::deg(4),
                fire_within: fx::deg(3) / 2,
                burst: 40,
                lead_pct: 100,
                dodges: true,
                fall_back_at: 40,
                arms: true,
            },
        }
    }

    /// Its name's key in the strings.
    pub fn key(self) -> &'static str {
        match self {
            Skill::Easy => "skill_easy",
            Skill::Normal => "skill_normal",
            Skill::Hard => "skill_hard",
        }
    }

    pub fn of(n: u8) -> Skill {
        SKILLS[(n as usize).min(SKILLS.len() - 1)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pilot {
    pub waypoint: usize,
    /// ticks it has seen the player for, and ticks since it last saw them
    pub seen: i32,
    pub lost: i32,
    /// fires back, or only moves
    pub shoots: bool,
    /// T19: where it heads for on its way, ticks until it finds its way
    /// again, ticks it has stood still, ticks left turned aside
    pub steer: V3,
    pub plan: i32,
    pub still: i32,
    pub aside: i32,
    /// T20: ticks left dodging and the shooter it dodges; ticks left
    /// falling back; ticks before it switches weapon again
    pub dodge: i32,
    pub dodging: V3,
    pub falling_back: i32,
    pub switched: i32,
}

/// What the bot knows: itself, its map's route (M5) and, if it can see
/// one, the nearest other seat's chest.
pub struct View<'a> {
    pub route: &'a [(i32, i32)],
    /// T19: in a match, the map's open floor, and the feet of the nearest
    /// other seat standing
    pub nav: Option<&'a crate::nav::Nav>,
    pub hunt: Option<V3>,
    /// T19: its speed across the ground (m/s), and whether nobody moves
    pub speed: Fx,
    pub frozen: bool,
    /// T19: the reach of the blade it holds; none for a gun
    pub reach: Option<Fx>,
    pub tick: u64,
    pub feet: V3,
    pub eye: V3,
    pub yaw: i32,
    pub pitch: i32,
    pub on_ground: bool,
    pub chest: Option<V3>,
    /// T20: the bots' skill; the velocity of the target it sees (m/s);
    /// where the shot that hit it this tick came from; its health; the
    /// nearest place out of its target's sight, when it is low; what it
    /// holds and owns; and the weapons lying on the map
    pub skill: Skill,
    pub chest_velocity: V3,
    pub hit_from: Option<V3>,
    pub health: i32,
    pub cover: Option<V3>,
    pub weapon: Weapon,
    pub owned: u16,
    pub pickups: &'a [crate::world::Pickup],
}

/// T20: how well weapon `g` suits a target `d` off: a shot's damage, all
/// its pellets, within the distance it suits, and nothing beyond. A shot's
/// and not a second's, since a bot taps the trigger. A blade suits only
/// what is nearly in reach.
pub fn suits(g: Weapon, d: Fx) -> i32 {
    let s = g.stats();
    let reach = match s.kind {
        Kind::Melee(sw) => sw.reach + ONE,
        Kind::Projectile(f) => fx::mul(f.speed, FLIGHT_SUITS).min(s.range),
        // half a body's width over the tangent of the spread
        _ if s.spread > 0 => fx::div(fx::mul(HALF_BODY, fx::cos(s.spread)), fx::sin(s.spread)).min(s.range),
        _ => s.range,
    };
    if d > reach {
        return 0;
    }
    // the Pincel tapped fires uncharged
    s.damage * s.pellets.max(1)
}

/// The key that takes weapon `g` into the hand.
fn key_for(g: Weapon) -> u32 {
    if g.swing().is_some() {
        MELEE
    } else {
        SLOTS[g as usize]
    }
}

/// A turn of `d` angle units as the mouse movement (hundredths of a pixel)
/// that gives it at a look speed of 100% (`camera::Aim::turned`, inverted:
/// moving right turns to a smaller yaw, down to a smaller pitch).
pub fn mouse_for(d: i32) -> i32 {
    -((d as i64 * 10_000) / LOOK_UNITS_PER_100_PX) as i32
}

/// The heading change toward `want`, at most `rate`.
fn toward(from: i32, want: i32, rate: i32) -> i32 {
    fx::wrap(want.wrapping_sub(from)).clamp(-rate, rate)
}

/// The movement keys that carry the bot toward heading `go` while it faces
/// `face`: the nearest of the eight.
fn keys_toward(face: i32, go: i32) -> u32 {
    let rel = fx::wrap(go.wrapping_sub(face));
    let eighth = fx::TURN / 8;
    let k = ((rel + eighth / 2).rem_euclid(fx::TURN)) / eighth;
    // 0 ahead, then turning left (a larger heading) round the eight
    [
        FORWARD,
        FORWARD | LEFT,
        LEFT,
        BACK | LEFT,
        BACK,
        BACK | RIGHT,
        RIGHT,
        FORWARD | RIGHT,
    ][k as usize]
}

/// One tick's record for the bot.
pub fn record(p: &mut Pilot, v: &View) -> Input {
    let t = v.skill.traits();
    let target = if p.shoots { v.chest } else { None };
    // T20: hit, it dodges the shooter; hit when low, it falls back
    if let Some(from) = v.hit_from {
        if t.dodges {
            p.dodge = DODGE;
            p.dodging = from;
        }
        if v.health <= t.fall_back_at {
            p.falling_back = FALL_BACK;
        }
    }
    p.dodge = (p.dodge - 1).max(0);
    p.falling_back = (p.falling_back - 1).max(0);
    p.switched = (p.switched - 1).max(0);
    // T20: out of a fight, a weapon it lacks lying near
    let wanted = v
        .pickups
        .iter()
        .filter(|k| {
            t.arms
                && v.hunt.is_some()
                && target.is_none()
                && !k.taken
                && v.owned & (1 << k.weapon as u16) == 0
                && k.at.sub(v.feet).len_xz() <= DETOUR
        })
        .min_by_key(|k| k.at.sub(v.feet).len_xz())
        .map(|k| k.at);
    let cover = v.cover.filter(|_| p.falling_back > 0);
    let goal = match cover.or(wanted).or(v.hunt) {
        Some(h) => h,
        None => {
            let route = if v.route.is_empty() { &LOOP[..] } else { v.route };
            let (x, z) = route[p.waypoint % route.len()];
            let goal = v3(int(x), v.feet.y, int(z));
            if goal.sub(v.feet).len_xz() < REACHED {
                p.waypoint = (p.waypoint + 1) % route.len();
            }
            let (x, z) = route[p.waypoint % route.len()];
            v3(int(x), 0, int(z))
        }
    };
    let mut toward_goal = goal;
    let mut aside = 0;
    if let Some(nav) = v.nav {
        // T19: the way round the walls, found again every `REPLAN` ticks
        // or on reaching where it headed
        p.plan -= 1;
        if p.plan <= 0 || p.steer.sub(v.feet).len_xz() < REACHED {
            p.steer = nav.toward(v.feet, goal).unwrap_or(goal);
            p.plan = REPLAN;
        }
        toward_goal = p.steer;
        p.still = if v.on_ground && !v.frozen && v.speed < STILL {
            p.still + 1
        } else {
            0
        };
        if p.still >= STILL_FOR {
            p.still = 0;
            p.aside = ASIDE;
            p.plan = 0;
        }
        if p.aside > 0 {
            p.aside -= 1;
            // left or right, by turns, the same on every browser
            aside = if (v.tick / ASIDE as u64).is_multiple_of(2) {
                fx::QUARTER_TURN
            } else {
                -fx::QUARTER_TURN
            };
        }
    }
    // yaw_of looks across the ground only
    let mut go = yaw_of(toward_goal.sub(v.feet));
    if let (Some(c), Some(_), None, None) = (target, v.hunt, v.reach, cover) {
        let to = c.sub(v.feet);
        let side = if (v.tick / CIRCLE_TURN).is_multiple_of(2) {
            fx::QUARTER_TURN
        } else {
            -fx::QUARTER_TURN
        };
        if to.len_xz() < CLOSE {
            go = yaw_of(to).wrapping_add(fx::HALF_TURN);
        } else if to.len_xz() < CIRCLE {
            go = yaw_of(to).wrapping_add(side);
        }
    }
    let dodging = p.dodge > 0 && cover.is_none();
    if dodging {
        // T20: across the shooter's line, one way then the other
        let side = if (v.tick / DODGE as u64).is_multiple_of(2) {
            fx::QUARTER_TURN
        } else {
            -fx::QUARTER_TURN
        };
        go = yaw_of(p.dodging.sub(v.feet)).wrapping_add(side);
    }
    let go = go.wrapping_add(aside);
    // it keeps a target lost from sight in mind for `MEMORY` ticks
    p.lost = if target.is_some() { 0 } else { p.lost + 1 };
    p.seen = match (target, p.lost > MEMORY) {
        (Some(_), _) => p.seen + 1,
        (None, false) => p.seen,
        (None, true) => 0,
    };
    let (want_yaw, want_pitch) = match target {
        Some(c) => {
            // T20: ahead of a moving target by its flight, or by the lag
            // of its own aim for a gun that fires at once
            let far = c.sub(v.eye).len();
            let flight = v
                .weapon
                .flight()
                .map(|f| fx::div(far, f.speed.max(ONE)))
                .unwrap_or(LAG);
            let lead = v.chest_velocity.scale(fx::mul(flight, ONE * t.lead_pct / 100));
            let to = c.add(lead).sub(v.eye);
            // a slow wobble on the aim, the same on every browser
            let tick = v.tick as i32;
            let settled = (p.seen - t.reaction).clamp(0, SETTLE);
            let wobble = t.wobble - t.wobble / 4 * settled / SETTLE;
            let wobble = wobble.min(fx::atan2(STRAY, far.max(ONE)));
            let wob = fx::mul(fx::sin(tick.wrapping_mul(fx::deg(7))), wobble);
            // and a slower one up and down, half as wide
            let bob = fx::mul(fx::cos(tick.wrapping_mul(fx::deg(3))), wobble / 2);
            // atan2 gives 0 to a whole turn; a pitch is signed (R5.3)
            (
                yaw_of(to).wrapping_add(wob),
                fx::wrap(fx::atan2(to.y, to.len_xz())) + bob,
            )
        }
        None => (go, 0),
    };
    let dyaw = toward(v.yaw, want_yaw, t.turn);
    let dpitch = (want_pitch - v.pitch).clamp(-t.turn, t.turn);
    let face = v.yaw.wrapping_add(dyaw);
    let mut keys = keys_toward(face, go) | JUMP;
    if !v.on_ground && target.is_none() && !dodging {
        // in the air, W alone steers it round the loop (T2)
        keys = FORWARD | JUMP;
    }
    if dodging || p.aside > 0 {
        if v.on_ground && v.speed >= crate::movement::TUNING.slide_from {
            // T20: a slide on the ground
            keys = (keys & !JUMP) | CROUCH;
        } else if !v.on_ground && v.tick.is_multiple_of(2) {
            // in the air, jump let go and pressed again: a wall jump off
            // any wall near
            keys &= !JUMP;
        }
    }
    // T20: the weapon that suits the distance, among those it owns
    if let (true, Some(c)) = (t.arms, target) {
        let d = c.sub(v.eye).len();
        let best = (0..crate::weapons::COUNT)
            .map(Weapon::from_index)
            .filter(|&g| v.owned & (1 << g as u16) != 0)
            .max_by_key(|&g| (suits(g, d), g == v.weapon))
            .unwrap_or(v.weapon);
        if p.switched == 0 && best != v.weapon && suits(best, d) > suits(v.weapon, d) {
            keys |= key_for(best);
            p.switched = SWITCH_REST;
        }
    }
    let within = if v.reach.is_some() {
        SWING_WITHIN
    } else {
        t.fire_within
    };
    let aimed = target.is_some()
        && fx::wrap(want_yaw.wrapping_sub(face)).abs() <= within
        && (want_pitch - (v.pitch + dpitch)).abs() <= within
        // T19: a blade swings only in reach
        && v.reach.is_none_or(|r| target.is_some_and(|c| c.sub(v.eye).len() <= r + SWING_SLACK));
    // a semi-automatic needs the button let go between shots
    let burst = (p.seen - t.reaction) % BURST_CYCLE < t.burst;
    let fire = aimed && p.seen >= t.reaction && burst && v.tick.is_multiple_of(2);
    Input {
        keys,
        buttons: if fire { FIRE } else { 0 },
        dx: mouse_for(dyaw),
        dy: mouse_for(dpitch),
        wheel: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Aim;
    use crate::input::Mouse;

    #[test]
    fn the_mouse_movement_for_a_turn_turns_the_view_by_it() {
        for d in [fx::deg(8), -fx::deg(5), fx::deg(1) / 3] {
            let a = Aim::new(0).turned(Mouse {
                dx: mouse_for(d),
                dy: 0,
                wheel: 0,
            });
            // within an angle unit or two of rounding
            assert!((a.yaw - fx::wrap(d)).abs() <= 2, "{d}: {}", a.yaw);
        }
    }

    #[test]
    fn the_keys_for_a_heading_are_the_nearest_of_the_eight() {
        assert_eq!(keys_toward(0, 0), FORWARD);
        assert_eq!(
            keys_toward(0, fx::QUARTER_TURN),
            LEFT,
            "a larger heading is to the left"
        );
        assert_eq!(keys_toward(0, -fx::QUARTER_TURN), RIGHT);
        assert_eq!(keys_toward(0, fx::HALF_TURN), BACK);
        assert_eq!(
            keys_toward(fx::QUARTER_TURN, fx::QUARTER_TURN + fx::deg(40)),
            FORWARD | LEFT
        );
    }

    #[test]
    fn it_waits_its_reaction_then_fires_only_with_its_aim_close() {
        let mut p = Pilot {
            shoots: true,
            ..Pilot::default()
        };
        // the player straight ahead, level, 20 m off: the aim is on them
        // but for the wobble, within 3° and so sometimes past FIRE_WITHIN
        let mut v = View {
            route: &LOOP,
            nav: None,
            hunt: None,
            speed: 0,
            frozen: false,
            reach: None,
            tick: 0,
            feet: v3(int(8), 0, int(-16)),
            eye: v3(int(8), ONE * 165 / 100, int(-16)),
            yaw: 0,
            pitch: 0,
            on_ground: true,
            chest: Some(v3(int(8), ONE * 165 / 100, int(-36))),
            skill: Skill::Normal,
            chest_velocity: V3::ZERO,
            hit_from: None,
            health: 100,
            cover: None,
            weapon: Weapon::Pistol,
            owned: crate::arms::ROUND_OWNED,
            pickups: &[],
        };
        let mut first = None;
        for t in 0..120 {
            v.tick = t;
            let r = record(&mut p, &v);
            if r.button(FIRE) && first.is_none() {
                first = Some(t);
            }
        }
        let first = first.expect("it fired");
        assert!(
            first >= REACTION as u64 - 1,
            "fired at {first}, before its reaction"
        );
        // a player out of sight: no shot, and past its memory the count
        // starts again
        v.chest = None;
        let r = record(&mut p, &v);
        assert!(!r.button(FIRE));
        assert!(p.seen > 0, "kept in mind");
        for _ in 0..MEMORY {
            record(&mut p, &v);
        }
        assert_eq!(p.seen, 0);
    }

    /// Sam, after T21 ("the bots are all still way too difficult"): at
    /// every skill it fires only in its bursts, and rests between them.
    #[test]
    fn it_fires_in_bursts_and_rests_between() {
        for skill in SKILLS {
            let t = skill.traits();
            let mut p = Pilot {
                shoots: true,
                ..Pilot::default()
            };
            // the player straight ahead, level, 10 m off
            let mut v = View {
                route: &LOOP,
                nav: None,
                hunt: None,
                speed: 0,
                frozen: false,
                reach: None,
                tick: 0,
                feet: v3(int(8), 0, int(-16)),
                eye: v3(int(8), ONE * 165 / 100, int(-16)),
                yaw: 0,
                pitch: 0,
                on_ground: true,
                chest: Some(v3(int(8), ONE * 165 / 100, int(-26))),
                skill,
                chest_velocity: V3::ZERO,
                hit_from: None,
                health: 100,
                cover: None,
                weapon: Weapon::Pistol,
                owned: crate::arms::ROUND_OWNED,
                pickups: &[],
            };
            let mut shots = 0;
            for tick in 0..(t.reaction + 3 * BURST_CYCLE) as u64 {
                v.tick = tick;
                if record(&mut p, &v).button(FIRE) {
                    shots += 1;
                    let into = (p.seen - t.reaction) % BURST_CYCLE;
                    assert!(into < t.burst, "{skill:?} fired {into} ticks into a cycle");
                }
            }
            assert!(shots > 0, "{skill:?} never fired");
        }
    }

    #[test]
    fn a_player_below_makes_it_look_down_and_one_above_up() {
        let base = |chest_y: Fx| View {
            route: &LOOP,
            nav: None,
            hunt: None,
            speed: 0,
            frozen: false,
            reach: None,
            tick: 0,
            feet: v3(int(8), 0, int(-16)),
            eye: v3(int(8), ONE * 165 / 100, int(-16)),
            yaw: 0,
            pitch: 0,
            on_ground: true,
            chest: Some(v3(int(8), chest_y, int(-26))),
            skill: Skill::Normal,
            chest_velocity: V3::ZERO,
            hit_from: None,
            health: 100,
            cover: None,
            weapon: Weapon::Pistol,
            owned: crate::arms::ROUND_OWNED,
            pickups: &[],
        };
        let mut p = Pilot {
            shoots: true,
            ..Pilot::default()
        };
        // down: a pitch below level is a larger mouse movement down, dy > 0
        assert!(record(&mut p, &base(0)).dy > 0);
        assert!(record(&mut p, &base(int(5))).dy < 0);
    }

    /// T20: a bot at (8, −16) facing −z, the target 10 m ahead, unhurt.
    fn fighting(skill: Skill) -> View<'static> {
        View {
            route: &LOOP,
            nav: None,
            hunt: Some(v3(int(8), 0, int(-26))),
            speed: 0,
            frozen: false,
            reach: None,
            tick: 0,
            feet: v3(int(8), 0, int(-16)),
            eye: v3(int(8), ONE * 165 / 100, int(-16)),
            yaw: 0,
            pitch: 0,
            on_ground: true,
            chest: Some(v3(int(8), ONE * 165 / 100, int(-26))),
            skill,
            chest_velocity: V3::ZERO,
            hit_from: None,
            health: 100,
            cover: None,
            weapon: Weapon::Pistol,
            owned: crate::arms::ROUND_OWNED,
            pickups: &[],
        }
    }

    fn shooting() -> Pilot {
        Pilot {
            shoots: true,
            ..Pilot::default()
        }
    }

    #[test]
    fn each_weapon_suits_its_distance() {
        let near = int(3);
        let far = int(40);
        assert!(suits(Weapon::Shotgun, near) > suits(Weapon::Rifle, near));
        assert!(suits(Weapon::Rifle, far) > 0);
        assert_eq!(suits(Weapon::Shotgun, far), 0, "the spread strays past a body");
        assert_eq!(
            suits(Weapon::Navaja, int(10)),
            0,
            "a blade suits only what is near"
        );
        assert!(suits(Weapon::Navaja, ONE) > 0);
    }

    #[test]
    fn up_close_it_takes_the_shotgun_it_owns_and_waits_before_switching_again() {
        let mut p = shooting();
        let mut v = fighting(Skill::Normal);
        v.chest = Some(v3(int(8), ONE * 165 / 100, int(-19)));
        v.owned |= 1 << Weapon::Shotgun as u16;
        let r = record(&mut p, &v);
        assert!(r.key(SLOTS[Weapon::Shotgun as usize]), "{:b}", r.keys);
        // not again at once, so the key is a fresh press when it comes
        assert!(!record(&mut p, &v).key(SLOTS[Weapon::Shotgun as usize]));
        // an Easy bot keeps what it holds
        v.skill = Skill::Easy;
        assert!(!record(&mut shooting(), &v).key(SLOTS[Weapon::Shotgun as usize]));
    }

    #[test]
    fn out_of_a_fight_it_goes_for_a_weapon_it_lacks() {
        let lying = [crate::world::Pickup {
            at: v3(int(16), 0, int(-16)),
            weapon: Weapon::Rifle,
            taken: false,
        }];
        let mut v = fighting(Skill::Normal);
        v.chest = None;
        v.pickups = &lying;
        // the hunt is ahead (−z), the rifle to the right (+x)
        let r = record(&mut shooting(), &v);
        assert!(r.key(RIGHT) && !r.key(FORWARD), "{:b}", r.keys);
        // one it owns already, it passes by
        v.owned |= 1 << Weapon::Rifle as u16;
        assert!(record(&mut shooting(), &v).key(FORWARD));
    }

    #[test]
    fn hit_it_dodges_across_the_shooters_line_and_slides_when_fast() {
        let mut p = shooting();
        let mut v = fighting(Skill::Normal);
        v.hit_from = v.chest;
        v.speed = int(8);
        let r = record(&mut p, &v);
        assert!(p.dodge > 0);
        assert!(r.key(LEFT) || r.key(RIGHT), "across the line: {:b}", r.keys);
        assert!(r.key(CROUCH) && !r.key(JUMP), "a slide: {:b}", r.keys);
        // in the air it lets jump go and presses it again, for a wall jump
        v.hit_from = None;
        v.on_ground = false;
        let jumps: Vec<bool> = (0..4)
            .map(|t| {
                v.tick = t;
                record(&mut p, &v).key(JUMP)
            })
            .collect();
        assert!(jumps.contains(&true) && jumps.contains(&false), "{jumps:?}");
        // an Easy bot does not dodge
        let mut v = fighting(Skill::Easy);
        v.hit_from = v.chest;
        let mut p = shooting();
        record(&mut p, &v);
        assert_eq!(p.dodge, 0);
    }

    #[test]
    fn hit_when_low_it_falls_back_to_cover_and_not_when_whole() {
        let mut v = fighting(Skill::Normal);
        v.hit_from = v.chest;
        // cover behind it (+z)
        v.cover = Some(v3(int(8), 0, int(0)));
        v.health = 100;
        let mut p = shooting();
        record(&mut p, &v);
        assert_eq!(p.falling_back, 0);
        v.health = 20;
        let r = record(&mut p, &v);
        assert!(p.falling_back > 0);
        assert!(r.key(BACK), "away to cover: {:b}", r.keys);
    }

    #[test]
    fn it_aims_ahead_of_a_moving_target_by_its_skill() {
        // the target 10 m ahead, running left (−x) at 8 m/s
        let turned = |skill: Skill| {
            let mut v = fighting(skill);
            v.chest_velocity = v3(-int(8), 0, 0);
            v.tick = 0;
            record(&mut shooting(), &v).dx
        };
        // turning left is a smaller mouse movement (dx < 0)
        assert_eq!(turned(Skill::Easy), 0, "no lead");
        assert!(turned(Skill::Normal) < 0);
        assert!(turned(Skill::Hard) < turned(Skill::Normal), "more lead");
    }

    #[test]
    fn a_bot_that_only_moves_never_fires() {
        let mut p = Pilot::default();
        let v = View {
            route: &LOOP,
            nav: None,
            hunt: None,
            speed: 0,
            frozen: false,
            reach: None,
            tick: 10,
            feet: v3(int(8), 0, int(-16)),
            eye: v3(int(8), ONE * 165 / 100, int(-16)),
            yaw: 0,
            pitch: 0,
            on_ground: true,
            chest: Some(v3(int(8), ONE, int(-30))),
            skill: Skill::Normal,
            chest_velocity: V3::ZERO,
            hit_from: None,
            health: 100,
            cover: None,
            weapon: Weapon::Pistol,
            owned: crate::arms::ROUND_OWNED,
            pickups: &[],
        };
        for _ in 0..100 {
            assert!(!record(&mut p, &v).button(FIRE));
        }
    }
}
