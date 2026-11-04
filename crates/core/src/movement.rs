//! Moving a player: Source's CGameMovement as CSSurf ports it (its
//! `movement.rs`, from the 2013 SDK's gamemovement.cpp), in Q16.16 metres
//! with Y up (R1.3), and the journal's moves on top (CS-Q8: A, the Mockup
//! page's table): a slide, a slide jump, three wall jumps a flight, and a
//! slam. The details that make surfing and hopping work are Source's:
//!
//! - a face is ground only when its normal's y is at least 0.7; a surf ramp
//!   is steeper, so on a ramp the air rules hold;
//! - in the air the wish speed is capped (`AIR_CAP`) before it is compared
//!   with the speed already along the wish, but the acceleration itself
//!   uses the uncapped wish speed, so turning while holding the strafe key
//!   into the turn adds speed;
//! - a face removes only the part of the velocity going into it, and
//!   gravity, half before the move and half after, pulls a surfer along a
//!   ramp;
//! - a jump on the tick of landing skips that tick's friction. Holding jump
//!   hops on every landing (CS-Q9: A).
//!
//! The numbers are `TUNING`'s, one place, so tuning by feel is one edit.

use crate::fx::{self, int, Fx, ONE};
use crate::geom::{facing, right_of, v3, V3};
use crate::input::{Input, BACK, CROUCH, FORWARD, JUMP, LEFT, RIGHT};
use crate::map::Map;
use crate::trace::{trace_box, Scratch, Trace, DIST_EPSILON};

/// The ticks a second; every rate below is per second and is divided by it.
pub const HZ: i32 = 60;

/// Every number the movement uses. The Mockup page's table (2026-11-04)
/// gave the run, jump, air and slide numbers; the rest are Source's,
/// carried from inches to metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// the ground's top speed, and the wish speed in the air (m/s)
    pub run: Fx,
    pub accelerate: Fx,
    pub air_accelerate: Fx,
    /// the most the wish can add along itself in the air (m/s)
    pub air_cap: Fx,
    pub friction: Fx,
    /// friction acts as if at least this fast (m/s)
    pub stop_speed: Fx,
    pub gravity: Fx,
    pub jump: Fx,
    /// no axis of the velocity goes past this (m/s)
    pub max_velocity: Fx,
    /// the highest step walked up without a jump (m)
    pub step: Fx,
    pub duck_speed_pct: i32,
    /// moving up faster than this, a player is never on the ground (m/s)
    pub non_jump_velocity: Fx,
    /// air acceleration steps a tick, the view turning between them (CSSurf
    /// uses 2, as CS2's half-tick rounding does)
    pub air_substeps: i32,
    /// a move that has hit a plane is aimed this far off it next bump (CS2's
    /// fix for snagging on ramp seams)
    pub snag_offset: Fx,
    /// a slide starts at this share of the run speed, or the speed already
    /// held, whichever is more (percent), at least this fast to begin (m/s),
    /// and ends below the second
    pub slide_boost_pct: i32,
    pub slide_from: Fx,
    pub slide_until: Fx,
    pub slide_friction: Fx,
    /// ticks before another slide gives the boost again
    pub slide_rest: i32,
    /// a jump while sliding, or this many ticks after, adds this much along
    /// the way of travel and this much more up (m/s)
    pub slide_jump_ticks: i32,
    pub slide_jump: Fx,
    pub slide_jump_up: Fx,
    /// a wall jump: so far off the wall to find it (m), and out and up (m/s)
    pub wall_reach: Fx,
    pub wall_out: Fx,
    pub wall_up: Fx,
    pub wall_jumps: i32,
    /// a slam: this fast down (m/s), from at least this high (m); a jump
    /// within so many ticks of its landing bounces, up to the cap's height
    pub slam_speed: Fx,
    pub slam_above: Fx,
    pub slam_bounce_ticks: i32,
    pub slam_bounce_pct: i32,
    pub slam_bounce_most: Fx,
}

