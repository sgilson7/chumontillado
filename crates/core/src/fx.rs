//! Q16.16 fixed point: the reference default's answer to "no floats in the
//! core" (BUILDER-SPEC.md, D2). Part of the realtime kit
//! (`kits/realtime/README.md`).
//!
//! A value is an `i32` holding the number times 65536. Add and subtract
//! with `+` and `-`; multiply and divide through `mul` and `div`, which
//! widen to `i64` and saturate, so a product never wraps silently. The same
//! inputs give the same bits on every machine, which is what replays,
//! ghosts and lockstep play need.
//!
//! Angles are in turns: `TURN` is a full circle.
//!
//! This project's change to the kit (SECOND-ORDER.md R2.2): `sin` and `cos`
//! are a Taylor series in Q32 rounded to Q16, exact to the last bit at the
//! angles the tests name, instead of Bhaskara's approximation (within 0.002).
//! A rotation basis built from Bhaskara's values is up to 0.4% off unit
//! length, which shows as lighting that wobbles while the camera turns. The
//! helpers after `cos` (`int`, `deg`, `wrap`, `atan2`, `lerp`, ...) come from
//! Long Time Comin's `fx.rs`.

pub type Fx = i32;

pub const SHIFT: u32 = 16;
pub const ONE: Fx = 1 << SHIFT;
pub const HALF: Fx = ONE / 2;

/// A full turn, a half and a quarter, in angle units.
pub const TURN: i32 = 1 << 16;
pub const HALF_TURN: i32 = TURN / 2;
pub const QUARTER_TURN: i32 = TURN / 4;

fn saturate(v: i64) -> Fx {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as Fx
}

/// A whole number as a fixed-point value. Holds -32768 to 32767.
pub const fn from_int(n: i32) -> Fx {
    n << SHIFT
}

/// Rounds toward negative infinity.
pub const fn floor(x: Fx) -> i32 {
    x >> SHIFT
}

/// Rounds half up.
pub const fn round(x: Fx) -> i32 {
    (x + HALF) >> SHIFT
}

/// `num / den` as a fixed-point value. `den` must not be zero.
pub fn ratio(num: i32, den: i32) -> Fx {
    saturate(((num as i64) << SHIFT) / den as i64)
}

pub fn mul(a: Fx, b: Fx) -> Fx {
    saturate((a as i64 * b as i64) >> SHIFT)
}

/// `a / b`. `b` must not be zero; a rule that can divide by zero decides
/// what that means before it calls this.
pub fn div(a: Fx, b: Fx) -> Fx {
    saturate(((a as i64) << SHIFT) / b as i64)
}

fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // Newton's method from a guess above the root
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// The square root of a non-negative value; negative values give 0.
pub fn sqrt(x: Fx) -> Fx {
    if x <= 0 {
        return 0;
    }
    saturate(isqrt((x as u64) << SHIFT) as i64)
}

/// The length of the vector (x, y).
pub fn length(x: Fx, y: Fx) -> Fx {
    let sq = x as i64 * x as i64 + y as i64 * y as i64;
    saturate(isqrt(sq as u64) as i64)
}

/// 2π in Q32 (6.283 185 307 × 2³² = 26 986 075 409.4).
const TWO_PI_Q32: i128 = 26_986_075_409;

/// The sine of an angle in turns, rounded to the nearest Q16 value.
///
/// The angle is folded into a quarter wave, turned into radians in Q32 and
/// summed as a Taylor series to the fifteenth power, which is below one Q16
/// step over the quarter wave. Computed on the magnitude, so `sin(-a)` is
/// exactly `-sin(a)`.
pub fn sin(angle: i32) -> Fx {
    let a = angle.rem_euclid(TURN);
    // fold to [0, QUARTER_TURN] with the sign kept apart
    let (x, sign) = if a <= QUARTER_TURN {
        (a, 1)
    } else if a <= HALF_TURN {
        (HALF_TURN - a, 1)
    } else if a <= HALF_TURN + QUARTER_TURN {
        (a - HALF_TURN, -1)
    } else {
        (TURN - a, -1)
    };
    let r = x as i128 * TWO_PI_Q32 / TURN as i128;
    let r2 = (r * r) >> 32;
    let mut term = r;
    let mut sum = r;
    for k in 1..=7i128 {
        term = -((term * r2) >> 32) / ((2 * k) * (2 * k + 1));
        sum += term;
    }
    let q16 = ((sum + (1 << 15)) >> 16).clamp(0, ONE as i128) as Fx;
    sign * q16
}

pub fn cos(angle: i32) -> Fx {
    sin(angle.wrapping_add(QUARTER_TURN))
}

// ---- helpers after Long Time Comin's fx.rs ----

