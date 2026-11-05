//! The weapon in the hands, and how it moves (T7 for the guns, asked for
//! again on 2026-11-04; the journal's Feel mockups page). Drawn only:
//! nothing here reaches the record, the world or another browser.
//!
//! Each weapon is a list of parts in the hands' frame, some of which move
//! on their own: the lever rifle's lever, the pump shotgun's forend, a
//! magazine or drum, the Pincel's tip, the Campana's clapper. What moves
//! them is the player's own state, all already in the world: the ticks
//! since a shot (`Arms::cooldown` against the weapon's interval), a reload
//! (`reloading`), a raise after a switch (`raise`), the kick (`recoil`),
//! the Pincel's charge, a swing, and how fast the body moves. The same
//! parts and poses draw another player's weapon in their figure's hands
//! (`skins.rs`), so you see them fire, reload and swing.

use crate::arms::{Arms, RAISE, RECOIL};
use crate::fx::{self, Fx, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::look::{rgb, Palette};
use crate::movement::TUNING;
use crate::prim::Mesh;
use crate::render::Shapes;
use crate::weapons::{Weapon, COUNT};
use crate::world::{Player, Swinging};

/// What a part does as the weapon is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Fixed,
    /// the lever rifle's lever: swung down and back between shots
    Lever,
    /// the pump shotgun's forend: slid back and forward between shots
    Pump,
    /// a magazine or drum: out and back in during a reload
    Mag,
    /// the Pincel's tip: grows and glows as it charges
    Tip,
    /// the Campana's clapper: swings after a shot
    Clapper,
}

/// One part of a weapon in the hands' frame: its mesh, centre, size and
/// colour, in centimetres, x right, y up, z back toward the eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GunPart {
    pub mesh: Mesh,
    pub at: [i32; 3],
    pub size: [i32; 3],
    pub colour: [i32; 3],
    pub role: Role,
}

fn part(mesh: Mesh, at: [i32; 3], size: [i32; 3], colour: [i32; 3], role: Role) -> GunPart {
    GunPart {
        mesh,
        at,
        size,
        colour,
        role,
    }
}

/// Each weapon's kick, in the hands' frame and the view, by `Weapon` order:
/// how far it comes back (cm), how far its muzzle rises (degrees), and how
/// far the view tips up (tenths of a degree). The heavy guns rock; the
/// automatics barely move.
pub const KICK: [(i32, i32, i32); COUNT] = [
    (6, 9, 12),   // service pistol
    (12, 14, 30), // pump shotgun
    (3, 3, 4),    // machine pistol
    (14, 16, 35), // lever rifle
    (8, 6, 15),   // Pincel
    (10, 10, 20), // drum launcher
    (14, 16, 30), // Trabuco
    (16, 10, 25), // Toro
    (10, 12, 20), // Campana
    (5, 5, 6),    // Collage
    (0, 0, 0),    // katana
    (0, 0, 0),    // navaja
    (0, 0, 0),    // guadaña
    (0, 0, 0),    // puñal
];

/// The view's tip up after a shot, angle units: the weapon's kick at the
/// shot, falling to nothing over the kick's ticks. Drawn only (the
/// camera); the aim the world uses never moves.
pub fn view_kick(a: &Arms) -> i32 {
    let tenths = KICK[a.weapon as usize].2;
    fx::deg(tenths) / 10 * a.recoil / RECOIL
}

/// How far along `from..until` the value `t` is, 0 to `ONE`.
fn along(t: i32, from: i32, until: i32) -> Fx {
    if until <= from {
        return 0;
    }
    (ONE * (t - from) / (until - from)).clamp(0, ONE)
}

/// Up and back down: 0 at the ends, `ONE` in the middle.
fn hump(t: Fx) -> Fx {
    fx::sin(fx::mul(t, fx::HALF_TURN))
}

/// The weapon's pose: where the whole thing sits and turns in the hands'
/// frame, and how far each moving part has moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pose {
    /// offset of the whole weapon (cm, Q16.16)
    pub shift: V3,
    /// turns of the whole weapon: up, left, roll (angle units)
    pub pitch: i32,
    pub yaw: i32,
    pub roll: i32,
    /// 0 to `ONE`: how far the lever or pump has worked, the magazine has
    /// dropped, the tip has charged, the clapper has swung
    pub work: Fx,
    pub mag: Fx,
    pub charge: Fx,
    /// the muzzle flash shows
    pub flash: bool,
}

