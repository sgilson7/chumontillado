//! An online match as the game runs it (M4): hosting and joining by room
//! code or invite link, the lobby where the host waits for players and
//! starts, the rollback session (`rollback.rs`) on the match's world, and
//! what the page's transport is told to do.
//!
//! After Chumworld Arena's `versus.rs` (e78945b), for four seats and room
//! codes only (R8.4). The page's transport (`web/links/rtc.js`, through
//! `web/online.js`) decides nothing: the core gives it numbered commands in
//! the view model (`Cmd`: open a room, close), which the page runs once
//! each, in order, and reports back through the shim's `online_connected`,
//! `online_left`, `online_message`, and `act("online.error", key)`.
//!
//! Nothing here runs until the reader hosts or joins; until then the view
//! model asks for no link and the page loads nothing for online play.

use serde::Serialize;

use crate::online::{self, Event, Net, Peer, Seats};
use crate::rollback::Rollback;
use crate::world::World;

/// The letters a room code is made of: no I or O, which read as 1 and 0.
pub const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
/// How many letters a room code has.
pub const CODE_LENGTH: usize = 5;
/// The key an invite link carries its room code under: `?join=CODE`.
pub const LINK_KEY: &str = "join";
/// No word from the host for this long ends a guest's match: 10 s.
pub const TIMEOUT_US: i64 = 10_000_000;
/// The status key for that (the others are `online.rs`'s and the
/// transport's).
pub const TIMEOUT: &str = "online.timeout";
/// The transport's error keys: each has a string in `data/strings.json`.
pub const TRANSPORT_ERRORS: &[&str] = &["online.no_bundle", "online.no_relay", "online.no_path"];
/// The statuses that end a match or a lobby.
pub const ENDED: &[&str] = &[
    online::LEFT_ROOM,
    online::HOST_LEFT,
    online::RELOAD,
    online::FULL,
    online::DESYNC,
    online::GARBLED,
    TIMEOUT,
];

/// A room code from what a reader typed or pasted: its letters, upper
/// case, with spaces and dashes ignored; or a whole invite link, whose code
/// is read from it. None if that is not five of `CODE_LETTERS`.
pub fn read_code(text: &str) -> Option<String> {
    let key = format!("{LINK_KEY}=");
    let text = match text.find(&key) {
        Some(at) => text[at + key.len()..].split(['&', '#']).next().unwrap_or(""),
        None => text,
    };
    let code: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let ok = code.len() == CODE_LENGTH && code.bytes().all(|b| CODE_LETTERS.contains(&b));
    ok.then_some(code)
}

/// A room code from a seed: `CODE_LENGTH` letters.
pub fn make_code(seed: u32) -> String {
    let mut r = crate::rng::Rng::new(seed as u64);
    (0..CODE_LENGTH)
        .map(|_| CODE_LETTERS[r.below(CODE_LETTERS.len() as u32) as usize] as char)
        .collect()
}

/// The invite link for a room: this page's address with the code.
pub fn invite_link(address: &str, code: &str) -> String {
    format!("{address}?{LINK_KEY}={code}")
}

/// One thing for the page's transport to do: `open` a room or `close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cmd {
    pub id: u32,
    pub act: String,
    pub host: bool,
    /// always `room` (by room code); the transport's pasted codes are not
    /// used here (R8.4)
    pub mode: String,
    /// the room's name
    pub room: String,
    pub code: String,
}

/// Before the host starts, or after.
#[derive(Debug, Clone)]
pub enum Stage {
    Lobby(Net),
    Match(Box<Rollback<World>>),
}

#[derive(Debug, Clone)]
pub struct Versus {
    pub host: bool,
    pub code: String,
    pub stage: Stage,
    /// the transport's error, or a timeout
    pub error: Option<&'static str>,
    /// the page's clock when the host was last heard (a guest's)
    pub heard_us: Option<i64>,
}

impl Versus {
    pub fn new(host: bool, code: &str, build: &str) -> Versus {
        Versus {
            host,
            code: code.to_string(),
            stage: Stage::Lobby(Net::new(host, build)),
            error: None,
            heard_us: None,
        }
    }

    pub fn links(&self) -> &Net {
        match &self.stage {
            Stage::Lobby(n) => n,
            Stage::Match(s) => &s.links,
        }
    }

    fn net_mut(&mut self) -> &mut Net {
        match &mut self.stage {
            Stage::Lobby(n) => n,
            Stage::Match(s) => &mut s.links,
        }
    }

