//! Four browsers played against each other through a simulated wire, with
//! lag, a pair that cannot link, and a guest who leaves. The world each ends
//! on is held to the world one browser reaches with every record on time.

use super::*;
use crate::input::{BACK, CROUCH, FIRE, FORWARD, JUMP, LEFT, RIGHT, SLOT_8};
use crate::online::{Net, LEFT_ROOM, PLAYING};
use crate::world::World;

/// A seat's record for a tick: moving, jumping, turning and firing,
/// changing every few ticks so guesses go wrong often. Fixed by the seat and
/// the tick alone.
fn script(seat: usize, t: u32) -> Input {
    if t < DELAY {
        return Input::default();
    }
    let k = t.wrapping_mul(2_654_435_761).rotate_left(seat as u32 * 7 + 3) ^ (t / 11);
    let walk = [FORWARD, FORWARD | LEFT, FORWARD | RIGHT | JUMP, BACK, CROUCH][(k % 5) as usize];
    Input {
        keys: walk | if t == 40 + seat as u32 { SLOT_8 } else { 0 },
        buttons: if (t / 9 + seat as u32).is_multiple_of(3) {
            FIRE
        } else {
            0
        },
        dx: ((k >> 8) % 801) as i32 - 400,
        dy: ((k >> 16) % 201) as i32 - 100,
        wheel: 0,
    }
}

/// The world a browser reaches with every seat's record on time, the seats
/// `seats` holds by people and seat `gone` (if any) its bot's from `from`.
fn truth(seats: Seats, ticks: u32, gone: Option<(usize, u32)>) -> World {
    let mut w = World::online(seats, 0, [0, 1, 2, 3]);
    for t in 0..ticks {
        if let Some((s, from)) = gone {
            if t == from {
                w.hand_to_bot(s);
            }
        }
        let rec: [Input; SEATS] = std::array::from_fn(|s| script(s, t));
        Sim::tick(&mut w, &rec);
    }
    w
}

/// Every checksum browser `s` took (of worlds made from true records
/// only) is the on-time world's at that tick; and it took some.
fn sums_true(s: &Rollback<World>, seats: Seats, gone: Option<(usize, u32)>, at_least: u32) {
    assert!(
        s.made.back().is_some_and(|(t, _)| *t >= at_least),
        "seat {} summed only to {:?}",
        s.seat,
        s.made.back()
    );
    for &(t, sum) in &s.made {
        assert_eq!(sum, truth(seats, t, gone).sum(), "seat {} at tick {t}", s.seat);
    }
}

/// Text in flight from one browser to another, and the frame it lands.
struct Wire {
    lag: u32,
    flight: VecDeque<(u32, String)>,
}

/// Four (or fewer) browsers, each with its peer id for every other, and
/// the wires between them; a missing wire is a pair that cannot link.
struct Room {
    nets: Vec<Net>,
    sessions: Vec<Option<Rollback<World>>>,
    wires: BTreeMap<(usize, usize), Wire>,
    frame: u32,
    /// browsers that have left
    gone: Vec<usize>,
}

/// Browser `b`'s peer id for browser `a`.
fn id(a: usize) -> Peer {
    100 + a as Peer
}

impl Room {
    /// `n` browsers, browser 0 the host, linked pair by pair with `lag`
    /// frames each way, but for the pairs in `apart`.
    fn new(n: usize, lag: impl Fn(usize, usize) -> u32, apart: &[(usize, usize)]) -> Room {
        let mut nets: Vec<Net> = (0..n).map(|b| Net::new(b == 0, "b")).collect();
        let mut wires = BTreeMap::new();
        for a in 0..n {
            for b in 0..n {
                let cut = apart.contains(&(a, b)) || apart.contains(&(b, a));
                if a != b && !cut {
                    wires.insert(
                        (a, b),
                        Wire {
                            lag: lag(a, b),
                            flight: VecDeque::new(),
                        },
                    );
                }
            }
        }
        // the host meets each guest first, so seats go in browser order
        for b in 1..n {
            for a in 0..b {
                if wires.contains_key(&(a, b)) {
                    nets[a].connected(id(b));
                    nets[b].connected(id(a));
                }
            }
        }
        let mut room = Room {
            nets,
            sessions: (0..n).map(|_| None).collect(),
            wires,
            frame: 0,
            gone: Vec::new(),
        };
        // the hellos, welcomes and rosters, before anyone plays
        for _ in 0..20 {
            room.carry();
        }
        room
    }

