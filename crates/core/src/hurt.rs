//! T18: being shot, as the one shot feels it. The view shakes, the screen's
//! edges flush red, an arc round the crosshair points to the shooter, and
//! the health bar keeps what was lost showing for a moment before it
//! drains. All of it is drawn only: the aim a tick reads never shakes.

use crate::fx::{self, Fx, ONE};
use crate::geom::{right_of, V3};

/// Ticks the red edge and the arc to the shooter last (a second).
pub const HURT: i32 = 60;
/// Ticks the view shakes after a hit (half a second).
pub const SHAKE: i32 = 30;
/// Ticks the bar holds what was lost before it drains, and how much it
/// drains a tick (the whole bar in under a second).
pub const TRAIL_HOLD: i32 = 24;
pub const TRAIL_STEP: i32 = 2;
/// Ticks the bar jolts after a hit.
pub const STRUCK: i32 = 8;
/// The shake at the least and at the most (tenths of a degree), reached
/// at `HARD` damage or more.
pub const SHAKE_LEAST: i32 = 6;
pub const SHAKE_MOST: i32 = 25;
pub const HARD: i32 = 100;
/// The shake's periods across and up (ticks): two that do not divide, so
/// it never settles into a line.
const PERIOD_ACROSS: i32 = 5;
const PERIOD_UP: i32 = 7;
/// The red edge just after a hit, at the least and the most (hundredths of
/// a percent of full).
pub const EDGE_LEAST: i32 = 3500;
pub const EDGE_MOST: i32 = 8500;

/// The last hit, as the one hit feels it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hurt {
    /// ticks left on the red edge and the arc
    pub ticks: i32,
    /// how hard: the damage, plus what is left of a hit still shaking
    pub dmg: i32,
    /// where the shot came from; none for your own blast
    pub from: Option<V3>,
    /// the health the bar still shows, draining down to the health
    pub trail: i32,
    /// ticks before the bar starts to drain
    pub hold: i32,
}

impl Hurt {
    /// A hit of `dmg` taken with `before` health, from `from`.
    pub fn struck(&mut self, before: i32, dmg: i32, from: Option<V3>) {
        let left = self.dmg * self.shaking() / SHAKE;
        self.dmg = (dmg + left).min(HARD);
        self.ticks = HURT;
        self.from = from;
        self.trail = self.trail.max(before);
        self.hold = TRAIL_HOLD;
    }

