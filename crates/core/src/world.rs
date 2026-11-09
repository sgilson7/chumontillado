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
//!
//! M3 adds what flies and what swings. A projectile gun's shot is a body in
//! the air (`Projectile`) that falls, bounces and meets bodies tick by tick;
//! a burst damages and pushes everything near it, its maker too at 30%
//! (CS-Q16). A melee weapon's swing hits what is in its arc during its
//! active ticks (CS-Q7: A); the katana's first six turn projectiles back
//! (CS-Q15), its lunge flings you, and the guadaña's hook pulls you to a
//! wall. A nail can pin a body to a wall behind it.

use std::rc::Rc;

use crate::arms::{Arms, Shot};
use crate::camera::{self, Aim};
use crate::fx::{self, int, Fx, ONE};
use crate::geom::{facing, right_of, v3, Affine, V3};
use crate::gravity::{self, Turn};
use crate::input::{Input, Mouse, ALT, BACK, CROUCH, FORWARD, JUMP, LEFT, RESET, RIGHT, VINE};
use crate::look::{rgb, Look, Palette, Rgb};
use crate::map::{self, Map};
use crate::movement::{Body, Events, Mover, Tuning, TUNING};
use crate::pilot::{self, Pilot};
use crate::prim::Mesh;
use crate::render::{Camera, Shapes};
use crate::sound::{Heard, Voice};
use crate::trace::trace_box;
use crate::trace::Scratch;
use crate::weapons::{self, Alt, Blast, Kind, Target, Weapon};

pub const HEALTH: i32 = 100;
/// Ticks a dead player waits before standing again at their spawn.
pub const RESPAWN: i32 = 120;
/// T27: how far below a map's lowest point a fall fells you (20 m).
pub const FALL_OUT: Fx = int(20);
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
/// Ticks a burst shows.
pub const BURST: i32 = 14;
/// The most projectiles in the air at once; the oldest goes first.
pub const FLYING_MOST: usize = 96;
/// A parry turns projectiles this far from the eye (m).
pub const PARRY_REACH: Fx = int(3);

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
    /// T18: its last hit as it feels it: the shake, the red edge, the arc
    /// to the shooter, the bar's lost part (drawn only)
    pub hurt: crate::hurt::Hurt,
    /// a bot's pilot; none for a person
    pub pilot: Option<Pilot>,
    /// the melee swing under way
    pub swing: Option<Swinging>,
    /// T23: the view's turn of the last few ticks, across (right) and up,
    /// which picks a cut (`weapons::cut_of`); and the way of its last cut
    pub flick: (i32, i32),
    pub last_cut: i32,
    /// the guadaña's line to a wall
    pub hook: Option<Hook>,
    /// T33: the vine it hangs from; the last it let go of, and ticks until
    /// that one takes it again
    pub vine: Option<Clinging>,
    pub last_vine: u16,
    pub vine_rest: i32,
    /// ticks a nail still holds it to a wall
    pub pinned: i32,
    /// M4: a seat nobody holds and the host left empty: not in the match
    pub out: bool,
    /// CS-Q22: its shooting by speed, and ticks since its last wall jump or
    /// slide
    pub shooting: Shooting,
    pub since_trick: i32,
    /// M5: the skin it wears (`skins::SKINS` by number)
    pub skin: u8,
    /// T34: and whether lifelike, else painted. Drawn only, so not summed
    pub lifelike: bool,
    /// T37: its gravity's frame: the identity but on the Cubo
    pub frame: Turn,
    /// T37: the view's turn after its gravity turned: the up and the
    /// forward it turns from, in the world, and ticks left. Drawn only
    pub turning: Option<(V3, V3, i32)>,
}

/// CS-Q22 (the journal's Feel mockups page): a seat's shots and hits in
/// four bands of how fast it was going when it fired, and its ticks alive
/// and at run speed or more. Part of the world, so every browser's copy
/// agrees and rollback keeps it right; it changes nothing about play.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shooting {
    pub fired: [i32; BANDS],
    pub hit: [i32; BANDS],
    pub ticks: i32,
    pub fast_ticks: i32,
}

/// The bands: under 3 m/s, 3 to 9 (a run), over 9, and within half a
/// second of a wall jump or a slide, which takes precedence.
pub const BANDS: usize = 4;
pub const BAND_WALK: Fx = int(3);
pub const BAND_TRICK_TICKS: i32 = 30;

impl Player {
    /// T33: off the vine it hangs from, which will not take it again for
    /// `VINE_AGAIN` ticks.
    fn let_go_of_vine(&mut self) {
        if self.vine.take().is_some() {
            self.vine_rest = VINE_AGAIN;
        }
    }

    /// The band a shot fired now falls in.
    pub fn band(&self) -> usize {
        let v = self.body.speed();
        if self.since_trick < BAND_TRICK_TICKS {
            3
        } else if v >= TUNING.run {
            2
        } else if v >= BAND_WALK {
            1
        } else {
            0
        }
    }
}

/// A melee swing under way: its weapon, its age in ticks, whether it is the
/// katana's lunge, its cut's way on the screen (T23; none for a thrust),
/// and which bodies it has hit (one bit each, seats then straw targets), so
/// a swing hits each once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swinging {
    pub weapon: Weapon,
    pub age: i32,
    pub lunge: bool,
    pub cut: Option<i32>,
    pub struck: u64,
}

/// T33: a hold on a vine: which of the map's, and how far the hands are
/// below its top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clinging {
    pub vine: u16,
    pub length: Fx,
}

/// T33: how far across from a vine the hands take hold of it (m), how high
/// over the feet they hold (m), the push the movement keys give a swing
/// (m/s²), the lift a let-go gives (m/s), and the ticks before the vine let
/// go of takes you again.
pub const VINE_REACH: Fx = ONE * 8 / 10;
pub const VINE_GRIP: Fx = ONE * 17 / 10;
pub const VINE_PUMP: Fx = int(6);
pub const VINE_LET_GO: Fx = int(3);
pub const VINE_AGAIN: i32 = 30;

/// The guadaña's line: where it holds, and ticks left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hook {
    pub at: V3,
    pub ticks: i32,
}

/// A shot in the air.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Projectile {
    pub weapon: Weapon,
    /// the seat it came from, or the seat that parried it last
    pub owner: usize,
    pub at: V3,
    /// last tick's place, for drawing between ticks
    pub prev: V3,
    pub velocity: V3,
    pub age: i32,
    pub bounces: i32,
    /// the band its owner fired it in (CS-Q22), and whether it has hit
    /// another body yet
    pub band: u8,
    pub scored: bool,
}

/// An X on the head a shot struck: where, whose shot, ticks left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadMark {
    pub at: V3,
    pub by: usize,
    pub ticks: i32,
}

/// Ticks an X stays on a struck head (a third of a second).
pub const HEAD_MARK: i32 = 20;

/// A burst, drawn as a ball of light that grows and fades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Burst {
    pub at: V3,
    pub radius: Fx,
    pub ticks: i32,
}

