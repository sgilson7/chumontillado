//! The view: where a player looks, and the first-person camera at the eye.
//!
//! From Chumworld Arena (e78945b): `Aim` turns by the mouse with the part
//! of an angle unit not yet turned kept, so slow movement is not rounded
//! away tick by tick; the view a frame draws adds the mouse movement that
//! has arrived since the last tick (`input::Pending`), so the picture turns
//! on the frame the mouse moves. The arena's over-the-shoulder camera is
//! gone: the shooter is first person only (M0, R1.2).

use crate::fx::{self, deg, Fx};
use crate::geom::{facing, V3};
use crate::input::Mouse;
use crate::render::Camera;

/// How far the view may look down and up: a hair short of straight, so the
/// view's forward never lines up with up.
pub const PITCH_MIN: i32 = -deg(89);
pub const PITCH_MAX: i32 = deg(89);
/// The look speed a reader may set, percent.
pub const LOOK_PCT_MIN: i32 = 10;
pub const LOOK_PCT_MAX: i32 = 400;
/// The field of view across, degrees, a reader may set, and the default
/// (the arena's CA-Q15 range).
pub const FOV_MIN: i32 = 90;
pub const FOV_MAX: i32 = 120;
pub const FOV_START: i32 = 105;
/// Turning: 0.12° a CSS pixel of mouse movement, held as angle units per
/// 100 pixels (12°); the record carries hundredths of a pixel.
pub const LOOK_UNITS_PER_100_PX: i64 = fx::TURN as i64 * 12 / 360;

/// Where the player looks: heading and pitch, and the part of an angle unit
/// the mouse has moved that has not yet turned the view (so the same
/// movement turns the view the same amount however the ticks divide it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Aim {
    pub yaw: i32,
    pub pitch: i32,
    /// ten-thousandths of an angle unit not yet turned, each way
    pub rest: (i32, i32),
    /// the look speed, percent of `LOOK_UNITS_PER_100_PX`
    pub pct: i32,
}

impl Aim {
    pub fn new(yaw: i32) -> Aim {
        Aim {
            yaw,
            pitch: 0,
            rest: (0, 0),
            pct: 100,
        }
    }

    /// The aim after the mouse moves by `m` (right turns right, down looks
    /// down).
    pub fn turned(self, m: Mouse) -> Aim {
        let split = |d: i32, rest: i32| {
            let total = d as i64 * LOOK_UNITS_PER_100_PX * self.pct as i64 / 100 + rest as i64;
            (total.div_euclid(10_000) as i32, total.rem_euclid(10_000) as i32)
        };
        let (dyaw, rx) = split(m.dx, self.rest.0);
        let (dpitch, ry) = split(m.dy, self.rest.1);
        Aim {
            yaw: fx::wrap(self.yaw.wrapping_sub(dyaw)),
            pitch: (self.pitch - dpitch).clamp(PITCH_MIN, PITCH_MAX),
            rest: (rx, ry),
            pct: self.pct,
        }
    }
}

/// The vertical field of view (angle units) that shows `across` degrees
/// across a screen of this aspect (width over height, `Fx`).
pub fn vertical_fov(across: i32, aspect: Fx) -> i32 {
    let half = deg(across) / 2;
    2 * fx::atan2(fx::sin(half), fx::mul(fx::cos(half), aspect.max(1)))
}

/// The camera at `eye` looking along `look`.
pub fn eye_view(eye: V3, look: Aim, fov: i32) -> Camera {
    Camera {
        eye,
        fwd: facing(look.yaw, look.pitch),
        fov,
        roll: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hundred_pixels_right_turns_twelve_degrees_right_however_the_ticks_split_it() {
        let whole = Aim::new(0).turned(Mouse {
            dx: 10_000,
            dy: 0,
            wheel: 0,
        });
        let mut split = Aim::new(0);
        for _ in 0..7 {
            split = split.turned(Mouse {
                dx: 10_000 / 7,
                dy: 0,
                wheel: 0,
            });
        }
        split = split.turned(Mouse {
            dx: 10_000 - 7 * (10_000 / 7),
            dy: 0,
            wheel: 0,
        });
        // 12° = 65 536 * 12 / 360 = 2 184.5 units, so 2 184 turned and the
        // half kept; right is a smaller yaw
        assert_eq!(whole.yaw, fx::wrap(-2184));
        assert_eq!(split.yaw, whole.yaw);
        assert_eq!(split.rest, whole.rest);
    }

    #[test]
    fn the_pitch_stops_short_of_straight_up_and_down() {
        let up = Aim::new(0).turned(Mouse {
            dx: 0,
            dy: -10_000_000,
            wheel: 0,
        });
        let down = Aim::new(0).turned(Mouse {
            dx: 0,
            dy: 10_000_000,
            wheel: 0,
        });
        assert_eq!(up.pitch, deg(89));
        assert_eq!(down.pitch, -deg(89));
    }

    #[test]
    fn a_wider_screen_needs_a_narrower_vertical_view_for_the_same_view_across() {
        let square = vertical_fov(105, fx::ONE);
        let wide = vertical_fov(105, fx::ratio(16, 9));
        // tan(v/2) = tan(52.5°) / aspect: 105° on a square screen; at 16:9,
        // 1.3032 / 1.7778 = 0.7331, atan 36.2°, so 72.5°
        assert!((square - deg(105)).abs() < deg(1), "{square}");
        assert!((wide - deg(725) / 10).abs() < deg(1), "{wide}");
    }
}