/// The pose of `a`'s weapon, with `swing` under way, the body at `speed`
/// on the ground or not, at `moment` (ticks, Q16.16, for the bob).
pub fn pose(a: &Arms, swing: Option<Swinging>, speed: Fx, on_ground: bool, moment: Fx) -> Pose {
    let cm = |c: i32| c * ONE / 100;
    let st = a.weapon.stats();
    let mut p = Pose::default();
    // the kick: back and up, settling over the kick's ticks
    let (back, up, _) = KICK[a.weapon as usize];
    let k = ONE * a.recoil / RECOIL;
    p.shift = v3(0, 0, fx::mul(cm(back), k));
    p.pitch = fx::mul(fx::deg(up), k);
    p.flash = a.recoil > RECOIL - 2;
    // between shots: the lever and the pump work through the middle of the
    // interval; the clapper swings out and back
    if st.interval > 0 && a.cooldown > 0 {
        let t = st.interval - a.cooldown;
        p.work = hump(along(t, st.interval / 5, st.interval * 4 / 5));
    }
    // a reload: down and rolled over, the magazine out and back in, up again
    if a.reloading > 0 && st.reload > 0 {
        let t = along(st.reload - a.reloading, 0, st.reload);
        let low = hump(t);
        p.shift = p.shift.add(v3(0, -fx::mul(cm(10), low), 0));
        p.roll += fx::mul(fx::deg(35), low);
        p.pitch += fx::mul(fx::deg(10), low);
        p.mag = hump(along(st.reload - a.reloading, st.reload / 5, st.reload * 4 / 5));
    }
    // raised from below after a switch
    if a.raise > 0 {
        let t = ONE * a.raise / RAISE;
        p.shift = p.shift.add(v3(0, -fx::mul(cm(22), t), 0));
        p.pitch -= fx::mul(fx::deg(30), t);
    }
    // the Pincel's charge
    if st.charge > 0 {
        p.charge = ONE * a.charge / st.charge;
    }
    // a bob in step with a run, none in the air
    if on_ground && speed > 0 {
        let pace = speed.min(TUNING.run);
        let amp = fx::div(pace, TUNING.run);
        let phase = fx::mul(moment, fx::deg(13));
        p.shift = p.shift.add(v3(
            fx::mul(fx::mul(cm(1), amp), fx::sin(phase)),
            fx::mul(fx::mul(cm(1), amp), fx::sin(phase.wrapping_mul(2)).abs()),
            0,
        ));
    }
    // a melee weapon sweeps across through its swing; the navaja jabs; the
    // katana's lunge thrusts
    if let Some(s) = swing {
        let sw = s.weapon.swing().expect("a swing is a melee weapon's");
        if s.lunge {
            p.shift = p.shift.add(v3(-cm(10), cm(4), -cm(20)));
        } else if matches!(s.weapon, Weapon::Navaja | Weapon::Punal) {
            let t = hump(along(s.age, 0, sw.until + 2));
            p.shift = p.shift.add(v3(-cm(6), cm(2), -fx::mul(cm(22), t)));
        } else {
            // from 60° right to 60° left over the swing's ticks
            let t = along(s.age, 0, sw.until);
            p.yaw = fx::deg(60) - fx::mul(fx::deg(120), t);
            p.roll -= fx::mul(fx::deg(25), hump(t));
        }
    }
    p
}

