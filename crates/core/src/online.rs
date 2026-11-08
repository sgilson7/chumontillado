//! Online play, core side (M4): what crosses the wire between up to four
//! browsers, and the bookkeeping each keeps about the others.
//!
//! Chumworld Arena's `online.rs` (e78945b), from two seats to four (CS-2,
//! R8.1). The transport (`web/links/rtc.js`) connects browsers and carries
//! text; it hands this module three things: a peer arrived, a peer left,
//! and text from a peer. This module answers by leaving text in an outbox,
//! which the transport sends. It never sees a connection, so `cargo test`
//! plays a whole match between four simulated browsers, with lag, in a few
//! milliseconds.
//!
//! The shape (CS-Q6: A): every browser links to every other through the
//! room, and the host also passes on every record, checksum and drop it
//! hears from one guest to the others. So two guests that cannot link to
//! each other still hear each other, a little later, through the host; a
//! record that comes twice, once directly and once passed on, is the same
//! record and is kept once.
//!
//! The host is seat 0. It gives each guest a seat as its hello is accepted,
//! tells everyone who holds a seat, and starts the match when its player
//! chooses; seats nobody holds are played by bots (CS-Q17: A). A guest who
//! leaves during the match is played by its bot from a tick the host
//! chooses and tells everyone (`Msg::Drop`).
//!
//! T24: a match that is won brings everyone back to the room's lobby on
//! the host's word (`Msg::Again`), still linked, for the next match. Each
//! match has a number, and a record or checksum carries its match's, so
//! one from a match just ended that arrives late is let go.
//!
//! Every status is a key for a string in `data/strings.json`.

use crate::input::Input;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Bumped whenever a message changes shape or a rule changes how a tick
/// runs. It is in the room's name, so a stale tab and a fresh one still
/// meet, and the hello turns the stale one away with a reason.
pub const PROTOCOL: u32 = 2;

/// The first part of each room's name, keeping this game's rooms apart
/// from every other app on the same public relays.
pub const APP_ID: &str = "chumontillado";

/// The seats in a match: the host's and three more.
pub const SEATS: usize = 4;

/// Ticks between a local record and the tick that uses it (two ticks,
/// 33 ms, as the arena's).
pub const DELAY: u32 = 2;

/// Ticks between checksums (half a second).
pub const SUM_EVERY: u32 = 30;

/// Status keys. Each has a string in `data/strings.json`.
pub const WAITING: &str = "online.waiting";
pub const LOBBY: &str = "online.lobby";
pub const PLAYING: &str = "online.playing";
pub const RELOAD: &str = "online.reload";
pub const GARBLED: &str = "online.garbled";
pub const LEFT_ROOM: &str = "online.left";
pub const HOST_LEFT: &str = "online.host_left";
pub const FULL: &str = "online.full";
pub const DESYNC: &str = "online.desync";

/// The reasons a `Bye` may carry; anything else reads as `LEFT_ROOM`.
const REASONS: &[&str] = &[RELOAD, FULL];

/// The transport's id for another browser.
pub type Peer = u32;

/// Which seats people hold, one bit a seat.
pub type Seats = u8;

/// T34: every seat's bit.
pub const ALL_SEATS: Seats = (1 << SEATS) - 1;

pub fn holds(seats: Seats, seat: usize) -> bool {
    seat < SEATS && seats & (1 << seat) != 0
}

