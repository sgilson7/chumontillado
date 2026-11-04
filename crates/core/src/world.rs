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

use crate::arms::{Arms, Shot, RECOIL};
use crate::camera::{self, Aim};
use crate::fx::{self, int, Fx, ONE};
use crate::geom::{facing, right_of, v3, Affine, V3};
use crate::input::{Input, Mouse, ALT, RESET};
use crate::look::{rgb, Look, Palette, Rgb};
use crate::map::{self, Map};
use crate::movement::{Body, Events, Mover, Tuning, TUNING};
use crate::pilot::{self, Pilot};
use crate::prim::Mesh;
use crate::render::{Camera, Shapes};
use crate::trace::trace_box;
use crate::trace::Scratch;
use crate::weapons::{self, Alt, Blast, Kind, Target, Weapon};

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
    /// a bot's pilot; none for a person
    pub pilot: Option<Pilot>,
    /// the melee swing under way
    pub swing: Option<Swinging>,
    /// the guadaña's line to a wall
    pub hook: Option<Hook>,
    /// ticks a nail still holds it to a wall
    pub pinned: i32,
}

/// A melee swing under way: its weapon, its age in ticks, whether it is the
/// katana's lunge, and which bodies it has hit (one bit each, seats then
/// straw targets), so a swing hits each once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swinging {
    pub weapon: Weapon,
    pub age: i32,
    pub lunge: bool,
    pub struck: u64,
}

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
}