/// A weapon's parts posed in `hold` (the hands' frame), each moving part
/// moved by the pose; and the muzzle flash.
pub fn draw_weapon(out: &mut Shapes, hold: &Affine, g: Weapon, pose: &Pose, pal: &Palette) {
    let cm = |c: i32| c * ONE / 100;
    let frame = hold
        .then(&Affine::translate(pose.shift))
        .then(&Affine::rot_y(pose.yaw))
        .then(&Affine::rot_x(pose.pitch))
        .then(&Affine::rot_z(pose.roll));
    let mut tip = v3(0, cm(2), -cm(30));
    for pt in gun_parts(g, pal) {
        let at = v3(cm(pt.at[0]), cm(pt.at[1]), cm(pt.at[2]));
        tip = if at.z - cm(pt.size[2]) / 2 < tip.z {
            v3(at.x, at.y, at.z - cm(pt.size[2]) / 2)
        } else {
            tip
        };
        let (moved, turned, grow) = match pt.role {
            // the lever swings down and forward about its front, 50°
            Role::Lever => (
                v3(0, -fx::mul(cm(3), pose.work), 0),
                fx::mul(-fx::deg(50), pose.work),
                ONE,
            ),
            // the forend slides back 9 cm
            Role::Pump => (v3(0, 0, fx::mul(cm(9), pose.work)), 0, ONE),
            // the magazine drops 14 cm, out of the hands' frame
            Role::Mag => (v3(0, -fx::mul(cm(14), pose.mag), 0), 0, ONE),
            // the tip swells to twice its size at full charge
            Role::Tip => (V3::ZERO, 0, ONE + pose.charge),
            // the clapper swings 6 cm
            Role::Clapper => (v3(fx::mul(cm(6), pose.work), 0, 0), 0, ONE),
            Role::Fixed => (V3::ZERO, 0, ONE),
        };
        let size = v3(cm(pt.size[0]), cm(pt.size[1]), cm(pt.size[2])).scale(grow);
        let a = frame
            .then(&Affine::translate(at.add(moved)))
            .then(&Affine::rot_x(turned))
            .then(&Affine::scale(size));
        // no ink: an outline sized for figures across the room is a black
        // band half a metre from the eye (R5.4)
        out.solid(pt.mesh, a, rgb(pt.colour), 0);
        if pt.role == Role::Tip && pose.charge > 0 {
            out.glow(
                Mesh::Sphere,
                a.then(&Affine::scale(v3(2 * ONE, 2 * ONE, 2 * ONE))),
                rgb(pal.stroke),
                pose.charge,
            );
        }
    }
    if pose.flash {
        let f = frame
            .then(&Affine::translate(tip))
            .then(&Affine::scale(v3(cm(12), cm(12), cm(12))));
        out.glow(Mesh::Sphere, f, rgb(pal.flash), ONE);
    }
}

/// The weapon in your own hands: below and right of the eye, posed.
pub fn draw_hands(out: &mut Shapes, eye: V3, yaw: i32, pitch: i32, p: &Player, moment: Fx, pal: &Palette) {
    let cm = |c: i32| c * ONE / 100;
    let hold = Affine::translate(eye)
        .then(&Affine::looking(facing(yaw, pitch)))
        .then(&Affine::translate(v3(cm(18), -cm(16), -cm(42))));
    let pose = pose(&p.arms, p.swing, p.body.speed(), p.body.on_ground, moment);
    draw_weapon(out, &hold, p.arms.weapon, &pose, pal);
}