/// Everything that crosses the wire, as JSON tagged by `t`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Msg {
    Hello {
        protocol: u32,
        build: String,
        host: bool,
        /// M5: the skin this browser's player wears
        #[serde(default)]
        skin: u8,
        /// T34: and whether lifelike
        #[serde(default)]
        lifelike: bool,
    },
    /// M5: a guest to the host, in the lobby: its player's skin now
    Skin {
        skin: u8,
        /// T34: and whether lifelike
        #[serde(default)]
        lifelike: bool,
    },
    /// host to a guest: the seat it holds
    Welcome {
        seat: usize,
    },
    /// host to everyone, before the match: which seats people hold, and
    /// which of the rest the host has left empty (no bot)
    Roster {
        seats: Seats,
        #[serde(default)]
        empty: Seats,
        /// M5: each seat's skin
        #[serde(default)]
        skins: [u8; SEATS],
        /// T34: which seats wear theirs lifelike
        #[serde(default)]
        lifelike: Seats,
    },
    /// host to everyone: the match begins, these seats held by people and
    /// these left empty; bots play the others
    Start {
        seats: Seats,
        #[serde(default)]
        empty: Seats,
        /// M5: each seat's skin, bots' too
        #[serde(default)]
        skins: [u8; SEATS],
        /// T34: which seats wear theirs lifelike (bots never do)
        #[serde(default)]
        lifelike: Seats,
        /// T20: the bots' skill (`pilot::SKILLS` by number)
        #[serde(default = "normal_skill")]
        skill: u8,
        /// T32: the maps in order and the weapons' draw
        #[serde(default)]
        plan: crate::plan::Plan,
    },
    /// one seat's record for one tick
    Input {
        seat: usize,
        tick: u32,
        input: Input,
        /// T24: the match's number (none on the first)
        #[serde(default, skip_serializing_if = "is_first")]
        g: u32,
    },
    /// one seat's checksum of the world before a tick
    Sum {
        seat: usize,
        tick: u32,
        sum: u64,
        #[serde(default, skip_serializing_if = "is_first")]
        g: u32,
    },
    /// T24: host to everyone: the match is over; back to the lobby for
    /// match number `game`
    Again {
        game: u32,
    },
    /// host to everyone: a seat's player has gone, and its bot plays it
    /// from tick `from` on
    Drop {
        seat: usize,
        from: u32,
    },
    Bye {
        why: String,
    },
}

fn is_first(g: &u32) -> bool {
    *g == 0
}

fn normal_skill() -> u8 {
    crate::pilot::Skill::Normal as u8
}

pub fn encode(m: &Msg) -> String {
    serde_json::to_string(m).expect("a Msg always serialises")
}

pub fn decode(text: &str) -> Option<Msg> {
    serde_json::from_str(text).ok()
}

/// The room name the transport joins: the app id, the protocol and the
/// room code. No build hash, so mismatched builds meet and are told why.
pub fn room_name(code: &str) -> String {
    format!("{APP_ID}-p{PROTOCOL}-{code}")
}

/// What one browser knows of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Other {
    /// its hello has been accepted
    pub said: bool,
    pub host: bool,
    /// the seat it holds, once the host has given one
    pub seat: Option<usize>,
}

/// One browser's links to the others: who they are, the status, the
/// outbox, and the checksums waiting for their match.
#[derive(Debug, Clone)]
pub struct Net {
    pub host: bool,
    pub status: &'static str,
    build: String,
    pub others: BTreeMap<Peer, Other>,
    /// this browser's seat: 0 for the host, a guest's once welcomed
    pub seat: Option<usize>,
    /// which seats people hold
    pub seats: Seats,
    /// which seats the host has left empty: no bot plays them (the rest
    /// that nobody holds are bots')
    pub empty: Seats,
    /// M5: each seat's skin, as the host last said (or, for this browser's
    /// own seat, as chosen)
    pub skins: [u8; SEATS],
    /// this browser's player's own choice of skin
    pub chosen: u8,
    /// T34: which seats wear their skin lifelike, and this browser's own choice
    pub lifelike: Seats,
    pub chosen_lifelike: bool,
    /// T20: the bots' skill: the host's setting, sent with the start
    pub skill: u8,
    /// T32: the host's plan for the match, or the one it sent
    pub plan: Box<crate::plan::Plan>,
    /// the match has begun
    pub started: bool,
    /// T24: the match's number in this room, from 0
    pub game: u32,
    /// this browser's checksums and the others', by tick, kept until each
    /// pair can be compared
    mine: BTreeMap<u32, u64>,
    theirs: BTreeMap<(u32, usize), u64>,
    /// the last tick whose checksums all agreed
    pub agreed: Option<u32>,
    outbox: VecDeque<(Peer, String)>,
}

/// A message `Net` hands back to the session: a record, or a drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Input {
        seat: usize,
        tick: u32,
        input: Input,
    },
    Drop {
        seat: usize,
        from: u32,
    },
    Start {
        seats: Seats,
        empty: Seats,
        skins: [u8; SEATS],
        skill: u8,
    },
    /// a guest left during the match (the host only): drop its seat
    Gone {
        seat: usize,
    },
    /// T24: the host has called everyone back to the lobby
    Again,
}