pub const TUNING: Tuning = Tuning {
    run: int(8),
    accelerate: int(10),
    air_accelerate: int(40),
    air_cap: ONE * 8 / 10,
    friction: int(6),
    stop_speed: ONE * 25 / 10,
    gravity: int(18),
    // T1: twice as high as the Mockup page's 0.87 m: height goes with the
    // square of the speed, so 5.6 × √2 = 7.92 m/s, 1.74 m
    jump: ONE * 792 / 100,
    max_velocity: int(50),
    step: ONE * 4 / 10,
    duck_speed_pct: 34,
    non_jump_velocity: ONE * 35 / 10,
    air_substeps: 2,
    snag_offset: ONE / 1280,
    slide_boost_pct: 140,
    slide_from: int(6),
    slide_until: int(4),
    slide_friction: ONE * 6 / 10,
    // T1: a quarter second, shorter than any jump's flight, so a slide on
    // every landing gets its boost (a second blocked every chained slide)
    slide_rest: 15,
    // T1: 200 ms to jump out of a slide (was 50 ms), +2.5 m/s along (was 1.5)
    // and 1.5 m/s more up
    slide_jump_ticks: 12,
    slide_jump: ONE * 25 / 10,
    slide_jump_up: ONE * 15 / 10,
    wall_reach: ONE / 5,
    wall_out: int(5),
    wall_up: int(6),
    wall_jumps: 3,
    slam_speed: int(30),
    // T1: above a jump's 1.74 m, so crouch on the way down readies a slide
    // instead of slamming (was 1.5 m)
    slam_above: int(3),
    slam_bounce_ticks: 6,
    slam_bounce_pct: 60,
    slam_bounce_most: int(6),
};

/// The player's box: 0.64 m across (the shared envelope, CS-Q12: A), 1.85 m
/// tall standing and 1.15 m crouched, the feet at the origin.
pub const HALF_WIDTH: Fx = ONE * 32 / 100;
pub const STAND_HIGH: Fx = ONE * 185 / 100;
pub const DUCK_HIGH: Fx = ONE * 115 / 100;
pub const EYE_STAND: Fx = ONE * 165 / 100;
pub const EYE_DUCK: Fx = ONE * 95 / 100;
pub const GROUND_NORMAL_Y: Fx = ONE * 7 / 10;
/// A wall to jump off stands within this of upright (its normal's y).
const WALL_NORMAL_Y: Fx = ONE * 3 / 10;
const MAX_CLIP_PLANES: usize = 5;
/// Source makes four bumps a tick; the rampbug fixes use eight, so a seam
/// between two ramp brushes cannot use them all up.
const BUMPS: usize = 8;
/// Two clip planes this close to parallel are one plane (the rampbug).
const SAME_PLANE: Fx = ONE * 999 / 1000;
/// How hard a surfer is pushed off a ramp hit twice in one tick: far too
/// little to feel, enough to clear rounding (6 mm/s).
const PUSH_OFF: Fx = ONE * 6 / 1000;
pub const MINS: V3 = v3(-HALF_WIDTH, 0, -HALF_WIDTH);

/// Everything that moves with a player from tick to tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Body {
    pub origin: V3,
    pub velocity: V3,
    pub on_ground: bool,
    pub ducked: bool,
    /// jump and crouch were held last tick (wall jumps and slams need a
    /// fresh press)
    pub old_jump: bool,
    pub old_crouch: bool,
    pub surface_friction: Fx,
    /// wall jumps used since the ground
    pub wall_jumps: i32,
    pub sliding: bool,
    /// ticks since a slide ended, and since one began (for the boost's rest)
    pub since_slide: i32,
    pub since_slide_began: i32,
    /// a slam's starting height while falling; the ticks since one landed
    pub slam_from: Option<Fx>,
    pub slam_fell: Fx,
    pub since_slam: i32,
}

impl Body {
    pub fn at(origin: V3) -> Body {
        Body {
            origin,
            surface_friction: ONE,
            since_slide: 1_000,
            since_slide_began: 1_000,
            since_slam: 1_000,
            ..Body::default()
        }
    }

    pub fn maxs(&self) -> V3 {
        v3(
            HALF_WIDTH,
            if self.ducked { DUCK_HIGH } else { STAND_HIGH },
            HALF_WIDTH,
        )
    }

    pub fn eye(&self) -> Fx {
        if self.ducked {
            EYE_DUCK
        } else {
            EYE_STAND
        }
    }