pub fn gun_parts(g: Weapon, pal: &Palette) -> Vec<GunPart> {
    let (steel, dark, wood, brass) = (pal.steel, pal.dark_steel, pal.wood, pal.brass);
    let (bone, blue, ochre, umber, red) = (pal.bone, pal.blue, pal.ochre, pal.umber, pal.stroke);
    match g {
        Weapon::Pistol => vec![
            part(Mesh::Cube, [0, 2, -6], [4, 4, 18], steel, Role::Fixed),
            part(Mesh::Cube, [0, -5, 0], [3, 9, 4], dark, Role::Mag),
        ],
        Weapon::Shotgun => vec![
            part(Mesh::Cube, [0, 2, -28], [3, 3, 60], steel, Role::Fixed),
            part(Mesh::Cube, [0, -1, -22], [3, 3, 44], dark, Role::Fixed),
            part(Mesh::Cube, [0, -1, -30], [5, 5, 12], wood, Role::Pump),
            part(Mesh::Cube, [0, 0, 2], [5, 7, 14], steel, Role::Fixed),
            part(Mesh::Cube, [0, -3, 20], [5, 8, 24], wood, Role::Fixed),
        ],
        Weapon::MachinePistol => vec![
            part(Mesh::Cube, [0, 1, -8], [5, 6, 22], dark, Role::Fixed),
            part(Mesh::Cube, [0, 2, -24], [2, 2, 10], steel, Role::Fixed),
            part(Mesh::Cube, [0, -8, -6], [3, 12, 3], steel, Role::Mag),
            part(Mesh::Cube, [0, -6, 2], [3, 8, 4], dark, Role::Fixed),
        ],
        Weapon::Rifle => vec![
            part(Mesh::Cube, [0, 3, -40], [2, 2, 70], steel, Role::Fixed),
            part(Mesh::Cube, [0, 1, -2], [4, 6, 16], brass, Role::Fixed),
            part(Mesh::Cube, [0, 0, -25], [4, 4, 25], wood, Role::Fixed),
            part(Mesh::Cube, [0, -2, 20], [5, 9, 26], wood, Role::Fixed),
            part(Mesh::Cube, [0, -6, 0], [1, 5, 8], steel, Role::Lever),
        ],
        Weapon::Pincel => vec![
            part(Mesh::Cube, [0, 2, -35], [3, 3, 60], bone, Role::Fixed),
            part(Mesh::Cube, [0, -1, 10], [6, 7, 22], blue, Role::Fixed),
            part(Mesh::Cube, [0, 4, -20], [4, 2, 12], ochre, Role::Fixed),
            part(Mesh::Facet, [0, 2, -68], [5, 5, 8], umber, Role::Fixed),
            part(Mesh::Facet, [0, 2, -74], [3, 3, 5], red, Role::Tip),
        ],
        // a fat tube over a drum, drawn true
        Weapon::Drum => vec![
            part(Mesh::Cube, [0, 2, -28], [6, 6, 36], dark, Role::Fixed),
            part(Mesh::Cyl, [0, -5, -16], [10, 7, 10], steel, Role::Mag),
            part(Mesh::Cube, [0, -6, 0], [3, 9, 4], wood, Role::Fixed),
            part(Mesh::Cube, [0, 0, 4], [4, 6, 10], wood, Role::Fixed),
        ],
        // a painted blunderbuss: a flared mouth and a blue stock
        Weapon::Trabuco => vec![
            part(Mesh::Cube, [0, 2, -30], [4, 4, 46], umber, Role::Fixed),
            part(Mesh::Facet, [0, 2, -54], [11, 11, 6], ochre, Role::Fixed),
            part(Mesh::Cube, [0, 2, -14], [5, 5, 3], bone, Role::Fixed),
            part(Mesh::Cube, [0, -2, 10], [6, 8, 24], blue, Role::Fixed),
        ],
        // a painted tube with horns
        Weapon::Toro => vec![
            part(Mesh::Cube, [0, 3, -30], [7, 7, 46], red, Role::Fixed),
            part(Mesh::Facet, [0, 3, -54], [9, 9, 4], umber, Role::Fixed),
            part(Mesh::Cube, [5, 9, -48], [2, 7, 2], bone, Role::Fixed),
            part(Mesh::Cube, [-5, 9, -48], [2, 7, 2], bone, Role::Fixed),
            part(Mesh::Cube, [0, -6, 0], [4, 10, 5], umber, Role::Fixed),
        ],
        // a painted bell on a handle
        Weapon::Campana => vec![
            part(Mesh::Cone, [0, 0, -30], [16, 15, 16], brass, Role::Fixed),
            part(Mesh::Facet, [0, -7, -30], [4, 4, 4], dark, Role::Clapper),
            part(Mesh::Cube, [0, -4, -6], [4, 4, 34], wood, Role::Fixed),
            part(Mesh::Cube, [0, -8, 8], [4, 10, 5], umber, Role::Fixed),
        ],
        // painted planes stuck together
        Weapon::Collage => vec![
            part(Mesh::Cube, [0, 2, -14], [5, 6, 26], blue, Role::Fixed),
            part(Mesh::Cube, [2, 5, -20], [3, 4, 10], ochre, Role::Fixed),
            part(Mesh::Cube, [-1, -1, -28], [4, 3, 12], red, Role::Fixed),
            part(Mesh::Cube, [0, 3, -32], [2, 2, 8], bone, Role::Fixed),
            part(Mesh::Cube, [0, -6, 0], [4, 9, 5], umber, Role::Fixed),
        ],
        Weapon::Katana => vec![
            part(Mesh::Cube, [0, 2, -50], [1, 4, 80], steel, Role::Fixed),
            part(Mesh::Cube, [0, 2, -9], [7, 7, 1], brass, Role::Fixed),
            part(Mesh::Cube, [0, 2, 2], [3, 3, 20], dark, Role::Fixed),
        ],
        Weapon::Navaja => vec![
            part(Mesh::Cube, [0, 2, -17], [1, 3, 16], steel, Role::Fixed),
            part(Mesh::Cube, [0, 1, -8], [3, 4, 2], brass, Role::Fixed),
            part(Mesh::Cube, [0, 0, 0], [3, 4, 14], wood, Role::Fixed),
        ],
        // T16: a straight double-edged dagger narrowing to its point, a wide
        // cross guard and a red tassel at the pommel, so it reads apart from
        // the navaja's folding blade
        Weapon::Punal => vec![
            part(Mesh::Cube, [0, 2, -20], [1, 5, 26], steel, Role::Fixed),
            part(Mesh::Cube, [0, 2, -35], [1, 3, 6], steel, Role::Fixed),
            part(Mesh::Cube, [0, 2, -6], [12, 2, 2], brass, Role::Fixed),
            part(Mesh::Cube, [0, 1, 1], [3, 3, 12], dark, Role::Fixed),
            part(Mesh::Sphere, [0, 1, 8], [4, 4, 4], brass, Role::Fixed),
            part(Mesh::Cube, [0, -4, 10], [2, 8, 2], red, Role::Fixed),
        ],
        // a painted scythe: a long shaft, the blade across its end
        Weapon::Guadana => vec![
            part(Mesh::Cube, [0, 0, -34], [3, 3, 90], umber, Role::Fixed),
            part(Mesh::Cube, [-16, 6, -78], [34, 3, 5], bone, Role::Fixed),
            part(Mesh::Cube, [0, 0, 6], [4, 4, 8], ochre, Role::Fixed),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Input, FIRE};

    fn armed(g: Weapon) -> Arms {
        let mut a = Arms::default();
        a.weapon = g;
        a
    }

    #[test]
    fn the_view_kicks_with_a_shot_and_settles_over_the_kicks_ticks() {
        let mut a = armed(Weapon::Rifle);
        a.tick(&Input {
            buttons: FIRE,
            ..Input::default()
        });
        // the lever rifle: 3.5° at the shot, falling to nothing over 8 ticks
        assert_eq!(view_kick(&a), fx::deg(35) / 10);
        for _ in 0..RECOIL {
            a.tick(&Input::default());
        }
        assert_eq!(view_kick(&a), 0);
    }

    #[test]
    fn the_lever_works_between_rifle_shots_and_is_home_when_it_can_fire_again() {
        let mut a = armed(Weapon::Rifle);
        a.tick(&Input {
            buttons: FIRE,
            ..Input::default()
        });
        let work = |a: &Arms| pose(a, None, 0, true, 0).work;
        // the interval is 60 ticks; the lever works from tick 12 to 48
        assert_eq!(work(&a), 0, "the tick of the shot");
        for _ in 0..29 {
            a.tick(&Input::default());
        }
        assert!(work(&a) > ONE * 9 / 10, "halfway: fully worked, {}", work(&a));
        // the shot's tick set the cooldown to 60 and each tick since took
        // one: 59 later it is 1, the next tick it is 0 and the rifle fires
        for _ in 0..31 {
            a.tick(&Input::default());
        }
        assert_eq!((a.cooldown, work(&a)), (0, 0), "home, and ready");
    }

    #[test]
    fn a_reload_drops_the_magazine_and_brings_it_back() {
        let mut a = armed(Weapon::MachinePistol);
        a.rounds[Weapon::MachinePistol as usize] = 0;
        a.tick(&Input {
            buttons: FIRE,
            ..Input::default()
        });
        let reload = Weapon::MachinePistol.stats().reload;
        assert_eq!(a.reloading, reload);
        let mut most = 0;
        for _ in 0..reload {
            most = most.max(pose(&a, None, 0, true, 0).mag);
            a.tick(&Input::default());
        }
        assert!(most > ONE * 9 / 10, "{most}");
        assert_eq!(pose(&a, None, 0, true, 0).mag, 0);
    }

    #[test]
    fn a_still_player_holds_the_weapon_still_and_a_runner_bobs_it() {
        let a = armed(Weapon::Pistol);
        assert_eq!(pose(&a, None, 0, true, ONE * 5).shift, V3::ZERO);
        assert_ne!(pose(&a, None, TUNING.run, true, ONE * 5).shift, V3::ZERO);
        assert_eq!(
            pose(&a, None, TUNING.run, false, ONE * 5).shift,
            V3::ZERO,
            "no bob in the air"
        );
    }
}
