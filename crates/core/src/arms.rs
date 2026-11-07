//! What a player holds and does with it (M2, M3): the weapon in hand and the
//! last one, each gun's rounds, the time to the next shot or swing, a
//! reload, the Pincel's charge, the raise after a switch, and the second
//! button's use. One record a tick in, at most one shot or swing out;
//! `world.rs` casts, launches or swings it.

use crate::input::{Input, ALT, FIRE, LAST, MELEE, RELOAD, SLOTS};
use crate::weapons::{Alt, Weapon, COUNT, LUNGE_INTERVAL, MELEE as MELEE_WEAPONS, WEAPONS};

/// Ticks to raise a weapon after taking it (no shot meanwhile).
pub const RAISE: i32 = 12;
/// Ticks the gun kicks back in the hands after a shot (drawn only).
pub const RECOIL: i32 = 8;
/// Ticks between two hooks, so a held line is let go before the next.
pub const HOOK_AGAIN: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arms {
    pub weapon: Weapon,
    pub last: Weapon,
    /// M5: the weapons held, one bit each by `Weapon` order: every one in
    /// the practice room, the pistol and navaja at a round's start
    /// (CS-Q10: A), more as they are picked up
    pub owned: u16,
    /// the melee weapon the melee key takes first
    pub melee: Weapon,
    pub rounds: [i32; COUNT],
    pub cooldown: i32,
    /// ticks until the second button can be used again
    pub alt_cooldown: i32,
    /// ticks left in a reload; 0 when not reloading
    pub reloading: i32,
    /// the Pincel's charge, ticks held
    pub charge: i32,
    pub raise: i32,
    pub recoil: i32,
    old_fire: bool,
    old_alt: bool,
    old_keys: u32,
}

/// A shot or swing leaving a weapon: which, for the Pincel how charged, and
/// whether it is the second button's use (a lunge or a hook).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub weapon: Weapon,
    pub charged: i32,
    pub alt: bool,
}

impl Default for Arms {
    fn default() -> Arms {
        Arms {
            weapon: Weapon::Pistol,
            last: Weapon::Shotgun,
            owned: ALL_OWNED,
            melee: Weapon::Navaja,
            rounds: WEAPONS.map(|g| g.magazine),
            cooldown: 0,
            alt_cooldown: 0,
            reloading: 0,
            charge: 0,
            raise: 0,
            recoil: 0,
            old_fire: false,
            old_alt: false,
            old_keys: 0,
        }
    }
}

/// Every weapon, and what a round starts with (CS-Q10: A).
pub const ALL_OWNED: u16 = (1 << COUNT) - 1;
pub const ROUND_OWNED: u16 = (1 << Weapon::Pistol as u16) | (1 << Weapon::Navaja as u16);

impl Arms {
    /// A round's start: the pistol in hand and the navaja, full.
    pub fn for_round() -> Arms {
        Arms {
            owned: ROUND_OWNED,
            last: Weapon::Navaja,
            ..Arms::default()
        }
    }

    pub fn owns(&self, g: Weapon) -> bool {
        self.owned & (1 << g as u16) != 0
    }

    /// M5: a weapon picked up off the map: held from now, full, in hand.
    pub fn pick_up(&mut self, g: Weapon) {
        self.owned |= 1 << g as u16;
        self.rounds[g as usize] = g.stats().magazine;
        self.take(g);
    }

    fn take(&mut self, g: Weapon) {
        if !self.owns(g) {
            return;
        }
        if g != self.weapon {
            self.last = self.weapon;
            self.weapon = g;
            self.raise = RAISE;
            self.reloading = 0;
            self.charge = 0;
        }
        if g.swing().is_some() {
            self.melee = g;
        }
    }

    /// The melee key: the last melee weapon, or the next of the four when
    /// one is already in hand.
    fn take_melee(&mut self) {
        let next = if self.weapon.swing().is_some() {
            // the next melee weapon held, round the four
            let k = MELEE_WEAPONS.iter().position(|&m| m == self.weapon).unwrap_or(0);
            (1..=MELEE_WEAPONS.len())
                .map(|j| MELEE_WEAPONS[(k + j) % MELEE_WEAPONS.len()])
                .find(|&m| self.owns(m))
                .unwrap_or(self.weapon)
        } else {
            self.melee
        };
        self.take(next);
    }

