//! The look: light, sky, haze, ink and palette, read from `data/look.json`
//! and turned into the shaders' uniforms. The shaders hold no number but 0
//! and 1 (D3); every constant they use arrives here, by name, in the order of
//! `UNIFORMS`.
//!
//! Protection of the Spheres' lighting (its `scene.frag`): a key light that
//! wraps past the terminator with a gentle ramp, a fill from the sky above
//! and a bounce from the ground below, no hard shadows. Iron Vector's haze:
//! far things fade into the colour of the sky behind them, so the horizon
//! has no seam.
//!
//! The ordered dither (`BAYER`) that M2's detail hand-over uses lives here
//! too: the thresholds are the core's, and the shader compares against them
//! (CW-5, CW-18).

use serde::Deserialize;

use crate::fx::{self, Fx, ONE};
use crate::geom::{v3, V3};

/// A colour in Q16, 0 to `ONE` a channel.
pub type Rgb = [Fx; 3];

/// A colour from 0–255 channels.
pub fn rgb(c: [i32; 3]) -> Rgb {
    c.map(|v| v.clamp(0, 255) * ONE / 255)
}

/// A colour scaled by a percentage.
pub fn shade(c: Rgb, pct: i32) -> Rgb {
    c.map(|v| v * pct / 100)
}

