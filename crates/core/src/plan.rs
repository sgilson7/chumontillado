//! T32: a match's plan, the same on every browser in it: the maps in the
//! order the rounds play them, and whether each map's weapons lie where it
//! places them or are drawn at random.
//!
//! Sam: "you should be able to select the starting map in the rotation, and
//! whether you want to randomize the rotation or not ... a list of maps on
//! the RHS you can select from and order to build a map playlist, then when
//! you play online you can select from map playlists ... and the weapons on
//! each map should be both fixed and have a random mode". The playlists
//! themselves are this browser's (`game.rs`); a match carries only the
//! plan drawn from one, which the host sends in its start.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;
use crate::weapons::{Weapon, ALL};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// the maps by number (`maps::map`), in the order played; none for
    /// every map in turn
    #[serde(default)]
    pub maps: Vec<u8>,
    /// 0: each map's weapons as it places them; any other number, the seed
    /// each round's are drawn from
    #[serde(default)]
    pub weapons: u32,
}

impl Plan {
    /// How many rounds go by before the order comes round again.
    pub fn len(&self) -> usize {
        if self.maps.is_empty() {
            crate::maps::COUNT
        } else {
            self.maps.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.maps.is_empty()
    }

    /// The map played at place `k` in the order.
    pub fn map(&self, k: usize) -> usize {
        match self.maps.get(k % self.len()) {
            Some(&m) => m as usize % crate::maps::COUNT,
            None => k % crate::maps::COUNT,
        }
    }

    /// A plan of `maps` in that order, or shuffled by `rng`, the weapons
    /// placed or drawn at random from it.
    pub fn draw(maps: &[u8], shuffle: bool, random_weapons: bool, rng: &mut Rng) -> Plan {
        let mut maps = maps.to_vec();
        if shuffle {
            // Fisher and Yates
            for i in (1..maps.len()).rev() {
                let j = rng.below(i as u32 + 1) as usize;
                maps.swap(i, j);
            }
        }
        let weapons = if random_weapons { rng.next_u32() | 1 } else { 0 };
        Plan { maps, weapons }
    }

    /// The weapons lying at a map's pickup places for the round at place
    /// `k`: as the map places them, or each drawn from those nobody starts
    /// with (the pistol and the navaja every seat holds).
    pub fn weapons_at(&self, placed: &[Weapon], k: usize) -> Vec<Weapon> {
        if self.weapons == 0 {
            return placed.to_vec();
        }
        let pool: Vec<Weapon> = ALL
            .iter()
            .copied()
            .filter(|&g| g != Weapon::Pistol && g != Weapon::Navaja)
            .collect();
        let mut rng = Rng::new(((self.weapons as u64) << 32) ^ k as u64);
        placed
            .iter()
            .map(|_| pool[rng.below(pool.len() as u32) as usize])
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_plays_its_maps_in_order_and_round_again_and_with_none_every_map() {
        let p = Plan {
            maps: vec![11, 3, 3],
            weapons: 0,
        };
        let order: Vec<usize> = (0..5).map(|k| p.map(k)).collect();
        assert_eq!(order, [11, 3, 3, 11, 3]);
        let every = Plan::default();
        assert_eq!(every.len(), crate::maps::COUNT);
        assert_eq!(every.map(13), 1);
    }

    #[test]
    fn shuffled_it_keeps_every_map_once_and_the_seed_gives_one_order() {
        let maps: Vec<u8> = (0..12).collect();
        let a = Plan::draw(&maps, true, false, &mut Rng::new(7));
        let b = Plan::draw(&maps, true, false, &mut Rng::new(7));
        let c = Plan::draw(&maps, true, false, &mut Rng::new(8));
        assert_eq!(a, b, "one seed, one order");
        assert_ne!(a.maps, c.maps);
        assert_ne!(a.maps, maps, "shuffled");
        let mut sorted = a.maps.clone();
        sorted.sort();
        assert_eq!(sorted, maps, "each map once");
        assert_eq!(Plan::draw(&maps, false, false, &mut Rng::new(7)).maps, maps);
    }

    #[test]
    fn random_weapons_differ_round_to_round_and_never_are_the_ones_everyone_holds() {
        let placed = crate::maps::plaza()
            .pickups
            .iter()
            .map(|p| p.1)
            .collect::<Vec<_>>();
        let fixed = Plan::default();
        assert_eq!(fixed.weapons_at(&placed, 0), placed);
        let random = Plan::draw(&[], false, true, &mut Rng::new(3));
        let (a, b) = (random.weapons_at(&placed, 0), random.weapons_at(&placed, 1));
        assert_eq!(a.len(), placed.len());
        assert_ne!(a, placed);
        assert_ne!(a, b, "a new draw each round");
        assert_eq!(a, random.weapons_at(&placed, 0), "the same draw on every browser");
        let all: Vec<Weapon> = (0..20).flat_map(|k| random.weapons_at(&placed, k)).collect();
        assert!(!all.contains(&Weapon::Pistol) && !all.contains(&Weapon::Navaja));
    }
}
