//! The input record: what one tick of one body is given to act on.
//!
//! Chumworld Arena's (e78945b), with the shooter's actions (M0, R1.2). The
//! player's keys and mouse become one `Input` per tick, and a bot will
//! return the same record. Nothing else moves a body, so a round replays
//! from its start state and its list of records alone, which is what
//! rollback needs.
//!
//! The page sends one bit set per display frame: `web/input.js` turns each
//! key code into an action name through `data/controls.json`, and
//! `numbers()` gives each name its bit from `ACTIONS` below. Keys use the
//! low 24 bits and mouse buttons the bits from `BUTTON_SHIFT` up; the core
//! splits them into `keys` and `buttons`. Mouse movement and the wheel arrive
//! in hundredths of a CSS pixel and add up between ticks (`Pending`), and the
//! first tick a frame runs takes them all.

use serde::{Deserialize, Serialize};

/// Key actions, one bit each.
pub const FORWARD: u32 = 1 << 0;
pub const BACK: u32 = 1 << 1;
pub const LEFT: u32 = 1 << 2;
pub const RIGHT: u32 = 1 << 3;
/// Space: jump; held, a hop on every landing tick (CS-Q9: A). In the air,
/// a fresh press beside a wall is a wall jump.
pub const JUMP: u32 = 1 << 4;
/// Shift or C: crouch (not Ctrl, whose W closes the tab); at speed on the ground, a slide; a fresh press in the
/// air, a slam.
pub const CROUCH: u32 = 1 << 5;
/// Backspace: back to the room's start (M1's practice room; R until M2).
pub const RESET: u32 = 1 << 6;
/// M2: R reloads; 1 to 5 take a gun; Q takes the last one again.
pub const RELOAD: u32 = 1 << 7;
pub const SLOT_1: u32 = 1 << 8;
pub const SLOT_2: u32 = 1 << 9;
pub const SLOT_3: u32 = 1 << 10;
pub const SLOT_4: u32 = 1 << 11;
pub const SLOT_5: u32 = 1 << 12;
pub const LAST: u32 = 1 << 13;
pub const SLOTS: [u32; 5] = [SLOT_1, SLOT_2, SLOT_3, SLOT_4, SLOT_5];

/// Buttons sit above the keys in the page's bit set.
pub const BUTTON_SHIFT: u32 = 24;
const KEY_MASK: u32 = (1 << BUTTON_SHIFT) - 1;

/// Mouse buttons, one bit each, as `Input::buttons` holds them.
/// Left: fire (M2).
pub const FIRE: u32 = 1 << 0;
/// Right: the weapon's second use (M3: the katana's parry and lunge).
pub const ALT: u32 = 1 << 1;

/// Every action's name and its bit in the page's bit set, in the order
/// `numbers()` reports them. `data/controls.json` names these.
pub const ACTIONS: &[(&str, u32)] = &[
    ("forward", FORWARD),
    ("back", BACK),
    ("left", LEFT),
    ("right", RIGHT),
    ("jump", JUMP),
    ("crouch", CROUCH),
    ("reset", RESET),
    ("reload", RELOAD),
    ("slot_1", SLOT_1),
    ("slot_2", SLOT_2),
    ("slot_3", SLOT_3),
    ("slot_4", SLOT_4),
    ("slot_5", SLOT_5),
    ("last", LAST),
    ("fire", FIRE << BUTTON_SHIFT),
    ("alt", ALT << BUTTON_SHIFT),
];

/// One tick of input for one body: the player's, or a pilot's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// Key actions held this tick (`FORWARD`, `JUMP`, ...).
    pub keys: u32,
    /// Mouse buttons held this tick (`FIRE`, `ALT`).
    pub buttons: u32,
    /// Mouse movement this tick, hundredths of a CSS pixel; right and down
    /// are positive.
    pub dx: i32,
    pub dy: i32,
    /// Wheel movement this tick, hundredths of the browser's wheel unit;
    /// toward the reader is positive.
    pub wheel: i32,
}

