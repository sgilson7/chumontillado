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

use crate::camera::LOOK_UNITS_PER_100_PX;
use crate::fx::{self, int, Fx, ONE};
use crate::geom::{v3, yaw_of, V3};
use crate::input::{Input, BACK, FIRE, FORWARD, JUMP, LEFT, RIGHT};

/// The bot's loop in the practice room, clear of the blocks.
pub const LOOP: [(i32, i32); 4] = [(8, -28), (16, -28), (16, -16), (6, -16)];
/// A waypoint is reached within this distance (m).
const REACHED: Fx = ONE * 15 / 10;
/// The most the bot turns in a tick, and the wobble on its aim (angle units).
pub const TURN_RATE: i32 = fx::deg(8);
pub const WOBBLE: i32 = fx::deg(3);
/// Ticks between first seeing the player and the first shot.
pub const REACTION: i32 = 24;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pilot {
    pub waypoint: usize,
    /// ticks it has seen the player for
    pub seen: i32,
    /// fires back, or only moves
    pub shoots: bool,
    /// T19: where it heads for on its way, ticks until it finds its way
    /// again, ticks it has stood still, ticks left turned aside
    pub steer: V3,
    pub plan: i32,
    pub still: i32,
    pub aside: i32,
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
}

/// A turn of `d` angle units as the mouse movement (hundredths of a pixel)
/// that gives it at a look speed of 100% (`camera::Aim::turned`, inverted:
/// moving right turns to a smaller yaw, down to a smaller pitch).
pub fn mouse_for(d: i32) -> i32 {
    -((d as i64 * 10_000) / LOOK_UNITS_PER_100_PX) as i32
}

/// The heading change toward `want`, at most `TURN_RATE`.
fn toward(from: i32, want: i32) -> i32 {
    fx::wrap(want.wrapping_sub(from)).clamp(-TURN_RATE, TURN_RATE)
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
    let goal = match v.hunt {
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
    let target = if p.shoots { v.chest } else { None };
    if let (Some(c), Some(_), None) = (target, v.hunt, v.reach) {
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
    let go = go.wrapping_add(aside);
    p.seen = if target.is_some() { p.seen + 1 } else { 0 };
    let (want_yaw, want_pitch) = match target {
        Some(c) => {
            let to = c.sub(v.eye);
            // a slow wobble on the aim, the same on every browser
            let wob = fx::mul(fx::sin((v.tick as i32).wrapping_mul(fx::deg(7))), WOBBLE);
            // atan2 gives 0 to a whole turn; a pitch is signed (R5.3)
            (
                yaw_of(to).wrapping_add(wob),
                fx::wrap(fx::atan2(to.y, to.len_xz())),
            )
        }
        None => (go, 0),
    };
    let dyaw = toward(v.yaw, want_yaw);
    let dpitch = (want_pitch - v.pitch).clamp(-TURN_RATE, TURN_RATE);
    let face = v.yaw.wrapping_add(dyaw);
    let mut keys = keys_toward(face, go) | JUMP;
    if !v.on_ground && target.is_none() {
        // in the air, W alone steers it round the loop (T2)
        keys = FORWARD | JUMP;
    }
    let within = if v.reach.is_some() {
        SWING_WITHIN
    } else {
        FIRE_WITHIN
    };
    let aimed = target.is_some()
        && fx::wrap(want_yaw.wrapping_sub(face)).abs() <= within
        && (want_pitch - (v.pitch + dpitch)).abs() <= within
        // T19: a blade swings only in reach
        && v.reach.is_none_or(|r| target.is_some_and(|c| c.sub(v.eye).len() <= r + SWING_SLACK));
    // a semi-automatic needs the button let go between shots
    let fire = aimed && p.seen >= REACTION && v.tick.is_multiple_of(2);
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
        // a player out of sight: no shot, and the count starts again
        v.chest = None;
        let r = record(&mut p, &v);
        assert!(!r.button(FIRE));
        assert_eq!(p.seen, 0);
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
        };
        let mut p = Pilot {
            shoots: true,
            ..Pilot::default()
        };
        // down: a pitch below level is a larger mouse movement down, dy > 0
        assert!(record(&mut p, &base(0)).dy > 0);
        assert!(record(&mut p, &base(int(5))).dy < 0);
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
        };
        for _ in 0..100 {
            assert!(!record(&mut p, &v).button(FIRE));
        }
    }
}
