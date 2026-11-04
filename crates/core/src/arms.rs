//! What a player holds and does with it (M2): the gun in hand and the last
//! one, each gun's rounds, the time to the next shot, a reload, the Pincel's
//! charge, and the raise after a switch. One record a tick in, at most one
//! shot out; `world.rs` casts it.

use crate::input::{Input, FIRE, LAST, RELOAD, SLOTS};
use crate::weapons::{Gun, GUNS, GUN_COUNT};

/// Ticks to raise a gun after taking it (no shot meanwhile).
pub const RAISE: i32 = 12;
/// Ticks the gun kicks back in the hands after a shot (drawn only).
pub const RECOIL: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arms {
    pub gun: Gun,
    pub last: Gun,
    pub rounds: [i32; GUN_COUNT],
    pub cooldown: i32,
    /// ticks left in a reload; 0 when not reloading
    pub reloading: i32,
    /// the Pincel's charge, ticks held
    pub charge: i32,
    pub raise: i32,
    pub recoil: i32,
    old_fire: bool,
    old_keys: u32,
}

/// A shot leaving a gun: which, and for the Pincel how charged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub gun: Gun,
    pub charged: i32,
}

impl Default for Arms {
    fn default() -> Arms {
        Arms {
            gun: Gun::Pistol,
            last: Gun::Shotgun,
            rounds: GUNS.map(|g| g.magazine),
            cooldown: 0,
            reloading: 0,
            charge: 0,
            raise: 0,
            recoil: 0,
            old_fire: false,
            old_keys: 0,
        }
    }
}

impl Arms {
    fn take(&mut self, g: Gun) {
        if g != self.gun {
            self.last = self.gun;
            self.gun = g;
            self.raise = RAISE;
            self.reloading = 0;
            self.charge = 0;
        }
    }

    /// One tick of the record: switches, reloads and shots.
    pub fn tick(&mut self, input: &Input) -> Option<Shot> {
        let old = self.old_keys;
        let fresh = |bit: u32| input.keys & bit != 0 && old & bit == 0;
        for (k, bit) in SLOTS.iter().enumerate() {
            if fresh(*bit) {
                self.take(Gun::from_index(k));
            }
        }
        if fresh(LAST) {
            self.take(self.last);
        }
        if input.wheel != 0 {
            let step = if input.wheel > 0 { 1 } else { GUN_COUNT - 1 };
            self.take(Gun::from_index(self.gun as usize + step));
        }
        self.old_keys = input.keys;
        self.cooldown = (self.cooldown - 1).max(0);
        self.raise = (self.raise - 1).max(0);
        self.recoil = (self.recoil - 1).max(0);
        let g = self.gun.stats();
        let held = input.button(FIRE);
        let was = self.old_fire;
        self.old_fire = held;

        if self.reloading > 0 {
            self.reloading -= 1;
            if self.reloading == 0 {
                self.rounds[self.gun as usize] = g.magazine;
            }
            return None;
        }
        let rounds = self.rounds[self.gun as usize];
        if (fresh(RELOAD) && rounds < g.magazine) || (rounds == 0 && held) {
            self.reloading = g.reload;
            self.charge = 0;
            return None;
        }
        if self.raise > 0 || self.cooldown > 0 || rounds == 0 {
            return None;
        }
        let shoot = if g.charge > 0 {
            // the Pincel: charge while held, fire on release
            if held {
                self.charge = (self.charge + 1).min(g.charge);
                false
            } else {
                was && self.charge > 0
            }
        } else if g.auto {
            held
        } else {
            held && !was
        };
        if !shoot {
            return None;
        }
        let charged = self.charge;
        self.charge = 0;
        self.rounds[self.gun as usize] -= 1;
        self.cooldown = g.interval;
        self.recoil = RECOIL;
        Some(Shot {
            gun: self.gun,
            charged,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{SLOT_2, SLOT_5};

    fn rec(keys: u32, fire: bool) -> Input {
        Input {
            keys,
            buttons: if fire { FIRE } else { 0 },
            ..Input::default()
        }
    }

    #[test]
    fn the_pistol_fires_once_a_press_and_no_faster_than_four_a_second() {
        let mut a = Arms::default();
        let mut shots = 0;
        // press and release every 5 ticks for a second: 12 presses, but the
        // pistol's 15-tick interval allows a shot at ticks 0, 15, 30 and 45
        for t in 0..60 {
            if a.tick(&rec(0, t % 5 < 2)).is_some() {
                shots += 1;
            }
        }
        assert_eq!(shots, 4);
        // and holding fires once
        let mut b = Arms::default();
        let held = (0..60).filter(|_| b.tick(&rec(0, true)).is_some()).count();
        assert_eq!(held, 1);
    }

    #[test]
    fn the_machine_pistol_fires_while_held_twelve_a_second_until_it_is_empty() {
        let mut a = Arms::default();
        a.tick(&rec(crate::input::SLOT_3, false));
        for _ in 0..RAISE {
            a.tick(&rec(0, false));
        }
        let shots = (0..60).filter(|_| a.tick(&rec(0, true)).is_some()).count();
        // every 5 ticks: 12 in a second
        assert_eq!(shots, 12);
        assert_eq!(a.rounds[2], 30 - 12);
    }

    #[test]
    fn an_empty_gun_reloads_when_fired_and_r_reloads_a_part_used_one() {
        let mut a = Arms::default();
        a.rounds[0] = 0;
        a.tick(&rec(0, true));
        assert_eq!(a.reloading, 60);
        for _ in 0..60 {
            a.tick(&rec(0, false));
        }
        assert_eq!((a.reloading, a.rounds[0]), (0, 8));
        a.rounds[0] = 3;
        a.tick(&rec(RELOAD, false));
        assert_eq!(a.reloading, 60);
    }

    #[test]
    fn the_pincel_charges_while_held_and_fires_on_release() {
        let mut a = Arms::default();
        a.tick(&rec(SLOT_5, false));
        for _ in 0..RAISE {
            a.tick(&rec(0, false));
        }
        for _ in 0..30 {
            assert!(a.tick(&rec(0, true)).is_none());
        }
        let shot = a.tick(&rec(0, false)).expect("fires on release");
        assert_eq!((shot.gun, shot.charged), (Gun::Pincel, 30));
    }

    #[test]
    fn a_switch_raises_the_gun_before_it_fires_and_q_takes_the_last_again() {
        let mut a = Arms::default();
        a.tick(&rec(SLOT_2, false));
        assert_eq!((a.gun, a.last), (Gun::Shotgun, Gun::Pistol));
        // pressed at once: still raising
        assert!(a.tick(&rec(0, true)).is_none());
        a.tick(&rec(LAST, false));
        assert_eq!(a.gun, Gun::Pistol);
        let mut b = Arms::default();
        b.tick(&rec(SLOT_2, false));
        for _ in 0..RAISE {
            b.tick(&rec(0, false));
        }
        assert!(b.tick(&rec(0, true)).is_some());
    }
}