impl Net {
    pub fn new(host: bool, build: &str) -> Net {
        Net {
            host,
            status: WAITING,
            build: build.to_string(),
            others: BTreeMap::new(),
            seat: host.then_some(0),
            seats: if host { 1 } else { 0 },
            empty: 0,
            skins: [0, 1, 2, 3],
            chosen: 0,
            lifelike: 0,
            chosen_lifelike: false,
            skill: normal_skill(),
            plan: Box::default(),
            started: false,
            game: 0,
            mine: BTreeMap::new(),
            theirs: BTreeMap::new(),
            agreed: None,
            outbox: VecDeque::new(),
        }
    }

    pub fn send(&mut self, to: Peer, m: &Msg) {
        self.outbox.push_back((to, encode(m)));
    }

    /// To every browser whose hello was accepted, but `except`.
    pub fn broadcast(&mut self, m: &Msg, except: Option<Peer>) {
        let to: Vec<Peer> = self
            .others
            .iter()
            .filter(|(p, o)| o.said && Some(**p) != except)
            .map(|(p, _)| *p)
            .collect();
        let text = encode(m);
        for p in to {
            self.outbox.push_back((p, text.clone()));
        }
    }

    pub fn take_outbox(&mut self) -> Vec<(Peer, String)> {
        self.outbox.drain(..).collect()
    }

    /// The host's peer, once its hello is in.
    pub fn host_peer(&self) -> Option<Peer> {
        self.others
            .iter()
            .find(|(_, o)| o.said && o.host)
            .map(|(p, _)| *p)
    }

    /// The transport says a browser arrived: say hello.
    pub fn connected(&mut self, peer: Peer) {
        if self.others.contains_key(&peer) {
            return;
        }
        self.others.insert(
            peer,
            Other {
                said: false,
                host: false,
                seat: None,
            },
        );
        let hello = Msg::Hello {
            protocol: PROTOCOL,
            build: self.build.clone(),
            host: self.host,
            skin: self.my_skin(),
            lifelike: self.chosen_lifelike,
        };
        self.send(peer, &hello);
    }

    /// The transport says a browser left. The host frees its seat before
    /// the match, or reports it gone during it; a guest that loses the host
    /// has lost the match. A guest losing another guest waits for the
    /// host's word.
    pub fn left(&mut self, peer: Peer) -> Option<Event> {
        let o = self.others.remove(&peer)?;
        if !o.said {
            return None;
        }
        if o.host {
            self.status = HOST_LEFT;
            return None;
        }
        if !self.host {
            return None;
        }
        let seat = o.seat?;
        if self.started {
            Some(Event::Gone { seat })
        } else {
            self.seats &= !(1 << seat);
            self.send_roster();
            None
        }
    }

    /// This browser leaves: everyone is told.
    pub fn leave(&mut self) {
        self.broadcast(
            &Msg::Bye {
                why: LEFT_ROOM.to_string(),
            },
            None,
        );
        self.others.clear();
    }

    /// The host gives a newly accepted guest the lowest free seat, or turns
    /// it away when the four are taken or the match has begun.
    fn seat_guest(&mut self, peer: Peer) {
        let free = (1..SEATS).find(|&s| !holds(self.seats, s));
        match free {
            Some(seat) if !self.started => {
                self.seats |= 1 << seat;
                if let Some(o) = self.others.get_mut(&peer) {
                    o.seat = Some(seat);
                }
                self.send(peer, &Msg::Welcome { seat });
                self.send_roster();
            }
            _ => {
                self.send(
                    peer,
                    &Msg::Bye {
                        why: FULL.to_string(),
                    },
                );
                self.others.remove(&peer);
            }
        }
    }

    /// The host tells everyone who holds which seat and which are empty.
    fn send_roster(&mut self) {
        // a seat a guest takes is no longer empty
        self.empty &= !self.seats;
        let (seats, empty, skins, lifelike) = (self.seats, self.empty, self.skins, self.lifelike);
        self.broadcast(
            &Msg::Roster {
                seats,
                empty,
                skins,
                lifelike,
            },
            None,
        );
    }