    /// Speed across the ground (m/s).
    pub fn speed(&self) -> Fx {
        self.velocity.len_xz()
    }
}

/// What happened in a tick that the HUD, the sound and the tests care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Events {
    pub jumped: bool,
    pub landed: bool,
    pub wall_jumped: bool,
    pub slid: bool,
    pub slammed: bool,
    pub bounced: bool,
    pub slide_jumped: bool,
}

/// The keys of a record as forward and side moves, each `run` or none, the
/// pair no longer than `run` (Source's CheckParameters).
fn moves(t: &Tuning, input: &Input) -> (Fx, Fx) {
    let mut f = 0;
    let mut s = 0;
    if input.key(FORWARD) {
        f += t.run;
    }
    if input.key(BACK) {
        f -= t.run;
    }
    if input.key(RIGHT) {
        s += t.run;
    }
    if input.key(LEFT) {
        s -= t.run;
    }
    if f != 0 && s != 0 {
        // a diagonal: both scaled by 1/√2 so the pair is `run` long
        f = fx::mul(f, ONE * 7071 / 10_000);
        s = fx::mul(s, ONE * 7071 / 10_000);
    }
    (f, s)
}

/// The wish: the flattened view's forward and right times the moves.
fn wish(yaw: i32, f: Fx, s: Fx) -> (V3, Fx) {
    let fwd = facing(yaw, 0);
    let right = right_of(yaw);
    let v = v3(
        fx::mul(fwd.x, f) + fx::mul(right.x, s),
        0,
        fx::mul(fwd.z, f) + fx::mul(right.z, s),
    );
    let speed = v.len();
    if speed == 0 {
        return (V3::ZERO, 0);
    }
    (v.norm(), speed)
}

/// Source's ClipVelocity: remove the part of `v` going into the plane, then
/// make sure nothing still goes into it.
pub fn clip_velocity(v: V3, n: V3) -> V3 {
    let backoff = v.dot(n);
    let mut out = v.sub(n.scale(backoff));
    let adjust = out.dot(n);
    if adjust < 0 {
        out = out.sub(n.scale(adjust));
    }
    out
}

pub struct Mover<'a> {
    pub map: &'a Map,
    pub t: Tuning,
    pub scratch: Scratch,
}