    /// Everything each browser has to send onto its wires, and everything
    /// due off them handed over.
    fn carry(&mut self) {
        let n = self.nets.len();
        for a in 0..n {
            let out = match &mut self.sessions[a] {
                Some(s) => s.take_outbox(),
                None => self.nets[a].take_outbox(),
            };
            for (to, text) in out {
                let b = (to - 100) as usize;
                if let Some(w) = self.wires.get_mut(&(a, b)) {
                    w.flight.push_back((self.frame + w.lag, text));
                }
            }
        }
        for ((a, b), w) in self.wires.iter_mut() {
            while w.flight.front().is_some_and(|(at, _)| *at <= self.frame) {
                let (_, text) = w.flight.pop_front().expect("checked");
                if self.gone.contains(b) {
                    continue;
                }
                let start = match &mut self.sessions[*b] {
                    Some(s) => {
                        s.message(id(*a), &text);
                        None
                    }
                    None => self.nets[*b].message(id(*a), &text),
                };
                if let Some(crate::online::Event::Start { seats, empty, skins }) = start {
                    let links = std::mem::replace(&mut self.nets[*b], Net::new(false, "b"));
                    let seat = links.seat.expect("a started guest has a seat");
                    self.sessions[*b] = Some(Rollback::new(
                        links,
                        seat,
                        seats,
                        World::online(seats, empty, skins),
                    ));
                }
            }
        }
        self.frame += 1;
    }

    /// The host starts the match.
    fn start(&mut self) {
        let mut links = std::mem::replace(&mut self.nets[0], Net::new(true, "b"));
        let (seats, empty, skins) = links.start();
        self.sessions[0] = Some(Rollback::new(links, 0, seats, World::online(seats, empty, skins)));
    }

    /// One frame: every browser in the match makes its record and steps once.
    fn frame(&mut self) {
        for (b, s) in self.sessions.iter_mut().enumerate() {
            if self.gone.contains(&b) {
                continue;
            }
            if let Some(s) = s {
                s.local(script(s.seat, s.tick + DELAY));
                s.step();
            }
        }
        self.carry();
    }

    /// Browser `b` leaves without a word: the others' transports report it.
    fn vanish(&mut self, b: usize) {
        self.gone.push(b);
        for (a, s) in self.sessions.iter_mut().enumerate() {
            if a != b && self.wires.contains_key(&(a, b)) {
                if let Some(s) = s {
                    s.left(id(b));
                }
            }
        }
    }

    fn session(&self, b: usize) -> &Rollback<World> {
        self.sessions[b].as_ref().expect("in the match")
    }

    /// Frames until every browser still here has run `ticks` ticks.
    fn play_to(&mut self, ticks: u32) {
        for _ in 0..ticks * 4 {
            let behind = (0..self.sessions.len())
                .filter(|b| !self.gone.contains(b))
                .any(|b| self.session(b).tick < ticks);
            if !behind {
                return;
            }
            self.frame();
        }
        panic!("the match stalled");
    }
}

#[test]
fn four_browsers_with_lag_end_on_the_world_one_browser_reaches_on_time() {
    // lags of 1 to 6 frames (17 to 100 ms) each way, different per pair
    let mut room = Room::new(4, |a, b| 1 + ((a * 3 + b * 5) % 6) as u32, &[]);
    assert_eq!(room.nets[0].seats, 0b1111);
    room.start();
    room.play_to(600);
    // the world each browser shows ends on a few guessed ticks; what it
    // has confirmed is held to the truth
    for b in 0..4 {
        let s = room.session(b);
        assert_eq!(s.links.status, PLAYING, "browser {b}");
        sums_true(s, 0b1111, None, 540);
    }
    // rollback did its work: guesses went wrong and were run again
    assert!(room.session(1).stats.rollbacks > 10);
    // and the checksums agreed past the first seconds
    assert!(room.session(0).links.agreed.is_some_and(|t| t >= 300));
}

#[test]
fn two_guests_that_cannot_link_hear_each_other_through_the_host() {
    let mut room = Room::new(4, |_, _| 2, &[(1, 2), (2, 3)]);
    room.start();
    room.play_to(400);
    for b in 0..4 {
        sums_true(room.session(b), 0b1111, None, 330);
        assert_eq!(room.session(b).links.status, PLAYING);
    }
}