    /// This browser's own skin.
    pub fn my_skin(&self) -> u8 {
        self.chosen
    }

    /// M5: this browser's player chooses a skin, and (T34) whether to wear
    /// it lifelike. A guest tells the host, which tells everyone; the host
    /// tells everyone itself. Only in the lobby.
    pub fn choose_skin(&mut self, skin: u8, lifelike: bool) {
        if self.started {
            return;
        }
        self.chosen = skin;
        self.chosen_lifelike = lifelike;
        if let Some(seat) = self.seat {
            self.skins[seat] = skin;
            self.set_lifelike(seat, lifelike);
        }
        if self.host {
            self.send_roster();
        } else if let Some(h) = self.host_peer() {
            self.send(h, &Msg::Skin { skin, lifelike });
        }
    }

    fn set_lifelike(&mut self, seat: usize, on: bool) {
        if seat < SEATS {
            self.lifelike = self.lifelike & !(1 << seat) | (on as Seats) << seat;
        }
    }

    /// The skins at the start: each person's as chosen, and each bot's the
    /// first not worn by a person, in seat order, so the four differ where
    /// they can.
    pub fn starting_skins(&self) -> [u8; SEATS] {
        let mut skins = self.skins;
        let worn: Vec<u8> = (0..SEATS)
            .filter(|&s| holds(self.seats, s))
            .map(|s| skins[s])
            .collect();
        let mut free = (0..SEATS as u8).filter(|k| !worn.contains(k));
        for (s, skin) in skins.iter_mut().enumerate() {
            if !holds(self.seats, s) {
                *skin = free.next().unwrap_or(s as u8);
            }
        }
        skins
    }

    /// The host switches the bot in a seat nobody holds on or off. Whether
    /// it changed anything: a seat a person holds, or a match under way,
    /// is not the host's to change.
    pub fn toggle_bot(&mut self, seat: usize) -> bool {
        if !self.host || self.started || seat == 0 || seat >= SEATS || holds(self.seats, seat) {
            return false;
        }
        self.empty ^= 1 << seat;
        self.send_roster();
        true
    }

    /// The host starts the match with the seats held now, and those left
    /// empty.
    pub fn start(&mut self) -> (Seats, Seats, [u8; SEATS]) {
        self.started = true;
        self.status = PLAYING;
        let (seats, empty) = (self.seats, self.empty & !self.seats);
        let skins = self.starting_skins();
        self.skins = skins;
        // T34: the bots wear theirs painted
        self.lifelike &= seats;
        let lifelike = self.lifelike;
        let skill = self.skill;
        let plan = (*self.plan).clone();
        self.broadcast(
            &Msg::Start {
                seats,
                empty,
                skins,
                lifelike,
                skill,
                plan,
            },
            None,
        );
        (seats, empty, skins)
    }

    /// T24: the host ends the match and calls everyone back to the lobby
    /// for the next, with the seats of those still here.
    pub fn again(&mut self) {
        if !self.host || !self.started {
            return;
        }
        self.back(self.game + 1);
        self.broadcast(&Msg::Again { game: self.game }, None);
        self.seats = 1 | self
            .others
            .values()
            .filter_map(|o| o.seat)
            .fold(0, |seats, s| seats | 1 << s);
        self.send_roster();
    }

    /// T24: back in the lobby, waiting for match `game`.
    fn back(&mut self, game: u32) {
        self.game = game;
        self.started = false;
        self.status = LOBBY;
        self.mine.clear();
        self.theirs.clear();
        self.agreed = None;
        if let Some(s) = self.seat {
            self.skins[s] = self.chosen;
            self.set_lifelike(s, self.chosen_lifelike);
        }
    }