    /// One tick of the record: switches, reloads, shots and swings.
    pub fn tick(&mut self, input: &Input) -> Option<Shot> {
        let old = self.old_keys;
        let fresh = |bit: u32| input.keys & bit != 0 && old & bit == 0;
        for (k, bit) in SLOTS.iter().enumerate() {
            if fresh(*bit) {
                self.take(Weapon::from_index(k));
            }
        }
        if fresh(MELEE) {
            self.take_melee();
        }
        if fresh(LAST) {
            self.take(self.last);
        }
        if input.wheel != 0 {
            // the next weapon held, either way round
            let step = if input.wheel > 0 { 1 } else { COUNT - 1 };
            let next = (1..=COUNT)
                .map(|j| Weapon::from_index(self.weapon as usize + step * j))
                .find(|&g| self.owns(g))
                .unwrap_or(self.weapon);
            self.take(next);
        }
        self.old_keys = input.keys;
        self.cooldown = (self.cooldown - 1).max(0);
        self.alt_cooldown = (self.alt_cooldown - 1).max(0);
        self.raise = (self.raise - 1).max(0);
        self.recoil = (self.recoil - 1).max(0);
        let g = self.weapon.stats();
        let held = input.button(FIRE);
        let was = self.old_fire;
        self.old_fire = held;
        let alt = input.button(ALT);
        let alt_fresh = alt && !self.old_alt;
        self.old_alt = alt;

        if let Some(sw) = self.weapon.swing() {
            if self.raise > 0 {
                return None;
            }
            // the second button first: a lunge waits for the swing to end
            let use_alt = match sw.alt {
                Alt::Lunge => alt_fresh && self.alt_cooldown == 0 && self.cooldown == 0,
                Alt::Hook => alt_fresh && self.alt_cooldown == 0,
                Alt::None => false,
            };
            if use_alt {
                self.alt_cooldown = match sw.alt {
                    Alt::Lunge => LUNGE_INTERVAL,
                    _ => HOOK_AGAIN,
                };
                if sw.alt == Alt::Lunge {
                    self.cooldown = g.interval;
                }
                return Some(Shot {
                    weapon: self.weapon,
                    charged: 0,
                    alt: true,
                });
            }
            if held && self.cooldown == 0 {
                self.cooldown = g.interval;
                return Some(Shot {
                    weapon: self.weapon,
                    charged: 0,
                    alt: false,
                });
            }
            return None;
        }

        if self.reloading > 0 {
            self.reloading -= 1;
            if self.reloading == 0 {
                self.rounds[self.weapon as usize] = g.magazine;
            }
            return None;
        }
        let rounds = self.rounds[self.weapon as usize];
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
        self.rounds[self.weapon as usize] -= 1;
        self.cooldown = g.interval;
        self.recoil = RECOIL;
        Some(Shot {
            weapon: self.weapon,
            charged,
            alt: false,
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
        assert_eq!((shot.weapon, shot.charged), (Weapon::Pincel, 30));
    }

    #[test]
    fn a_switch_raises_the_gun_before_it_fires_and_q_takes_the_last_again() {
        let mut a = Arms::default();
        a.tick(&rec(SLOT_2, false));
        assert_eq!((a.weapon, a.last), (Weapon::Shotgun, Weapon::Pistol));
        // pressed at once: still raising
        assert!(a.tick(&rec(0, true)).is_none());
        a.tick(&rec(LAST, false));
        assert_eq!(a.weapon, Weapon::Pistol);
        let mut b = Arms::default();
        b.tick(&rec(SLOT_2, false));
        for _ in 0..RAISE {
            b.tick(&rec(0, false));
        }
        assert!(b.tick(&rec(0, true)).is_some());
    }
    // ---- M3 ----------------------------------------------------------------

    fn raised(a: &mut Arms, keys: u32) {
        a.tick(&rec(keys, false));
        for _ in 0..RAISE {
            a.tick(&rec(0, false));
        }
    }

    #[test]
    fn six_to_zero_take_the_projectile_guns_and_e_takes_the_melee_weapons_in_turn() {
        use crate::input::{SLOT_10, SLOT_8};
        let mut a = Arms::default();
        a.tick(&rec(SLOT_8, false));
        assert_eq!(a.weapon, Weapon::Toro);
        a.tick(&rec(SLOT_10, false));
        assert_eq!(a.weapon, Weapon::Collage);
        // the first press takes the navaja every player starts with; each
        // further press, the next of the four (T16 added the puñal)
        let mut order = Vec::new();
        for _ in 0..5 {
            a.tick(&rec(MELEE, false));
            a.tick(&rec(0, false));
            order.push(a.weapon);
        }
        assert_eq!(
            order,
            [
                Weapon::Navaja,
                Weapon::Guadana,
                Weapon::Punal,
                Weapon::Garrocha,
                Weapon::Katana
            ]
        );
        // from a gun, E takes the melee weapon last held
        a.tick(&rec(SLOT_8, false));
        a.tick(&rec(0, false));
        a.tick(&rec(MELEE, false));
        assert_eq!(a.weapon, Weapon::Katana);
    }

    #[test]
    fn a_held_katana_swings_every_forty_five_ticks_and_lunges_once_in_ninety() {
        let mut a = Arms {
            weapon: Weapon::Katana,
            ..Arms::default()
        };
        // held for two seconds: swings at 0, 45, 90: three (a beat and a
        // half apart, CS-Q20)
        let swings = (0..120).filter(|_| a.tick(&rec(0, true)).is_some()).count();
        assert_eq!(swings, 3);
        assert_eq!(a.rounds, Arms::default().rounds, "no rounds are spent");
        // the right button pressed every 10 ticks for two seconds: lunges at
        // 0 and 90 only
        let mut b = Arms {
            weapon: Weapon::Katana,
            ..Arms::default()
        };
        let alt = |t: i32| Input {
            buttons: if t % 10 == 0 { ALT } else { 0 },
            ..Input::default()
        };
        let lunges: Vec<i32> = (0..120)
            .filter(|&t| b.tick(&alt(t)).is_some_and(|s| s.alt))
            .collect();
        assert_eq!(lunges, vec![0, 90]);
    }

    #[test]
    fn a_trabuco_reloads_after_its_one_shot() {
        use crate::input::SLOT_7;
        let mut a = Arms::default();
        raised(&mut a, SLOT_7);
        assert!(a.tick(&rec(0, true)).is_some());
        a.tick(&rec(0, false));
        // empty: the next press reloads, two beats (CS-Q20): 60 ticks
        a.tick(&rec(0, true));
        assert_eq!(a.reloading, 60);
    }

    #[test]
    fn a_round_starts_with_the_pistol_and_navaja_and_a_pickup_is_held_full_and_in_hand() {
        use crate::input::{SLOT_2, SLOT_8};
        let mut a = Arms::for_round();
        assert!(a.owns(Weapon::Pistol) && a.owns(Weapon::Navaja));
        // the shotgun is not held: 2 does nothing
        a.tick(&rec(SLOT_2, false));
        assert_eq!(a.weapon, Weapon::Pistol);
        // E takes the navaja, and again stays on it (the only melee held)
        a.tick(&rec(MELEE, false));
        a.tick(&rec(0, false));
        a.tick(&rec(MELEE, false));
        assert_eq!(a.weapon, Weapon::Navaja);
        // the wheel goes between the two held
        let mut b = Arms::for_round();
        b.tick(&Input {
            wheel: 100,
            ..Input::default()
        });
        assert_eq!(b.weapon, Weapon::Navaja);
        a.pick_up(Weapon::Toro);
        assert_eq!((a.weapon, a.rounds[Weapon::Toro as usize]), (Weapon::Toro, 3));
        a.tick(&rec(0, false));
        a.tick(&rec(SLOT_8, false));
        assert_eq!(a.weapon, Weapon::Toro);
    }
}