/// A burst, drawn as a ball of light that grows and fades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Burst {
    pub at: V3,
    pub radius: Fx,
    pub ticks: i32,
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
            swing: None,
            hook: None,
            pinned: 0,
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

    pub fn eye_now(&self) -> V3 {
        self.body.origin.add(v3(0, self.body.eye(), 0))
    }

    /// The movement's numbers while holding its weapon: the navaja's faster
    /// run, the katana's fourth wall jump.
    pub fn tuning(&self) -> Tuning {
        let mut t = TUNING;
        if let Some(sw) = self.arms.weapon.swing() {
            t.run = t.run * sw.run_pct / 100;
            t.wall_jumps = sw.wall_jumps;
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
    pub map: Map,
    pub players: Vec<Player>,
    pub dummies: Vec<Dummy>,
    pub trails: Vec<Trail>,
    pub marks: Vec<Mark>,
    pub flying: Vec<Projectile>,
    pub bursts: Vec<Burst>,
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
            flying: Vec::new(),
            bursts: Vec::new(),
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
                pull(p, input);
                if p.pinned > 0 {
                    // held to the wall by a nail: it aims and fires, but stays
                    p.pinned -= 1;
                    p.body.velocity = V3::ZERO;
                    p.events = Events::default();
                } else {
                    mover.t = p.tuning();
                    p.events = mover.tick(&mut p.body, input, from, p.aim.yaw);
                }
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
        self.tick += 1;
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
        }
        if k < seats {
            let victim = &mut self.players[k];
            victim.health -= dmg;
            victim.flash = FLASH;
            if victim.health <= 0 {
                victim.health = 0;
                victim.dead = RESPAWN;
                victim.swing = None;
                victim.hook = None;
                victim.pinned = 0;
                if k != by {
                    self.players[by].kill_mark = MARK;
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
                    } else if let Some(b) = f.blast {
                        self.burst(q.owner, wall.end.add(n.scale(ONE / 50)), &b, None, scratch);
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
                    self.burst(q.owner, q.at, &b, None, scratch);
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
    fn burst(&mut self, by: usize, at: V3, b: &Blast, spared: Option<usize>, scratch: &mut Scratch) {
        self.bursts.push(Burst {
            at,
            radius: b.outer,
            ticks: BURST,
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
            }
        }
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
        } else {
            p.body.velocity = p.body.velocity.add(facing(yaw, 0).scale(sw.carry));
        }
        p.swing = Some(Swinging {
            weapon: s.weapon,
            age: 0,
            lunge,
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
            if s.weapon == Weapon::Katana && !s.lunge && s.age <= weapons::PARRY {
                self.parry(id, eye, aim);
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
                    if s.struck & (1 << k) != 0 || !weapons::in_arc(&sw, eye, aim, t) {
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
            self.players[id].swing = (s.age < until.max(weapons::PARRY)).then_some(s);
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
        let from = muzzle(eye, yaw, pitch);
        let salt = (self.tick as u32).wrapping_mul(31).wrapping_add(id as u32);
        for d in weapons::pellets(g, yaw, pitch, salt) {
            let (hits, stop) = weapons::cast(&self.map, scratch, eye, d, g.range, &targets, g.pierce);
            for h in &hits {
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
        for q in &self.flying {
            draw_projectile(out, q, alpha, pal, ink);
        }
        for b in &self.bursts {
            // grows from a third of its reach to its whole reach as it fades
            let grown = ONE - ONE * b.ticks / BURST;
            let r = fx::mul(b.radius, ONE / 3 + fx::mul(ONE * 2 / 3, grown));
            let at = Affine::translate(b.at).then(&Affine::scale(v3(r, r, r)));
            out.glow(Mesh::Sphere, at, rgb(pal.flash), ONE * b.ticks / BURST);
        }
        for (k, p) in self.players.iter().enumerate() {
            if let Some(h) = p.hook {
                let aim = if k == me { p.aim.turned(pending) } else { p.aim };
                let from = muzzle(p.eye(alpha), aim.yaw, aim.pitch);
                out.solid(Mesh::Cube, bar(from, h.at, ONE * 2 / 100), rgb(pal.wood), 0);
            }
        }
        let p = &self.players[me];
        if hands && p.alive() {
            let aim = p.aim.turned(pending);
            draw_gun(out, p.eye(alpha), aim.yaw, aim.pitch, p, pal);
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

/// One part of a weapon in the hands' frame: its mesh, centre, size and
/// colour, in centimetres, x right, y up, z back toward the eye.
type GunPart = (Mesh, [i32; 3], [i32; 3], [i32; 3]);

/// A weapon's parts: the true ones in steel, wood and brass, the painted
/// ones in the painted palette (the Roster page).
fn gun_parts(g: Weapon, pal: &Palette) -> Vec<GunPart> {
    let (steel, dark, wood, brass) = (pal.steel, pal.dark_steel, pal.wood, pal.brass);
    let (bone, blue, ochre, umber, red) = (pal.bone, pal.blue, pal.ochre, pal.umber, pal.stroke);
    match g {
        Weapon::Pistol => vec![
            (Mesh::Cube, [0, 2, -6], [4, 4, 18], steel),
            (Mesh::Cube, [0, -5, 0], [3, 9, 4], dark),
        ],
        Weapon::Shotgun => vec![
            (Mesh::Cube, [0, 2, -28], [3, 3, 60], steel),
            (Mesh::Cube, [0, -1, -22], [3, 3, 44], dark),
            (Mesh::Cube, [0, -1, -30], [5, 5, 12], wood),
            (Mesh::Cube, [0, 0, 2], [5, 7, 14], steel),
            (Mesh::Cube, [0, -3, 20], [5, 8, 24], wood),
        ],
        Weapon::MachinePistol => vec![
            (Mesh::Cube, [0, 1, -8], [5, 6, 22], dark),
            (Mesh::Cube, [0, 2, -24], [2, 2, 10], steel),
            (Mesh::Cube, [0, -8, -6], [3, 12, 3], steel),
            (Mesh::Cube, [0, -6, 2], [3, 8, 4], dark),
        ],
        Weapon::Rifle => vec![
            (Mesh::Cube, [0, 3, -40], [2, 2, 70], steel),
            (Mesh::Cube, [0, 1, -2], [4, 6, 16], brass),
            (Mesh::Cube, [0, 0, -25], [4, 4, 25], wood),
            (Mesh::Cube, [0, -2, 20], [5, 9, 26], wood),
            (Mesh::Cube, [0, -6, 0], [1, 5, 8], steel),
        ],
        Weapon::Pincel => vec![
            (Mesh::Cube, [0, 2, -35], [3, 3, 60], bone),
            (Mesh::Cube, [0, -1, 10], [6, 7, 22], blue),
            (Mesh::Cube, [0, 4, -20], [4, 2, 12], ochre),
            (Mesh::Facet, [0, 2, -68], [5, 5, 8], umber),
            (Mesh::Facet, [0, 2, -74], [3, 3, 5], red),
        ],
        // a fat tube over a drum, drawn true
        Weapon::Drum => vec![
            (Mesh::Cube, [0, 2, -28], [6, 6, 36], dark),
            (Mesh::Cyl, [0, -5, -16], [10, 7, 10], steel),
            (Mesh::Cube, [0, -6, 0], [3, 9, 4], wood),
            (Mesh::Cube, [0, 0, 4], [4, 6, 10], wood),
        ],
        // a painted blunderbuss: a flared mouth and a blue stock
        Weapon::Trabuco => vec![
            (Mesh::Cube, [0, 2, -30], [4, 4, 46], umber),
            (Mesh::Facet, [0, 2, -54], [11, 11, 6], ochre),
            (Mesh::Cube, [0, 2, -14], [5, 5, 3], bone),
            (Mesh::Cube, [0, -2, 10], [6, 8, 24], blue),
        ],
        // a painted tube with horns
        Weapon::Toro => vec![
            (Mesh::Cube, [0, 3, -30], [7, 7, 46], red),
            (Mesh::Facet, [0, 3, -54], [9, 9, 4], umber),
            (Mesh::Cube, [5, 9, -48], [2, 7, 2], bone),
            (Mesh::Cube, [-5, 9, -48], [2, 7, 2], bone),
            (Mesh::Cube, [0, -6, 0], [4, 10, 5], umber),
        ],
        // a painted bell on a handle
        Weapon::Campana => vec![
            (Mesh::Cone, [0, 0, -30], [16, 15, 16], brass),
            (Mesh::Facet, [0, -7, -30], [4, 4, 4], dark),
            (Mesh::Cube, [0, -4, -6], [4, 4, 34], wood),
            (Mesh::Cube, [0, -8, 8], [4, 10, 5], umber),
        ],
        // painted planes stuck together
        Weapon::Collage => vec![
            (Mesh::Cube, [0, 2, -14], [5, 6, 26], blue),
            (Mesh::Cube, [2, 5, -20], [3, 4, 10], ochre),
            (Mesh::Cube, [-1, -1, -28], [4, 3, 12], red),
            (Mesh::Cube, [0, 3, -32], [2, 2, 8], bone),
            (Mesh::Cube, [0, -6, 0], [4, 9, 5], umber),
        ],
        Weapon::Katana => vec![
            (Mesh::Cube, [0, 2, -50], [1, 4, 80], steel),
            (Mesh::Cube, [0, 2, -9], [7, 7, 1], brass),
            (Mesh::Cube, [0, 2, 2], [3, 3, 20], dark),
        ],
        Weapon::Navaja => vec![
            (Mesh::Cube, [0, 2, -17], [1, 3, 16], steel),
            (Mesh::Cube, [0, 1, -8], [3, 4, 2], brass),
            (Mesh::Cube, [0, 0, 0], [3, 4, 14], wood),
        ],
        // a painted scythe: a long shaft, the blade across its end
        Weapon::Guadana => vec![
            (Mesh::Cube, [0, 0, -34], [3, 3, 90], umber),
            (Mesh::Cube, [-16, 6, -78], [34, 3, 5], bone),
            (Mesh::Cube, [0, 0, 6], [4, 4, 8], ochre),
        ],
    }
}

/// The weapon in the hands: below and right of the eye, kicked back for a
/// moment after a shot, lowered while reloading or raising; a melee
/// weapon sweeps right to left through its swing, and thrusts in a lunge.
fn draw_gun(out: &mut Shapes, eye: V3, yaw: i32, pitch: i32, p: &Player, pal: &Palette) {
    let a = &p.arms;
    let frame = Affine::translate(eye).then(&Affine::looking(facing(yaw, pitch)));
    let cm = |c: i32| c * ONE / 100;
    let back = cm(12) * a.recoil / RECOIL;
    let low = if a.reloading > 0 || a.raise > 0 { cm(8) } else { 0 };
    let mut hold = frame.then(&Affine::translate(v3(cm(18), -cm(16) - low, -cm(42) + back)));
    if let Some(s) = p.swing {
        let sw = s.weapon.swing().expect("a swing is a melee weapon's");
        if s.lunge {
            hold = hold.then(&Affine::translate(v3(-cm(10), cm(4), -cm(20))));
        } else {
            // from 60° right to 60° left over the swing's ticks
            let along = ONE * s.age.min(sw.until) / sw.until.max(1);
            let turn = fx::deg(60) - fx::mul(fx::deg(120), along);
            hold = hold.then(&Affine::rot_y(turn));
        }
    }
    for (mesh, at, size, colour) in gun_parts(a.weapon, pal) {
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

/// A projectile where it is drawn, `alpha` of the way from last tick.
fn draw_projectile(out: &mut Shapes, q: &Projectile, alpha: Fx, pal: &Palette, ink: Rgb) {
    let at = q.prev.lerp(q.at, alpha);
    let cm = |c: i32| c * ONE / 100;
    let ball = |d: i32| Affine::translate(at).then(&Affine::scale(v3(cm(d), cm(d), cm(d))));
    match q.weapon {
        Weapon::Drum => out.inked(Mesh::Facet, ball(24), rgb(pal.dark_steel), ink),
        Weapon::Trabuco => {
            let back = q.velocity.norm().scale(cm(20));
            out.solid(Mesh::Cube, bar(at.sub(back), at, cm(2)), rgb(pal.steel), 0);
        }
        Weapon::Toro => {
            out.inked(Mesh::Facet, ball(30), rgb(pal.stroke), ink);
            let tail = at.sub(q.velocity.norm().scale(cm(40)));
            out.glow(
                Mesh::Sphere,
                Affine::translate(tail).then(&Affine::scale(v3(cm(18), cm(18), cm(18)))),
                rgb(pal.flash),
                ONE,
            );
        }
        Weapon::Campana => out.inked(
            Mesh::Cone,
            Affine::translate(at).then(&Affine::scale(v3(cm(50), cm(45), cm(50)))),
            rgb(pal.brass),
            ink,
        ),
        _ => {
            // the Collage's scrap turns as it flies
            let a = Affine::translate(at)
                .then(&Affine::rot_y(fx::deg(23) * q.age))
                .then(&Affine::scale(v3(cm(18), cm(18), cm(4))));
            out.inked(Mesh::Cube, a, rgb(pal.ochre), ink);
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