    /// Text from a browser. Anything that cannot be a message, or that no
    /// honest copy would send, changes the status and nothing else. The
    /// host passes on what one guest sends to the others.
    pub fn message(&mut self, from: Peer, text: &str) -> Option<Event> {
        let o = *self.others.get(&from)?;
        let Some(msg) = decode(text) else {
            self.status = GARBLED;
            return None;
        };
        match msg {
            Msg::Hello {
                protocol,
                build,
                host,
                skin,
                lifelike,
            } => {
                if o.said {
                    return None;
                }
                // two hosts, or a guest meeting a guest before it has a
                // host, are both allowed; another build is not
                if protocol != PROTOCOL || build != self.build {
                    self.send(
                        from,
                        &Msg::Bye {
                            why: RELOAD.to_string(),
                        },
                    );
                    self.others.remove(&from);
                    self.status = RELOAD;
                    return None;
                }
                if host && self.host {
                    // one host to a room
                    self.others.remove(&from);
                    return None;
                }
                let me = self.others.get_mut(&from).expect("known above");
                me.said = true;
                me.host = host;
                if self.host {
                    self.seat_guest(from);
                    if let Some(seat) = self.others.get(&from).and_then(|o| o.seat) {
                        self.skins[seat] = skin.min(crate::skins::COUNT as u8 - 1);
                        self.set_lifelike(seat, lifelike);
                        self.send_roster();
                    }
                    if self.status == WAITING {
                        self.status = LOBBY;
                    }
                }
                None
            }
            Msg::Bye { why } => {
                self.others.remove(&from);
                // to a guest only the host's bye matters: the room was full,
                // this build was turned away, or the host left. (The host's
                // hello comes first on its ordered channel, so `o.host` is
                // known by then.)
                if o.host {
                    self.status = REASONS.iter().copied().find(|r| *r == why).unwrap_or(HOST_LEFT);
                }
                if let (true, Some(seat)) = (self.host, o.seat) {
                    if self.started {
                        return Some(Event::Gone { seat });
                    }
                    self.seats &= !(1 << seat);
                    self.send_roster();
                }
                None
            }
            _ if !o.said => {
                // data before a hello: not one of ours
                self.status = GARBLED;
                None
            }
            Msg::Welcome { seat } if o.host && !self.host => {
                if seat == 0 || seat >= SEATS {
                    self.status = GARBLED;
                    return None;
                }
                self.seat = Some(seat);
                self.status = LOBBY;
                None
            }
            Msg::Roster {
                seats,
                empty,
                skins,
                lifelike,
            } if o.host && !self.host => {
                // this browser's own choice stands until the host has it
                self.seats = seats;
                self.empty = empty;
                self.skins = skins;
                self.lifelike = lifelike & ALL_SEATS;
                if let Some(s) = self.seat {
                    self.skins[s] = self.chosen;
                    self.set_lifelike(s, self.chosen_lifelike);
                }
                None
            }
            Msg::Skin { skin, lifelike } if self.host && !self.started => {
                if let Some(seat) = o.seat {
                    self.skins[seat] = skin.min(crate::skins::COUNT as u8 - 1);
                    self.set_lifelike(seat, lifelike);
                    self.send_roster();
                }
                None
            }
            Msg::Start {
                seats,
                empty,
                skins,
                lifelike,
                skill,
                plan,
            } if o.host && !self.host => {
                if self.started || self.seat.is_none_or(|s| !holds(seats, s)) {
                    return None;
                }
                self.seats = seats;
                self.empty = empty;
                self.skins = skins;
                // T34: a seat a person does not hold is a bot's, painted
                self.lifelike = lifelike & seats & ALL_SEATS;
                self.skill = skill;
                *self.plan = plan;
                self.started = true;
                self.status = PLAYING;
                Some(Event::Start {
                    seats,
                    empty,
                    skins,
                    skill,
                })
            }
            Msg::Drop { seat, from: at } if o.host && !self.host => Some(Event::Drop { seat, from: at }),
            Msg::Again { game } if o.host && !self.host => {
                // only the next match: a second word of it changes nothing
                if !self.started || game != self.game + 1 {
                    return None;
                }
                self.back(game);
                Some(Event::Again)
            }
            // T24: a record or checksum of another match than this one,
            // come late or early, is let go
            Msg::Input { g, .. } | Msg::Sum { g, .. } if g != self.game => None,
            Msg::Input {
                seat, tick, input, ..
            } => {
                if !self.started || !holds(self.seats, seat) || Some(seat) == self.seat {
                    self.status = GARBLED;
                    return None;
                }
                Some(Event::Input { seat, tick, input })
            }
            Msg::Sum { seat, tick, sum, .. } => {
                if !holds(self.seats, seat) || Some(seat) == self.seat {
                    self.status = GARBLED;
                    return None;
                }
                if self.theirs.contains_key(&(tick, seat)) {
                    return None;
                }
                if self.host {
                    let g = self.game;
                    self.broadcast(&Msg::Sum { seat, tick, sum, g }, Some(from));
                }
                self.theirs.insert((tick, seat), sum);
                self.compare();
                None
            }
            // a host-only message from a guest
            _ => {
                self.status = GARBLED;
                None
            }
        }
    }

