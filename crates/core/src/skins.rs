//! The four skins (M6, asked for on 2026-11-04): the Roster page's
//! figures, after El Greco, Velázquez, Picasso and Goya, built from the
//! arena's primitives (CS-6: painters' styles only, no painting copied).
//!
//! Every skin's body stays inside the one hit envelope (CS-5, CS-Q12: A):
//! 0.48 m from its upright line and no higher than 1.93 m. That cuts the
//! Hidalgo's tall hat and the Harlequin's bicorne down to what fits (R9.3);
//! only the held weapon reaches out of it. The figure holds its weapon in
//! the pose your own hands show (`hands.rs`), so another player is seen to
//! aim, fire, reload and swing; its legs swing with its speed and tuck in
//! the air. Drawn only; nothing here reaches the world.
//!
//! Colours are in `data/look.json` under `skins`, by skin and part name.

use crate::arms::Arms;
use crate::fx::{self, Fx, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::hands;
use crate::look::{rgb, Look, Rgb};
use crate::movement::TUNING;
use crate::prim::Mesh;
use crate::render::Shapes;
use crate::world::Swinging;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skin {
    Hidalgo,
    Infanta,
    Harlequin,
    Sordo,
    // T16: two more, asked for on 2026-11-04
    Fraile,
    Torero,
}

/// How many skins there are to choose from.
pub const COUNT: usize = 6;

/// The skins in the order the setting steps through them; by default seat
/// 1 wears the Hidalgo, then the Infanta, the Harlequin and El Sordo.
pub const SKINS: [Skin; COUNT] = [
    Skin::Hidalgo,
    Skin::Infanta,
    Skin::Harlequin,
    Skin::Sordo,
    Skin::Fraile,
    Skin::Torero,
];

impl Skin {
    /// Its key in `look.json`'s `skins`.
    pub fn key(self) -> &'static str {
        match self {
            Skin::Hidalgo => "hidalgo",
            Skin::Infanta => "infanta",
            Skin::Harlequin => "harlequin",
            Skin::Sordo => "sordo",
            Skin::Fraile => "fraile",
            Skin::Torero => "torero",
        }
    }

    pub fn of_seat(seat: usize) -> Skin {
        SKINS[seat % 4]
    }
}

/// Which way a part moves with the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limb {
    Still,
    LegL,
    LegR,
}

/// One part of a figure: its mesh, centre and size in centimetres (x to
/// the figure's right, y up, z toward its back: it faces −z), a turn about
/// z then y (degrees), its colour's name, and which leg carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    pub mesh: Mesh,
    pub at: [i32; 3],
    pub size: [i32; 3],
    pub turn: [i32; 2],
    pub colour: &'static str,
    pub limb: Limb,
}

const fn p(mesh: Mesh, at: [i32; 3], size: [i32; 3], colour: &'static str) -> Part {
    Part {
        mesh,
        at,
        size,
        turn: [0, 0],
        colour,
        limb: Limb::Still,
    }
}

const fn turned(mut part: Part, z: i32, y: i32) -> Part {
    part.turn = [z, y];
    part
}

const fn leg(mut part: Part, limb: Limb) -> Part {
    part.limb = limb;
    part
}

/// The most a leg swings in stride, and how far both tuck in the air.
pub const STRIDE: i32 = fx::deg(16);
pub const TUCK: i32 = fx::deg(16);

/// The hip, about which the legs swing (cm).
pub const HIP: i32 = 92;
/// The shoulders (cm): each arm runs from its shoulder to a hand on the
/// weapon.
pub const SHOULDER: [i32; 3] = [22, 140, -2];
/// The arms' thickness (cm).
pub const ARM: i32 = 12;
/// Where the right hand holds the weapon, in the aim's frame from the right
/// shoulder (cm): forward and down.
pub const GRIP: [i32; 3] = [-4, -14, -28];
/// The left hand, further along the weapon.
pub const FORE: [i32; 3] = [-20, -10, -36];

