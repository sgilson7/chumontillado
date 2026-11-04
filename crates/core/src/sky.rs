//! `sky`: the sky's share of the haze (M2 World, task W).
//!
//! The light, the sky's gradient and the haze into the sky's colour are
//! Protection of the Spheres' and Iron Vector's, in `look.rs` and
//! `data/look.json`; the sun stays where M0 put it, low in the
//! west-south-west. Two things are added here, in `data/sky.json`, for the
//! draw distance:
//!
//! - **The edge.** The haze grows as 1 − e^(−kd), which never reaches the
//!   sky: 9% of the land's own colour is left at 16 km, enough to see where
//!   the land ends at the far plane. Between `edge_m` the haze is made whole,
//!   so the last land is the sky's colour before the far plane cuts it.
//! - **The air's colour.** Over the middle distance the haze turns toward a
//!   cool blue (`aerial`), so far ridges stand as blue layers against the
//!   warm horizon; by the horizon the haze is the sky's colour again, so the
//!   land still meets the sky with no seam.
//!
//! The values reach `scene.frag` and `water.frag` as uniforms after the
//! look's (`api::numbers`, the shim's `uniforms`), D3.

use serde::Deserialize;

use crate::fx::{self, ONE};
use crate::look::rgb;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkySpec {
    pub edge_m: [i32; 2],
    pub aerial: [i32; 3],
    pub aerial_pct: i32,
    pub aerial_m: [i32; 4],
}

impl SkySpec {
    pub fn parse(json: &str) -> Result<SkySpec, String> {
        let s: SkySpec =
            serde_json::from_str(json).map_err(|e| format!("The sky file could not be read: {e}"))?;
        let [a, b, c, d] = s.aerial_m;
        if s.edge_m[0] >= s.edge_m[1] || !(a < b && b <= c && c < d) {
            return Err("The sky file's distances must rise.".into());
        }
        if s.edge_m[1] as i64 * ONE as i64 > crate::render::FAR as i64 {
            return Err("The sky file's edge must end before the far plane.".into());
        }
        Ok(s)
    }

    pub fn shipped() -> SkySpec {
        SkySpec::parse(include_str!("../../../data/sky.json")).expect("data/sky.json")
    }
}

/// The sky's uniforms, after the look's, in the order `uniform_values`
/// writes them.
pub const UNIFORMS: &[(&str, &str, usize)] = &[
    ("uHazeEdge", "uniform4fv", 4),
    ("uAerial", "uniform3fv", 3),
    ("uAerialK", "uniform4fv", 4),
];

/// The values: where the haze is made whole (m, m), the tint's share and
/// none; the tint; where it rises, is whole, falls and is gone (m).
pub fn uniform_values(s: &SkySpec) -> Vec<i32> {
    let mut out = vec![
        fx::int(s.edge_m[0]),
        fx::int(s.edge_m[1]),
        s.aerial_pct * ONE / 100,
        0,
    ];
    out.extend(rgb(s.aerial));
    out.extend(s.aerial_m.map(fx::int));
    out
}

/// Every uniform the scene's program takes: the look's, then the sky's.
/// (CHUMWORLD added its scatter's bands and its wind's waves; the ring has
/// no grass or wind, R2.6.)
pub fn all_uniforms(look: &crate::look::Look) -> Vec<i32> {
    let mut u = look.uniforms();
    u.extend(uniform_values(&SkySpec::shipped()));
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_haze_is_whole_before_the_far_plane_and_the_tint_is_gone_by_it() {
        let s = SkySpec::shipped();
        // 16 000 m is inside the 16 384 m far plane, and the tint is gone at 15 000 m
        assert!(s.edge_m[1] * ONE < crate::render::FAR);
        assert!(s.aerial_m[3] <= s.edge_m[1]);
        let u = uniform_values(&s);
        let n: usize = UNIFORMS.iter().map(|x| x.2).sum();
        assert_eq!(u.len(), n);
        // 38% is 24 903.68 in Q16, rounded down
        assert_eq!(u[2], 24_903);
        let mut bad: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/sky.json")).unwrap();
        bad["edge_m"] = serde_json::json!([12_500, 17_000]);
        assert_eq!(
            SkySpec::parse(&bad.to_string()).unwrap_err(),
            "The sky file's edge must end before the far plane."
        );
    }

    #[test]
    fn the_shaders_make_the_haze_whole_at_the_edge() {
        let src = include_str!("../../../data/shaders/scene.frag");
        assert!(src.contains("f = max(f, smoothstep(uHazeEdge.x, uHazeEdge.y, d));"));
    }
}
