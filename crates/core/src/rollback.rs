//! Rollback (M4): one browser's side of an online match that does not wait
//! for the others' records.
//!
//! Chumworld Arena's `rollback.rs` (e78945b), from two seats to four (CS-2,
//! R8.1). The session runs each tick with every other seat's record
//! guessed when it has not come: the last one that has (`predict`). Before
//! each tick it keeps a copy of the whole world (`Sim` is `Clone`, so
//! whatever the world holds is saved). When a true record for a tick it has
//! run arrives and differs from the guess, it puts back the copy from
//! before that tick and runs again to where it was. The world it ends on is
//! the world a browser fed every record on time reaches (`tests`).
//!
//! Limits, as the arena's:
//!
//! - it runs at most `WINDOW` ticks past the first tick whose record from
//!   some other seat has not come (`frontier`); there it waits, so a
//!   rollback never runs more than `WINDOW` ticks again;
//! - its own records are used `DELAY` ticks after they are made;
//! - checksums are taken only of worlds made from true records for every
//!   seat, so every browser sums the same world for the same tick.
//!
//! A seat whose player has gone is played by its bot from the tick the
//! host chose (`drop_seat`); records for it from that tick on are thrown
//! away, and a world already run past that tick is run again.

use crate::input::Input;
use crate::online::{holds, Event, Msg, Net, Peer, Seats, DELAY, DESYNC, GARBLED, SEATS, SUM_EVERY};
use crate::sound::Heard;
use std::collections::{BTreeMap, VecDeque};

/// The most ticks run past the last confirmed one: 8, 133 ms.
pub const WINDOW: u32 = 8;

/// The furthest ahead an honest record can be: its sender's window, both
/// delays, a tick of the order frames run in, and the host's passing on.
pub const LEAD: u32 = WINDOW + 2 * DELAY + 2;

/// Copies of the world kept at least this far back (a second), so a drop
/// the host decides a little behind this browser can still be run again.
pub const KEEP_BACK: u32 = 60;

/// A world every browser runs alike.
pub trait Sim: Clone {
    /// One tick on every seat's record, in seat order; a seat a bot plays
    /// ignores its record.
    fn tick(&mut self, inputs: &[Input; SEATS]);
    fn sum(&self) -> u64;
    /// From now on the seat is played by its bot.
    fn hand_to_bot(&mut self, seat: usize);
    /// M6: the sounds the last tick made.
    fn heard(&self) -> &[Heard];
}

/// M6: a sound from a tick run again is played if it came this many ticks
/// late at most (133 ms); later than that it is left unplayed.
pub const HEARD_LATE: u32 = WINDOW;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// rollbacks done, ticks run again in all, and the most in one
    pub rollbacks: u32,
    pub resimulated: u32,
    pub most: u32,
    /// ticks the window held this browser back
    pub stalls: u32,
}

#[derive(Debug, Clone)]
pub struct Rollback<W: Sim> {
    pub seat: usize,
    pub seats: Seats,
    pub links: Net,
    /// the world as of `tick`, run on true and guessed records
    pub world: W,
    /// the next tick to run
    pub tick: u32,
    inputs: [BTreeMap<u32, Input>; SEATS],
    /// each seat's first tick whose record has not come
    frontiers: [u32; SEATS],
    /// the tick from which each seat is its bot's
    pub drops: [Option<u32>; SEATS],
    /// the records each tick was run with (true or guessed)
    used: BTreeMap<u32, [Input; SEATS]>,
    /// the world before each tick, back to the oldest that may run again
    snaps: VecDeque<(u32, W)>,
    redo: Option<u32>,
    next_sum: u32,
    pub stats: Stats,
    /// R10.12: ticks in a row the window has held this browser back
    pub stalled: u32,
    /// this browser's checksums, newest last (the last `MADE_KEPT`)
    pub made: VecDeque<(u32, u64)>,
    /// M6: the sounds already given out for each tick, so a tick run again
    /// gives only what it did not make before
    told: BTreeMap<u32, Vec<Heard>>,
    /// M6: sounds made and not yet taken by `take_heard`
    fresh: Vec<Heard>,
}

/// How many of its own checksums a session keeps for tests and the page.
pub const MADE_KEPT: usize = 16;

impl<W: Sim> Rollback<W> {
    /// A session on `world` for seat `seat`, with `seats` held by people,
    /// over the links `links` already holds.
    pub fn new(links: Net, seat: usize, seats: Seats, world: W) -> Rollback<W> {
        let mut inputs: [BTreeMap<u32, Input>; SEATS] = Default::default();
        for seat_inputs in inputs.iter_mut() {
            for t in 0..DELAY {
                seat_inputs.insert(t, Input::default());
            }
        }
        Rollback {
            seat,
            seats,
            links,
            world,
            tick: 0,
            inputs,
            frontiers: [DELAY; SEATS],
            drops: [None; SEATS],
            used: BTreeMap::new(),
            snaps: VecDeque::new(),
            redo: None,
            next_sum: SUM_EVERY,
            stats: Stats::default(),
            stalled: 0,
            made: VecDeque::new(),
            told: BTreeMap::new(),
            fresh: Vec::new(),
        }
    }