impl Input {
    /// A tick's record from the page's bit set and the movement it takes.
    pub fn from_page(bits: u32, m: Mouse) -> Input {
        Input {
            keys: bits & KEY_MASK,
            buttons: bits >> BUTTON_SHIFT,
            dx: m.dx,
            dy: m.dy,
            wheel: m.wheel,
        }
    }

    pub fn key(&self, bit: u32) -> bool {
        self.keys & bit != 0
    }

    pub fn button(&self, bit: u32) -> bool {
        self.buttons & bit != 0
    }

    /// The movement keys as a direction in the body's frame: x to the
    /// right, z backward, each −1, 0 or 1. Opposite keys cancel.
    pub fn walk_axes(&self) -> (i32, i32) {
        let x = self.key(RIGHT) as i32 - self.key(LEFT) as i32;
        let z = self.key(BACK) as i32 - self.key(FORWARD) as i32;
        (x, z)
    }
}

/// Mouse movement and wheel, in hundredths of a CSS pixel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mouse {
    pub dx: i32,
    pub dy: i32,
    pub wheel: i32,
}

/// Movement the page has reported that no tick has taken yet. Display
/// frames come faster or slower than ticks; whatever arrives between two
/// ticks goes to the next one, so nothing is lost or counted twice. The
/// camera is drawn with it already applied (`camera::view`), so turning
/// responds on the frame the mouse moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pending {
    m: Mouse,
}

impl Pending {
    pub fn add(&mut self, dx: i32, dy: i32, wheel: i32) {
        self.m.dx = self.m.dx.saturating_add(dx);
        self.m.dy = self.m.dy.saturating_add(dy);
        self.m.wheel = self.m.wheel.saturating_add(wheel);
    }

    /// What has arrived since the last take, and nothing left behind.
    pub fn take(&mut self) -> Mouse {
        std::mem::take(&mut self.m)
    }

    pub fn peek(&self) -> Mouse {
        self.m
    }

    pub fn clear(&mut self) {
        self.m = Mouse::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_bits_split_into_keys_and_buttons() {
        // forward (bit 0) + jump (bit 4) + the right button (bit 25)
        let bits = 1 | (1 << 4) | (1 << 25);
        let i = Input::from_page(bits, Mouse::default());
        assert_eq!(i.keys, 0b1_0001);
        assert_eq!(i.buttons, 0b10);
        assert!(i.key(FORWARD) && i.key(JUMP) && i.button(ALT));
        assert!(!i.button(FIRE) && !i.key(BACK));
    }

    #[test]
    fn every_action_has_its_own_bit_and_name() {
        for (k, (name, bit)) in ACTIONS.iter().enumerate() {
            assert_eq!(bit.count_ones(), 1, "{name}");
            for (other, obit) in &ACTIONS[k + 1..] {
                assert_ne!(name, other);
                assert_ne!(bit, obit, "{name} and {other}");
            }
        }
    }

    #[test]
    fn opposite_movement_keys_cancel() {
        let i = Input {
            keys: FORWARD | BACK | RIGHT,
            ..Input::default()
        };
        assert_eq!(i.walk_axes(), (1, 0));
        let j = Input {
            keys: FORWARD | LEFT,
            ..Input::default()
        };
        assert_eq!(j.walk_axes(), (-1, -1));
    }

    #[test]
    fn pending_movement_goes_to_one_tick_once() {
        let mut p = Pending::default();
        p.add(150, -20, 0);
        p.add(50, 0, 300);
        assert_eq!(
            p.take(),
            Mouse {
                dx: 200,
                dy: -20,
                wheel: 300
            }
        );
        assert_eq!(p.take(), Mouse::default(), "taken once");
    }

    #[test]
    fn an_input_record_survives_a_trip_through_json() {
        let i = Input {
            keys: FORWARD | JUMP,
            buttons: FIRE,
            dx: -340,
            dy: 25,
            wheel: 100,
        };
        let text = serde_json::to_string(&i).unwrap();
        assert_eq!(serde_json::from_str::<Input>(&text).unwrap(), i);
    }
}
