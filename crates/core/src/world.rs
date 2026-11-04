//! The world: the map, the players in it and the straw targets, and the
//! order of one tick.
//!
//! Each seat moves and shoots only by its input record (`input::Input`); a
//! bot's record is made by its pilot from the world (`pilot.rs`). So a tick
//! is a function of the world and one record per human seat, which is what
//! rollback needs (M4). Within a tick every seat moves first, then the
//! shots are resolved in seat order.
//!
//! M2 adds the guns (`arms.rs`, `weapons.rs`), health, the straw targets
//! and the bot, and what a shot leaves: a tracer, a mark on the wall, a hit
//! mark for the shooter. In one browser a hit is final at once; online
//! (M4) a hit shown before every seat's record is in can be taken back by a
//! re-run (CS-Q5: B, R5.2).

use crate::arms::{Arms, Shot, RECOIL};
use crate::camera::{self, Aim};
use crate::fx::{self, int, Fx, ONE};
use crate::geom::{facing, right_of, v3, Affine, V3};
use crate::input::{Input, Mouse, RESET};
use crate::look::{rgb, Look, Palette, Rgb};
use crate::map::{self, Map};
use crate::movement::{Body, Events, Mover};
use crate::pilot::{self, Pilot};
use crate::prim::Mesh;
use crate::render::{Camera, Shapes};
use crate::trace::Scratch;
use crate::weapons::{self, Gun, Target};

pub const HEALTH: i32 = 100;
/// Ticks a dead player waits before standing again at their spawn.
pub const RESPAWN: i32 = 120;
/// Ticks a felled straw target lies down.
pub const TARGET_DOWN: i32 = 180;
/// Ticks the hit mark and the kill mark show, and a body flashes when hit.
pub const MARK: i32 = 12;
pub const FLASH: i32 = 6;
/// Ticks a tracer shows, and a Pincel's stroke (2 s).
pub const TRACER: i32 = 5;
pub const STROKE: i32 = 120;
/// The most marks on the walls at once; the oldest goes first.
pub const MARKS_MOST: usize = 48;

/// One seat: its body, where it looks, what it holds, its health, and what
/// it did last tick.
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
    pub arms: Arms,
    pub health: i32,
    /// ticks until it stands again; 0 while alive
    pub dead: i32,
    /// ticks left on its hit mark and kill mark (as a shooter), and on its
    /// own flash when hit
    pub hit_mark: i32,
    pub kill_mark: i32,
    pub flash: i32,
    /// a bot's pilot; none for a person
    pub pilot: Option<Pilot>,
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
            arms: Arms::default(),
            health: HEALTH,
            dead: 0,
            hit_mark: 0,
            kill_mark: 0,
            flash: 0,
            pilot: None,
        }
    }

    /// Back at its spawn, as new, keeping its look speed and its pilot.
    fn respawn(&mut self) {
        let (pct, pilot) = (self.aim.pct, self.pilot);
        *self = Player::at(self.spawn);
        self.aim.pct = pct;
        self.pilot = pilot;
    }

    /// The eye between last tick and this one, `alpha` of the way.
    pub fn eye(&self, alpha: Fx) -> V3 {
        let feet = self.prev.lerp(self.body.origin, alpha);
        feet.add(v3(0, fx::lerp(self.prev_eye, self.body.eye(), alpha), 0))
    }

    pub fn alive(&self) -> bool {
        self.dead == 0
    }

    /// What a shot meets of it.
    pub fn target(&self) -> Target {
        Target::of(self.body.origin, self.body.eye())
    }
}

/// A straw target on a post: it takes shots, falls at no health, and
/// stands again full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dummy {
    pub feet: V3,
    pub health: i32,
    pub down: i32,
    pub flash: i32,
}

/// A shot's line through the air: a tracer, or a Pincel's stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trail {
    pub from: V3,
    pub to: V3,
    pub ticks: i32,
    pub stroke: bool,
}

/// Where a shot struck a wall, and the way back along the shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub at: V3,
    pub normal: V3,
}

/// Whether the room's bot only moves (the practice room) or shoots back
/// (a duel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bot {
    Moves,
    Shoots,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub tick: u64,
    pub map: Map,
    pub players: Vec<Player>,
    pub dummies: Vec<Dummy>,
    pub trails: Vec<Trail>,
    pub marks: Vec<Mark>,
    /// the field of view across, degrees (a reader's setting)
    pub fov: i32,
}