    /// Whether seat `s`'s record for tick `t` comes from another browser.
    fn remote(&self, s: usize, t: u32) -> bool {
        s != self.seat && holds(self.seats, s) && self.drops[s].is_none_or(|d| t < d)
    }

    /// The first tick some other seat's record has not come for. A seat
    /// dropped once every record before its drop is in waits for nothing.
    pub fn frontier(&self) -> u32 {
        (0..SEATS)
            .filter(|&s| s != self.seat && holds(self.seats, s))
            .map(|s| match self.drops[s] {
                Some(d) if self.frontiers[s] >= d => u32::MAX,
                _ => self.frontiers[s],
            })
            .min()
            .unwrap_or(u32::MAX)
    }

    pub fn take_outbox(&mut self) -> Vec<(Peer, String)> {
        self.links.take_outbox()
    }

    /// Text from a browser.
    pub fn message(&mut self, from: Peer, text: &str) {
        match self.links.message(from, text) {
            Some(Event::Input { seat, tick, input }) => {
                if self.take(seat, tick, input) {
                    self.links.pass_on(&Msg::Input { seat, tick, input }, from);
                }
            }
            Some(Event::Drop { seat, from: at }) => self.drop_seat(seat, at),
            Some(Event::Gone { seat }) => self.gone(seat),
            _ => {}
        }
    }

    /// The transport says a browser left.
    pub fn left(&mut self, peer: Peer) {
        if let Some(Event::Gone { seat }) = self.links.left(peer) {
            self.gone(seat);
        }
    }

    /// The host hears a guest has gone: its bot plays its seat from the
    /// first tick the host has no record for, and everyone is told.
    pub fn gone(&mut self, seat: usize) {
        if !self.links.host || self.drops[seat].is_some() {
            return;
        }
        let from = self.frontiers[seat];
        self.drop_seat(seat, from);
        self.links.broadcast(&Msg::Drop { seat, from }, None);
    }

    /// Seat `seat` is its bot's from tick `from`.
    pub fn drop_seat(&mut self, seat: usize, from: u32) {
        if seat >= SEATS || seat == self.seat {
            self.links.status = GARBLED;
            return;
        }
        if self.drops[seat].is_some() {
            return;
        }
        // a drop before a tick this browser has already summed changes a
        // summed world; the checksums then say so (R8.3)
        self.drops[seat] = Some(from);
        let _ = self.inputs[seat].split_off(&from);
        if from < self.tick {
            self.redo = Some(self.redo.map_or(from, |r| r.min(from)));
        }
    }

    /// A record from another seat. Whether it was new (and so is passed on
    /// by the host). A record that comes twice, directly and passed on, is
    /// kept once; two different records for one tick are not honest.
    fn take(&mut self, seat: usize, tick: u32, input: Input) -> bool {
        if let Some(had) = self.inputs[seat].get(&tick) {
            if *had != input {
                self.links.status = GARBLED;
            }
            return false;
        }
        if tick < self.frontiers[seat] || self.drops[seat].is_some_and(|d| tick >= d) {
            // older than anything kept, or for a seat its bot now plays
            return false;
        }
        if tick > self.tick + LEAD {
            self.links.status = GARBLED;
            return false;
        }
        self.inputs[seat].insert(tick, input);
        if tick < self.tick && self.used.get(&tick).map(|u| u[seat]) != Some(input) {
            self.redo = Some(self.redo.map_or(tick, |r| r.min(tick)));
        }
        while self.inputs[seat].contains_key(&self.frontiers[seat]) {
            self.frontiers[seat] += 1;
        }
        true
    }

    /// This browser's record, made now, for the tick `DELAY` ahead; sent
    /// once. A frame that runs no tick adds nothing.
    pub fn local(&mut self, input: Input) {
        let tick = self.tick + DELAY;
        if self.inputs[self.seat].contains_key(&tick) {
            return;
        }
        self.inputs[self.seat].insert(tick, input);
        let seat = self.seat;
        self.links.broadcast(&Msg::Input { seat, tick, input }, None);
    }

    /// A seat's record for tick `t`: the true one if it has come, else the
    /// last one that has (the guess).
    fn predict(&self, s: usize, t: u32) -> Input {
        let theirs = &self.inputs[s];
        theirs
            .get(&t)
            .or_else(|| theirs.range(..t).next_back().map(|(_, i)| i))
            .copied()
            .unwrap_or_default()
    }

    /// Every seat's record for tick `t`.
    fn records(&self, t: u32) -> [Input; SEATS] {
        std::array::from_fn(|s| {
            if s == self.seat {
                self.inputs[s].get(&t).copied().unwrap_or_default()
            } else if self.remote(s, t) {
                self.predict(s, t)
            } else {
                Input::default()
            }
        })
    }