pub fn parts(skin: Skin) -> Vec<Part> {
    use Limb::*;
    use Mesh::*;
    match skin {
        Skin::Hidalgo => vec![
            leg(p(Rod, [-11, 46, 0], [18, 92, 18], "hose"), LegL),
            leg(p(Rod, [11, 46, 0], [18, 92, 18], "hose"), LegR),
            leg(p(Round, [-11, 4, -3], [14, 8, 22], "shoe"), LegL),
            leg(p(Round, [11, 4, -3], [14, 8, 22], "shoe"), LegR),
            p(Bell, [0, 100, 0], [74, 26, 60], "black"),
            p(Sphere, [0, 128, 0], [80, 70, 56], "black"),
            turned(p(Round, [0, 120, -26], [74, 9, 5], "crimson"), 15, 0),
            p(Dot, [0, 128, -29], [4, 4, 3], "ochre"),
            p(Dot, [0, 138, -27], [4, 4, 3], "ochre"),
            p(Dot, [0, 148, -24], [4, 4, 3], "ochre"),
            p(Rod, [0, 160, 0], [66, 10, 62], "bone"),
            p(Sphere, [0, 170, 0], [36, 40, 38], "flesh"),
            p(Round, [0, 155, -15], [12, 12, 8], "beard"),
            p(Dot, [-7, 174, -18], [5, 3, 3], "ink"),
            p(Dot, [7, 174, -18], [5, 3, 3], "ink"),
            // the tall hat, cut to the envelope's top (R9.3)
            p(Rod, [0, 185, 0], [40, 3, 40], "black"),
            p(Rod, [0, 188, 0], [26, 9, 26], "black"),
        ],
        Skin::Infanta => vec![
            p(Round, [-12, 3, -4], [14, 6, 18], "shoe"),
            p(Round, [12, 3, -4], [14, 6, 18], "shoe"),
            p(Bell, [0, 50, 0], [94, 96, 94], "silver"),
            p(Rod, [0, 40, 0], [86, 6, 86], "salmon"),
            p(Bell, [0, 121, 0], [56, 46, 44], "silver"),
            p(Round, [0, 121, -21], [12, 40, 4], "salmon"),
            p(Sphere, [-18, 136, -16], [20, 14, 12], "salmon"),
            p(Sphere, [18, 136, -16], [20, 14, 12], "salmon"),
            p(Sphere, [0, 170, 6], [60, 44, 46], "hair"),
            p(Sphere, [0, 168, -2], [34, 40, 36], "flesh"),
            p(Dot, [-7, 170, -19], [4, 3, 3], "ink"),
            p(Dot, [7, 170, -19], [4, 3, 3], "ink"),
            p(Dot, [-10, 163, -18], [6, 4, 2], "blush"),
            p(Dot, [10, 163, -18], [6, 4, 2], "blush"),
            p(Pyramid, [18, 186, 0], [16, 10, 8], "red"),
        ],
        Skin::Harlequin => vec![
            leg(p(Rod, [-11, 45, 0], [20, 90, 20], "blue"), LegL),
            leg(p(Rod, [11, 45, 0], [20, 90, 20], "ochre"), LegR),
            leg(p(Round, [-11, 4, -3], [14, 8, 22], "black"), LegL),
            leg(p(Round, [11, 4, -3], [14, 8, 22], "black"), LegR),
            p(Sphere, [0, 126, 0], [82, 76, 58], "ochre"),
            // the diamonds, on the tunic's front
            turned(p(Cube, [-20, 142, -23], [12, 12, 3], "red"), 45, 0),
            turned(p(Cube, [0, 142, -27], [12, 12, 3], "blue"), 45, 0),
            turned(p(Cube, [20, 142, -23], [12, 12, 3], "bone"), 45, 0),
            turned(p(Cube, [-20, 126, -27], [12, 12, 3], "black"), 45, 0),
            turned(p(Cube, [0, 126, -30], [12, 12, 3], "red"), 45, 0),
            turned(p(Cube, [20, 126, -27], [12, 12, 3], "blue"), 45, 0),
            turned(p(Cube, [-20, 110, -23], [12, 12, 3], "bone"), 45, 0),
            turned(p(Cube, [0, 110, -27], [12, 12, 3], "black"), 45, 0),
            turned(p(Cube, [20, 110, -23], [12, 12, 3], "red"), 45, 0),
            p(Rod, [0, 164, 0], [62, 8, 48], "bone"),
            p(Sphere, [0, 170, 2], [34, 40, 36], "bone"),
            // the face in two views at once: a front plane, a profile plane
            p(Cube, [-6, 170, -18], [20, 32, 4], "bone"),
            turned(p(Cube, [10, 168, -14], [16, 28, 4], "profile"), 0, -35),
            p(Dot, [-11, 174, -21], [6, 4, 3], "ink"),
            p(Dot, [14, 175, -17], [4, 6, 3], "ink"),
            // the bicorne, cut to the envelope's top (R9.3)
            turned(p(Wedge, [0, 187, 0], [20, 10, 56], "black"), 0, 90),
            p(Dot, [0, 186, -10], [8, 8, 4], "red"),
        ],
        Skin::Sordo => vec![
            leg(p(Rod, [-11, 20, 0], [18, 40, 18], "umber"), LegL),
            leg(p(Rod, [11, 20, 0], [18, 40, 18], "umber"), LegR),
            leg(p(Round, [-11, 4, -3], [14, 8, 22], "shoe"), LegL),
            leg(p(Round, [11, 4, -3], [14, 8, 22], "shoe"), LegR),
            p(Bell, [0, 77, 0], [90, 138, 80], "black"),
            p(Round, [-12, 90, -29], [10, 90, 3], "umber"),
            p(Sphere, [0, 146, 0], [78, 30, 52], "black"),
            p(Sphere, [0, 174, 6], [56, 36, 50], "grey"),
            p(Sphere, [0, 168, -2], [34, 40, 36], "flesh"),
            p(Dot, [-8, 172, -19], [10, 9, 4], "bone"),
            p(Dot, [8, 172, -19], [10, 9, 4], "bone"),
            p(Dot, [-8, 172, -21], [3, 3, 2], "ink"),
            p(Dot, [8, 172, -21], [3, 3, 2], "ink"),
            p(Dot, [0, 157, -18], [9, 11, 3], "mouth"),
        ],
        // T16: after Zurbarán: a white habit to the ground in deep folds, a
        // scapular down its front, a rope belt and its cord, the hood
        // behind a face in shadow
        Skin::Fraile => vec![
            p(Round, [-11, 3, -6], [12, 6, 18], "sandal"),
            p(Round, [11, 3, -6], [12, 6, 18], "sandal"),
            p(Bell, [0, 56, 0], [86, 112, 78], "habit"),
            p(Round, [-20, 52, -30], [6, 96, 3], "fold"),
            p(Round, [20, 52, -30], [6, 96, 3], "fold"),
            p(Round, [0, 48, 34], [10, 90, 3], "fold"),
            p(Sphere, [0, 126, 0], [72, 54, 52], "habit"),
            p(Round, [0, 104, -27], [26, 92, 4], "scapular"),
            p(Rod, [0, 100, 0], [74, 6, 60], "rope"),
            p(Round, [-16, 78, -30], [3, 42, 3], "rope"),
            p(Dot, [-16, 56, -30], [6, 6, 6], "rope"),
            p(Sphere, [0, 150, 6], [62, 22, 50], "habit"),
            p(Sphere, [0, 170, 6], [46, 42, 44], "fold"),
            p(Sphere, [0, 168, -3], [30, 36, 32], "flesh"),
            p(Round, [0, 181, -14], [30, 8, 8], "fold"),
            p(Dot, [-6, 169, -19], [5, 3, 3], "ink"),
            p(Dot, [6, 169, -19], [5, 3, 3], "ink"),
            p(Pyramid, [0, 186, 10], [24, 8, 24], "fold"),
        ],
        // T16: after Zuloaga: a suit of lights in green and gold, breeches
        // to the knee over pink stockings, a red sash, a black two-lobed
        // hat, and the pink cape folded over the left shoulder
        Skin::Torero => vec![
            leg(p(Rod, [-10, 22, 0], [15, 44, 15], "stocking"), LegL),
            leg(p(Rod, [10, 22, 0], [15, 44, 15], "stocking"), LegR),
            leg(p(Rod, [-10, 68, 0], [19, 48, 19], "suit"), LegL),
            leg(p(Rod, [10, 68, 0], [19, 48, 19], "suit"), LegR),
            leg(p(Round, [-19, 68, 0], [3, 46, 5], "gold"), LegL),
            leg(p(Round, [19, 68, 0], [3, 46, 5], "gold"), LegR),
            leg(p(Round, [-10, 3, -3], [12, 6, 22], "black"), LegL),
            leg(p(Round, [10, 3, -3], [12, 6, 22], "black"), LegR),
            p(Rod, [0, 97, 0], [50, 12, 38], "sash"),
            p(Sphere, [0, 125, 0], [62, 50, 44], "suit"),
            p(Round, [-14, 124, -21], [10, 40, 3], "gold"),
            p(Round, [14, 124, -21], [10, 40, 3], "gold"),
            p(Round, [0, 130, -22], [12, 30, 3], "shirt"),
            p(Round, [0, 140, -24], [4, 14, 2], "sash"),
            p(Sphere, [-27, 146, 0], [20, 12, 22], "gold"),
            p(Sphere, [27, 146, 0], [20, 12, 22], "gold"),
            // the cape over the left shoulder, its yellow lining at the hem
            turned(p(Round, [-36, 112, 2], [6, 70, 38], "cape"), 4, 0),
            p(Round, [-37, 78, 2], [7, 6, 38], "lining"),
            p(Rod, [0, 152, 0], [14, 10, 14], "flesh"),
            p(Sphere, [0, 168, -2], [32, 38, 34], "flesh"),
            p(Round, [-15, 166, -6], [3, 12, 6], "hair"),
            p(Round, [15, 166, -6], [3, 12, 6], "hair"),
            p(Dot, [0, 160, 17], [8, 8, 6], "hair"),
            p(Dot, [-7, 171, -19], [5, 3, 3], "ink"),
            p(Dot, [7, 171, -19], [5, 3, 3], "ink"),
            // the montera: a brim and two black lobes
            p(Rod, [0, 181, 2], [36, 4, 30], "black"),
            p(Sphere, [-9, 185, 2], [20, 12, 24], "black"),
            p(Sphere, [9, 185, 2], [20, 12, 24], "black"),
        ],
    }
}