impl Player {
    pub fn at(spawn: (V3, i32)) -> Player {
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
            hurt: crate::hurt::Hurt::default(),
            pilot: None,
            swing: None,
            flick: (0, 0),
            last_cut: weapons::CUT_RIGHT,
            hook: None,
            vine: None,
            last_vine: 0,
            vine_rest: 0,
            pinned: 0,
            out: false,
            shooting: Shooting::default(),
            since_trick: 1_000,
            skin: 0,
            lifelike: false,
            frame: Turn::IDENTITY,
            turning: None,
        }
    }

    /// Back at its spawn, as new, keeping its look speed, its pilot and its
    /// shooting's counts.
    fn respawn(&mut self) {
        let (pct, pilot, shooting, skin, out, lifelike) = (
            self.aim.pct,
            self.pilot,
            self.shooting,
            self.skin,
            self.out,
            self.lifelike,
        );
        *self = Player::at(self.spawn);
        self.aim.pct = pct;
        self.pilot = pilot;
        self.shooting = shooting;
        self.skin = skin;
        self.lifelike = lifelike;
        self.out = out;
    }

    /// The eye between last tick and this one, `alpha` of the way.
    pub fn eye(&self, alpha: Fx) -> V3 {
        let feet = self.prev.lerp(self.body.origin, alpha);
        feet.add(self.up().scale(fx::lerp(self.prev_eye, self.body.eye(), alpha)))
    }

    /// T37: its up in the world.
    pub fn up(&self) -> V3 {
        self.frame.up()
    }

    /// T37: where it looks, in the world.
    pub fn look(&self) -> V3 {
        gravity::look(&self.frame, self.aim.yaw, self.aim.pitch)
    }

    /// T37: its speed across its own floor (m/s).
    pub fn speed(&self) -> Fx {
        self.frame.back(self.body.velocity).len_xz()
    }

    pub fn alive(&self) -> bool {
        self.dead == 0 && !self.out
    }

    /// What a shot meets of it.
    pub fn target(&self) -> Target {
        Target::of(self.body.origin, self.body.eye()).turned(self.frame)
    }

    pub fn eye_now(&self) -> V3 {
        self.body.origin.add(self.up().scale(self.body.eye()))
    }

    /// The movement's numbers while holding its weapon: the navaja's faster
    /// run, the katana's fourth wall jump, the puñal's higher jump.
    pub fn tuning(&self) -> Tuning {
        let mut t = TUNING;
        if let Some(sw) = self.arms.weapon.swing() {
            t.run = t.run * sw.run_pct / 100;
            t.wall_jumps = sw.wall_jumps;
            t.jump = t.jump * sw.jump_pct / 100;
        }
        t
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
    /// shared, not copied, by rollback's copies of the world (M4): it
    /// never changes in a match
    pub map: Rc<Map>,
    /// T37: on a map whose gravity turns, the map as each of the 24 frames
    /// meets it (`gravity::all` order); empty elsewhere
    pub turned: Rc<Vec<Map>>,
    /// T32: the maps a match plays in order, and its weapons placed or at
    /// random
    pub plan: crate::plan::Plan,
    pub players: Vec<Player>,
    pub dummies: Vec<Dummy>,
    pub trails: Vec<Trail>,
    pub marks: Vec<Mark>,
    pub flying: Vec<Projectile>,
    pub bursts: Vec<Burst>,
    /// the X held on a head a shot struck, seen by the shooter (the
    /// journal's Feel page, claim 1)
    pub head_marks: Vec<HeadMark>,
    /// the field of view across, degrees (a reader's setting)
    pub fov: i32,
    /// an online match (M4)
    pub online: bool,
    /// M5: a match's rounds, if this is a match
    pub rounds: Option<crate::rounds::Rounds>,
    /// M5: the weapons lying on the map this round
    pub pickups: Vec<Pickup>,
    /// M6: the sounds of the last tick run (`sound.rs`)
    pub heard: Vec<crate::sound::Heard>,
    /// T19: in a match, where the bots can walk on the map and the way
    /// round its walls; shared like the map
    pub nav: Option<Rc<crate::nav::Nav>>,
    /// T20: how good the bots are, the same for every bot; a match's is
    /// the host's setting, sent with its start
    pub skill: pilot::Skill,
}

/// A weapon lying on the map (CS-Q10: A), until someone walks over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pickup {
    pub at: V3,
    pub weapon: Weapon,
    pub taken: bool,
}

/// A pickup is taken within this of a body's feet, across (m) and up.
pub const PICKUP_REACH: Fx = ONE * 13 / 10;
pub const PICKUP_UP: Fx = int(2);
/// A seat felled in a round is down until the next one (it watches).
pub const DOWN_FOR_ROUND: i32 = i32::MAX / 2;

/// The straw targets in the practice room, down the hop lane.
pub const DUMMIES: [(i32, i32); 3] = [(8, -4), (14, -24), (22, -28)];
/// The bot's spawn: the first corner of its loop.
pub const BOT_SPAWN: (i32, i32) = (8, -28);

/// M4: the world as rollback runs it, on every seat's record in seat order.
impl crate::rollback::Sim for World {
    fn tick(&mut self, inputs: &[Input; crate::online::SEATS]) {
        // `step` takes a record for each seat a person holds, in order
        let people: Vec<Input> = self
            .players
            .iter()
            .zip(inputs)
            .filter(|(p, _)| p.pilot.is_none() && !p.out)
            .map(|(_, i)| *i)
            .collect();
        self.step(&people);
    }

    fn sum(&self) -> u64 {
        World::sum(self)
    }

    fn hand_to_bot(&mut self, seat: usize) {
        if let Some(p) = self.players.get_mut(seat) {
            if p.pilot.is_none() {
                p.pilot = Some(Pilot {
                    shoots: true,
                    ..Pilot::default()
                });
            }
        }
    }
    fn heard(&self) -> &[Heard] {
        &self.heard
    }
}

impl World {
    /// The practice room with the player at its spawn, three straw targets,
    /// and a bot that hops round its loop without shooting.
    pub fn practice() -> World {
        World::room(Bot::Moves)
    }