impl<'a> Mover<'a> {
    pub fn new(map: &'a Map) -> Mover<'a> {
        Mover {
            map,
            t: TUNING,
            scratch: Scratch::default(),
        }
    }

    fn dt(&self) -> Fx {
        ONE / HZ
    }

    pub fn trace(&mut self, b: &Body, start: V3, end: V3) -> Trace {
        trace_box(self.map, &mut self.scratch, start, end, MINS, b.maxs())
    }

    /// One tick of movement. `yaw_from` is last tick's heading: the air's
    /// sub-steps turn from it to `yaw`.
    pub fn tick(&mut self, b: &mut Body, input: &Input, yaw_from: i32, yaw: i32) -> Events {
        let mut ev = Events::default();
        if self.unstick(b) {
            let t = self.trace(b, b.origin, b.origin);
            if t.start_solid {
                b.velocity = V3::ZERO;
                return ev;
            }
        }
        let was_ground = b.on_ground;
        let jump = input.key(JUMP);
        let crouch = input.key(CROUCH);
        let fresh_jump = jump && !b.old_jump;
        let fresh_crouch = crouch && !b.old_crouch;
        b.since_slide = b.since_slide.saturating_add(1);
        b.since_slide_began = b.since_slide_began.saturating_add(1);
        b.since_slam = b.since_slam.saturating_add(1);

        self.duck(b, crouch);
        if !b.on_ground
            && fresh_crouch
            && b.slam_from.is_none()
            && self.height_above_ground(b) >= self.t.slam_above
        {
            // the slam: straight down, the speed across the ground kept
            b.velocity.y = -self.t.slam_speed;
            b.slam_from = Some(b.origin.y);
            ev.slammed = true;
        }
        self.slide(b, crouch, &mut ev);

        let (mut f, mut s) = moves(&self.t, input);
        if b.ducked && b.on_ground && !b.sliding {
            f = f * self.t.duck_speed_pct / 100;
            s = s * self.t.duck_speed_pct / 100;
        }

        self.start_gravity(b);
        if jump {
            if b.on_ground {
                self.jump(b, &mut ev);
            } else if fresh_jump && b.wall_jumps < self.t.wall_jumps {
                self.wall_jump(b, &mut ev);
            }
        }
        if b.on_ground {
            b.velocity.y = 0;
            self.friction(b);
        }
        self.check_velocity(b);

        if b.on_ground {
            self.walk_move(b, yaw, f, s);
        } else {
            self.air_move(b, yaw_from, yaw, f, s);
        }
        self.categorize_position(b);
        self.check_velocity(b);
        if !b.on_ground {
            self.finish_gravity(b);
        }
        if b.on_ground {
            b.velocity.y = 0;
            b.wall_jumps = 0;
            if !was_ground {
                ev.landed = true;
                if let Some(from) = b.slam_from.take() {
                    b.slam_fell = (from - b.origin.y).max(0);
                    b.since_slam = 0;
                }
            }
        }
        b.old_jump = jump;
        b.old_crouch = crouch;
        ev
    }

    fn duck(&mut self, b: &mut Body, want: bool) {
        let tuck = STAND_HIGH - DUCK_HIGH;
        if want && !b.ducked {
            // in the air the feet tuck up; on the ground the head lowers
            if !b.on_ground {
                b.origin.y += tuck;
            }
            b.ducked = true;
            self.categorize_position(b);
        } else if !want && b.ducked {
            let mut stand = *b;
            stand.ducked = false;
            if !b.on_ground {
                stand.origin.y -= tuck;
            }
            let t = self.trace(&stand, stand.origin, stand.origin);
            if !t.start_solid {
                *b = stand;
                self.categorize_position(b);
            }
        }
    }

    /// A slide begins on the ground when crouch is held at speed, and ends
    /// when it is let go, when the speed falls away, or in the air.
    fn slide(&mut self, b: &mut Body, crouch: bool, ev: &mut Events) {
        if b.sliding && (!crouch || !b.on_ground || b.speed() < self.t.slide_until) {
            b.sliding = false;
            b.since_slide = 0;
        }
        if !b.sliding && crouch && b.on_ground && b.speed() >= self.t.slide_from {
            b.sliding = true;
            ev.slid = true;
            if b.since_slide_began >= self.t.slide_rest {
                let boost = self.t.run * self.t.slide_boost_pct / 100;
                let sp = b.speed();
                if sp < boost {
                    let k = fx::div(boost, sp);
                    b.velocity.x = fx::mul(b.velocity.x, k);
                    b.velocity.z = fx::mul(b.velocity.z, k);
                }
            }
            b.since_slide_began = 0;
        }
    }

    fn height_above_ground(&mut self, b: &Body) -> Fx {
        let t = self.trace(b, b.origin, b.origin.sub(v3(0, int(64), 0)));
        b.origin.y - t.end.y
    }

    fn start_gravity(&self, b: &mut Body) {
        b.velocity.y -= fx::mul(self.t.gravity, self.dt()) / 2;
        self.check_velocity(b);
    }

    fn finish_gravity(&self, b: &mut Body) {
        b.velocity.y -= fx::mul(self.t.gravity, self.dt()) / 2;
        self.check_velocity(b);
    }

    fn check_velocity(&self, b: &mut Body) {
        let m = self.t.max_velocity;
        b.velocity = v3(
            b.velocity.x.clamp(-m, m),
            b.velocity.y.clamp(-m, m),
            b.velocity.z.clamp(-m, m),
        );
    }

    /// A jump from the ground (held jump hops on every landing). A slide or
    /// its last few ticks add a push along the way of travel; a slam's
    /// landing just before turns the fall into height.
    fn jump(&mut self, b: &mut Body, ev: &mut Events) {
        b.on_ground = false;
        let mut up = self.t.jump;
        if b.since_slam <= self.t.slam_bounce_ticks && b.slam_fell > 0 {
            // v = √(2 g h) for a share of the height fallen, capped
            let h = fx::mul(b.slam_fell, ONE * self.t.slam_bounce_pct / 100).min(self.t.slam_bounce_most);
            let v = fx::sqrt(2 * fx::mul(self.t.gravity, h));
            if v > up {
                up = v;
                ev.bounced = true;
            }
            b.slam_fell = 0;
        }
        if b.sliding || b.since_slide <= self.t.slide_jump_ticks {
            up += self.t.slide_jump_up;
            ev.slide_jumped = true;
            let sp = b.speed();
            if sp > 0 {
                let k = fx::div(sp + self.t.slide_jump, sp);
                b.velocity.x = fx::mul(b.velocity.x, k);
                b.velocity.z = fx::mul(b.velocity.z, k);
            }
            b.sliding = false;
            b.since_slide = 1_000;
        }
        if b.ducked {
            b.velocity.y = up;
        } else {
            b.velocity.y += up;
        }
        self.finish_gravity(b);
        ev.jumped = true;
    }

    /// A wall jump: the nearest upright face within `wall_reach` across the
    /// ground. What went into the wall is removed, a push out and up is
    /// added, and the speed along the wall is kept.
    fn wall_jump(&mut self, b: &mut Body, ev: &mut Events) {
        let r = self.t.wall_reach;
        let mut best: Option<(Fx, V3)> = None;
        for k in 0..8 {
            let a = fx::TURN * k / 8;
            let d = v3(fx::mul(fx::cos(a), r), 0, fx::mul(fx::sin(a), r));
            let t = self.trace(b, b.origin, b.origin.add(d));
            if t.hit() && t.normal.y.abs() <= WALL_NORMAL_Y && best.is_none_or(|(f, _)| t.fraction < f) {
                best = Some((t.fraction, t.normal));
            }
        }
        let Some((_, n)) = best else {
            return;
        };
        let flat = v3(n.x, 0, n.z).norm();
        let into = b.velocity.dot(flat);
        if into < 0 {
            b.velocity = b.velocity.sub(flat.scale(into));
        }
        b.velocity = b.velocity.add(flat.scale(self.t.wall_out));
        b.velocity.y = b.velocity.y.max(self.t.wall_up);
        b.wall_jumps += 1;
        b.slam_from = None;
        ev.wall_jumped = true;
    }

    fn friction(&self, b: &mut Body) {
        let speed = b.velocity.len();
        if speed < ONE / 400 {
            return;
        }
        let friction = if b.sliding {
            self.t.slide_friction
        } else {
            self.t.friction
        };
        let friction = fx::mul(friction, b.surface_friction);
        let control = speed.max(self.t.stop_speed);
        let drop = fx::mul(fx::mul(control, friction), self.dt());
        let new = (speed - drop).max(0);
        if new != speed {
            b.velocity = b.velocity.scale(fx::div(new, speed));
        }
    }

    fn accelerate(&self, b: &mut Body, dir: V3, speed: Fx, accel: Fx) {
        let current = b.velocity.dot(dir);
        let add = speed - current;
        if add <= 0 {
            return;
        }
        let a = fx::mul(fx::mul(fx::mul(accel, self.dt()), speed), b.surface_friction).min(add);
        b.velocity = b.velocity.add(dir.scale(a));
    }

    /// The heart of surfing: the cap applies to the comparison, not to the
    /// acceleration.
    pub fn air_accelerate(&self, b: &mut Body, dir: V3, speed: Fx, dt: Fx) {
        let capped = speed.min(self.t.air_cap);
        let current = b.velocity.dot(dir);
        let add = capped - current;
        if add <= 0 {
            return;
        }
        let a = fx::mul(
            fx::mul(fx::mul(self.t.air_accelerate, speed), dt),
            b.surface_friction,
        )
        .min(add);
        b.velocity = b.velocity.add(dir.scale(a));
    }

    /// Air acceleration in sub-steps, the view turning from last tick's yaw
    /// to this tick's, then one move.
    fn air_move(&mut self, b: &mut Body, yaw_from: i32, yaw: i32, f: Fx, s: Fx) {
        let n = self.t.air_substeps.max(1);
        let turn = fx::wrap(yaw.wrapping_sub(yaw_from));
        let dt = self.dt() / n;
        for k in 1..=n {
            let y = yaw_from.wrapping_add(turn * k / n);
            let (dir, speed) = wish(y, f, s);
            if speed > 0 {
                self.air_accelerate(b, dir, speed, dt);
            }
        }
        self.try_move(b);
    }

    fn walk_move(&mut self, b: &mut Body, yaw: i32, f: Fx, s: Fx) {
        let (dir, speed) = wish(yaw, f, s);
        b.velocity.y = 0;
        if !b.sliding && speed > 0 {
            self.accelerate(b, dir, speed, self.t.accelerate);
        }
        b.velocity.y = 0;
        if b.velocity.len() < ONE / 100 {
            b.velocity = V3::ZERO;
            return;
        }
        let mut dest = b.origin.add(b.velocity.scale(self.dt()));
        dest.y = b.origin.y;
        let t = self.trace(b, b.origin, dest);
        if !t.hit() {
            b.origin = t.end;
            self.stay_on_ground(b);
            return;
        }
        self.step_move(b);
        self.stay_on_ground(b);
    }

    fn step_move(&mut self, b: &mut Body) {
        let (pos, vel) = (b.origin, b.velocity);
        self.try_move(b);
        let (down_pos, down_vel) = (b.origin, b.velocity);
        b.origin = pos;
        b.velocity = vel;
        let up = pos.add(v3(0, self.t.step + DIST_EPSILON, 0));
        let t = self.trace(b, b.origin, up);
        if !t.start_solid && !t.all_solid {
            b.origin = t.end;
        }
        self.try_move(b);
        let down = b.origin.sub(v3(0, self.t.step + DIST_EPSILON, 0));
        let t = self.trace(b, b.origin, down);
        if t.normal.y < GROUND_NORMAL_Y {
            b.origin = down_pos;
            b.velocity = down_vel;
            return;
        }
        if !t.start_solid && !t.all_solid {
            b.origin = t.end;
        }
        let flat = |p: V3| v3(p.x - pos.x, 0, p.z - pos.z).len();
        if flat(down_pos) > flat(b.origin) {
            b.origin = down_pos;
            b.velocity = down_vel;
        } else {
            b.velocity.y = down_vel.y;
        }
    }

    fn stay_on_ground(&mut self, b: &mut Body) {
        let start = b.origin.add(v3(0, ONE / 50, 0));
        let end = b.origin.sub(v3(0, self.t.step, 0));
        let t = self.trace(b, b.origin, start);
        let start = t.end;
        let t = self.trace(b, start, end);
        if t.fraction > 0 && t.hit() && !t.start_solid && t.normal.y >= GROUND_NORMAL_Y {
            b.origin = t.end;
        }
    }

    /// Source's TryPlayerMove: move, and on each hit clip the velocity along
    /// the plane.
    pub fn try_move(&mut self, b: &mut Body) {
        let primal = b.velocity;
        let mut original = b.velocity;
        let mut planes = [V3::ZERO; MAX_CLIP_PLANES];
        let mut count = 0usize;
        let mut moved = 0;
        let mut time_left = self.dt();
        for _ in 0..BUMPS {
            if b.velocity == V3::ZERO {
                break;
            }
            let mut end = b.origin.add(b.velocity.scale(time_left));
            if count > 0 {
                end = end.add(planes[count - 1].scale(self.t.snag_offset));
            }
            let t = self.trace(b, b.origin, end);
            moved += t.fraction;
            if t.all_solid {
                // keep the velocity; `unstick` moves the box out next tick
                return;
            }
            if t.fraction > 0 {
                if !t.hit() {
                    let stuck = self.trace(b, t.end, t.end);
                    if stuck.start_solid || stuck.hit() {
                        break;
                    }
                }
                b.origin = t.end;
                original = b.velocity;
                count = 0;
            }
            if !t.hit() {
                break;
            }
            time_left -= fx::mul(time_left, t.fraction);
            if count >= MAX_CLIP_PLANES {
                b.velocity = V3::ZERO;
                break;
            }
            if planes[..count].iter().any(|p| p.dot(t.normal) > SAME_PLANE) {
                // the same ramp again: clip once more and push off by a hair
                b.velocity = clip_velocity(b.velocity, t.normal).add(t.normal.scale(PUSH_OFF));
                original = b.velocity;
                continue;
            }
            planes[count] = t.normal;
            count += 1;
            if count == 1 && !b.on_ground {
                let nv = clip_velocity(original, planes[0]);
                b.velocity = nv;
                original = nv;
            } else {
                let mut i = 0;
                while i < count {
                    b.velocity = clip_velocity(original, planes[i]);
                    let fits = (0..count).all(|j| j == i || b.velocity.dot(planes[j]) >= 0);
                    if fits {
                        break;
                    }
                    i += 1;
                }
                if i == count {
                    // along the crease of two planes
                    if count != 2 {
                        b.velocity = V3::ZERO;
                        break;
                    }
                    let dir = planes[0].cross(planes[1]).norm();
                    b.velocity = dir.scale(dir.dot(b.velocity));
                }
                // back against the way it came: stop, so a corner does not
                // make it shake
                if b.velocity.dot(primal) <= 0 {
                    b.velocity = V3::ZERO;
                    break;
                }
            }
        }
        if moved == 0 && b.on_ground {
            b.velocity = V3::ZERO;
        }
    }

    /// On the ground? A face is ground only at a normal's y of 0.7 or more,
    /// and never while moving up faster than `non_jump_velocity`.
    pub fn categorize_position(&mut self, b: &mut Body) {
        b.surface_friction = ONE;
        let point = b.origin.sub(v3(0, ONE / 50, 0));
        if b.velocity.y > self.t.non_jump_velocity {
            b.on_ground = false;
            return;
        }
        let t = self.trace(b, b.origin, point);
        let mut ground = t.hit() && t.normal.y >= GROUND_NORMAL_Y;
        if !ground {
            ground = self.touch_ground_in_quadrants(b, point);
        }
        if ground {
            if !b.on_ground {
                // landing on a walkable slope: the better of dropping the
                // vertical speed and clipping it along the slope (slopefix)
                let snapped = v3(b.velocity.x, 0, b.velocity.z);
                let n = if t.hit() && t.normal.y >= GROUND_NORMAL_Y {
                    t.normal
                } else {
                    v3(0, ONE, 0)
                };
                let clipped = clip_velocity(b.velocity, n);
                b.velocity = if clipped.len_xz() > snapped.len_xz() {
                    v3(clipped.x, 0, clipped.z)
                } else {
                    snapped
                };
            }
            b.on_ground = true;
        } else {
            b.on_ground = false;
        }
    }

    /// Four half-boxes, for standing on an edge the whole box finds only as
    /// a steep bevel (TryTouchGroundInQuadrants).
    fn touch_ground_in_quadrants(&mut self, b: &Body, end: V3) -> bool {
        let maxs = b.maxs();
        let quads = [
            (v3(0, 0, 0), v3(maxs.x, maxs.y, maxs.z)),
            (MINS, v3(0, maxs.y, 0)),
            (v3(MINS.x, 0, 0), v3(0, maxs.y, maxs.z)),
            (v3(0, 0, MINS.z), v3(maxs.x, maxs.y, 0)),
        ];
        for (lo, hi) in quads {
            let t = trace_box(self.map, &mut self.scratch, b.origin, end, lo, hi);
            if t.hit() && t.normal.y >= GROUND_NORMAL_Y {
                return true;
            }
        }
        false
    }

    /// A box that starts inside a solid is moved out along the smallest of
    /// a few nudges (Source's CheckStuck). Returns whether it was stuck.
    pub fn unstick(&mut self, b: &mut Body) -> bool {
        let t = self.trace(b, b.origin, b.origin);
        if !t.start_solid {
            return false;
        }
        let steps = [ONE / 256, ONE / 64, ONE / 32, ONE / 16, ONE / 8, ONE / 4, ONE / 2];
        let dirs = [
            v3(0, ONE, 0),
            v3(ONE, 0, 0),
            v3(-ONE, 0, 0),
            v3(0, 0, ONE),
            v3(0, 0, -ONE),
            v3(0, -ONE, 0),
        ];
        for s in steps {
            let back = b.velocity.norm().scale(-s);
            for d in std::iter::once(back).chain(dirs.iter().map(|d| d.scale(s))) {
                let p = b.origin.add(d);
                let t = self.trace(b, p, p);
                if !t.start_solid {
                    b.origin = p;
                    return true;
                }
            }
        }
        true
    }
}
