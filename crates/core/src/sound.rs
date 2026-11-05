//! Sound (M6): what was heard each tick, and how loud and from where each
//! sound is for the one listening. The core decides what makes a sound and
//! how loud it is; the page synthesizes each voice from its recipe in
//! `data/sounds.json` and plays it. There are no audio files.
//!
//! A tick's sounds are kept in the world (`World::heard`) and cleared at
//! the next tick. Online, a tick run again by rollback gives only the
//! sounds it did not make the first time (`Rollback::take_heard`), so a
//! shot whose record came late is heard, late, and nothing is heard twice.

use serde::Serialize;

use crate::fx::{self, Fx, ONE};
use crate::geom::{right_of, V3};
use crate::weapons::Weapon;

/// A kind of sound; its name is its recipe's key in `data/sounds.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    Shot(Weapon),
    Swing,
    Burst,
    Bounce,
    /// a body struck, heard where it is
    Hurt,
    /// the shooter's confirmation of a hit, a headshot, a kill
    Hit,
    Headshot,
    Kill,
    Jump,
    Land,
    WallJump,
    Slide,
    Slam,
    Pickup,
    /// the round's count, its go, its end
    Count,
    Go,
    RoundEnd,
}

impl Voice {
    pub fn key(self) -> &'static str {
        match self {
            Voice::Shot(g) => match g {
                Weapon::Pistol => "shot_pistol",
                Weapon::Shotgun => "shot_shotgun",
                Weapon::MachinePistol => "shot_machine_pistol",
                Weapon::Rifle => "shot_rifle",
                Weapon::Pincel => "shot_pincel",
                Weapon::Drum | Weapon::Campana => "shot_lob",
                Weapon::Trabuco => "shot_trabuco",
                Weapon::Toro => "shot_toro",
                Weapon::Collage => "shot_collage",
                _ => "swing",
            },
            Voice::Swing => "swing",
            Voice::Burst => "burst",
            Voice::Bounce => "bounce",
            Voice::Hurt => "hurt",
            Voice::Hit => "hit",
            Voice::Headshot => "headshot",
            Voice::Kill => "kill",
            Voice::Jump => "jump",
            Voice::Land => "land",
            Voice::WallJump => "wall_jump",
            Voice::Slide => "slide",
            Voice::Slam => "slam",
            Voice::Pickup => "pickup",
            Voice::Count => "count",
            Voice::Go => "go",
            Voice::RoundEnd => "round_end",
        }
    }

    /// Heard only by the seat it concerns (a hit's confirmation), or by
    /// everyone wherever they are (the round's count), or by everyone by
    /// distance.
    fn reach(self) -> Reach {
        match self {
            Voice::Hit | Voice::Headshot | Voice::Kill => Reach::Own,
            Voice::Count | Voice::Go | Voice::RoundEnd => Reach::All,
            _ => Reach::Near,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Own,
    All,
    Near,
}

/// One sound made in a tick: what, where, and by (or for) which seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub voice: Voice,
    pub at: V3,
    pub by: usize,
}

/// A sound as the page plays it: its recipe, its gain (0 to `ONE`), and
/// where it sits from left (−`ONE`) to right (`ONE`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Play {
    pub voice: &'static str,
    /// thousandths
    pub gain: i32,
    /// thousandths, −1000 left to 1000 right
    pub pan: i32,
}

/// Full loudness within this of the listener (m); it falls to a quarter at
/// `QUARTER` and is not played past `FAR`.
pub const NEAR: Fx = fx::int(3);
pub const QUARTER: Fx = fx::int(24);
pub const FAR: Fx = fx::int(70);

/// How loud a sound `d` m away is, 0 to `ONE`: full to `NEAR`, then
/// falling as k / (k + d − NEAR) with k = (QUARTER − NEAR) / 3, which is a
/// quarter at `QUARTER`; nothing past `FAR`.
pub fn falloff(d: Fx) -> Fx {
    if d <= NEAR {
        return ONE;
    }
    if d >= FAR {
        return 0;
    }
    let k = (QUARTER - NEAR) / 3;
    fx::div(k, k + (d - NEAR))
}