    /// The practice room with its bot moving only, or shooting back.
    pub fn room(bot: Bot) -> World {
        let map = Rc::new(map::practice());
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
            flying: Vec::new(),
            bursts: Vec::new(),
            head_marks: Vec::new(),
            fov: camera::FOV_START,
            online: false,
            rounds: None,
            pickups: Vec::new(),
            plan: crate::plan::Plan::default(),
            heard: Vec::new(),
            nav: None,
            turned: Rc::new(Vec::new()),
            skill: pilot::Skill::Normal,
        }
    }

    /// A match (M4, M5): four seats, those `seats` does not hold played by
    /// bots that shoot back (CS-Q17: A) unless the host left them `empty`,
    /// each in its skin; rounds on the three maps in turn, from the first.
    pub fn online(
        seats: crate::online::Seats,
        empty: crate::online::Seats,
        skins: [u8; 4],
        lifelike: crate::online::Seats,
    ) -> World {
        let mut w = World::room(Bot::Shoots);
        w.dummies.clear();
        w.players = (0..crate::online::SEATS)
            .map(|s| {
                let mut p = Player::at(w.map.spawns[s]);
                p.skin = skins[s];
                p.lifelike = crate::online::holds(lifelike, s);
                if crate::online::holds(empty, s) && !crate::online::holds(seats, s) {
                    p.out = true;
                } else if !crate::online::holds(seats, s) {
                    p.pilot = Some(Pilot {
                        shoots: true,
                        ..Pilot::default()
                    });
                }
                p
            })
            .collect();
        w.online = true;
        w.rounds = Some(crate::rounds::Rounds::new());
        w.start_round(0);
        w
    }

    /// M5: a round's start at place `k` in the plan's order (T32; with
    /// the plan of every map, map `k`): every seat in the match whole at
    /// its spawn with the pistol and navaja, the weapons back on the map,
    /// as it places them or drawn at random, and nothing left in the air.
    pub fn start_round(&mut self, k: usize) {
        let m = crate::maps::map(self.plan.map(k));
        let placed: Vec<Weapon> = m.pickups.iter().map(|p| p.1).collect();
        let round = self.rounds.map_or(0, |r| r.round.max(0) as usize);
        let weapons = self
            .plan
            .weapons_at(&placed, k.wrapping_mul(1000).wrapping_add(round));
        self.pickups = m
            .pickups
            .iter()
            .zip(weapons)
            .map(|(&(at, _), weapon)| Pickup {
                at,
                weapon,
                taken: false,
            })
            .collect();
        for (s, p) in self.players.iter_mut().enumerate() {
            p.spawn = m.spawns[s % m.spawns.len()];
            p.respawn();
            p.arms = Arms::for_round();
            p.prev = p.body.origin;
        }
        self.nav = Some(Rc::new(crate::nav::Nav::of(&m)));
        self.turned = Rc::new(match m.cube {
            Some(_) => gravity::all().iter().map(|t| m.turned_back(t)).collect(),
            None => Vec::new(),
        });
        self.map = Rc::new(m);
        self.flying.clear();
        self.bursts.clear();
        self.trails.clear();
        self.marks.clear();
        self.head_marks.clear();
    }

    /// M4: one number for the whole state a tick can change, the same on
    /// every browser that ran the same records (FNV-1a over the fields in a
    /// fixed order). Two browsers whose sums differ have come apart.
    pub fn sum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: i64| {
            h ^= v as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        };
        mix(self.tick as i64);
        let v3s = |mix: &mut dyn FnMut(i64), v: V3| {
            mix(v.x as i64);
            mix(v.y as i64);
            mix(v.z as i64);
        };
        for p in &self.players {
            v3s(&mut mix, p.body.origin);
            v3s(&mut mix, p.body.velocity);
            for n in [
                p.aim.yaw,
                p.aim.pitch,
                p.health,
                p.dead,
                p.pinned,
                p.out as i32,
                p.body.wall_jumps,
                p.arms.weapon as i32,
                p.arms.cooldown,
                p.arms.charge,
                p.body.on_ground as i32,
                p.body.ducked as i32,
                p.pilot.is_some() as i32,
            ] {
                mix(n as i64);
            }
            for r in p.arms.rounds {
                mix(r as i64);
            }
            if let Some(sw) = p.swing {
                mix(sw.age as i64);
                mix(sw.struck as i64);
                mix(sw.cut.unwrap_or(i32::MIN) as i64);
            }
            mix(p.flick.0 as i64);
            mix(p.flick.1 as i64);
            mix(p.last_cut as i64);
            if let Some(hk) = p.hook {
                v3s(&mut mix, hk.at);
            }
            if let Some(c) = p.vine {
                mix(c.vine as i64);
                mix(c.length as i64);
            }
            mix(p.vine_rest as i64);
            mix(p.arms.burst_left as i64);
            mix(p.frame.index() as i64);
        }
        for q in &self.flying {
            v3s(&mut mix, q.at);
            v3s(&mut mix, q.velocity);
            mix(q.owner as i64);
        }
        for d in &self.dummies {
            mix(d.health as i64);
            mix(d.down as i64);
        }
        if let Some(r) = self.rounds {
            for n in [r.map as i32, r.round, r.ticks, r.winner.map_or(-1, |w| w as i32)] {
                mix(n as i64);
            }
            for n in r.scores {
                mix(n as i64);
            }
        }
        for k in &self.pickups {
            mix(k.taken as i64);
        }
        h
    }

    /// What a bot in seat `id` knows this tick: itself, its map's route,
    /// and the chest of the nearest other living seat it can see. In the
    /// practice room only the person is a target (the room's bot is the
    /// only other); in a match every other seat is, bots too, or a round
    /// of bots would never end (R10.5).
    fn view(&self, id: usize, scratch: &mut Scratch) -> pilot::View<'_> {
        let me = &self.players[id];
        let eye = me.body.origin.add(v3(0, me.body.eye(), 0));
        let in_match = self.rounds.is_some();
        let seen = self
            .players
            .iter()
            .enumerate()
            .filter(|(k, p)| *k != id && p.alive() && (in_match || p.pilot.is_none()))
            .map(|(_, p)| {
                (
                    p.body.origin.add(v3(0, p.body.eye() - ONE / 4, 0)),
                    p.body.velocity,
                )
            })
            .filter(|&(c, _)| {
                let to = c.sub(eye);
                let d = to.len();
                d <= pilot::SIGHT && self.clear(scratch, eye, c)
            })
            .min_by_key(|(c, _)| c.sub(eye).len());
        let chest = seen.map(|(c, _)| c);
        // T20: hit this tick, from whom; and low, the nearest spawn out of
        // the target's sight to fall back to
        let hit_from = me.hurt.from.filter(|_| me.hurt.ticks >= crate::hurt::HURT - 1);
        let low = me.health <= self.skill.traits().fall_back_at;
        let cover = chest.filter(|_| low && in_match).and_then(|c| {
            self.map
                .spawns
                .iter()
                .map(|&(at, _)| at)
                .filter(|&at| !self.clear(scratch, c, at.add(v3(0, me.body.eye() - ONE / 4, 0))))
                .min_by_key(|at| at.sub(me.body.origin).len_xz())
        });
        // T19: in a match, a bot hunts the nearest other seat standing,
        // seen or not, so a round of bots ends
        let hunt = self
            .players
            .iter()
            .enumerate()
            .filter(|(k, p)| in_match && *k != id && p.alive())
            .map(|(_, p)| p.body.origin)
            .min_by_key(|f| f.sub(me.body.origin).len());
        pilot::View {
            route: &self.map.bot_loop,
            nav: self.nav.as_deref().filter(|_| in_match),
            hunt,
            speed: me.body.velocity.len_xz(),
            frozen: self.rounds.is_some_and(|r| r.frozen()),
            reach: me.arms.weapon.swing().map(|s| s.reach),
            tick: self.tick,
            feet: me.body.origin,
            eye,
            yaw: me.aim.yaw,
            pitch: me.aim.pitch,
            on_ground: me.body.on_ground,
            chest,
            skill: self.skill,
            chest_velocity: seen.map_or(V3::ZERO, |(_, vel)| vel),
            hit_from,
            health: me.health,
            cover,
            weapon: me.arms.weapon,
            owned: me.arms.owned,
            pickups: &self.pickups,
        }
    }

    /// T20: whether nothing on the map stands between `a` and `b`.
    fn clear(&self, scratch: &mut Scratch, a: V3, b: V3) -> bool {
        let to = b.sub(a);
        let d = to.len();
        weapons::wall_distance(&self.map, scratch, a, to.norm(), d) >= d - ONE / 50
    }

    /// One tick. `inputs` holds a record for each person's seat in order;
    /// a bot's is made by its pilot. Every seat turns and moves, then the
    /// shots fired are resolved in seat order.
    pub fn step(&mut self, inputs: &[Input]) {
        self.heard.clear();
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
                // a seat left empty takes no record
                None if self.players[id].out => Input::default(),
                None => given.next().copied().unwrap_or_default(),
            };
            // M5: in a round's count, after it, and after the match, nobody
            // moves or fires; the mouse still looks
            let r = if self.rounds.is_some_and(|x| x.frozen()) {
                Input {
                    keys: 0,
                    buttons: 0,
                    ..r
                }
            } else {
                r
            };
            records.push(r);
        }

        // T27: whoever has fallen out of the map, `FALL_OUT` below its
        // lowest point, is felled by the fall, as by their own shot
        let floor = self.map.lowest() - FALL_OUT;
        for k in 0..self.players.len() {
            let p = &self.players[k];
            if !p.out && p.alive() && p.body.origin.y < floor {
                let all = p.health;
                self.hurt(k, k, all);
            }
        }

        let mut shots: Vec<(usize, Shot)> = Vec::new();
        let mut moved: Vec<Heard> = Vec::new();
        // online (M4), Backspace does not stand you back at your spawn whole
        let online = self.online;
        let inside = self.map.inside();
        let held = Rc::clone(&self.map);
        let vines = &held.vines;
        let turned = Rc::clone(&self.turned);
        {
            let mut mover = Mover::new(&self.map);
            for (id, (p, input)) in self.players.iter_mut().zip(&records).enumerate() {
                p.prev = p.body.origin;
                p.prev_eye = p.body.eye();
                p.hit_mark = (p.hit_mark - 1).max(0);
                p.kill_mark = (p.kill_mark - 1).max(0);
                p.flash = (p.flash - 1).max(0);
                p.hurt.step(p.health);
                if p.out {
                    continue;
                }
                if p.dead > 0 {
                    // in a round the felled stay down until the next
                    if p.dead != DOWN_FOR_ROUND {
                        p.dead -= 1;
                        if p.dead == 0 {
                            p.respawn();
                        }
                    }
                    continue;
                }
                if input.key(RESET) && !online {
                    p.respawn();
                    continue;
                }
                let (from, from_pitch) = (p.aim.yaw, p.aim.pitch);
                p.aim = p.aim.turned(Mouse {
                    dx: input.dx,
                    dy: input.dy,
                    wheel: input.wheel,
                });
                // T23: a yaw grows to the left; the flick's across, right
                let turn = (-fx::wrap(p.aim.yaw.wrapping_sub(from)), p.aim.pitch - from_pitch);
                p.flick = (p.flick.0 * 5 / 6 + turn.0, p.flick.1 * 5 / 6 + turn.1);
                pull(p, input);
                let swinging = swing_on_vine(p, input, vines);
                // on a vine the keys pump the swing, not the mover's air
                // control, and crouch does not slam
                let input = &if swinging {
                    Input {
                        keys: input.keys & !(FORWARD | BACK | LEFT | RIGHT | CROUCH),
                        ..*input
                    }
                } else {
                    *input
                };
                if p.pinned > 0 {
                    // held to the wall by a nail: it aims and fires, but stays
                    p.pinned -= 1;
                    p.body.velocity = V3::ZERO;
                    p.events = Events::default();
                } else if let Some(cube) = held.cube {
                    // T37: moved in its own frame against the map turned
                    // into it, then perhaps onto another face
                    turn_and_move(p, input, from, &turned, cube);
                } else {
                    mover.t = p.tuning();
                    p.events = mover.tick(&mut p.body, input, from, p.aim.yaw);
                    if let Some(inside) = inside {
                        keep_inside(&mut p.body, inside);
                    }
                }
                p.top_speed = p.top_speed.max(p.body.speed());
                p.since_trick = p.since_trick.saturating_add(1);
                if p.events.wall_jumped || p.events.slid || p.events.slide_jumped {
                    p.since_trick = 0;
                }
                p.shooting.ticks += 1;
                if p.body.speed() >= TUNING.run {
                    p.shooting.fast_ticks += 1;
                }
                if let Some(s) = p.arms.tick(input) {
                    if s.weapon.swing().is_none() {
                        let band = p.band();
                        p.shooting.fired[band] += 1;
                    }
                    shots.push((id, s));
                }
                // M6: what the body did, heard where it is
                let e = p.events;
                for (did, voice) in [
                    (e.wall_jumped, Voice::WallJump),
                    (e.jumped && !e.wall_jumped, Voice::Jump),
                    (e.landed, Voice::Land),
                    (e.slid, Voice::Slide),
                    (e.slammed, Voice::Slam),
                ] {
                    if did {
                        moved.push(Heard {
                            voice,
                            at: p.body.origin,
                            by: id,
                        });
                    }
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
        self.heard.extend(moved);
        self.take_pickups();
        for &(id, s) in &shots {
            let voice = if s.weapon.swing().is_some() {
                Voice::Swing
            } else {
                Voice::Shot(s.weapon)
            };
            let at = self.players[id].eye_now();
            self.heard.push(Heard { voice, at, by: id });
        }
        for (id, s) in shots {
            match s.weapon.stats().kind {
                Kind::Hitscan => self.shoot(id, s, &mut scratch),
                Kind::Projectile(_) => self.launch(id, s),
                Kind::Melee(_) => self.swing(id, s, &mut scratch),
            }
        }
        self.swings();
        self.fly(&mut scratch);

        for t in &mut self.trails {
            t.ticks -= 1;
        }
        self.trails.retain(|t| t.ticks > 0);
        for b in &mut self.bursts {
            b.ticks -= 1;
        }
        self.bursts.retain(|b| b.ticks > 0);
        for m in &mut self.head_marks {
            m.ticks -= 1;
        }
        self.head_marks.retain(|m| m.ticks > 0);
        self.step_rounds();
        self.tick += 1;
    }

    /// M5: a weapon lying where a living body stands is taken, full, into
    /// its hands; each is taken once a round, by the first seat to reach it.
    fn take_pickups(&mut self) {
        for k in self.pickups.iter_mut().filter(|k| !k.taken) {
            if let Some((by, p)) = self.players.iter_mut().enumerate().find(|(_, p)| {
                let d = p.body.origin.sub(k.at);
                p.alive() && d.len_xz() <= PICKUP_REACH && d.y.abs() <= PICKUP_UP
            }) {
                p.arms.pick_up(k.weapon);
                k.taken = true;
                self.heard.push(Heard {
                    voice: Voice::Pickup,
                    at: k.at,
                    by,
                });
            }
        }
    }

    /// M5: a tick of the match's rounds: the count, the round, who takes it,
    /// and the next round's map.
    fn step_rounds(&mut self) {
        let Some(mut r) = self.rounds else { return };
        let present = self.players.iter().filter(|p| !p.out).count();
        let alive: Vec<usize> = (0..self.players.len())
            .filter(|&k| self.players[k].alive())
            .collect();
        let alone = (alive.len() == 1).then(|| alive[0]);
        let before = r;
        let next = crate::rounds::tick(&mut r, alive.len(), alone, present, self.plan.len());
        self.rounds = Some(r);
        // M6: the count's three beats, its go, and a round's end, heard by all
        use crate::rounds::Phase;
        let beat = match (before.phase, r.phase) {
            (Phase::Count, Phase::Count) if r.ticks % 60 == 1 => Some(Voice::Count),
            (Phase::Count, Phase::Live) => Some(Voice::Go),
            (Phase::Live, Phase::Over) => Some(Voice::RoundEnd),
            _ => None,
        };
        if let Some(voice) = beat {
            self.heard.push(Heard {
                voice,
                at: V3::ZERO,
                by: 0,
            });
        }
        if let crate::rounds::Next::Round(map) = next {
            self.start_round(map);
        }
    }

    /// M6: the seat whose ears a seat hears with: its own, or while it is
    /// down in a match, the one it watches.
    pub fn watching_or(&self, me: usize) -> usize {
        if self.rounds.is_some() && !self.players[me].alive() {
            self.watching(me)
        } else {
            me
        }
    }

    /// M5: the seat a felled player watches: the first one still standing
    /// after it, round the seats; itself if none.
    pub fn watching(&self, me: usize) -> usize {
        let n = self.players.len();
        (1..n)
            .map(|k| (me + k) % n)
            .find(|&k| self.players[k].alive())
            .unwrap_or(me)
    }

    /// Every body a shot, burst or swing from seat `by` could meet: the
    /// other living players (or, with `itself`, the seat too), then the
    /// standing targets.
    fn targets(&self, by: usize, itself: bool) -> Vec<Option<Target>> {
        let mut out: Vec<Option<Target>> = self
            .players
            .iter()
            .enumerate()
            .map(|(k, p)| ((k != by || itself) && p.alive()).then(|| p.target()))
            .collect();
        out.extend(
            self.dummies
                .iter()
                .map(|d| (d.down == 0).then(|| Target::of(d.feet, ONE * 165 / 100))),
        );
        out
    }

    /// `dmg` to body `k` of a `targets` list from seat `by`: a seat, or past
    /// the seats a straw target. Whether it fell.
    fn hurt(&mut self, by: usize, k: usize, dmg: i32) -> bool {
        let seats = self.players.len();
        if k != by {
            self.players[by].hit_mark = MARK;
            let at = self.players[by].eye_now();
            self.heard.push(Heard {
                voice: Voice::Hit,
                at,
                by,
            });
        }
        if k < seats {
            let at = self.players[k].body.origin;
            // T18: your own blast shakes you but has no shooter to point to
            let from = (k != by).then(|| self.players[by].eye_now());
            self.heard.push(Heard {
                voice: Voice::Hurt,
                at,
                by: k,
            });
            let victim = &mut self.players[k];
            victim.hurt.struck(victim.health, dmg, from);
            victim.health -= dmg;
            victim.flash = FLASH;
            if victim.health <= 0 {
                victim.health = 0;
                victim.dead = if self.rounds.is_some() {
                    DOWN_FOR_ROUND
                } else {
                    RESPAWN
                };
                victim.swing = None;
                victim.hook = None;
                victim.pinned = 0;
                if k != by {
                    self.players[by].kill_mark = MARK;
                    let at = self.players[by].eye_now();
                    self.heard.push(Heard {
                        voice: Voice::Kill,
                        at,
                        by,
                    });
                }
                return true;
            }
        } else {
            let dummy = &mut self.dummies[k - seats];
            dummy.health -= dmg;
            dummy.flash = FLASH;
            if dummy.health <= 0 {
                dummy.down = TARGET_DOWN;
                return true;
            }
        }
        false
    }

    fn mark(&mut self, at: V3, normal: V3) {
        if self.marks.len() >= MARKS_MOST {
            self.marks.remove(0);
        }
        self.marks.push(Mark { at, normal });
    }

    /// A projectile gun's shot: each pellet leaves the muzzle along its way
    /// at the gun's speed, the Campana's with its lift.
    fn launch(&mut self, id: usize, s: Shot) {
        let g = s.weapon.stats();
        let Kind::Projectile(f) = g.kind else { return };
        let p = &self.players[id];
        let (yaw, pitch) = (p.aim.yaw, p.aim.pitch);
        let from = muzzle(p.eye_now(), yaw, pitch);
        let p_band = p.band();
        let salt = (self.tick as u32).wrapping_mul(31).wrapping_add(id as u32);
        for d in weapons::pellets(g, yaw, pitch, salt) {
            if self.flying.len() >= FLYING_MOST {
                self.flying.remove(0);
            }
            self.flying.push(Projectile {
                weapon: s.weapon,
                owner: id,
                at: from,
                prev: from,
                velocity: d.scale(f.speed).add(v3(0, f.lift, 0)),
                age: 0,
                bounces: f.bounces,
                band: p_band as u8,
                scored: false,
            });
        }
    }

    /// One tick of every projectile: it falls, then moves; the first body
    /// or wall on the way stops it, bounces it, or bursts it; its fuse bursts
    /// it where it is.
    fn fly(&mut self, scratch: &mut Scratch) {
        let dt = ONE / crate::movement::HZ;
        let mut k = 0;
        while k < self.flying.len() {
            let mut q = self.flying[k];
            let f = q.weapon.flight().expect("only projectiles fly");
            q.prev = q.at;
            q.age += 1;
            q.velocity.y -= fx::mul(f.gravity, dt);
            let step = q.velocity.scale(dt);
            let len = step.len();
            let mut gone = false;
            if len > 0 {
                let d = step.norm();
                let end = q.at.add(step);
                let wall = trace_box(&self.map, scratch, q.at, end, V3::ZERO, V3::ZERO);
                let wall_at = fx::mul(wall.fraction, len);
                let targets = self.targets(q.owner, false);
                let body = targets
                    .iter()
                    .enumerate()
                    .filter_map(|(who, t)| {
                        let (at, _) = weapons::meets(q.at, d, t.as_ref()?, f.size)?;
                        (at <= wall_at).then_some((at, who))
                    })
                    .min();
                if let Some((at, who)) = body {
                    let hit = q.at.add(d.scale(at));
                    let dmg = q.weapon.stats().damage;
                    self.hurt(q.owner, who, dmg);
                    if who != q.owner && !q.scored {
                        self.players[q.owner].shooting.hit[q.band as usize] += 1;
                        q.scored = true;
                    }
                    if f.pins > 0 && who < self.players.len() {
                        let behind = weapons::wall_distance(&self.map, scratch, hit, d, weapons::PIN_BEHIND);
                        if behind < weapons::PIN_BEHIND && self.players[who].alive() {
                            self.players[who].pinned = f.pins;
                        }
                    }
                    if let Some(b) = f.blast {
                        self.burst(q.owner, hit, &b, Some(who), scratch);
                    }
                    gone = true;
                } else if wall.hit() {
                    let n = wall.normal;
                    if q.bounces > 0 {
                        // off the face, keeping `keep_pct` of the speed
                        let v = q.velocity;
                        let back = n.scale(2 * v.dot(n));
                        q.velocity = v.sub(back).scale(ONE * f.keep_pct / 100);
                        q.at = wall.end.add(n.scale(ONE / 100));
                        q.bounces -= 1;
                        // a hard bounce is heard; a grenade settling is not
                        if q.velocity.len() > int(3) {
                            self.heard.push(Heard {
                                voice: Voice::Bounce,
                                at: q.at,
                                by: q.owner,
                            });
                        }
                    } else if let Some(b) = f.blast {
                        if self.burst(q.owner, wall.end.add(n.scale(ONE / 50)), &b, None, scratch)
                            && !q.scored
                        {
                            self.players[q.owner].shooting.hit[q.band as usize] += 1;
                        }
                        gone = true;
                    } else {
                        self.mark(wall.end, n);
                        gone = true;
                    }
                } else {
                    q.at = end;
                }
            }
            if !gone && f.fuse > 0 && q.age >= f.fuse {
                if let Some(b) = f.blast {
                    if self.burst(q.owner, q.at, &b, None, scratch) && !q.scored {
                        self.players[q.owner].shooting.hit[q.band as usize] += 1;
                    }
                }
                gone = true;
            }
            if gone {
                self.flying.remove(k);
            } else {
                self.flying[k] = q;
                k += 1;
            }
        }
    }

    /// A burst at `at` from seat `by`: damage to every body it can see by
    /// how near its surface is, `by`'s own at 30% (CS-Q16), and a push away
    /// from it on every player, its maker too: the weapon jump. `spared`
    /// took the direct hit already.
    /// Whether it hurt any body but its maker's.
    fn burst(&mut self, by: usize, at: V3, b: &Blast, spared: Option<usize>, scratch: &mut Scratch) -> bool {
        let mut hurt_another = false;
        self.bursts.push(Burst {
            at,
            radius: b.outer,
            ticks: BURST,
        });
        self.heard.push(Heard {
            voice: Voice::Burst,
            at,
            by,
        });
        let targets = self.targets(by, true);
        let seats = self.players.len();
        for (k, t) in targets.iter().enumerate() {
            let Some(t) = t else { continue };
            let gap = weapons::gap_to(t, at);
            if gap > b.outer {
                continue;
            }
            // a wall between keeps it out
            let near = weapons::nearest_on(t, at);
            let to = near.sub(at);
            let far = to.len();
            if far > ONE / 20
                && weapons::wall_distance(&self.map, scratch, at, to.norm(), far) < far - ONE / 20
            {
                continue;
            }
            if k < seats {
                // pushed from the burst toward the body's middle, so a burst
                // at the feet throws it up
                let p = &mut self.players[k];
                let middle = p.body.origin.add(v3(0, ONE * 9 / 10, 0));
                let away = middle.sub(at);
                let dir = if away.len() > ONE / 100 {
                    away.norm()
                } else {
                    V3::UP
                };
                p.body.velocity = p.body.velocity.add(dir.scale(weapons::blast_push(b, gap)));
                if dir.y > 0 {
                    p.body.on_ground = false;
                }
            }
            if Some(k) == spared {
                continue;
            }
            let mut dmg = weapons::blast_damage(b, gap);
            if k == by {
                dmg = dmg * weapons::SELF_PCT / 100;
            }
            if dmg > 0 {
                self.hurt(by, k, dmg);
                hurt_another |= k != by;
            }
        }
        hurt_another
    }

    /// A melee weapon's use: a swing that carries you on a little, the
    /// katana's lunge that flings you along the aim, or the guadaña's hook
    /// thrown at the wall in front.
    fn swing(&mut self, id: usize, s: Shot, scratch: &mut Scratch) {
        let Some(sw) = s.weapon.swing() else { return };
        let p = &self.players[id];
        let (eye, yaw, pitch) = (p.eye_now(), p.aim.yaw, p.aim.pitch);
        if s.alt && sw.alt == Alt::Hook {
            let d = facing(yaw, pitch);
            let reach = weapons::wall_distance(&self.map, scratch, eye, d, weapons::HOOK_REACH);
            if reach < weapons::HOOK_REACH {
                let p = &mut self.players[id];
                p.hook = Some(Hook {
                    at: eye.add(d.scale(reach)),
                    ticks: weapons::HOOK_TICKS,
                });
                if p.body.on_ground && d.y > 0 {
                    p.body.velocity.y = p.body.velocity.y.max(weapons::HOOK_LIFT);
                    p.body.on_ground = false;
                }
            }
            return;
        }
        let p = &mut self.players[id];
        let lunge = s.alt && sw.alt == Alt::Lunge;
        if lunge {
            let along = facing(yaw, pitch).scale(weapons::LUNGE_SPEED.max(p.body.speed()));
            p.body.velocity = v3(along.x, along.y.max(weapons::LUNGE_LIFT), along.z);
            p.body.on_ground = false;
            p.let_go_of_vine();
        } else if sw.launch > 0 {
            // T30: the garrocha flings you once wound up (`swings`)
        } else {
            p.body.velocity = p.body.velocity.add(facing(yaw, 0).scale(sw.carry));
        }
        // T23: the cut the flick picks; none, a level cut the other way
        // from the last, or a knife's thrust
        let cut = match (lunge || sw.launch > 0, weapons::cut_of(p.flick)) {
            (true, _) => None,
            (false, Some(c)) => Some(c),
            (false, None) if sw.thrusts => None,
            (false, None) if weapons::is_level(p.last_cut) => {
                Some(fx::wrap(p.last_cut.wrapping_add(fx::HALF_TURN)))
            }
            (false, None) => Some(weapons::CUT_LEFT),
        };
        if let Some(c) = cut {
            p.last_cut = c;
        }
        p.swing = Some(Swinging {
            weapon: s.weapon,
            age: 0,
            lunge,
            cut,
            struck: 0,
        });
    }

    /// One tick of every swing under way: the katana's parry in its first
    /// ticks, then a hit on each body in the arc during the active ticks.
    fn swings(&mut self) {
        for id in 0..self.players.len() {
            let Some(mut s) = self.players[id].swing else {
                continue;
            };
            if !self.players[id].alive() {
                self.players[id].swing = None;
                continue;
            }
            s.age += 1;
            let sw = s.weapon.swing().expect("a swing is a melee weapon's");
            let p = &self.players[id];
            let (eye, yaw, pitch) = (p.eye_now(), p.aim.yaw, p.aim.pitch);
            let aim = facing(yaw, pitch);
            let cut = s.cut.map(|c| {
                (
                    weapons::cut_frame(yaw, pitch, c),
                    weapons::sweep(&sw, s.age - 1),
                    weapons::sweep(&sw, s.age),
                )
            });
            if s.weapon == Weapon::Katana && !s.lunge && s.age <= weapons::PARRY {
                self.parry(id, eye, aim);
            }
            // T30: the garrocha, wound up, flings you along the aim; put
            // away before then, it does nothing
            if sw.launch > 0 && s.age <= sw.from && self.players[id].arms.weapon != s.weapon {
                self.players[id].swing = None;
                continue;
            }
            if sw.launch > 0 && s.age == sw.from {
                // off the ground as the lunge leaves it, or friction eats
                // the fling within a third of a second
                let p = &mut self.players[id];
                let along = aim.scale(sw.launch);
                let up = if p.body.on_ground {
                    along.y.max(weapons::LUNGE_LIFT)
                } else {
                    along.y
                };
                p.body.velocity = v3(along.x, up, along.z);
                p.body.on_ground = false;
                p.let_go_of_vine();
            }
            let (from, until) = if s.lunge {
                (1, weapons::LUNGE_TICKS)
            } else {
                (sw.from, sw.until)
            };
            if s.age >= from && s.age <= until {
                let targets = self.targets(id, false);
                let seats = self.players.len();
                for (k, t) in targets.iter().enumerate() {
                    let Some(t) = t else { continue };
                    let meets = match cut {
                        Some((frame, was, now)) => weapons::in_cut(&sw, eye, frame, was, now, t),
                        None => weapons::in_arc(&sw, eye, aim, t),
                    };
                    if s.struck & (1 << k) != 0 || !meets {
                        continue;
                    }
                    s.struck |= 1 << k;
                    let facing_yaw = if k < seats {
                        self.players[k].aim.yaw
                    } else {
                        // a straw target faces the room's spawn, +z
                        fx::HALF_TURN
                    };
                    let dmg = if s.lunge {
                        weapons::LUNGE_DAMAGE
                    } else if sw.behind > 0 && weapons::from_behind(t.feet, facing_yaw, eye) {
                        sw.behind
                    } else {
                        s.weapon.stats().damage
                    };
                    self.hurt(id, k, dmg);
                }
            }
            // T23: it stays to be drawn through its follow-through and back
            // to rest, hitting nothing
            let drawn = until + weapons::FOLLOW_TICKS + weapons::RECOVER_TICKS;
            self.players[id].swing = (s.age < drawn.max(weapons::PARRY)).then_some(s);
        }
    }

    /// The katana's parry: every other seat's projectile close in front of
    /// the eye is turned back along the aim at its speed, and is the
    /// parrier's now, its fuse begun again (CS-Q15: A).
    fn parry(&mut self, id: usize, eye: V3, aim: V3) {
        for q in &mut self.flying {
            let to = q.at.sub(eye);
            if q.owner == id || to.len() > PARRY_REACH || to.dot(aim) <= 0 {
                continue;
            }
            let f = q.weapon.flight().expect("only projectiles fly");
            q.velocity = aim.scale(q.velocity.len().max(f.speed));
            q.owner = id;
            q.age = 0;
            q.bounces = f.bounces;
            self.players[id].hit_mark = MARK;
        }
    }

    /// A shot from seat `id`: each pellet's ray from the eye, its damage to
    /// what it meets, its tracer, its mark on a wall, and the shotgun's
    /// kick in the air.
    fn shoot(&mut self, id: usize, s: Shot, scratch: &mut Scratch) {
        let g = s.weapon.stats();
        let (eye, yaw, pitch) = {
            let p = &self.players[id];
            (p.body.origin.add(v3(0, p.body.eye(), 0)), p.aim.yaw, p.aim.pitch)
        };
        let mut targets = self.targets(id, false);
        let band = self.players[id].band();
        let mut landed = false;
        let from = muzzle(eye, yaw, pitch);
        let salt = (self.tick as u32).wrapping_mul(31).wrapping_add(id as u32);
        for d in weapons::pellets(g, yaw, pitch, salt) {
            let (hits, stop) = weapons::cast(&self.map, scratch, eye, d, g.range, &targets, g.pierce);
            landed |= !hits.is_empty();
            for h in &hits {
                if h.head {
                    self.heard.push(Heard {
                        voice: Voice::Headshot,
                        at: eye,
                        by: id,
                    });
                    if let Some(t) = targets[h.who] {
                        self.head_marks.push(HeadMark {
                            at: t.head,
                            by: id,
                            ticks: HEAD_MARK,
                        });
                    }
                }
                let dmg = weapons::damage(g, s.charged, h.head);
                if self.hurt(id, h.who, dmg) {
                    targets[h.who] = None;
                }
            }
            let end = eye.add(d.scale(stop));
            let stroke = s.weapon == Weapon::Pincel;
            self.trails.push(Trail {
                from,
                to: end,
                ticks: if stroke { STROKE } else { TRACER },
                stroke,
            });
            if (hits.is_empty() || g.pierce) && stop < g.range {
                self.mark(end, d.neg());
            }
        }
        if landed {
            self.players[id].shooting.hit[band] += 1;
        }
        let p = &mut self.players[id];
        if g.kick > 0 && !p.body.on_ground {
            // the shotgun's jump: pushed back along the aim, so a shot at the
            // floor lifts
            p.body.velocity = p.body.velocity.sub(facing(yaw, pitch).scale(g.kick));
        }
    }

    /// The first-person camera for seat `id`, `alpha` of the way to the next
    /// tick, with the mouse movement not yet given to a tick already turned,
    /// and (T18, a setting) shaken when hit.
    pub fn camera(&self, id: usize, alpha: Fx, pending: Mouse, aspect: Fx, shake: bool) -> Camera {
        let p = &self.players[id];
        // the view tips up with a shot's kick, drawn only (hands.rs)
        let mut aim = p.aim.turned(pending);
        aim.pitch = (aim.pitch + crate::hands::view_kick(&p.arms)).min(camera::PITCH_MAX);
        // T18: and shakes when hit, likewise drawn only
        let (across, up) = if shake { p.hurt.shake(alpha) } else { (0, 0) };
        aim.yaw += across;
        aim.pitch = (aim.pitch + up).clamp(camera::PITCH_MIN, camera::PITCH_MAX);
        camera::eye_view(p.eye(alpha), aim, camera::vertical_fov(self.fov, aspect))
    }

    /// The moving things, seen by seat `me`: the other players, the straw
    /// targets, tracers and strokes, marks on the walls, and with `hands`
    /// (in first person) the gun in `me`'s hands.
    pub fn draw(&self, me: usize, hands: bool, alpha: Fx, pending: Mouse, look: &Look, out: &mut Shapes) {
        let (pal, ink) = (&look.palette, look.ink());
        for (k, p) in self.players.iter().enumerate() {
            if k != me && p.alive() {
                // M6: each seat in its skin, holding its weapon as it
                // holds it, legs in stride
                let figure = crate::skins::Figure {
                    skin: crate::skins::SKINS[p.skin as usize % crate::skins::SKINS.len()],
                    lifelike: p.lifelike,
                    feet: p.prev.lerp(p.body.origin, alpha),
                    yaw: p.aim.yaw,
                    pitch: p.aim.pitch,
                    ducked: p.body.ducked,
                    flashed: p.flash > 0,
                    speed: p.body.speed(),
                    on_ground: p.body.on_ground,
                    moment: (self.tick as i32).wrapping_mul(ONE).wrapping_add(alpha),
                    arms: Some((&p.arms, p.swing)),
                    sliding: p.body.sliding,
                    hanging: p.vine.is_some(),
                };
                crate::skins::draw(out, &figure, look);
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
        for q in &self.flying {
            draw_projectile(out, q, alpha, pal, ink);
        }
        // M5: the weapons lying on the map, turning slowly a metre up, a
        // glow on the floor beneath
        let moment = (self.tick as i32).wrapping_mul(ONE).wrapping_add(alpha);
        for k in self.pickups.iter().filter(|k| !k.taken) {
            let up = k.at.add(v3(0, ONE * 9 / 10, 0));
            let hold = Affine::translate(up)
                .then(&Affine::rot_y(fx::mul(moment, fx::deg(2))))
                .then(&Affine::translate(v3(0, 0, ONE / 5)));
            crate::hands::draw_weapon(out, &hold, k.weapon, &crate::hands::Pose::default(), pal);
            let glow =
                Affine::translate(k.at.add(v3(0, ONE / 20, 0))).then(&Affine::scale(v3(ONE, ONE / 10, ONE)));
            out.glow(Mesh::Sphere, glow, rgb(pal.brass), ONE / 2);
        }
        for b in &self.bursts {
            draw_burst(out, b, pal, ink);
        }
        // T33: each vine hangs straight down, or runs to the hands that
        // hold it and hangs on below them
        for (k, v) in self.map.vines.iter().enumerate() {
            let thick = ONE * 12 / 100;
            let held = self
                .players
                .iter()
                .find(|p| p.alive() && p.vine.is_some_and(|c| c.vine as usize == k));
            let (hands, rest) = match held.and_then(|p| p.vine.map(|c| (p, c))) {
                Some((p, c)) => {
                    let feet = p.prev.lerp(p.body.origin, alpha);
                    (feet.add(v3(0, VINE_GRIP, 0)), v.length - c.length)
                }
                None => (v.top.sub(v3(0, v.length, 0)), 0),
            };
            out.solid(Mesh::Cube, bar(v.top, hands, thick), rgb(pal.vine), 0);
            if rest > ONE / 10 {
                out.solid(
                    Mesh::Cube,
                    bar(hands, hands.sub(v3(0, rest, 0)), thick),
                    rgb(pal.vine),
                    0,
                );
            }
        }
        for (k, p) in self.players.iter().enumerate() {
            if let Some(h) = p.hook {
                let aim = if k == me { p.aim.turned(pending) } else { p.aim };
                let from = muzzle(p.eye(alpha), aim.yaw, aim.pitch);
                out.solid(Mesh::Cube, bar(from, h.at, ONE * 2 / 100), rgb(pal.wood), 0);
            }
        }
        // the X on a head your shot struck, square to your view, fading
        let eye = self.players[me].eye(alpha);
        for m in self.head_marks.iter().filter(|m| m.by == me) {
            let to = m.at.sub(eye).norm();
            let right = crate::geom::v3(-to.z, 0, to.x).norm();
            let up = right.cross(to);
            let fade = ONE * m.ticks / HEAD_MARK;
            let arm = ONE * 32 / 100;
            for (a, b) in [
                (right.add(up), right.neg().sub(up)),
                (right.sub(up), right.neg().add(up)),
            ] {
                let from = m.at.add(a.scale(arm)).sub(to.scale(ONE / 3));
                let till = m.at.add(b.scale(arm)).sub(to.scale(ONE / 3));
                out.glow(Mesh::Cube, bar(from, till, ONE * 5 / 100), rgb(pal.stroke), fade);
            }
        }
        let p = &self.players[me];
        if hands && p.alive() {
            let aim = p.aim.turned(pending);
            let moment = (self.tick as i32).wrapping_mul(ONE).wrapping_add(alpha);
            crate::hands::draw_hands(out, p.eye(alpha), aim.yaw, aim.pitch, p, moment, pal);
        }
    }
}

/// The guadaña's line, before the body moves: while the second button is
/// held, the guadaña in hand and the line not run out, it bears the body's
/// weight and pulls it toward where it holds; let go when near it.
fn pull(p: &mut Player, input: &Input) {
    let Some(mut h) = p.hook else { return };
    let middle = p.body.origin.add(v3(0, ONE * 9 / 10, 0));
    let to = h.at.sub(middle);
    h.ticks -= 1;
    if !input.button(ALT) || p.arms.weapon != Weapon::Guadana || h.ticks <= 0 || to.len() < ONE * 15 / 10 {
        p.hook = None;
        return;
    }
    let dir = to.norm();
    let dt = ONE / crate::movement::HZ;
    let weight = v3(0, fx::mul(TUNING.gravity, dt), 0);
    p.body.velocity = p
        .body
        .velocity
        .add(dir.scale(fx::mul(weapons::HOOK_PULL, dt)))
        .add(weight);
    if dir.y > 0 {
        p.body.on_ground = false;
    }
    p.hook = Some(h);
}

/// T33: a vine, before the body moves. In the air, with the vine key held
/// (T36), hands within `VINE_REACH` of a vine take hold of it, and the vine
/// bears the body on a swing as long as their height under its top. The
/// movement keys push the swing along the view; letting go of the vine key
/// drops off keeping the swing's speed, and a fresh jump does too with a
/// little lift; the ground, a hook or a nail let go too. Says whether the
/// body hangs from a vine this tick.
fn swing_on_vine(p: &mut Player, input: &Input, vines: &[map::Vine]) -> bool {
    p.vine_rest = (p.vine_rest - 1).max(0);
    let grip = p.body.origin.add(v3(0, VINE_GRIP, 0));
    let free = !p.body.on_ground && p.hook.is_none() && p.pinned == 0;
    // a bot's route knows nothing of vines, and its pilot never holds the
    // vine key, so a bot does not take hold
    let holding = input.key(VINE);
    if p.vine.is_none() && free && holding {
        let found = vines.iter().enumerate().find(|(k, v)| {
            let across = v3(grip.x - v.top.x, 0, grip.z - v.top.z).len();
            !(p.vine_rest > 0 && *k as u16 == p.last_vine)
                && grip.y <= v.top.y
                && grip.y >= v.top.y - v.length
                && across <= VINE_REACH
        });
        if let Some((k, v)) = found {
            let out = grip.sub(v.top);
            p.vine = Some(Clinging {
                vine: k as u16,
                length: out.len().clamp(ONE, v.length),
            });
            p.last_vine = k as u16;
            // the vine draws taut: nothing goes on away from its top
            let n = out.norm();
            let away = p.body.velocity.dot(n);
            if away > 0 {
                p.body.velocity = p.body.velocity.sub(n.scale(away));
            }
        }
    }
    let Some(c) = p.vine else { return false };
    let fresh_jump = input.key(JUMP) && !p.body.old_jump;
    if fresh_jump || !holding || !free {
        p.let_go_of_vine();
        if fresh_jump {
            p.body.velocity.y += VINE_LET_GO;
        }
        return false;
    }
    let top = vines[c.vine as usize].top;
    let dt = ONE / crate::movement::HZ;
    let (x, z) = input.walk_axes();
    let push = facing(p.aim.yaw, 0)
        .scale(int(-z))
        .add(right_of(p.aim.yaw).scale(int(x)))
        .norm();
    let v = p.body.velocity.add(push.scale(fx::mul(VINE_PUMP, dt)));
    // the mover takes half a tick's gravity before it moves and half after:
    // where this tick's move would carry the hands, held to the vine's
    // length, and the speed that carries them there
    let half_g = v3(0, fx::mul(TUNING.gravity, dt) / 2, 0);
    let moving = v.sub(half_g);
    let next = grip.add(moving.scale(dt)).sub(top);
    let held = if next.len() > c.length {
        next.norm().scale(c.length)
    } else {
        next
    };
    let step = held.sub(grip.sub(top));
    let along = v3(fx::div(step.x, dt), fx::div(step.y, dt), fx::div(step.z, dt));
    p.body.velocity = along.add(half_g);
    true
}

/// T37: a body's tick on a map whose gravity turns. The body moves in its
/// own frame, against the map as that frame meets it; then, if its gravity
/// turns (`gravity::shift`) and its body fits where the turn puts it, its
/// frame turns, its speed and view with it where it ran round a corner, and
/// the view starts its turn on screen.
fn turn_and_move(p: &mut Player, input: &Input, from: i32, turned: &[Map], cube: gravity::Cube) {
    let t = p.frame;
    let mut m = Mover::new(&turned[t.index() as usize]);
    m.t = p.tuning();
    let was_on_ground = p.body.on_ground;
    let mut b = p.body;
    b.origin = t.back(b.origin);
    b.velocity = t.back(b.velocity);
    p.events = m.tick(&mut b, input, from, p.aim.yaw);
    let local_cube = gravity::Cube {
        centre: t.back(cube.centre),
        half: cube.half,
    };
    let free = p.vine.is_none() && p.hook.is_none() && p.pinned == 0;
    let s = if free {
        gravity::shift(&mut m, &b, was_on_ground, input, p.aim.yaw, &local_cube)
    } else {
        None
    };
    let world_v = t.apply(b.velocity);
    p.body = b;
    p.body.origin = t.apply(b.origin);
    p.body.velocity = world_v;
    let Some(s) = s else { return };
    let next = t.then(&s.turn);
    let origin = t.apply(s.origin);
    let mut fit = Mover::new(&turned[next.index() as usize]);
    if !gravity::clear(&mut fit, &p.body, next.back(origin)) {
        return;
    }
    let (up_from, fwd_from) = (p.up(), p.look());
    p.frame = next;
    p.body.origin = origin;
    p.body.on_ground = false;
    p.body.slam_from = None;
    if s.carry {
        // speed and view turn with the corner
        p.body.velocity = next.apply(b.velocity);
    } else {
        let (yaw, pitch) = gravity::aim_toward(&next, fwd_from);
        p.aim.yaw = yaw;
        p.aim.pitch = pitch;
    }
    p.turning = Some((up_from, fwd_from, gravity::TURN_TICKS));
}

/// T27: a body that has moved past the walls' inner faces, over the top
/// of them, is set back inside and stops going out; below the walls' tops
/// the walls already hold it. It is kept 3 cm clear of the faces, so it
/// cannot stand on a wall's top edge.
fn keep_inside(b: &mut crate::movement::Body, (lo, hi): (V3, V3)) {
    let r = crate::movement::HALF_WIDTH + ONE * 3 / 100;
    let o = &mut b.origin;
    let v = &mut b.velocity;
    if o.x < lo.x + r {
        o.x = lo.x + r;
        v.x = v.x.max(0);
    }
    if o.x > hi.x - r {
        o.x = hi.x - r;
        v.x = v.x.min(0);
    }
    if o.z < lo.z + r {
        o.z = lo.z + r;
        v.z = v.z.max(0);
    }
    if o.z > hi.z - r {
        o.z = hi.z - r;
        v.z = v.z.min(0);
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

/// A projectile where it is drawn, `alpha` of the way from last tick.
/// Where a burst's splats are thrown: six about its middle, three above
/// and three below, turned between the rings (unit length, in tenths).
const SPLATS: [(i32, i32, i32); 12] = [
    (10, 0, 0),
    (5, 0, 9),
    (-5, 0, 9),
    (-10, 0, 0),
    (-5, 0, -9),
    (5, 0, -9),
    (5, 7, 5),
    (-7, 7, 0),
    (2, 7, -7),
    (-5, -7, 5),
    (7, -7, 0),
    (-2, -7, -7),
];

/// R7.9: a burst painted as it is felt. Splats of paint in the light's,
/// the ochre's and the red's colours are thrown out to the blast's reach
/// and thin away as it fades (the ordered dither, so they stay flat paint),
/// each inked; a flash at the middle goes out in its first third.
fn draw_burst(out: &mut Shapes, b: &Burst, pal: &Palette, ink: Rgb) {
    let left = ONE * b.ticks / BURST;
    let grown = ONE - left;
    // thrown fast, slowing: a third of the way at once, all of it by the end
    let out_to = ONE / 3 + fx::mul(ONE * 2 / 3, ONE - fx::mul(left, left));
    let size = fx::mul(b.radius, ONE / 5 + fx::mul(ONE / 10, left));
    let colours = [pal.flash, pal.ochre, pal.stroke];
    for (k, &(x, y, z)) in SPLATS.iter().enumerate() {
        let dir = v3(x * ONE / 10, y * ONE / 10, z * ONE / 10).norm();
        // a splat is `size` across and three fifths of it deep (the facet
        // ball is a unit across), so its middle stays 0.3 of `size` inside
        // the reach
        let reach = fx::mul(b.radius - size * 3 / 10, out_to);
        let at = Affine::translate(b.at.add(dir.scale(reach)))
            .then(&Affine::looking(dir))
            .then(&Affine::scale(v3(size, size, size * 3 / 5)));
        out.dithered(Mesh::Facet, at, rgb(colours[k % colours.len()]), left);
        out.outline(Mesh::Facet, &at, ink, left);
    }
    let flash = (left * 3 - ONE * 2).max(0);
    if flash > 0 {
        let r = fx::mul(b.radius, ONE / 3 + grown);
        out.glow(
            Mesh::Sphere,
            Affine::translate(b.at).then(&Affine::scale(v3(r, r, r))),
            rgb(pal.flash),
            flash,
        );
    }
}

fn draw_projectile(out: &mut Shapes, q: &Projectile, alpha: Fx, pal: &Palette, ink: Rgb) {
    let at = q.prev.lerp(q.at, alpha);
    let cm = |c: i32| c * ONE / 100;
    let ball = |d: i32| Affine::translate(at).then(&Affine::scale(v3(cm(d), cm(d), cm(d))));
    let back = q.velocity.norm();
    match q.weapon {
        Weapon::Drum => {
            // R7.9: a dark grenade with a brass band and its fuse's spark,
            // which beats faster as the fuse burns
            out.inked(Mesh::Facet, ball(24), rgb(pal.dark_steel), ink);
            let band = Affine::translate(at).then(&Affine::scale(v3(cm(26), cm(6), cm(26))));
            out.inked(Mesh::Tube, band, rgb(pal.brass), ink);
            if (q.age / (1 + 8 - (q.age / 15).min(7))) % 2 == 0 {
                let spark =
                    Affine::translate(at.add(v3(0, cm(14), 0))).then(&Affine::scale(v3(cm(8), cm(8), cm(8))));
                out.glow(Mesh::Sphere, spark, rgb(pal.flash), ONE);
            }
        }
        Weapon::Trabuco => {
            // R7.9: a nail, its head behind it
            let tail = at.sub(back.scale(cm(20)));
            out.inked(Mesh::Cube, bar(tail, at, cm(2)), rgb(pal.steel), ink);
            let head = Affine::translate(tail)
                .then(&Affine::looking(back))
                .then(&Affine::scale(v3(cm(6), cm(6), cm(1))));
            out.inked(Mesh::Cyl, head, rgb(pal.steel), ink);
        }
        Weapon::Toro => {
            // R7.9: a red ball dragging a stroke of paint, its dabs thinning
            out.inked(Mesh::Facet, ball(30), rgb(pal.stroke), ink);
            for k in 1..=3 {
                let d = cm(14 - 3 * k);
                let dab = Affine::translate(at.sub(back.scale(cm(18 * k))))
                    .then(&Affine::looking(back))
                    .then(&Affine::scale(v3(d, d, d * 2)));
                out.dithered(Mesh::Facet, dab, rgb(pal.stroke), ONE - ONE * k / 4);
                out.outline(Mesh::Facet, &dab, ink, ONE - ONE * k / 4);
            }
            out.glow(
                Mesh::Sphere,
                Affine::translate(at).then(&Affine::scale(v3(cm(26), cm(26), cm(26)))),
                rgb(pal.flash),
                ONE / 2,
            );
        }
        Weapon::Campana => {
            // R7.9: the bell and its clapper beneath
            out.inked(
                Mesh::Cone,
                Affine::translate(at).then(&Affine::scale(v3(cm(50), cm(45), cm(50)))),
                rgb(pal.brass),
                ink,
            );
            let clapper =
                Affine::translate(at.sub(v3(0, cm(20), 0))).then(&Affine::scale(v3(cm(9), cm(9), cm(9))));
            out.inked(Mesh::Facet, clapper, rgb(pal.dark_steel), ink);
        }
        _ => {
            // the Collage's scrap turns as it flies; R7.9: two scraps
            // pasted together, the smaller turning the other way
            let a = Affine::translate(at)
                .then(&Affine::rot_y(fx::deg(23) * q.age))
                .then(&Affine::scale(v3(cm(18), cm(18), cm(4))));
            out.inked(Mesh::Cube, a, rgb(pal.ochre), ink);
            let b = Affine::translate(at)
                .then(&Affine::rot_y(-fx::deg(31) * q.age))
                .then(&Affine::translate(v3(0, 0, cm(3))))
                .then(&Affine::scale(v3(cm(11), cm(13), cm(3))));
            out.inked(Mesh::Cube, b, rgb(pal.blue), ink);
        }
    }
}

/// Speed in m/s as text with one decimal (the HUD's readout).
pub fn speed_text(speed: Fx) -> String {
    let tenths = (speed as i64 * 10 + (ONE as i64 / 2)) >> fx::SHIFT;
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests;