/// Fraction bits, reported to the page so the shim's divisor has one home.
pub const FRAC_BITS: u32 = SHIFT;

/// A whole number of metres (or anything) as Q16.16; the short name the
/// geometry uses.
pub const fn int(n: i32) -> Fx {
    from_int(n)
}

/// Degrees to angle units.
pub const fn deg(d: i32) -> i32 {
    ((d as i64 * TURN as i64) / 360) as i32
}

/// Wraps an angle into `[-HALF_TURN, HALF_TURN)`, for differences between
/// headings.
pub fn wrap(a: i32) -> i32 {
    (a.wrapping_add(HALF_TURN)).rem_euclid(TURN) - HALF_TURN
}

/// The angle of the vector `(x, y)` from the +x axis, counter-clockwise, in
/// `[0, TURN)`. Accurate to about a quarter of a degree: the arctangent on
/// `[0, 1]` is `π/4·z + 0.273·z·(1 − z)`, a bounded polynomial with no table.
pub fn atan2(y: i32, x: i32) -> i32 {
    if x == 0 && y == 0 {
        return 0;
    }
    let (ax, ay) = (x.unsigned_abs() as i64, y.unsigned_abs() as i64);
    // 2π in Q16 (411 774.8)
    let two_pi = 411_775i64;
    let octant = |z: i64| -> i64 {
        // radians in Q16: π/4 = 51 472, 0.273 = 17 891
        let rad = (51_472 * z + (17_891 * ((z * (ONE as i64 - z)) >> SHIFT))) >> SHIFT;
        rad * TURN as i64 / two_pi
    };
    let a = if ax >= ay {
        octant((ay << SHIFT) / ax)
    } else {
        QUARTER_TURN as i64 - octant((ax << SHIFT) / ay)
    } as i32;
    let full = match (x >= 0, y >= 0) {
        (true, true) => a,
        (false, true) => HALF_TURN - a,
        (false, false) => HALF_TURN + a,
        (true, false) => TURN - a,
    };
    full.rem_euclid(TURN)
}

/// Integer square root, rounded down.
pub fn isqrt_u64(n: u64) -> u64 {
    isqrt(n)
}

/// Linear interpolation, `t` in `[0, ONE]`.
pub fn lerp(a: Fx, b: Fx, t: Fx) -> Fx {
    a + mul(b - a, t)
}

/// Interpolates headings the short way round.
pub fn lerp_angle(a: i32, b: i32, t: Fx) -> i32 {
    a.wrapping_add(mul(wrap(b.wrapping_sub(a)), t))
}

/// A value as decimal text with at most `places` decimals, rounded half
/// away from zero, trailing zeros dropped: 2.2 m as "2.2", 5 m as "5".
pub fn decimal(x: Fx, places: u32) -> String {
    let scale = 10i64.pow(places);
    let v = x as i64 * scale;
    let r = (v.abs() + ONE as i64 / 2) / ONE as i64;
    let (whole, frac) = (r / scale, r % scale);
    let sign = if x < 0 && r != 0 { "-" } else { "" };
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let digits = format!("{frac:0width$}", width = places as usize);
    format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
}