    /// The host passes a record it has newly taken in on to every guest but
    /// the one it came from (`relay` is that guest's peer).
    pub fn pass_on(&mut self, m: &Msg, relay: Peer) {
        if self.host {
            self.broadcast(m, Some(relay));
        }
    }

    /// This browser's checksum of tick `tick`, sent and kept for comparing.
    pub fn summed(&mut self, tick: u32, sum: u64) {
        let Some(seat) = self.seat else { return };
        self.mine.insert(tick, sum);
        let g = self.game;
        self.broadcast(&Msg::Sum { seat, tick, sum, g }, None);
        self.compare();
    }

    /// Each checksum another seat has sent for a tick this browser has
    /// summed: a mismatch is a desync.
    fn compare(&mut self) {
        let ready: Vec<(u32, usize)> = self
            .theirs
            .keys()
            .filter(|(t, _)| self.mine.contains_key(t))
            .copied()
            .collect();
        for k in ready {
            let theirs = self.theirs.remove(&k);
            if theirs != self.mine.get(&k.0).copied() {
                self.status = DESYNC;
            } else {
                self.agreed = Some(self.agreed.map_or(k.0, |a| a.max(k.0)));
            }
        }
        // a checksum of a tick long past no longer waits for its match
        if let Some(&newest) = self.mine.keys().next_back() {
            let keep = newest.saturating_sub(SUM_EVERY * 8);
            self.mine = self.mine.split_off(&keep);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::input::FORWARD;

    pub const HOST: Peer = 100;
    pub const A: Peer = 101;
    pub const B: Peer = 102;

    /// Every message from `from`'s outbox handed to the browsers `to`
    /// names, each under the peer id it has for `from`.
    fn deliver(from: &mut Net, from_id: Peer, to: &mut [(&mut Net, Peer)]) {
        for (dest, text) in from.take_outbox() {
            for (links, id) in to.iter_mut() {
                if *id == dest {
                    links.message(from_id, &text);
                }
            }
        }
    }

    #[test]
    fn every_message_survives_encoding_and_decoding() {
        let all = [
            Msg::Hello {
                protocol: PROTOCOL,
                build: "b".into(),
                host: true,
                skin: 2,
                lifelike: true,
            },
            Msg::Skin {
                skin: 1,
                lifelike: true,
            },
            Msg::Welcome { seat: 2 },
            Msg::Roster {
                seats: 0b1011,
                empty: 0b0100,
                skins: [3, 0, 1, 2],
                lifelike: 0b0010,
            },
            Msg::Start {
                seats: 0b11,
                empty: 0b1000,
                skins: [0, 1, 2, 3],
                lifelike: 0b0001,
                skill: 2,
                plan: crate::plan::Plan {
                    maps: vec![11, 0],
                    weapons: 5,
                },
            },
            Msg::Input {
                seat: 3,
                tick: 7,
                input: Input {
                    keys: FORWARD,
                    buttons: 1,
                    dx: -12,
                    dy: 30,
                    wheel: 0,
                },
                g: 0,
            },
            Msg::Sum {
                seat: 1,
                tick: 30,
                sum: u64::MAX - 3,
                g: 0,
            },
            Msg::Drop { seat: 2, from: 400 },
            Msg::Bye { why: FULL.into() },
        ];
        for m in all {
            assert_eq!(decode(&encode(&m)), Some(m));
        }
        assert_eq!(decode("{\"t\":\"nonsense\"}"), None);
    }

    #[test]
    fn the_room_name_holds_the_app_the_protocol_and_the_code_and_no_build() {
        assert_eq!(room_name("KQXRT"), "chumontillado-p2-KQXRT");
    }

    #[test]
    fn the_host_seats_guests_in_turn_and_turns_away_a_fifth() {
        let mut host = Net::new(true, "b");
        let mut guests: Vec<Net> = (0..4).map(|_| Net::new(false, "b")).collect();
        for (k, g) in guests.iter_mut().enumerate() {
            let id = A + k as Peer;
            host.connected(id);
            g.connected(HOST);
            deliver(g, id, &mut [(&mut host, HOST)]);
            for (dest, text) in host.take_outbox() {
                if dest == id {
                    g.message(HOST, &text);
                }
            }
        }
        let seats: Vec<Option<usize>> = guests.iter().map(|g| g.seat).collect();
        assert_eq!(seats, vec![Some(1), Some(2), Some(3), None]);
        assert_eq!(guests[3].status, FULL, "the fifth is told the room is full");
        assert_eq!(host.seats, 0b1111);
        assert_eq!(
            guests[1].seats, 0b0111,
            "seat 2, as the roster stood when it was told"
        );
    }

    #[test]
    fn another_build_is_turned_away_and_both_ends_say_why() {
        let mut host = Net::new(true, "new");
        let mut guest = Net::new(false, "old");
        host.connected(A);
        guest.connected(HOST);
        deliver(&mut guest, A, &mut [(&mut host, HOST)]);
        deliver(&mut host, HOST, &mut [(&mut guest, A)]);
        assert_eq!((host.status, guest.status), (RELOAD, RELOAD));
        assert_eq!(host.seats, 0b1, "no seat given");
    }

    #[test]
    fn garbage_changes_the_status_and_nothing_else_and_data_before_a_hello_is_refused() {
        let mut host = Net::new(true, "b");
        host.connected(A);
        assert_eq!(
            host.message(A, "{\"t\":\"sum\",\"seat\":1,\"tick\":0,\"sum\":1}"),
            None
        );
        assert_eq!(host.status, GARBLED);
        let mut host = Net::new(true, "b");
        host.connected(A);
        host.message(A, "not json");
        assert_eq!((host.status, host.seats), (GARBLED, 0b1));
        // a peer never connected is ignored outright
        let mut host = Net::new(true, "b");
        assert_eq!(host.message(B, "not json"), None);
        assert_eq!(host.status, WAITING);
    }

    #[test]
    fn a_guest_leaving_the_lobby_frees_its_seat_and_during_the_match_is_reported_gone() {
        let mut host = Net::new(true, "b");
        let mut g = Net::new(false, "b");
        host.connected(A);
        g.connected(HOST);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        assert_eq!(host.seats, 0b11);
        assert_eq!(host.left(A), None);
        assert_eq!(host.seats, 0b1);
        // again, and this time the match begins first
        host.connected(B);
        let mut g = Net::new(false, "b");
        g.connected(HOST);
        deliver(&mut g, B, &mut [(&mut host, HOST)]);
        host.start();
        assert_eq!(host.left(B), Some(Event::Gone { seat: 1 }));
    }

    #[test]
    fn a_guest_that_loses_the_host_has_lost_the_match() {
        let mut host = Net::new(true, "b");
        let mut g = Net::new(false, "b");
        host.connected(A);
        g.connected(HOST);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        deliver(&mut host, HOST, &mut [(&mut g, A)]);
        g.left(HOST);
        assert_eq!(g.status, HOST_LEFT);
    }

    #[test]
    fn a_checksum_that_differs_from_this_browsers_is_a_desync() {
        let mut host = Net::new(true, "b");
        let mut g = Net::new(false, "b");
        host.connected(A);
        g.connected(HOST);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        deliver(&mut host, HOST, &mut [(&mut g, A)]);
        host.start();
        deliver(&mut host, HOST, &mut [(&mut g, A)]);
        host.summed(30, 7);
        g.summed(30, 7);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        assert_eq!((host.status, host.agreed), (PLAYING, Some(30)));
        host.summed(60, 7);
        g.summed(60, 8);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        assert_eq!(host.status, DESYNC);
    }

    #[test]
    fn the_host_calls_everyone_back_to_the_lobby_and_a_late_record_of_the_match_before_is_let_go() {
        let mut host = Net::new(true, "b");
        let mut a = Net::new(false, "b");
        let mut b = Net::new(false, "b");
        host.connected(A);
        host.connected(B);
        a.connected(HOST);
        b.connected(HOST);
        a.connected(B);
        b.connected(A);
        for _ in 0..3 {
            deliver(&mut host, HOST, &mut [(&mut a, A), (&mut b, B)]);
            deliver(&mut a, A, &mut [(&mut host, HOST), (&mut b, B)]);
            deliver(&mut b, B, &mut [(&mut host, HOST), (&mut a, A)]);
        }
        host.start();
        deliver(&mut host, HOST, &mut [(&mut a, A), (&mut b, B)]);
        assert!(a.started && b.started);
        // b leaves during the match; the rest are called back
        host.left(B);
        a.left(B);
        host.again();
        let words = host.take_outbox();
        let again = words
            .iter()
            .find(|(p, _)| *p == A)
            .map(|(_, t)| t.clone())
            .unwrap();
        assert_eq!(a.message(HOST, &again), Some(Event::Again));
        for n in [&host, &a] {
            assert_eq!((n.started, n.status, n.game), (false, LOBBY, 1));
        }
        assert_eq!(a.message(HOST, &again), None, "a second word changes nothing");
        assert_eq!(a.game, 1);
        // the host's seats are those still here, and it says so
        assert_eq!(host.seats, 0b011);
        let roster = words
            .iter()
            .filter(|(p, _)| *p == A)
            .map(|(_, t)| t.clone())
            .next_back()
            .unwrap();
        a.message(HOST, &roster);
        assert_eq!(a.seats, 0b011);
        // a record from the match just ended, come late, is let go
        let late = Msg::Input {
            seat: 0,
            tick: 9_000,
            input: Input::default(),
            g: 0,
        };
        assert_eq!(a.message(HOST, &encode(&late)), None);
        assert_eq!(a.status, LOBBY);
        // and the next match's are taken
        host.start();
        deliver(&mut host, HOST, &mut [(&mut a, A)]);
        assert!(a.started);
        let next = Msg::Input {
            seat: 0,
            tick: 3,
            input: Input::default(),
            g: 1,
        };
        assert!(matches!(
            a.message(HOST, &encode(&next)),
            Some(Event::Input { tick: 3, .. })
        ));
        assert!(
            !encode(&late).contains("\"g\""),
            "the first match's records are as they were"
        );
    }

    #[test]
    fn the_host_switches_a_free_seats_bot_off_and_on_and_a_guest_taking_it_fills_it() {
        let mut host = Net::new(true, "b");
        let mut g = Net::new(false, "b");
        host.connected(A);
        g.connected(HOST);
        deliver(&mut g, A, &mut [(&mut host, HOST)]);
        deliver(&mut host, HOST, &mut [(&mut g, A)]);
        // the guest holds seat 1; seats 2 and 3 are free
        assert!(host.toggle_bot(3));
        assert_eq!(host.empty, 0b1000);
        deliver(&mut host, HOST, &mut [(&mut g, A)]);
        assert_eq!(g.empty, 0b1000, "every guest is told");
        assert!(
            !host.toggle_bot(1),
            "a seat a person holds is not the host's to empty"
        );
        assert!(!host.toggle_bot(0));
        assert!(!g.toggle_bot(2), "only the host switches");
        assert!(host.toggle_bot(3));
        assert_eq!(host.empty, 0);
        // switched off again, then a second guest arrives and takes seat 2,
        // the lowest free: seat 3 stays empty
        host.toggle_bot(3);
        host.toggle_bot(2);
        let mut g2 = Net::new(false, "b");
        host.connected(B);
        g2.connected(HOST);
        deliver(&mut g2, B, &mut [(&mut host, HOST)]);
        assert_eq!((host.seats, host.empty), (0b0111, 0b1000));
        assert_eq!(host.start().0..=host.seats, 0b0111..=0b0111);
        assert_eq!(host.empty, 0b1000);
        assert!(!host.toggle_bot(3), "not once the match is under way");
    }
}
