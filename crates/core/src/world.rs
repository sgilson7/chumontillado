//! The world: the map and the players in it, and the order of one tick.
//!
//! M1 has one player in the practice room; the seats are a list from the
//! start because the round has four (PLAN.md). Each player moves only by its
//! input record (`input::Input`), so a tick is a function of the world and
//! one record per seat, which is what rollback needs (M4).

use crate::camera::{self, Aim};
use crate::fx::{self, Fx, ONE};
use crate::geom::{v3, V3};
use crate::input::{Input, Mouse, RESET};
use crate::map::{self, Map};
use crate::movement::{Body, Events, Mover};
use crate::render::Camera;

/// One seat's body, where it looks, and what it did last tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub body: Body,
    pub aim: Aim,
    /// last tick's origin, for drawing between ticks
    pub prev: V3,
    /// last tick's eye height, likewise (a crouch lowers it)
    pub prev_eye: Fx,
    pub events: Events,
    /// the fastest it has gone across the ground since the room's start
    pub top_speed: Fx,
    pub spawn: (V3, i32),
}

impl Player {
    fn at(spawn: (V3, i32)) -> Player {
        let body = Body::at(spawn.0);
        Player {
            body,
            aim: Aim::new(spawn.1),
            prev: spawn.0,
            prev_eye: body.eye(),
            events: Events::default(),
            top_speed: 0,
            spawn,
        }
    }

    /// The eye between last tick and this one, `alpha` of the way.
    pub fn eye(&self, alpha: Fx) -> V3 {
        let feet = self.prev.lerp(self.body.origin, alpha);
        feet.add(v3(0, fx::lerp(self.prev_eye, self.body.eye(), alpha), 0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    pub map: Map,
    pub players: Vec<Player>,
    /// the field of view across, degrees (a reader's setting)
    pub fov: i32,
}

impl World {
    /// M1: the practice room with one player at its spawn.
    pub fn practice() -> World {
        let map = map::practice();
        let players = vec![Player::at(map.spawns[0])];
        World {
            tick: 0,
            map,
            players,
            fov: camera::FOV_START,
        }
    }

    /// One tick: each seat turns by its record's mouse movement, then moves.
    /// R sends a player back to its spawn.
    pub fn step(&mut self, inputs: &[Input]) {
        let mut mover = Mover::new(&self.map);
        for (p, input) in self.players.iter_mut().zip(inputs) {
            p.prev = p.body.origin;
            p.prev_eye = p.body.eye();
            if input.key(RESET) {
                let pct = p.aim.pct;
                *p = Player::at(p.spawn);
                p.aim.pct = pct;
                continue;
            }
            let from = p.aim.yaw;
            p.aim = p.aim.turned(Mouse {
                dx: input.dx,
                dy: input.dy,
                wheel: input.wheel,
            });
            p.events = mover.tick(&mut p.body, input, from, p.aim.yaw);
            p.top_speed = p.top_speed.max(p.body.speed());
        }
        self.tick += 1;
    }

    /// The first-person camera for seat `id`, `alpha` of the way to the next
    /// tick, with the mouse movement not yet given to a tick already turned.
    pub fn camera(&self, id: usize, alpha: Fx, pending: Mouse, aspect: Fx) -> Camera {
        let p = &self.players[id];
        camera::eye_view(
            p.eye(alpha),
            p.aim.turned(pending),
            camera::vertical_fov(self.fov, aspect),
        )
    }
}

/// Speed in m/s as text with one decimal (the HUD's readout).
pub fn speed_text(speed: Fx) -> String {
    let tenths = (speed as i64 * 10 + (ONE as i64 / 2)) >> fx::SHIFT;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests;