/// Moves `from` toward `to` by at most `step` (`step` not negative).
pub fn approach(from: Fx, to: Fx, step: Fx) -> Fx {
    if from < to {
        from.saturating_add(step).min(to)
    } else {
        from.saturating_sub(step).max(to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_and_quotients_keep_sixteen_fraction_bits() {
        assert_eq!(mul(from_int(3), HALF), ratio(3, 2));
        assert_eq!(mul(ratio(-5, 2), from_int(2)), from_int(-5));
        assert_eq!(div(from_int(7), from_int(2)), ratio(7, 2));
        assert_eq!(div(ONE, from_int(3)), 21845, "1/3 truncates to 21845/65536");
    }

    #[test]
    fn an_overflowing_product_saturates_rather_than_wrapping() {
        assert_eq!(mul(from_int(30000), from_int(30000)), i32::MAX);
        assert_eq!(mul(from_int(-30000), from_int(30000)), i32::MIN);
    }

    #[test]
    fn floor_and_round_go_the_stated_way() {
        assert_eq!(floor(ratio(-1, 2)), -1);
        assert_eq!(round(ratio(-1, 2)), 0);
        assert_eq!(round(ratio(5, 2)), 3);
        assert_eq!(floor(ratio(5, 2)), 2);
    }

    #[test]
    fn square_roots_and_lengths_match_hand_computed_values() {
        assert_eq!(sqrt(from_int(9)), from_int(3));
        assert_eq!(sqrt(ratio(1, 4)), HALF);
        assert_eq!(sqrt(from_int(-4)), 0);
        assert_eq!(length(from_int(3), from_int(4)), from_int(5));
        // sqrt(2) = 1.41421..., times 65536 = 92681.9
        assert_eq!(sqrt(from_int(2)), 92681);
    }

    #[test]
    fn sine_and_cosine_land_within_the_stated_error() {
        let close = |got: Fx, want: Fx| (got - want).abs() <= ONE / 500;
        assert_eq!(sin(0), 0);
        assert_eq!(sin(QUARTER_TURN), ONE);
        assert_eq!(sin(HALF_TURN), 0);
        assert_eq!(sin(-QUARTER_TURN), -ONE);
        assert_eq!(cos(0), ONE);
        // sin(45 degrees) = 0.70711, times 65536 = 46341
        assert!(close(sin(TURN / 8), 46341), "{}", sin(TURN / 8));
        // sin(30 degrees) = 0.5
        assert!(close(sin(TURN / 12), HALF), "{}", sin(TURN / 12));
        assert_eq!(
            sin(TURN + QUARTER_TURN),
            ONE,
            "a whole turn more is the same angle"
        );
    }

    #[test]
    fn sine_is_exact_to_the_last_bit_at_hand_computed_angles() {
        // Angles a turn divides exactly, worked out separately (Python's
        // math.sin in doubles, times 65536): 45° gives 46 340.950, 22.5°
        // 25 079.541 and 11.25° 12 785.439; cos 22.5° gives 60 547.369.
        // 5461 units (TURN / 12 truncated, 29.998°, not 30°) give 32 766.186.
        assert_eq!(sin(TURN / 8), 46341);
        assert_eq!(sin(TURN / 16), 25080);
        assert_eq!(sin(TURN / 32), 12785);
        assert_eq!(cos(TURN / 16), 60547);
        assert_eq!(sin(TURN / 12), 32766);
        assert_eq!(sin(-(TURN / 8)), -46341, "odd: sin(-a) = -sin(a)");
        assert_eq!(cos(HALF_TURN), -ONE);
    }

    #[test]
    fn a_rotation_basis_stays_unit_length_at_every_angle() {
        // sin² + cos² = 1 within two Q16 steps of rounding, all the way round
        for a in (0..TURN).step_by(37) {
            let (s, c) = (sin(a) as i64, cos(a) as i64);
            let sq = s * s + c * c;
            let one = (ONE as i64) * (ONE as i64);
            assert!((sq - one).abs() <= 2 * ONE as i64 + 1, "angle {a}: {sq} vs {one}");
        }
    }

    #[test]
    fn wrap_and_lerp_angle_take_the_short_way_round() {
        assert_eq!(wrap(TURN - 10), -10);
        assert_eq!(wrap(HALF_TURN), -HALF_TURN);
        assert_eq!(wrap(-HALF_TURN - 1), HALF_TURN - 1);
        // from 1024 units short of a turn to 1024 past it (±5.6°) the
        // halfway point is 0, not half a turn
        assert_eq!(wrap(lerp_angle(TURN - 1024, 1024, HALF)), 0);
        assert_eq!(lerp(from_int(2), from_int(4), HALF), from_int(3));
        assert_eq!(approach(from_int(1), from_int(5), from_int(1)), from_int(2));
        assert_eq!(approach(from_int(5), from_int(1), from_int(9)), from_int(1));
    }

    #[test]
    fn decimal_text_rounds_and_drops_trailing_zeros() {
        assert_eq!(decimal(from_int(5), 1), "5");
        // 2.2 is 144 179 Q16, 2.199 997: to one place, 2.2
        assert_eq!(decimal(ONE * 22 / 10, 1), "2.2");
        assert_eq!(decimal(ratio(-3, 4), 2), "-0.75");
        assert_eq!(
            decimal(ratio(1, 8), 2),
            "0.13",
            "0.125 rounds half away from zero"
        );
        assert_eq!(decimal(ratio(1, 1000), 1), "0");
    }

    #[test]
    fn atan2_finds_each_quadrant_within_a_quarter_degree() {
        let close = |got: i32, want: i32| (wrap(got - want)).abs() <= deg(1) / 4;
        assert!(close(atan2(0, ONE), 0));
        assert!(close(atan2(ONE, ONE), deg(45)), "{}", atan2(ONE, ONE));
        assert!(close(atan2(ONE, 0), deg(90)));
        assert!(close(atan2(ONE, -ONE), deg(135)));
        assert!(close(atan2(-ONE, -ONE), deg(225)));
        assert!(close(atan2(-ONE, ONE), deg(315)));
        // tan 30° = 0.57735: (y, x) = (0.57735, 1)
        assert!(close(atan2(37837, ONE), deg(30)), "{}", atan2(37837, ONE));
    }
}
