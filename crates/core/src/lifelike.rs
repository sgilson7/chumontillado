//! T34: the lifelike skins (asked for on 2026-11-08: "they can be very
//! realistic and keep their current form, but you should also be able to
//! play as the old stylized versions as well"). Style Lab's photographic
//! figures, baked for the browser by `tools/mesh/bake_game.py`, worn in
//! place of a skin's painted figure when its seat picks the lifelike style.
//!
//! The page draws each lifelike figure as a skinned mesh (`web/skin.js`);
//! the core chooses everything it draws and hands it one record a figure
//! (Espiochum's actor record, after its `actors.rs`): the model, two clip
//! rows and the blend between them for the legs and hips (the lower clip,
//! picked from the state the painted legs swing by), two rows and a blend
//! for the chest, arms and head (the upper clip: the aim over pitch, or a
//! reload, a raise, a cut or a thrust over how far through it is), the
//! model matrix, and the hit flash. The page draws an upper bone as the
//! lower row's boundary bone times its upper row's matrix (the manifest's
//! `above`); it computes no frame, angle or position (CS-1).
//!
//! The weapon is still the core's painted weapon, held where the baked right
//! hand is: each upper frame's hold frame, carried by the lower frame's
//! boundary, in the figure's place. Drawn only: no record enters the world
//! or rollback.

use std::sync::OnceLock;

use serde_json::Value;

use crate::arms::RAISE;
use crate::fx::{self, Fx, ONE, TURN};
use crate::geom::{v3, Affine};
use crate::hands;
use crate::look::{rgb, Look};
use crate::render::Shapes;
use crate::skins::{Figure, Skin, SKINS};
use crate::world::Swinging;

/// Values a record: model, the lower rows and blend, the upper rows and
/// blend, 12 matrix values (columns of three), the flash's share and colour.
pub const VALUES: usize = 23;
/// Where in a record the matrix starts, and its translation.
pub const MATRIX: usize = 7;
pub const TRANSLATION: usize = MATRIX + 9;
/// Where the flash's share is.
pub const FLASH: usize = MATRIX + 12;

/// The aim's frames: one each 15° of pitch (24 a turn) from -90°
/// (game_clips.py).
pub const PITCH_FROM: i32 = fx::deg(-90);
pub const PITCH_FRAMES_A_TURN: i32 = 24;
/// A cut's frames: one each 10° of its turn (36 a turn) from -70° (it winds
/// back at most 45° and `weapons::WIND`).
pub const CUT_FROM: i32 = fx::deg(-70);
pub const CUT_FRAMES_A_TURN: i32 = 36;
/// A reload's, a raise's and a thrust's frames: six, from none to all the
/// way (`ONE`).
pub const STEPS: i32 = 5;
/// The legs walk from 1 m/s and run from 5.5 m/s.
pub const WALK: Fx = ONE;
pub const RUN: Fx = ONE * 55 / 10;
/// The stride's phase, as the painted legs swing (`skins::draw`): 13° a tick.
pub const STRIDE_PER_TICK: i32 = fx::deg(13);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub name: String,
    pub frames: i32,
    pub row: i32,
    pub looped: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub lower: Vec<Clip>,
    pub upper: Vec<Clip>,
    /// for each lower row, the boundary bone's skin matrix, and how far it
    /// leans forward (angle units)
    pub boundary: Vec<Affine>,
    pub tilt: Vec<i32>,
    /// for each upper row (from the first upper row), the weapon's hold frame
    pub hold: Vec<Affine>,
}

#[derive(Debug, Clone, Default)]
pub struct Table {
    pub models: Vec<Model>,
}

fn affine(v: &Value) -> Result<Affine, String> {
    let n: Vec<i32> = v
        .as_array()
        .ok_or("a frame is not a list")?
        .iter()
        .map(|x| x.as_i64().map(|x| x as i32).ok_or("not a whole number"))
        .collect::<Result<_, _>>()?;
    if n.len() != 12 {
        return Err(format!("a frame has {} values", n.len()));
    }
    Ok(Affine {
        c0: v3(n[0], n[1], n[2]),
        c1: v3(n[3], n[4], n[5]),
        c2: v3(n[6], n[7], n[8]),
        t: v3(n[9], n[10], n[11]),
    })
}