    /// The status key: the transport's error or a timeout if any, else the
    /// links'.
    pub fn status(&self) -> &'static str {
        self.error.unwrap_or(self.links().status)
    }

    /// The match or lobby is over: a status that ends it, or the
    /// transport's error (no relay, the bundle missing, the host unreached).
    pub fn ended(&self) -> bool {
        ENDED.contains(&self.status()) || self.error.is_some()
    }

    pub fn session(&self) -> Option<&Rollback<World>> {
        match &self.stage {
            Stage::Match(s) => Some(s),
            Stage::Lobby(_) => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut Rollback<World>> {
        match &mut self.stage {
            Stage::Match(s) => Some(s),
            Stage::Lobby(_) => None,
        }
    }

    /// Which seats people hold, as this browser knows.
    pub fn seats(&self) -> Seats {
        match &self.stage {
            Stage::Lobby(n) => n.seats,
            Stage::Match(s) => s.seats,
        }
    }

    pub fn seat(&self) -> Option<usize> {
        self.links().seat
    }

    pub fn connected(&mut self, peer: Peer) {
        match &mut self.stage {
            Stage::Lobby(n) => n.connected(peer),
            Stage::Match(s) => s.links.connected(peer),
        }
    }

    pub fn left(&mut self, peer: Peer) {
        match &mut self.stage {
            Stage::Lobby(n) => {
                n.left(peer);
            }
            Stage::Match(s) => s.left(peer),
        }
    }

    /// Text from a browser, at the page's clock `now_us`. Whether the match
    /// began with it (a guest hearing the host's start).
    pub fn message(&mut self, peer: Peer, text: &str, now_us: i64) -> bool {
        if self.links().others.get(&peer).is_some_and(|o| o.host || !o.said) {
            self.heard_us = Some(now_us);
        }
        let started = match &mut self.stage {
            Stage::Lobby(n) => match n.message(peer, text) {
                Some(Event::Start { seats }) => Some(seats),
                _ => None,
            },
            Stage::Match(s) => {
                s.message(peer, text);
                None
            }
        };
        match started {
            Some(seats) => {
                self.begin(seats);
                true
            }
            None => false,
        }
    }

    /// The host starts the match with the seats held now: bots take the
    /// rest. Whether it began.
    pub fn start(&mut self) -> bool {
        let Stage::Lobby(n) = &mut self.stage else {
            return false;
        };
        if !self.host || self.error.is_some() {
            return false;
        }
        let seats = n.start();
        self.begin(seats);
        true
    }

    /// From the lobby to the match, on the world every browser builds alike.
    fn begin(&mut self, seats: Seats) {
        let links = match &mut self.stage {
            Stage::Lobby(n) => std::mem::replace(n, Net::new(self.host, "")),
            Stage::Match(_) => return,
        };
        let Some(seat) = links.seat else { return };
        self.stage = Stage::Match(Box::new(Rollback::new(links, seat, seats, World::online(seats))));
    }

    /// A guest that has not heard the host for `TIMEOUT_US` has lost it.
    pub fn check_time(&mut self, now_us: i64) {
        if self.host || self.ended() {
            return;
        }
        match self.heard_us {
            None => self.heard_us = Some(now_us),
            Some(t) if now_us - t > TIMEOUT_US && self.links().host_peer().is_some() => {
                self.error = Some(TIMEOUT);
            }
            _ => {}
        }
    }

    pub fn leave(&mut self) {
        self.net_mut().leave();
    }

    pub fn take_outbox(&mut self) -> Vec<(Peer, String)> {
        self.net_mut().take_outbox()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_room_code_is_read_from_what_was_typed_or_from_a_whole_invite_link() {
        assert_eq!(read_code("kqx-rt"), Some("KQXRT".into()));
        assert_eq!(read_code(" K Q X R T "), Some("KQXRT".into()));
        assert_eq!(
            read_code("https://sgilson7.github.io/chumontillado/?join=KQXRT"),
            Some("KQXRT".into())
        );
        assert_eq!(
            read_code("https://x.test/?a=1&join=kqxrt&b=2"),
            Some("KQXRT".into())
        );
        // an O or an I, too short, too long
        assert_eq!(read_code("KQXRO"), None);
        assert_eq!(read_code("KQXR"), None);
        assert_eq!(read_code("KQXRTT"), None);
    }

    #[test]
    fn a_made_code_is_five_of_the_letters_and_differs_by_seed() {
        let a = make_code(1);
        assert_eq!(read_code(&a), Some(a.clone()));
        let codes: std::collections::BTreeSet<String> = (0..50).map(make_code).collect();
        assert!(codes.len() > 45, "{} distinct of 50", codes.len());
    }

    #[test]
    fn the_invite_link_is_the_page_with_the_code_and_reads_back_to_it() {
        let link = invite_link("https://sgilson7.github.io/chumontillado/", "KQXRT");
        assert_eq!(link, "https://sgilson7.github.io/chumontillado/?join=KQXRT");
        assert_eq!(read_code(&link), Some("KQXRT".into()));
    }

    #[test]
    fn a_guest_that_hears_nothing_from_the_host_for_ten_seconds_has_lost_it() {
        let mut host = Versus::new(true, "KQXRT", "b");
        let mut guest = Versus::new(false, "KQXRT", "b");
        host.connected(1);
        guest.connected(2);
        for (_, t) in guest.take_outbox() {
            host.message(1, &t, 0);
        }
        for (_, t) in host.take_outbox() {
            guest.message(2, &t, 0);
        }
        guest.check_time(TIMEOUT_US);
        assert_eq!(guest.error, None, "ten seconds exactly is not yet");
        guest.check_time(TIMEOUT_US + 1);
        assert_eq!(guest.status(), TIMEOUT);
        assert!(guest.ended());
    }

    #[test]
    fn the_host_starting_starts_every_guest_on_the_same_seats() {
        let mut host = Versus::new(true, "KQXRT", "b");
        let mut guest = Versus::new(false, "KQXRT", "b");
        host.connected(1);
        guest.connected(2);
        for _ in 0..3 {
            for (_, t) in guest.take_outbox() {
                host.message(1, &t, 0);
            }
            for (_, t) in host.take_outbox() {
                guest.message(2, &t, 0);
            }
        }
        assert_eq!(guest.seat(), Some(1));
        assert!(host.start());
        let began = host.take_outbox().iter().any(|(_, t)| guest.message(2, t, 0));
        assert!(began);
        assert_eq!(guest.session().map(|s| s.seats), Some(0b11));
        assert_eq!(host.session().map(|s| s.seat), Some(0));
        assert!(!guest.start(), "only the host starts");
    }
}