/// A part's colour from the look, or the ink's if the look lacks it (the
/// look test holds every name to be there).
fn colour(look: &Look, skin: Skin, name: &str) -> Rgb {
    look.skins
        .get(skin.key())
        .and_then(|m| m.get(name))
        .map(|c| rgb(*c))
        .unwrap_or_else(|| look.ink())
}

/// A figure in motion: where it stands and looks, how it moves, and what
/// it holds.
pub struct Figure<'a> {
    pub skin: Skin,
    pub feet: V3,
    pub yaw: i32,
    pub pitch: i32,
    pub ducked: bool,
    pub flashed: bool,
    pub speed: Fx,
    pub on_ground: bool,
    /// ticks, Q16.16, for the stride
    pub moment: Fx,
    /// the weapon and its state, if drawn with one
    pub arms: Option<(&'a Arms, Option<Swinging>)>,
}

/// A point in the figure's frame (cm) in the world.
fn place(f: &Figure, at: [i32; 3]) -> V3 {
    let cm = |c: i32| c * ONE / 100;
    let k = if f.ducked { ONE * 62 / 100 } else { ONE };
    let local = v3(cm(at[0]), fx::mul(cm(at[1]), k), cm(at[2]));
    Affine::translate(f.feet).then(&Affine::rot_y(f.yaw)).apply(local)
}

