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
    /// R10.12: the page's clock when this room was opened, and when each
    /// browser that has reached it but not said hello first did
    pub opened_us: Option<i64>,
    pub knocking: std::collections::BTreeMap<Peer, i64>,
    /// T24: the last match in this room: who won it and each seat's rounds
    pub last: Option<(Option<usize>, [i32; online::SEATS])>,
}

/// R10.12: what to tell the player about the links, beyond the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// a guest looking for the room, `secs` so far
    Looking { secs: i64 },
    /// a guest that has looked for `STILL_LOOKING_US` and found nobody
    StillLooking { secs: i64 },
    /// the host: someone has reached the room but cannot finish joining
    Knocking,
    /// in the match: held waiting for this seat's records
    WaitingFor(usize),
    /// in the match: this seat's player left; its bot plays it
    Dropped(usize),
}

/// A guest that has not found the host in 20 s is told so, with advice.
pub const STILL_LOOKING_US: i64 = 20_000_000;
/// The host hears of a browser stuck short of hello after 10 s.
pub const KNOCKING_US: i64 = 10_000_000;
/// The match says it waits for a seat after half a second held back.
pub const WAITING_TICKS: u32 = 30;
/// A drop is told for four seconds.
pub const DROPPED_TICKS: u32 = 240;
/// T24: a match won is shown for five seconds before the host calls
/// everyone back to the lobby.
pub const AGAIN_TICKS: i32 = 300;

impl Versus {
    pub fn new(host: bool, code: &str, build: &str, skin: u8) -> Versus {
        let mut links = Net::new(host, build);
        links.chosen = skin;
        if host {
            links.skins[0] = skin;
        }
        Versus {
            host,
            code: code.to_string(),
            stage: Stage::Lobby(links),
            error: None,
            heard_us: None,
            opened_us: None,
            knocking: std::collections::BTreeMap::new(),
            last: None,
        }
    }