/// The sounds `heard` makes for the seat `me` at `eye` facing `yaw`, at the
/// player's volume (percent).
pub fn for_listener(heard: &[Heard], me: usize, eye: V3, yaw: i32, volume_pct: i32) -> Vec<Play> {
    let right = right_of(yaw);
    heard
        .iter()
        .filter_map(|h| {
            let (gain, pan) = match h.voice.reach() {
                Reach::Own if h.by != me => return None,
                Reach::Own | Reach::All => (ONE, 0),
                Reach::Near if h.by == me => (ONE, 0),
                Reach::Near => {
                    let to = h.at.sub(eye);
                    let d = to.len();
                    let g = falloff(d);
                    if g == 0 {
                        return None;
                    }
                    let pan = if d > 0 { fx::div(to.dot(right), d) } else { 0 };
                    (g, pan)
                }
            };
            // in thousandths, scaled in one step so 70 % is 700
            let gain = (gain as i64 * volume_pct.clamp(0, 100) as i64 * 10 / ONE as i64) as i32;
            (gain > 0).then(|| Play {
                voice: h.voice.key(),
                gain,
                pan: (pan.clamp(-ONE, ONE) as i64 * 1000 / ONE as i64) as i32,
            })
        })
        .collect()
}

/// White noise for the page's sounds: `len` samples from −`ONE` to `ONE`,
/// the same every time (the page has no chance of its own).
pub fn noise(len: usize) -> Vec<Fx> {
    let mut r = crate::rng::Rng::new(0x5eed_50d0);
    (0..len).map(|_| r.between(-ONE, ONE)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::v3;

    fn at(x: i32, z: i32) -> V3 {
        v3(fx::int(x), 0, fx::int(z))
    }

    #[test]
    fn a_sound_is_full_close_by_a_quarter_at_twenty_four_metres_and_silent_far_off() {
        assert_eq!(falloff(fx::int(2)), ONE);
        let q = falloff(QUARTER);
        assert!((q - ONE / 4).abs() < ONE / 100, "{}", fx::decimal(q, 3));
        assert!(falloff(fx::int(10)) > falloff(fx::int(20)));
        assert_eq!(falloff(FAR), 0);
    }

    #[test]
    fn a_shot_to_your_right_sounds_on_the_right_and_a_hit_is_heard_only_by_its_shooter() {
        // facing −z (heading 0), +x is to the right
        let heard = [
            Heard {
                voice: Voice::Shot(Weapon::Pistol),
                at: at(10, 0),
                by: 1,
            },
            Heard {
                voice: Voice::Hit,
                at: at(0, 0),
                by: 1,
            },
            Heard {
                voice: Voice::Count,
                at: at(50, 50),
                by: 0,
            },
        ];
        let plays = for_listener(&heard, 0, V3::ZERO, 0, 100);
        let shot = plays.iter().find(|p| p.voice == "shot_pistol").unwrap();
        assert!(shot.pan > 900, "{}", shot.pan);
        assert!(shot.gain < 1000 && shot.gain > 300, "{}", shot.gain);
        assert!(
            plays.iter().all(|p| p.voice != "hit"),
            "another seat's hit is not yours"
        );
        assert_eq!(plays.iter().find(|p| p.voice == "count").unwrap().gain, 1000);
        // the shooter hears their hit, at full
        let theirs = for_listener(&heard, 1, at(10, 0), 0, 100);
        assert!(theirs.iter().any(|p| p.voice == "hit" && p.gain == 1000));
    }

    #[test]
    fn every_sound_has_a_recipe_and_every_recipe_a_sound() {
        let mut voices = vec![
            Voice::Swing,
            Voice::Burst,
            Voice::Bounce,
            Voice::Hurt,
            Voice::Hit,
            Voice::Headshot,
            Voice::Kill,
            Voice::Jump,
            Voice::Land,
            Voice::WallJump,
            Voice::Slide,
            Voice::Slam,
            Voice::Pickup,
            Voice::Count,
            Voice::Go,
            Voice::RoundEnd,
        ];
        voices.extend(crate::weapons::ALL.iter().map(|&g| Voice::Shot(g)));
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/sounds.json")).expect("sounds.json");
        let recipes = data["voices"].as_object().expect("a voices table");
        let keys: std::collections::BTreeSet<&str> = voices.iter().map(|v| v.key()).collect();
        for k in &keys {
            assert!(recipes.contains_key(*k), "no recipe for {k}");
        }
        for k in recipes.keys() {
            assert!(keys.contains(k.as_str()), "the recipe {k} is never played");
        }
    }

    #[test]
    fn the_volume_scales_every_sound_and_none_is_none() {
        let heard = [Heard {
            voice: Voice::Jump,
            at: V3::ZERO,
            by: 0,
        }];
        assert_eq!(for_listener(&heard, 0, V3::ZERO, 0, 50)[0].gain, 500);
        assert!(for_listener(&heard, 0, V3::ZERO, 0, 0).is_empty());
    }
}