pub fn draw(out: &mut Shapes, f: &Figure, look: &Look) {
    let cm = |c: i32| c * ONE / 100;
    let ink = look.ink();
    let flash = rgb(look.palette.flash);
    let paint = |name: &str| {
        if f.flashed {
            flash
        } else {
            colour(look, f.skin, name)
        }
    };
    let k = if f.ducked { ONE * 62 / 100 } else { ONE };
    let body = Affine::translate(f.feet)
        .then(&Affine::rot_y(f.yaw))
        .then(&Affine::scale(v3(ONE, k, ONE)));
    // the stride: legs swing opposite ways with the pace, tuck in the air.
    // At 16° a toe 88 cm below the hip comes 88 sin 16° + 14 cos 16° = 38 cm
    // forward, 18 cm out: 42 cm from the line, inside the envelope's 48
    let pace = fx::div(f.speed.min(TUNING.run), TUNING.run);
    let swing = if f.on_ground {
        fx::mul(fx::mul(fx::sin(fx::mul(f.moment, fx::deg(13))), STRIDE), pace)
    } else {
        0
    };
    let tuck = if f.on_ground { 0 } else { TUCK };
    for part in parts(f.skin) {
        let angle = match part.limb {
            Limb::LegL => swing + tuck,
            Limb::LegR => -swing + tuck,
            Limb::Still => 0,
        };
        let at = v3(cm(part.at[0]), cm(part.at[1]), cm(part.at[2]));
        let hip = v3(0, cm(HIP), 0);
        let a = body
            .then(&Affine::translate(hip))
            .then(&Affine::rot_x(angle))
            .then(&Affine::translate(at.sub(hip)))
            .then(&Affine::rot_y(fx::deg(part.turn[1])))
            .then(&Affine::rot_z(fx::deg(part.turn[0])))
            .then(&Affine::scale(v3(
                cm(part.size[0]),
                cm(part.size[1]),
                cm(part.size[2]),
            )));
        out.inked(part.mesh, a, paint(part.colour), ink);
    }
    // the arms, from each shoulder to its hand on the weapon, which lies
    // along the aim
    let aim = facing(f.yaw, f.pitch);
    let hold = Affine::translate(place(f, SHOULDER)).then(&Affine::looking(aim));
    let grip = hold.apply(v3(cm(GRIP[0]), cm(GRIP[1]), cm(GRIP[2])));
    let fore = hold.apply(v3(cm(FORE[0]), cm(FORE[1]), cm(FORE[2])));
    for (shoulder, hand, sleeve) in [
        (place(f, SHOULDER), grip, "arm_r"),
        (place(f, [-SHOULDER[0], SHOULDER[1], SHOULDER[2]]), fore, "arm_l"),
    ] {
        out.inked(Mesh::Round, bar(shoulder, hand, cm(ARM)), paint(sleeve), ink);
        let h = Affine::translate(hand).then(&Affine::scale(v3(cm(10), cm(10), cm(10))));
        out.inked(Mesh::Sphere, h, paint("hand"), ink);
    }
    if let Some((arms, swinging)) = f.arms {
        let pose = hands::pose(arms, swinging, f.speed, f.on_ground, f.moment);
        let at = Affine::translate(grip).then(&Affine::looking(aim));
        hands::draw_weapon(out, &at, arms.weapon, &pose, &look.palette);
    }
}