/// The straw targets in the practice room, down the hop lane.
pub const DUMMIES: [(i32, i32); 3] = [(8, -4), (14, -24), (22, -28)];
/// The bot's spawn: the first corner of its loop.
pub const BOT_SPAWN: (i32, i32) = (8, -28);

impl World {
    /// The practice room with the player at its spawn, three straw targets,
    /// and a bot that hops round its loop without shooting.
    pub fn practice() -> World {
        World::room(Bot::Moves)
    }

    /// The practice room with its bot moving only, or shooting back.
    pub fn room(bot: Bot) -> World {
        let map = map::practice();
        let mut players = vec![Player::at(map.spawns[0])];
        let mut b = Player::at((v3(int(BOT_SPAWN.0), 0, int(BOT_SPAWN.1)), fx::HALF_TURN));
        b.pilot = Some(Pilot {
            shoots: bot == Bot::Shoots,
            ..Pilot::default()
        });
        players.push(b);
        World {
            tick: 0,
            map,
            players,
            dummies: DUMMIES
                .iter()
                .map(|&(x, z)| Dummy {
                    feet: v3(int(x), 0, int(z)),
                    health: HEALTH,
                    down: 0,
                    flash: 0,
                })
                .collect(),
            trails: Vec::new(),
            marks: Vec::new(),
            fov: camera::FOV_START,
        }
    }

    /// What a bot in seat `id` knows this tick: itself, and the first
    /// living person's chest if it can see it.
    fn view(&self, id: usize, scratch: &mut Scratch) -> pilot::View {
        let me = &self.players[id];
        let eye = me.body.origin.add(v3(0, me.body.eye(), 0));
        let chest = self
            .players
            .iter()
            .find(|p| p.pilot.is_none() && p.alive())
            .map(|p| p.body.origin.add(v3(0, p.body.eye() - ONE / 4, 0)))
            .filter(|&c| {
                let to = c.sub(eye);
                let d = to.len();
                d <= pilot::SIGHT
                    && weapons::wall_distance(&self.map, scratch, eye, to.norm(), d) >= d - ONE / 50
            });
        pilot::View {
            tick: self.tick,
            feet: me.body.origin,
            eye,
            yaw: me.aim.yaw,
            pitch: me.aim.pitch,
            on_ground: me.body.on_ground,
            chest,
        }
    }