pub fn mix(a: Rgb, b: Rgb, t: Fx) -> Rgb {
    [
        fx::lerp(a[0], b[0], t),
        fx::lerp(a[1], b[1], t),
        fx::lerp(a[2], b[2], t),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Palette {
    /// the practice room's surfaces (M1, `map::Surface`): a lime-washed wine
    /// cellar in the roster's palette
    pub floor: [i32; 3],
    pub wall: [i32; 3],
    pub ramp: [i32; 3],
    pub slope: [i32; 3],
    pub block: [i32; 3],
    pub trim: [i32; 3],
    pub shadow: [i32; 3],
}

/// `data/look.json`. Fields starting with `_` there are notes.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Look {
    pub sun_toward: [i32; 3],
    pub sun: [i32; 3],
    pub sun_pct: i32,
    pub sky_fill: [i32; 3],
    pub bounce: [i32; 3],
    pub wrap_pct: [i32; 4],
    pub zenith: [i32; 3],
    pub horizon: [i32; 3],
    pub sun_glow: [i32; 3],
    pub sun_glow_pct: i32,
    pub sky_curve_pct: i32,
    pub sun_glow_power: i32,
    pub sun_disc_tenths_deg: [i32; 2],
    pub haze_start_m: i32,
    pub haze_per_km_pct: i32,
    pub haze_most_pct: i32,
    pub haze_sunward_pct: i32,
    pub halo_power_pct: i32,
    pub ink_haze_pct: i32,
    pub shadow_pct: i32,
    pub ink: [i32; 3],
    pub ink_bias_millionths: [i32; 2],
    pub palette: Palette,
}

/// The 4 × 4 ordered-dither matrix (Bayer's): each cell's rank, 0 to 15.
/// A cell's threshold is (rank + ½) / 16. Indexed [row][column] on screen.
pub const BAYER: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
pub const BAYER_SIZE: i32 = 4;

/// A cell's threshold in Q16.
pub fn threshold(rank: i32) -> Fx {
    (2 * rank + 1) * ONE / 32
}

/// Whether a pixel whose cell has threshold `t` draws a shape with dither
/// fade `f`, the rule the fragment shader's `keep` applies: a fade from 0
/// to `ONE` keeps the cells below it, and a fade from −`ONE` to just under 0
/// keeps the cells at or above `f + ONE`. A fade of `ONE` keeps every cell
/// and 0 keeps none.
pub fn keeps(f: Fx, t: Fx) -> bool {
    if f >= 0 {
        t < f
    } else {
        t >= f + ONE
    }
}

/// The two fades of a detail hand-over at share `t` (0 to `ONE`) of the way
/// through: the level coming in, and the level going out. Between them they
/// draw every pixel exactly once (CW-18), so a level never pops.
pub fn hand_over(t: Fx) -> (Fx, Fx) {
    let t = t.clamp(0, ONE);
    (t, t - ONE)
}

/// The uniforms the page uploads once, in the order `Look::uniforms` writes
/// them: name, the WebGL call that sets it, and how many floats it takes.
/// The page passes a matrix's call a `false` (no transpose) before the values.
pub const UNIFORMS: &[(&str, &str, usize)] = &[
    ("uQ", "uniform1fv", 1),
    ("uLightDir", "uniform3fv", 3),
    ("uSun", "uniform3fv", 3),
    ("uSkyFill", "uniform3fv", 3),
    ("uBounce", "uniform3fv", 3),
    ("uHorizon", "uniform3fv", 3),
    ("uZenith", "uniform3fv", 3),
    ("uSunGlow", "uniform3fv", 3),
    ("uHazeShape", "uniform4fv", 4),
    ("uWrap", "uniform4fv", 4),
    ("uSky", "uniform4fv", 4),
    ("uK", "uniform4fv", 4),
    ("uBias", "uniform2fv", 2),
    ("uBayer", "uniformMatrix4fv", 16),
    ("uBayerSize", "uniform1fv", 1),
];

impl Look {
    pub fn parse(json: &str) -> Result<Look, String> {
        let l: Look =
            serde_json::from_str(json).map_err(|e| format!("The look file could not be read: {e}"))?;
        if l.sun_toward == [0, 0, 0] {
            return Err("The look file's sun has no direction.".into());
        }
        Ok(l)
    }

    /// The unit vector toward the sun.
    pub fn light_dir(&self) -> V3 {
        let [x, y, z] = self.sun_toward;
        v3(x * ONE / 100, y * ONE / 100, z * ONE / 100).norm()
    }

    pub fn ink(&self) -> Rgb {
        rgb(self.ink)
    }

    /// The colour of the sky at the horizon, away from the sun: the clear
    /// colour, under everything the sky dome does not cover.
    pub fn clear_color(&self) -> Vec<i32> {
        rgb(self.horizon).to_vec()
    }

    /// Every uniform's values in Q16, in `UNIFORMS` order. Values too small
    /// for Q16 (the haze per metre, the depth biases) are sent times `ONE`,
    /// and the shader multiplies them by `uQ`.
    pub fn uniforms(&self) -> Vec<i32> {
        let pct = |p: i32| p * ONE / 100;
        let mut out = vec![1];
        let l = self.light_dir();
        out.extend([l.x, l.y, l.z]);
        out.extend(shade(rgb(self.sun), self.sun_pct));
        out.extend(rgb(self.sky_fill));
        out.extend(rgb(self.bounce));
        out.extend(rgb(self.horizon));
        out.extend(rgb(self.zenith));
        out.extend(shade(rgb(self.sun_glow), self.sun_glow_pct));
        // haze: where it starts (m), how thick per metre (× ONE), the most
        // it hides, and how far the sun's glow tints it
        let per_m_times_one = (self.haze_per_km_pct as i64 * ONE as i64 * ONE as i64 / 100_000) as i32;
        out.extend([
            fx::int(self.haze_start_m),
            per_m_times_one,
            pct(self.haze_most_pct),
            pct(self.haze_sunward_pct),
        ]);
        out.extend(self.wrap_pct.map(pct));
        // sky: the gradient's curve, the cosines of the sun disc's inner and
        // outer radius, the sun glow's power
        let tenth = |t: i32| (t as i64 * fx::TURN as i64 / 3600) as i32;
        let [inner, outer] = self.sun_disc_tenths_deg;
        out.extend([
            pct(self.sky_curve_pct),
            fx::cos(tenth(inner)),
            fx::cos(tenth(outer)),
            fx::int(self.sun_glow_power),
        ]);
        // a half, the halo's power, how far ink fades into the haze, the
        // shadow's strength
        out.extend([
            fx::HALF,
            pct(self.halo_power_pct),
            pct(self.ink_haze_pct),
            pct(self.shadow_pct),
        ]);
        // the biases, as share of w, × ONE: millionths × ONE / 1 000 000 × ONE
        let bias = |m: i32| (m as i64 * ONE as i64 * ONE as i64 / 1_000_000) as i32;
        out.extend(self.ink_bias_millionths.map(bias));
        // column by column, as WebGL reads a mat4: uBayer[x][y]
        for x in 0..4 {
            for row in BAYER {
                out.push(threshold(row[x]));
            }
        }
        out.push(fx::int(BAYER_SIZE));
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn look() -> Look {
        Look::parse(include_str!("../../../data/look.json")).expect("data/look.json")
    }

    #[test]
    fn the_uniform_values_fill_the_named_uniforms_exactly() {
        let n: usize = UNIFORMS.iter().map(|u| u.2).sum();
        assert_eq!(look().uniforms().len(), n);
    }

    #[test]
    fn the_haze_and_the_biases_arrive_times_one_for_the_shader_to_scale() {
        let u = look().uniforms();
        let at = |name: &str| -> usize { UNIFORMS.iter().take_while(|x| x.0 != name).map(|x| x.2).sum() };
        // 15% a km = 0.000 15 a metre; times 65 536 = 9.830, and that as
        // Q16 is 0.000 15 × 2³² = 644 245.1
        let density = u[at("uHazeShape") + 1];
        assert_eq!(density, 644_245);
        // one millionth of w, times 65 536 = 0.065 536, as Q16 = 4 294
        assert_eq!(u[at("uBias")], 4294);
        assert_eq!(u[0], 1, "uQ is one Q16 step: 1 / 65 536");
    }

    #[test]
    fn the_dither_hand_over_covers_every_pixel_once_at_every_step() {
        for step in 0..=16 {
            let t = step * ONE / 16;
            let mut kept_a = 0;
            for row in BAYER {
                for rank in row {
                    let th = threshold(rank);
                    let (fa, fb) = hand_over(t);
                    let (a, b) = (keeps(fa, th), keeps(fb, th));
                    assert!(a != b, "fade {t}: cell {rank} drawn by both or neither");
                    kept_a += a as i32;
                }
            }
            // a fade of step/16 keeps exactly step of the 16 cells
            assert_eq!(kept_a, step, "fade {step}/16");
        }
        assert!(
            (0..16).all(|r| keeps(ONE, threshold(r))),
            "full fade keeps every cell"
        );
    }

    #[test]
    fn the_shader_keeps_a_dithered_pixel_by_the_same_rule() {
        let frag = include_str!("../../../data/shaders/scene.frag");
        assert!(
            frag.contains("return f >= 0.0 ? t < f : t >= f + 1.0;"),
            "scene.frag's keep() must match look::keeps"
        );
    }

    #[test]
    fn a_look_file_with_no_sun_or_a_missing_colour_is_refused_by_name() {
        let mut v: serde_json::Value = serde_json::from_str(include_str!("../../../data/look.json")).unwrap();
        v["sun_toward"] = serde_json::json!([0, 0, 0]);
        assert_eq!(
            Look::parse(&v.to_string()).unwrap_err(),
            "The look file's sun has no direction."
        );
        let mut w: serde_json::Value = serde_json::from_str(include_str!("../../../data/look.json")).unwrap();
        w["palette"].as_object_mut().unwrap().remove("ramp");
        assert!(Look::parse(&w.to_string()).unwrap_err().contains("ramp"));
    }
}