/// A box from `a` to `b`, `thick` across.
pub fn bar(a: V3, b: V3, thick: Fx) -> Affine {
    let d = b.sub(a);
    let len = d.len().max(ONE / 100);
    Affine::translate(a.add(d.scale(ONE / 2)))
        .then(&Affine::looking(d.norm()))
        .then(&Affine::scale(v3(thick, thick, len)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Look {
        Look::parse(include_str!("../../../data/look.json")).unwrap()
    }

    #[test]
    fn every_part_of_every_skin_has_its_colour_in_the_look() {
        let l = look();
        for s in SKINS {
            let names = l
                .skins
                .get(s.key())
                .unwrap_or_else(|| panic!("{} has no colours", s.key()));
            for part in parts(s) {
                assert!(names.contains_key(part.colour), "{}: {}", s.key(), part.colour);
            }
            for n in ["arm_l", "arm_r", "hand"] {
                assert!(names.contains_key(n), "{}: {n}", s.key());
            }
        }
    }

    #[test]
    fn the_seats_wear_the_four_skins_in_turn() {
        assert_eq!(
            (0..5).map(Skin::of_seat).collect::<Vec<_>>(),
            vec![
                Skin::Hidalgo,
                Skin::Infanta,
                Skin::Harlequin,
                Skin::Sordo,
                Skin::Hidalgo
            ]
        );
    }

    #[test]
    fn every_part_is_drawn_as_wide_as_it_says() {
        // R6.2: the arena's capsule mesh fills only a third of its box, so a
        // body scaled to 0.92 m drew 0.33 m. Every mesh a skin uses reaches
        // at least 90% of its box's half-width across x or z
        let table = crate::prim::MeshTable::default();
        for s in SKINS {
            for part in parts(s) {
                let v = table.vertices(part.mesh.into());
                let reach = v
                    .chunks(crate::prim::VERTEX_VALUES)
                    .map(|p| p[0].abs().max(p[2].abs()))
                    .max()
                    .unwrap_or(0);
                assert!(
                    reach >= ONE * 45 / 100,
                    "{s:?}'s {} part ({:?}) reaches {} of its box",
                    part.colour,
                    part.mesh,
                    fx::decimal(reach * 2, 2)
                );
            }
        }
    }
}