fn clips(v: &Value) -> Result<Vec<Clip>, String> {
    v.as_array()
        .ok_or("no clips")?
        .iter()
        .map(|c| {
            Ok(Clip {
                name: c["name"].as_str().ok_or("a clip without a name")?.to_string(),
                frames: c["frames"].as_i64().unwrap_or(1).max(1) as i32,
                row: c["row"].as_i64().ok_or("a clip without a row")? as i32,
                looped: c["loop"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

impl Table {
    /// The table compiled into the core, read once.
    pub fn get() -> &'static Table {
        static TABLE: OnceLock<Table> = OnceLock::new();
        TABLE.get_or_init(|| Table::parse(include_str!("../../../data/skins.json")).expect("data/skins.json"))
    }

    /// `data/skins.json`, written by `scripts/join_skins.py` from the bake:
    /// every number a Q16 integer, angles in angle units.
    pub fn parse(text: &str) -> Result<Table, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut models = vec![Model::default(); SKINS.len()];
        for m in v["skins"].as_array().ok_or("no skins")? {
            let id = m["skin"].as_str().ok_or("a skin without a name")?;
            let k = SKINS
                .iter()
                .position(|s| s.model() == id)
                .ok_or(format!("unknown skin {id}"))?;
            let list = |key: &str| -> Result<Vec<Affine>, String> {
                m[key]
                    .as_array()
                    .ok_or(format!("no {key}"))?
                    .iter()
                    .map(affine)
                    .collect()
            };
            models[k] = Model {
                lower: clips(&m["lower"])?,
                upper: clips(&m["upper"])?,
                boundary: list("boundary")?,
                tilt: m["tilt"]
                    .as_array()
                    .ok_or("no tilt")?
                    .iter()
                    .map(|x| x.as_i64().unwrap_or(0) as i32)
                    .collect(),
                hold: list("hold")?,
            };
        }
        Ok(Table { models })
    }
}

impl Model {
    fn lower_clip(&self, name: &str) -> &Clip {
        self.lower
            .iter()
            .find(|c| c.name == name)
            .unwrap_or(&self.lower[0])
    }

    fn upper_clip(&self, name: &str) -> &Clip {
        self.upper
            .iter()
            .find(|c| c.name == name)
            .unwrap_or(&self.upper[0])
    }
}

/// A clip at a frame (Q16): the two rows to blend and how far between them.
/// A looped clip wraps; one that does not holds its last frame.
pub fn rows(c: &Clip, frame: Fx) -> (i32, i32, Fx) {
    let f = frame.max(0);
    let (whole, blend) = (f / ONE, f % ONE);
    let (now, next) = if c.looped {
        (whole % c.frames, (whole + 1) % c.frames)
    } else {
        (whole.min(c.frames - 1), (whole + 1).min(c.frames - 1))
    };
    (c.row + now, c.row + next, if now == next { 0 } else { blend })
}

/// The frame (Q16) a gait shows at the stride's phase: one turn, one clip.
pub fn by_phase(c: &Clip, phase: i32) -> Fx {
    ((phase.rem_euclid(TURN) as i64 * c.frames as i64 * ONE as i64) / TURN as i64) as Fx
}

/// The frame (Q16) a sampled clip shows for `x`: `steps` frames to each
/// `span` of it from `from`.
pub fn sampled(c: &Clip, x: i32, from: i32, steps: i32, span: i32) -> Fx {
    let most = (c.frames - 1) * ONE;
    ((x - from) as i64 * steps as i64 * ONE as i64 / span as i64).clamp(0, most as i64) as Fx
}

/// The lower clip and frame for a figure: on a vine it hangs; sliding or
/// ducked (on the ground or in the air, where the hit envelope is as low)
/// it holds that pose; in the air it tucks; on its feet it stands, walks or
/// runs by speed, the gait's frame from the stride's phase.
pub fn lower(m: &Model, f: &Figure) -> (Clip, Fx) {
    let name = if f.hanging {
        "hang"
    } else if f.sliding {
        "slide"
    } else if f.ducked {
        "duck"
    } else if !f.on_ground {
        "air"
    } else if f.speed < WALK {
        "stand"
    } else if f.speed < RUN {
        "walk"
    } else {
        "run"
    };
    let c = m.lower_clip(name).clone();
    let frame = if matches!(name, "walk" | "run") {
        by_phase(&c, fx::mul(f.moment, STRIDE_PER_TICK))
    } else {
        0
    };
    (c, frame)
}

/// The upper clip and frame: a cut by its turn, a thrust or a lunge by how
/// far out, a raise and a reload by how low, or else the aim by its pitch
/// against how far the lower body leans.
pub fn upper(m: &Model, f: &Figure, lean: i32) -> (Clip, Fx) {
    let (name, x, from, steps, span) = match f.arms {
        Some((_, Some(s))) if hands::cut_turn(&s).is_some() => (
            "slash",
            hands::cut_turn(&s).unwrap_or(0),
            CUT_FROM,
            CUT_FRAMES_A_TURN,
            TURN,
        ),
        Some((_, Some(s))) if s.weapon.swing().is_some() => {
            ("thrust", hands::thrust_reach(&s), 0, STEPS, ONE)
        }
        Some((a, _)) if a.raise > 0 => ("raise", ONE * a.raise / RAISE, 0, STEPS, ONE),
        Some((a, _)) if a.reloading > 0 => ("low", hands::reload_low(a), 0, STEPS, ONE),
        _ => (
            "aim",
            (f.pitch + lean).clamp(PITCH_FROM, -PITCH_FROM),
            PITCH_FROM,
            PITCH_FRAMES_A_TURN,
            TURN,
        ),
    };
    let c = m.upper_clip(name).clone();
    let frame = sampled(&c, x, from, steps, span);
    (c, frame)
}

fn lerp(a: &Affine, b: &Affine, t: Fx) -> Affine {
    Affine {
        c0: a.c0.lerp(b.c0, t),
        c1: a.c1.lerp(b.c1, t),
        c2: a.c2.lerp(b.c2, t),
        t: a.t.lerp(b.t, t),
    }
}

/// The figure's place: at its feet, turned to its heading.
pub fn place(f: &Figure) -> Affine {
    Affine::translate(f.feet)
        .then(&f.frame.affine())
        .then(&Affine::rot_y(f.yaw))
}

/// What one figure shows: its record's rows, and the frame the weapon is
/// held in (in the world).
pub struct Shown {
    pub lower: (i32, i32, Fx),
    pub upper: (i32, i32, Fx),
    pub hold: Affine,
}

pub fn shown(m: &Model, f: &Figure) -> Shown {
    let (lc, lf) = lower(m, f);
    let lr = rows(&lc, lf);
    let at = |r: i32| r as usize;
    let lean = fx::lerp(m.tilt[at(lr.0)], m.tilt[at(lr.1)], lr.2);
    let (uc, uf) = upper(m, f, lean);
    let ur = rows(&uc, uf);
    let first = m.upper[0].row;
    let boundary = lerp(&m.boundary[at(lr.0)], &m.boundary[at(lr.1)], lr.2);
    let hold = lerp(&m.hold[at(ur.0 - first)], &m.hold[at(ur.1 - first)], ur.2);
    Shown {
        lower: lr,
        upper: ur,
        hold: place(f).then(&boundary).then(&hold),
    }
}

/// The record the page reads for one figure.
pub fn record(skin: Skin, f: &Figure, s: &Shown, look: &Look, out: &mut Vec<i32>) {
    let k = SKINS.iter().position(|x| *x == skin).unwrap_or(0) as i32;
    out.push(fx::int(k));
    for (a, b, t) in [s.lower, s.upper] {
        out.extend([fx::int(a), fx::int(b), t]);
    }
    let m = place(f);
    for c in [m.c0, m.c1, m.c2, m.t] {
        out.extend([c.x, c.y, c.z]);
    }
    let flash = rgb(look.palette.flash);
    out.push(if f.flashed { ONE } else { 0 });
    out.extend(flash);
}

/// A lifelike figure: its record, and the painted weapon in its right
/// hand (kicking, working and charging as it does in yours, the rest of its
/// movement already the hand's).
pub fn draw(out: &mut Shapes, f: &Figure, look: &Look) {
    let m = &Table::get().models[SKINS.iter().position(|x| *x == f.skin).unwrap_or(0)];
    let s = shown(m, f);
    let mut rec = Vec::with_capacity(VALUES);
    record(f.skin, f, &s, look, &mut rec);
    out.actors.extend(rec);
    if let Some((arms, swing)) = f.arms {
        let pose = hands::held_pose(arms);
        hands::draw_weapon(out, &s.hold, arms.weapon, &pose, &look.palette);
        // the cut's trail on the level, as the slash clip cuts
        let level = swing.map(|w| Swinging {
            cut: w.cut.map(|_| 0),
            ..w
        });
        let aim = Figure {
            arms: Some((arms, None)),
            pitch: 0,
            ..*f
        };
        let hold = shown(m, &aim).hold;
        hands::draw_trail(out, &hold, arms, level, &look.palette);
    }
}

/// Whether a figure in this skin can be drawn lifelike now: the page has
/// loaded its model (`Shapes::ready`).
pub fn ready(out: &Shapes, skin: Skin) -> bool {
    let k = SKINS.iter().position(|x| *x == skin).unwrap_or(0);
    out.ready & (1 << k) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arms::Arms;
    use crate::fx::int;
    use crate::geom::V3;

    fn clip(frames: i32, row: i32, looped: bool) -> Clip {
        Clip {
            name: String::new(),
            frames,
            row,
            looped,
        }
    }

    fn figure(arms: Option<(&Arms, Option<Swinging>)>) -> Figure<'_> {
        Figure {
            skin: Skin::Hidalgo,
            lifelike: true,
            feet: V3::ZERO,
            yaw: 0,
            pitch: 0,
            ducked: false,
            flashed: false,
            speed: 0,
            on_ground: true,
            moment: 0,
            arms,
            sliding: false,
            hanging: false,
            frame: crate::gravity::Turn::IDENTITY,
        }
    }

    #[test]
    fn a_looped_clip_wraps_and_one_that_is_not_holds_its_last_frame() {
        // frame 27.5 of a 28-frame loop from row 10: rows 37 and 10, half way
        assert_eq!(rows(&clip(28, 10, true), int(27) + ONE / 2), (37, 10, ONE / 2));
        // frame 40 of a 6-frame clip from row 100: its last, 105, held
        assert_eq!(rows(&clip(6, 100, false), int(40)), (105, 105, 0));
    }

    #[test]
    fn a_full_stride_plays_the_gait_once_and_half_a_turn_is_half_the_clip() {
        let c = clip(28, 0, true);
        assert_eq!(by_phase(&c, 0), 0);
        assert_eq!(by_phase(&c, TURN / 2), int(14));
        assert_eq!(by_phase(&c, TURN), 0);
    }

    #[test]
    fn the_legs_take_their_clip_from_the_state_the_painted_legs_swing_by() {
        let m = &Table::get().models[0];
        let name = |f: &Figure| lower(m, f).0.name;
        let mut f = figure(None);
        assert_eq!(name(&f), "stand");
        f.speed = int(3);
        assert_eq!(name(&f), "walk");
        f.speed = int(9);
        assert_eq!(name(&f), "run");
        // 7 ticks into the run is 7 × 13° = 91° of the stride: 91/360 of its
        // 28 frames, 7.08, so frame 7
        f.moment = int(7);
        assert_eq!(lower(m, &f).1 / ONE, 7);
        f.on_ground = false;
        assert_eq!(name(&f), "air");
        // ducked in the air it ducks, as low as its hit envelope
        f.ducked = true;
        assert_eq!(name(&f), "duck");
        f.on_ground = true;
        assert_eq!(name(&f), "duck");
        f.sliding = true;
        assert_eq!(name(&f), "slide");
        f.hanging = true;
        assert_eq!(name(&f), "hang");
    }

    #[test]
    fn the_arms_aim_by_pitch_and_reload_cut_and_raise_by_how_far_through() {
        let m = &Table::get().models[0];
        let arms = Arms::default();
        let mut f = figure(Some((&arms, None)));
        // level is the seventh aim frame: (0° + 90°) / 15° = 6
        let (c, k) = upper(m, &f, 0);
        assert_eq!((c.name.as_str(), k), ("aim", int(6)));
        // 45° up (an eighth of a turn) is frame 9; a lower body leaning 45°
        // forward asks three more, so the weapon still points 45° up
        f.pitch = TURN / 8;
        assert_eq!(upper(m, &f, 0).1, int(9));
        assert_eq!(upper(m, &f, TURN / 8).1, int(12));
        // half way through a raise: frame 2.5 of the raise's 0 to 5
        let mut raising = Arms::default();
        raising.raise = RAISE / 2;
        f.arms = Some((&raising, None));
        let (c, k) = upper(m, &f, 0);
        assert_eq!(c.name, "raise");
        assert_eq!(k, int(5) / 2);
    }

    #[test]
    fn the_weapon_sits_in_the_baked_right_hand() {
        // standing at (3, 0, -2) facing ahead, aiming level: the hold is the
        // stand's first boundary matrix applied to the level aim's hold, moved
        // to the feet; read from the table and composed here by hand
        let m = &Table::get().models[0];
        let arms = Arms::default();
        let mut f = figure(Some((&arms, None)));
        f.feet = v3(int(3), 0, -int(2));
        let s = shown(m, &f);
        let stand = m.lower.iter().find(|c| c.name == "stand").unwrap().row as usize;
        let aim = m.upper.iter().find(|c| c.name == "aim").unwrap();
        let b = m.boundary[stand];
        let h = m.hold[(aim.row - m.upper[0].row + 6) as usize];
        let at = |p: crate::geom::V3| b.c0.scale(p.x).add(b.c1.scale(p.y)).add(b.c2.scale(p.z)).add(b.t);
        let want = at(h.t).add(f.feet);
        let d = s.hold.t.sub(want);
        assert!(
            d.len() < ONE / 1000,
            "the weapon is {} m from the hand",
            fx::decimal(d.len(), 3)
        );
        // and the hand is where a right hand holds a weapon: right of the
        // middle, in front, between the waist and the shoulder
        let off = s.hold.t.sub(f.feet);
        assert!(off.x > 0 && off.z < 0, "{off:?}");
        assert!(off.y > int(1) && off.y < ONE * 16 / 10, "{off:?}");
    }

    #[test]
    fn a_skin_is_drawn_lifelike_only_once_the_page_has_its_model() {
        let look = crate::look::Look::parse(include_str!("../../../data/look.json")).unwrap();
        let f = figure(None);
        let mut painted = Shapes::default();
        crate::skins::draw(&mut painted, &f, &look);
        assert!(painted.actors.is_empty() && !painted.list.is_empty());
        let mut ready = Shapes {
            ready: 1,
            ..Shapes::default()
        };
        crate::skins::draw(&mut ready, &f, &look);
        assert_eq!(ready.actors.len(), VALUES);
        assert!(ready.list.is_empty(), "no painted part as well");
        // hit, the record says so, with the flash's colour
        let hit = Figure { flashed: true, ..f };
        let mut flashed = Shapes {
            ready: 1,
            ..Shapes::default()
        };
        crate::skins::draw(&mut flashed, &hit, &look);
        assert_eq!(flashed.actors[FLASH], ONE);
        assert_eq!(flashed.actors[FLASH + 1..FLASH + 4], rgb(look.palette.flash));
    }
}