    /// Runs tick `t` from the world as it stands, keeping the copy before it.
    fn run(&mut self, t: u32) {
        self.snaps.push_back((t, self.world.clone()));
        for s in 0..SEATS {
            if self.drops[s] == Some(t) {
                self.world.hand_to_bot(s);
            }
        }
        let records = self.records(t);
        self.used.insert(t, records);
        self.world.tick(&records);
        self.tell(t);
    }

    /// M6: keeps the sounds tick `t` made that were not given out for it
    /// before. A guess put right can add a sound (a shot the guess missed)
    /// but cannot take one back, since it has been played; the same sound
    /// made again (its place moved a little) is not played twice.
    fn tell(&mut self, t: u32) {
        if t + HEARD_LATE < self.tick {
            return;
        }
        let told = self.told.entry(t).or_default();
        let mut before = told.clone();
        for h in self.world.heard() {
            match before.iter().position(|b| b.voice == h.voice && b.by == h.by) {
                Some(k) => {
                    before.swap_remove(k);
                }
                None => {
                    told.push(*h);
                    self.fresh.push(*h);
                }
            }
        }
    }

    /// M6: the sounds made since the last call, each once.
    pub fn take_heard(&mut self) -> Vec<Heard> {
        std::mem::take(&mut self.fresh)
    }

    /// A wrong guess or a drop found: back to the copy before its tick, and
    /// every tick since run again.
    fn roll_back(&mut self) {
        let Some(from) = self.redo.take() else {
            return;
        };
        while self.snaps.back().is_some_and(|(t, _)| *t > from) {
            self.snaps.pop_back();
        }
        match self.snaps.pop_back() {
            Some((at, world)) if at == from => self.world = world,
            _ => {
                // the copy is gone: this browser cannot agree any more
                self.links.status = DESYNC;
                return;
            }
        }
        for t in from..self.tick {
            self.run(t);
        }
        let n = self.tick - from;
        self.stats.rollbacks += 1;
        self.stats.resimulated += n;
        self.stats.most = self.stats.most.max(n);
    }

    /// Checksums of every tick now made from true records for every seat.
    fn sum_ready(&mut self) {
        while self.next_sum <= self.tick && self.next_sum <= self.frontier() && self.redo.is_none() {
            let t = self.next_sum;
            let sum = if t == self.tick {
                self.world.sum()
            } else {
                match self.snaps.iter().find(|(s, _)| *s == t) {
                    Some((_, w)) => w.sum(),
                    None => return,
                }
            };
            self.links.summed(t, sum);
            self.made.push_back((t, sum));
            if self.made.len() > MADE_KEPT {
                self.made.pop_front();
            }
            self.next_sum += SUM_EVERY;
        }
    }

    /// Forgets copies and records no rollback or checksum can need again.
    fn prune(&mut self) {
        let keep = self
            .frontier()
            .min(self.next_sum)
            .min(self.tick.saturating_sub(KEEP_BACK));
        while self.snaps.front().is_some_and(|(t, _)| *t < keep) {
            self.snaps.pop_front();
        }
        self.used = self.used.split_off(&keep);
        self.told = self.told.split_off(&self.tick.saturating_sub(HEARD_LATE));
        let below = keep.saturating_sub(1);
        for seat_inputs in self.inputs.iter_mut() {
            *seat_inputs = seat_inputs.split_off(&below);
        }
    }

    /// R10.12: the seat whose records are furthest behind, the one this
    /// browser waits on when it is held back.
    pub fn behind(&self) -> Option<usize> {
        (0..SEATS)
            .filter(|&s| self.remote(s, self.frontiers[s]))
            .min_by_key(|&s| self.frontiers[s])
    }

    /// The seat a bot took over most recently, if within the last `ticks`.
    pub fn dropped_within(&self, ticks: u32) -> Option<usize> {
        (0..SEATS)
            .filter_map(|s| self.drops[s].map(|d| (s, d)))
            .filter(|&(_, d)| self.tick >= d && self.tick - d < ticks)
            .max_by_key(|&(_, d)| d)
            .map(|(s, _)| s)
    }

    /// Whether the next tick would run past the window.
    pub fn held_back(&self) -> bool {
        self.tick >= self.frontier().saturating_add(WINDOW)
    }

    /// Ticks run on a guess of some seat's record.
    pub fn guessed(&self) -> u32 {
        self.tick.saturating_sub(self.frontier())
    }

    /// Runs again what a wrong guess spoiled, then the next tick if the
    /// window allows and this browser's record for it is made. Whether the
    /// next tick ran.
    pub fn step(&mut self) -> bool {
        if self.links.status == DESYNC {
            return false;
        }
        self.roll_back();
        self.sum_ready();
        let t = self.tick;
        if self.held_back() {
            self.stats.stalls += 1;
            self.stalled += 1;
            return false;
        }
        self.stalled = 0;
        if !self.inputs[self.seat].contains_key(&t) {
            return false;
        }
        self.run(t);
        self.tick += 1;
        self.sum_ready();
        self.prune();
        true
    }
}

#[cfg(test)]
mod tests;