    /// One tick. `inputs` holds a record for each person's seat in order;
    /// a bot's is made by its pilot. Every seat turns and moves, then the
    /// shots fired are resolved in seat order.
    pub fn step(&mut self, inputs: &[Input]) {
        let mut scratch = Scratch::default();
        let mut records = Vec::with_capacity(self.players.len());
        let mut given = inputs.iter();
        for id in 0..self.players.len() {
            let r = match self.players[id].pilot {
                Some(mut p) if self.players[id].alive() => {
                    let v = self.view(id, &mut scratch);
                    let r = pilot::record(&mut p, &v);
                    self.players[id].pilot = Some(p);
                    r
                }
                Some(_) => Input::default(),
                None => given.next().copied().unwrap_or_default(),
            };
            records.push(r);
        }

        let mut shots: Vec<(usize, Shot)> = Vec::new();
        {
            let mut mover = Mover::new(&self.map);
            for (id, (p, input)) in self.players.iter_mut().zip(&records).enumerate() {
                p.prev = p.body.origin;
                p.prev_eye = p.body.eye();
                p.hit_mark = (p.hit_mark - 1).max(0);
                p.kill_mark = (p.kill_mark - 1).max(0);
                p.flash = (p.flash - 1).max(0);
                if p.dead > 0 {
                    p.dead -= 1;
                    if p.dead == 0 {
                        p.respawn();
                    }
                    continue;
                }
                if input.key(RESET) {
                    p.respawn();
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
                if let Some(s) = p.arms.tick(input) {
                    shots.push((id, s));
                }
            }
        }
        // the targets count down before this tick's shots, so a felled one
        // lies its whole time
        for d in &mut self.dummies {
            d.flash = (d.flash - 1).max(0);
            if d.down > 0 {
                d.down -= 1;
                if d.down == 0 {
                    d.health = HEALTH;
                }
            }
        }
        for (id, s) in shots {
            self.shoot(id, s, &mut scratch);
        }

        for t in &mut self.trails {
            t.ticks -= 1;
        }
        self.trails.retain(|t| t.ticks > 0);
        self.tick += 1;
    }

    /// A shot from seat `id`: each pellet's ray from the eye, its damage to
    /// what it meets, its tracer, its mark on a wall, and the shotgun's
    /// kick in the air.
    fn shoot(&mut self, id: usize, s: Shot, scratch: &mut Scratch) {
        let g = s.gun.stats();
        let (eye, yaw, pitch) = {
            let p = &self.players[id];
            (p.body.origin.add(v3(0, p.body.eye(), 0)), p.aim.yaw, p.aim.pitch)
        };
        // every body a shot could meet: the other living players, then the
        // standing targets
        let mut targets: Vec<Option<Target>> = self
            .players
            .iter()
            .enumerate()
            .map(|(k, p)| (k != id && p.alive()).then(|| p.target()))
            .collect();
        let seats = targets.len();
        targets.extend(
            self.dummies
                .iter()
                .map(|d| (d.down == 0).then(|| Target::of(d.feet, ONE * 165 / 100))),
        );
        let from = muzzle(eye, yaw, pitch);
        let salt = (self.tick as u32).wrapping_mul(31).wrapping_add(id as u32);
        for d in weapons::pellets(g, yaw, pitch, salt) {
            let (hits, stop) = weapons::cast(&self.map, scratch, eye, d, g.range, &targets, g.pierce);
            for h in &hits {
                let dmg = weapons::damage(g, s.charged, h.head);
                self.players[id].hit_mark = MARK;
                if h.who < seats {
                    let victim = &mut self.players[h.who];
                    victim.health -= dmg;
                    victim.flash = FLASH;
                    if victim.health <= 0 {
                        victim.health = 0;
                        victim.dead = RESPAWN;
                        targets[h.who] = None;
                        self.players[id].kill_mark = MARK;
                    }
                } else {
                    let dummy = &mut self.dummies[h.who - seats];
                    dummy.health -= dmg;
                    dummy.flash = FLASH;
                    if dummy.health <= 0 {
                        dummy.down = TARGET_DOWN;
                        targets[h.who] = None;
                    }
                }
            }
            let end = eye.add(d.scale(stop));
            let stroke = s.gun == Gun::Pincel;
            self.trails.push(Trail {
                from,
                to: end,
                ticks: if stroke { STROKE } else { TRACER },
                stroke,
            });
            if (hits.is_empty() || g.pierce) && stop < g.range {
                if self.marks.len() >= MARKS_MOST {
                    self.marks.remove(0);
                }
                self.marks.push(Mark {
                    at: end,
                    normal: d.neg(),
                });
            }
        }
        let p = &mut self.players[id];
        if g.kick > 0 && !p.body.on_ground {
            // the shotgun's jump: pushed back along the aim, so a shot at the
            // floor lifts
            p.body.velocity = p.body.velocity.sub(facing(yaw, pitch).scale(g.kick));
        }
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

    /// The moving things, seen by seat `me`: the other players, the straw
    /// targets, tracers and strokes, marks on the walls, and with `hands`
    /// (in first person) the gun in `me`'s hands.
    pub fn draw(&self, me: usize, hands: bool, alpha: Fx, pending: Mouse, look: &Look, out: &mut Shapes) {
        let (pal, ink) = (&look.palette, look.ink());
        for (k, p) in self.players.iter().enumerate() {
            if k != me && p.alive() {
                let feet = p.prev.lerp(p.body.origin, alpha);
                draw_figure(out, feet, p.aim.yaw, p.body.ducked, p.flash > 0, pal, ink);
            }
        }
        for d in &self.dummies {
            draw_dummy(out, d, pal, ink);
        }
        for t in &self.trails {
            let (fade, thick, colour) = if t.stroke {
                (ONE * t.ticks / STROKE, ONE * 6 / 100, rgb(pal.stroke))
            } else {
                (ONE * t.ticks / TRACER, ONE / 100, rgb(pal.tracer))
            };
            out.glow(Mesh::Cube, bar(t.from, t.to, thick), colour, fade);
        }
        for m in &self.marks {
            let at = Affine::translate(m.at.add(m.normal.scale(ONE / 200)))
                .then(&Affine::looking(m.normal.neg()))
                .then(&Affine::scale(v3(ONE * 9 / 100, ONE * 9 / 100, ONE / 100)));
            out.solid(Mesh::Cube, at, rgb(pal.mark), 0);
        }
        let p = &self.players[me];
        if hands && p.alive() {
            let aim = p.aim.turned(pending);
            draw_gun(out, p.eye(alpha), aim.yaw, aim.pitch, &p.arms, pal);
        }
    }
}

/// Where a shot leaves the gun: a little right of and below the eye, ahead.
pub fn muzzle(eye: V3, yaw: i32, pitch: i32) -> V3 {
    let f = facing(yaw, pitch);
    let r = right_of(yaw);
    let up = r.cross(f);
    eye.add(f.scale(ONE * 7 / 10))
        .add(r.scale(ONE * 18 / 100))
        .sub(up.scale(ONE * 14 / 100))
}

/// A thin box from `a` to `b`, `thick` across.
fn bar(a: V3, b: V3, thick: Fx) -> Affine {
    let d = b.sub(a);
    let len = d.len().max(ONE / 100);
    Affine::translate(a.add(d.scale(ONE / 2)))
        .then(&Affine::looking(d.norm()))
        .then(&Affine::scale(v3(thick, thick, len)))
}

/// The bot's figure (M6 brings the skins): a dark body, a pale head under
/// a tall hat, gone pale for a moment when hit.
pub(crate) fn draw_figure(
    out: &mut Shapes,
    feet: V3,
    yaw: i32,
    ducked: bool,
    flashed: bool,
    pal: &Palette,
    ink: Rgb,
) {
    let k = if ducked { ONE * 62 / 100 } else { ONE };
    let turn = Affine::translate(feet).then(&Affine::rot_y(yaw));
    let colour = |c: [i32; 3]| if flashed { rgb(pal.flash) } else { rgb(c) };
    let parts = [
        // T5: as wide as the hit envelope allows (the body 0.92 of its 0.96 m,
        // the head 0.52 of 0.56), and no hat above it, which a shot would
        // pass through
        // (a faceted ball fills its box; the capsule mesh is only 0.36 of it
        // across, which drew the body a third as wide as meant, R6.2)
        (Mesh::Facet, 72, [92, 140, 66], pal.figure),
        (Mesh::Facet, 165, [52, 54, 52], pal.head),
    ];
    for (mesh, y, size, c) in parts {
        let a = turn
            .then(&Affine::translate(v3(0, fx::mul(y * ONE / 100, k), 0)))
            .then(&Affine::scale(v3(
                size[0] * ONE / 100,
                fx::mul(size[1] * ONE / 100, k),
                size[2] * ONE / 100,
            )));
        out.inked(mesh, a, colour(c), ink);
    }
}

/// A straw target on its post, lying on its back while down.
pub(crate) fn draw_dummy(out: &mut Shapes, d: &Dummy, pal: &Palette, ink: Rgb) {
    let base = if d.down > 0 {
        Affine::translate(d.feet).then(&Affine::rot_x(-fx::QUARTER_TURN))
    } else {
        Affine::translate(d.feet)
    };
    let straw = if d.flash > 0 { pal.flash } else { pal.straw };
    // (mesh, centre, size) in centimetres, and colour
    let parts = [
        // T5: filling the hit envelope; the red disc on the chest faces the
        // room's spawn (+z)
        (Mesh::Cyl, [0, 25, 0], [12, 50, 12], pal.wood),
        (Mesh::Facet, [0, 98, 0], [88, 96, 64], straw),
        (Mesh::Facet, [0, 165, 0], [52, 54, 52], straw),
        (Mesh::Cube, [0, 105, 31], [26, 26, 3], pal.stroke),
    ];
    let cm = |c: i32| c * ONE / 100;
    for (mesh, at, size, c) in parts {
        let a = base
            .then(&Affine::translate(v3(cm(at[0]), cm(at[1]), cm(at[2]))))
            .then(&Affine::scale(v3(cm(size[0]), cm(size[1]), cm(size[2]))));
        out.inked(mesh, a, rgb(c), ink);
    }
}

/// A gun's parts, as (mesh, centre, size, colour) in the hands' frame:
/// x right, y up, z back toward the eye, in centimetres.
/// One part of a gun: its mesh, centre, size and colour.
type GunPart = (Mesh, [i32; 3], [i32; 3], [i32; 3]);

fn gun_parts(g: Gun, pal: &Palette) -> Vec<GunPart> {
    let (steel, dark, wood, brass) = (pal.steel, pal.dark_steel, pal.wood, pal.brass);
    match g {
        Gun::Pistol => vec![
            (Mesh::Cube, [0, 2, -6], [4, 4, 18], steel),
            (Mesh::Cube, [0, -5, 0], [3, 9, 4], dark),
        ],
        Gun::Shotgun => vec![
            (Mesh::Cube, [0, 2, -28], [3, 3, 60], steel),
            (Mesh::Cube, [0, -1, -22], [3, 3, 44], dark),
            (Mesh::Cube, [0, -1, -30], [5, 5, 12], wood),
            (Mesh::Cube, [0, 0, 2], [5, 7, 14], steel),
            (Mesh::Cube, [0, -3, 20], [5, 8, 24], wood),
        ],
        Gun::MachinePistol => vec![
            (Mesh::Cube, [0, 1, -8], [5, 6, 22], dark),
            (Mesh::Cube, [0, 2, -24], [2, 2, 10], steel),
            (Mesh::Cube, [0, -8, -6], [3, 12, 3], steel),
            (Mesh::Cube, [0, -6, 2], [3, 8, 4], dark),
        ],
        Gun::Rifle => vec![
            (Mesh::Cube, [0, 3, -40], [2, 2, 70], steel),
            (Mesh::Cube, [0, 1, -2], [4, 6, 16], brass),
            (Mesh::Cube, [0, 0, -25], [4, 4, 25], wood),
            (Mesh::Cube, [0, -2, 20], [5, 9, 26], wood),
            (Mesh::Cube, [0, -6, 0], [1, 5, 8], steel),
        ],
        Gun::Pincel => vec![
            (Mesh::Cube, [0, 2, -35], [3, 3, 60], pal.bone),
            (Mesh::Cube, [0, -1, 10], [6, 7, 22], pal.blue),
            (Mesh::Cube, [0, 4, -20], [4, 2, 12], pal.ochre),
            (Mesh::Facet, [0, 2, -68], [5, 5, 8], pal.umber),
            (Mesh::Facet, [0, 2, -74], [3, 3, 5], pal.stroke),
        ],
    }
}

/// The gun in the hands: below and right of the eye, kicked back for a
/// moment after a shot, lowered while reloading or raising.
fn draw_gun(out: &mut Shapes, eye: V3, yaw: i32, pitch: i32, a: &Arms, pal: &Palette) {
    let frame = Affine::translate(eye).then(&Affine::looking(facing(yaw, pitch)));
    let cm = |c: i32| c * ONE / 100;
    let back = cm(12) * a.recoil / RECOIL;
    let low = if a.reloading > 0 || a.raise > 0 { cm(8) } else { 0 };
    let hold = frame.then(&Affine::translate(v3(cm(18), -cm(16) - low, -cm(42) + back)));
    for (mesh, at, size, colour) in gun_parts(a.gun, pal) {
        let part = hold
            .then(&Affine::translate(v3(cm(at[0]), cm(at[1]), cm(at[2]))))
            .then(&Affine::scale(v3(cm(size[0]), cm(size[1]), cm(size[2]))));
        // no ink: an outline sized for figures across the room is a black
        // band half a metre from the eye (R5.4)
        out.solid(mesh, part, rgb(colour), 0);
    }
    if a.recoil > RECOIL - 2 {
        // the flash at the muzzle
        let tip = hold
            .then(&Affine::translate(v3(0, cm(2), -cm(70))))
            .then(&Affine::scale(v3(cm(12), cm(12), cm(12))));
        out.glow(Mesh::Sphere, tip, rgb(pal.flash), ONE);
    }
}

/// Speed in m/s as text with one decimal (the HUD's readout).
pub fn speed_text(speed: Fx) -> String {
    let tenths = (speed as i64 * 10 + (ONE as i64 / 2)) >> fx::SHIFT;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests;