    /// R10.12: what the player should be told now about the links.
    pub fn notice(&self, now_us: i64) -> Option<Notice> {
        if self.ended() {
            return None;
        }
        match &self.stage {
            Stage::Lobby(n) => {
                let opened = self.opened_us.unwrap_or(now_us);
                if !self.host && n.host_peer().is_none() {
                    let secs = (now_us - opened).max(0) / 1_000_000;
                    return Some(if now_us - opened >= STILL_LOOKING_US {
                        Notice::StillLooking { secs }
                    } else {
                        Notice::Looking { secs }
                    });
                }
                let stuck = self
                    .knocking
                    .iter()
                    .any(|(p, &t)| now_us - t >= KNOCKING_US && n.others.get(p).is_some_and(|o| !o.said));
                (self.host && stuck).then_some(Notice::Knocking)
            }
            Stage::Match(s) => {
                if let Some(d) = s.dropped_within(DROPPED_TICKS) {
                    return Some(Notice::Dropped(d));
                }
                if s.stalled >= WAITING_TICKS {
                    return s.behind().map(Notice::WaitingFor);
                }
                None
            }
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

    /// The host switches a free seat's bot on or off (in the lobby only).
    pub fn toggle_bot(&mut self, seat: usize) -> bool {
        match &mut self.stage {
            Stage::Lobby(n) => n.toggle_bot(seat),
            Stage::Match(_) => false,
        }
    }

    /// M5: this browser's player chooses a skin, in the lobby.
    pub fn choose_skin(&mut self, skin: u8) {
        if let Stage::Lobby(n) = &mut self.stage {
            n.choose_skin(skin);
        }
    }

    /// T20: the bots' skill the host will start the match with; a
    /// guest's is replaced by the host's when the match starts.
    /// T32: the host's plan for the next match it starts.
    pub fn set_plan(&mut self, plan: crate::plan::Plan) {
        if let Stage::Lobby(n) = &mut self.stage {
            *n.plan = plan;
        }
    }

    pub fn set_skill(&mut self, skill: crate::pilot::Skill) {
        if let Stage::Lobby(n) = &mut self.stage {
            n.skill = skill as u8;
        }
    }

    /// The seats left empty, as this browser knows.
    pub fn empty(&self) -> Seats {
        self.links().empty
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

    pub fn connected(&mut self, peer: Peer, now_us: i64) {
        self.knocking.entry(peer).or_insert(now_us);
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
                Some(Event::Start {
                    seats, empty, skins, ..
                }) => Some((seats, empty, skins)),
                _ => None,
            },
            Stage::Match(s) => {
                if s.message(peer, text) {
                    self.back();
                }
                None
            }
        };
        match started {
            Some((seats, empty, skins)) => {
                self.begin(seats, empty, skins);
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
        let (seats, empty, skins) = n.start();
        self.begin(seats, empty, skins);
        true
    }

    /// T24: the host, once a match won has been shown `AGAIN_TICKS`,
    /// calls everyone back to the lobby. Whether it did.
    pub fn again(&mut self) -> bool {
        let ended = self.ended();
        let Stage::Match(s) = &mut self.stage else {
            return false;
        };
        let done = s
            .world
            .rounds
            .is_some_and(|r| r.phase == crate::rounds::Phase::Done && r.ticks >= AGAIN_TICKS);
        if !self.host || !done || ended {
            return false;
        }
        s.links.again();
        self.back();
        true
    }

    /// T24: from the match back to the lobby, on the links it had; the
    /// links have been told already (`Net::again`, or the host's word).
    fn back(&mut self) {
        let Stage::Match(s) = &mut self.stage else {
            return;
        };
        let links = std::mem::replace(&mut s.links, Net::new(self.host, ""));
        self.last = s.world.rounds.map(|r| (r.winner, r.scores));
        self.stage = Stage::Lobby(links);
    }

    /// From the lobby to the match, on the world every browser builds alike.
    fn begin(&mut self, seats: Seats, empty: Seats, skins: [u8; online::SEATS]) {
        let links = match &mut self.stage {
            Stage::Lobby(n) => std::mem::replace(n, Net::new(self.host, "")),
            Stage::Match(_) => return,
        };
        let Some(seat) = links.seat else { return };
        // T20: the host's skill for the bots, as it sent it
        let mut world = World::online(seats, empty, skins);
        world.skill = crate::pilot::Skill::of(links.skill);
        // T32: the maps and weapons the host planned, round one again on
        // the first of them
        world.plan = (*links.plan).clone();
        world.start_round(0);
        self.stage = Stage::Match(Box::new(Rollback::new(links, seat, seats, world)));
    }

    /// A guest that has not heard the host for `TIMEOUT_US` has lost it.
    pub fn check_time(&mut self, now_us: i64) {
        self.opened_us.get_or_insert(now_us);
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
        let mut host = Versus::new(true, "KQXRT", "b", 0);
        let mut guest = Versus::new(false, "KQXRT", "b", 1);
        host.connected(1, 0);
        guest.connected(2, 0);
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
        let mut host = Versus::new(true, "KQXRT", "b", 0);
        let mut guest = Versus::new(false, "KQXRT", "b", 1);
        host.connected(1, 0);
        guest.connected(2, 0);
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

    #[test]
    fn the_hosts_bot_skill_reaches_the_guest_and_a_guests_own_does_not() {
        use crate::pilot::Skill;
        let mut host = Versus::new(true, "KQXRT", "b", 0);
        let mut guest = Versus::new(false, "KQXRT", "b", 1);
        host.set_skill(Skill::Hard);
        guest.set_skill(Skill::Easy);
        host.connected(1, 0);
        guest.connected(2, 0);
        for _ in 0..3 {
            for (_, t) in guest.take_outbox() {
                host.message(1, &t, 0);
            }
            for (_, t) in host.take_outbox() {
                guest.message(2, &t, 0);
            }
        }
        assert!(host.start());
        for (_, t) in host.take_outbox() {
            guest.message(2, &t, 0);
        }
        for v in [&host, &guest] {
            assert_eq!(v.session().map(|s| s.world.skill), Some(Skill::Hard));
        }
    }

    #[test]
    fn a_guest_that_finds_nobody_is_told_at_twenty_seconds_and_counts_up_before() {
        let mut guest = Versus::new(false, "KQXRT", "b", 0);
        guest.opened_us = Some(0);
        assert_eq!(guest.notice(5_000_000), Some(Notice::Looking { secs: 5 }));
        assert_eq!(
            guest.notice(STILL_LOOKING_US - 1),
            Some(Notice::Looking { secs: 19 })
        );
        assert_eq!(
            guest.notice(STILL_LOOKING_US),
            Some(Notice::StillLooking { secs: 20 })
        );
        // once the host's hello is in, nothing to say
        let mut host = Versus::new(true, "KQXRT", "b", 1);
        host.connected(1, 0);
        guest.connected(2, 0);
        for _ in 0..2 {
            for (_, t) in guest.take_outbox() {
                host.message(1, &t, 0);
            }
            for (_, t) in host.take_outbox() {
                guest.message(2, &t, 0);
            }
        }
        assert_eq!(guest.notice(STILL_LOOKING_US * 2), None);
    }

    #[test]
    fn the_host_hears_after_ten_seconds_of_someone_stuck_short_of_joining() {
        let mut host = Versus::new(true, "KQXRT", "b", 0);
        host.connected(7, 1_000_000);
        assert_eq!(host.notice(1_000_000 + KNOCKING_US - 1), None);
        assert_eq!(host.notice(1_000_000 + KNOCKING_US), Some(Notice::Knocking));
        // their hello arrives: they are in, and the note goes
        let mut guest = Versus::new(false, "KQXRT", "b", 1);
        guest.connected(2, 0);
        for (_, t) in guest.take_outbox() {
            host.message(7, &t, 0);
        }
        assert_eq!(host.notice(1_000_000 + KNOCKING_US * 2), None);
    }
}