#[test]
fn two_people_play_with_bots_in_the_empty_seats() {
    let mut room = Room::new(2, |_, _| 3, &[]);
    room.start();
    room.play_to(300);
    assert_eq!(room.session(1).seats, 0b11);
    for b in 0..2 {
        sums_true(room.session(b), 0b11, None, 240);
    }
    let w = &room.session(1).world;
    assert!(w.players[2].pilot.is_some() && w.players[3].pilot.is_some());
    assert!(w.players[0].pilot.is_none() && w.players[1].pilot.is_none());
}

#[test]
fn a_seat_the_host_left_empty_has_nobody_in_it_on_every_browser() {
    let mut room = Room::new(2, |_, _| 3, &[]);
    room.nets[0].toggle_bot(3);
    room.start();
    room.play_to(300);
    for b in 0..2 {
        let s = room.session(b);
        assert!(
            s.world.players[3].out && !s.world.players[3].alive(),
            "browser {b}"
        );
        assert!(s.world.players[2].pilot.is_some(), "seat 2 is still a bot's");
        assert_eq!(s.links.status, PLAYING);
        for &(t, sum) in &s.made {
            let mut w = World::online(0b11, 0b1000, [0, 1, 2, 3]);
            for k in 0..t {
                let rec: [Input; SEATS] = std::array::from_fn(|s| script(s, k));
                Sim::tick(&mut w, &rec);
            }
            assert_eq!(sum, w.sum(), "browser {b} at tick {t}");
        }
        assert!(!s.made.is_empty());
    }
}

#[test]
fn a_guest_who_leaves_is_played_by_its_bot_from_the_tick_the_host_chose_on_every_browser() {
    let mut room = Room::new(4, |a, b| 1 + ((a + 2 * b) % 4) as u32, &[]);
    room.start();
    room.play_to(200);
    room.vanish(2);
    room.play_to(500);
    let from = room.session(0).drops[2].expect("the host dropped seat 2");
    // R10.12: told for four seconds after the drop, then not
    let s = room.session(1);
    let since = s.tick - from;
    assert_eq!(s.dropped_within(since + 1), Some(2));
    assert_eq!(s.dropped_within(since), None);
    for b in [0, 1, 3] {
        let s = room.session(b);
        assert_eq!(s.drops[2], Some(from), "browser {b}");
        assert!(s.world.players[2].pilot.is_some());
        assert_eq!(s.links.status, PLAYING, "browser {b}");
    }
    for b in [0, 1, 3] {
        sums_true(room.session(b), 0b1111, Some((2, from)), 450);
    }
    assert!(room.session(1).links.agreed.is_some_and(|t| t > from));
}

#[test]
fn a_record_that_comes_twice_is_kept_once_and_two_different_ones_are_garbled() {
    let mut room = Room::new(2, |_, _| 1, &[]);
    room.start();
    room.play_to(60);
    let s = room.sessions[1].as_mut().expect("in");
    let tick = s.tick + 3;
    let rec = Msg::Input {
        seat: 0,
        tick,
        input: script(0, tick),
    };
    let text = crate::online::encode(&rec);
    s.message(id(0), &text);
    s.message(id(0), &text);
    assert_eq!(s.links.status, PLAYING);
    let other = Msg::Input {
        seat: 0,
        tick,
        input: Input {
            keys: JUMP,
            ..Input::default()
        },
    };
    s.message(id(0), &crate::online::encode(&other));
    assert_eq!(s.links.status, GARBLED);
}

#[test]
fn a_browser_runs_no_more_than_the_window_past_a_silent_seat() {
    let mut room = Room::new(2, |_, _| 1, &[]);
    room.start();
    room.play_to(30);
    // the guest's records stop arriving: cut its wire to the host
    room.wires.remove(&(1, 0));
    for _ in 0..100 {
        room.frame();
    }
    let host = room.session(0);
    assert_eq!(host.tick, host.frontier() + WINDOW, "held at the window");
    assert!(host.stats.stalls > 0);
    // R10.12: held for most of those 100 frames in a row, and waiting on
    // seat 2 (index 1), the guest
    assert!(host.stalled > 80, "{}", host.stalled);
    assert_eq!(host.behind(), Some(1));
    let _ = LEFT_ROOM;
}