    /// One tick on, with `health` now.
    pub fn step(&mut self, health: i32) {
        self.ticks = (self.ticks - 1).max(0);
        if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.trail -= TRAIL_STEP;
        }
        self.trail = self.trail.max(health);
    }

    /// Ticks left of the shake.
    fn shaking(&self) -> i32 {
        (self.ticks - (HURT - SHAKE)).max(0)
    }

    /// The bar jolts.
    pub fn jolted(&self) -> bool {
        self.ticks > HURT - STRUCK
    }

    /// How hard, 0 to `ONE`.
    fn hardness(&self) -> Fx {
        ONE * self.dmg.clamp(0, HARD) / HARD
    }

    /// The view's shake `alpha` of the way to the next tick: turns across
    /// and up (angle units). It fades as the square of what is left.
    pub fn shake(&self, alpha: Fx) -> (i32, i32) {
        let left = (fx::int(self.shaking()) - alpha).max(0);
        if left == 0 {
            return (0, 0);
        }
        let fade = fx::mul(left / SHAKE, left / SHAKE);
        let tenths = SHAKE_LEAST + fx::mul(SHAKE_MOST - SHAKE_LEAST, self.hardness());
        let most = fx::mul(fx::deg(tenths) / 10, fade);
        // ticks since the hit, in fixed point
        let t = fx::int(SHAKE) - left;
        (
            fx::mul(most, fx::sin(fx::mul(t, fx::TURN / PERIOD_ACROSS))),
            fx::mul(most, fx::sin(fx::mul(t, fx::TURN / PERIOD_UP) + fx::QUARTER_TURN)),
        )
    }

    /// How strong the red edge is, in hundredths of a percent: strong just
    /// after a hit, harder hits stronger, falling away over `HURT`.
    pub fn edge(&self) -> i32 {
        if self.ticks == 0 {
            return 0;
        }
        let most = EDGE_LEAST + fx::mul(EDGE_MOST - EDGE_LEAST, self.hardness());
        let left = ONE * self.ticks / HURT;
        fx::mul(most, fx::mul(left, left))
    }

    /// Which way the shooter is from `eye` facing `yaw`: clockwise from
    /// straight ahead, in hundredths of a degree. None without a shooter
    /// or once the arc has faded.
    pub fn bearing(&self, eye: V3, yaw: i32) -> Option<i32> {
        let from = self.from.filter(|_| self.ticks > 0)?;
        let d = from.sub(eye);
        let ahead = fx::mul(-fx::sin(yaw), d.x) + fx::mul(-fx::cos(yaw), d.z);
        let right = d.dot(right_of(yaw));
        Some((fx::atan2(right, ahead) as i64 * 36_000 / fx::TURN as i64) as i32)
    }

    /// How strong the arc is, in hundredths of a percent: full, fading in
    /// the last third.
    pub fn arc(&self) -> i32 {
        (10_000 * self.ticks * 3 / HURT).min(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::v3;

    #[test]
    fn a_hit_shakes_the_view_harder_for_more_damage_and_then_stills() {
        let (mut soft, mut hard) = (Hurt::default(), Hurt::default());
        soft.struck(100, 10, None);
        hard.struck(100, 90, None);
        let most = |h: &Hurt| {
            (0..SHAKE)
                .map(|k| {
                    let mut h = *h;
                    for _ in 0..k {
                        h.step(50);
                    }
                    let (a, b) = h.shake(0);
                    a.abs().max(b.abs())
                })
                .max()
                .unwrap()
        };
        assert!(most(&hard) > most(&soft) * 2, "{} {}", most(&hard), most(&soft));
        // under the most, 2.5°
        assert!(most(&hard) <= fx::deg(SHAKE_MOST) / 10);
        for _ in 0..SHAKE {
            hard.step(10);
        }
        assert_eq!(hard.shake(0), (0, 0), "still after half a second");
        assert!(hard.edge() > 0, "the red edge outlasts the shake");
        for _ in SHAKE..HURT {
            hard.step(10);
        }
        assert_eq!((hard.edge(), hard.arc()), (0, 0));
    }

    #[test]
    fn the_arc_points_to_the_shooter_clockwise_from_straight_ahead() {
        let mut h = Hurt::default();
        // facing −z (heading 0): +x is right, +z behind, −x left
        for (x, z, want) in [(0, -10, 0), (10, 0, 9000), (0, 10, 18000), (-10, 0, 27000)] {
            h.struck(100, 10, Some(v3(fx::int(x), 0, fx::int(z))));
            let b = h.bearing(V3::ZERO, 0).unwrap();
            let off = (b - want).rem_euclid(36_000).min((want - b).rem_euclid(36_000));
            assert!(off < 50, "({x}, {z}): {b} for {want}");
        }
        // turned to face +x (heading −90°), a shooter at +x is ahead
        h.struck(100, 10, Some(v3(fx::int(10), 0, 0)));
        let b = h.bearing(V3::ZERO, -fx::QUARTER_TURN).unwrap();
        assert!(!(50..=35_950).contains(&b), "{b}");
        // your own blast has no shooter
        h.struck(100, 10, None);
        assert_eq!(h.bearing(V3::ZERO, 0), None);
    }

    #[test]
    fn the_bar_holds_what_was_lost_then_drains_to_the_health() {
        let mut h = Hurt::default();
        h.step(100);
        assert_eq!(h.trail, 100);
        h.struck(100, 40, None);
        for _ in 0..TRAIL_HOLD {
            h.step(60);
        }
        assert_eq!(h.trail, 100, "held");
        h.step(60);
        assert_eq!(h.trail, 100 - TRAIL_STEP, "draining");
        for _ in 0..HURT {
            h.step(60);
        }
        assert_eq!(h.trail, 60, "down to the health and no further");
        // healed or stood again, the bar follows at once
        h.step(100);
        assert_eq!(h.trail, 100);
    }
}
