//! T38: the game's side of the lobby list (`browse.rs`): Find a game,
//! Quick match, and the record a public host keeps posted.
//!
//! The page's link to the relays opens when something needs it (a list on
//! screen, Quick match under way, or a public host's lobby waiting with a
//! seat free) and closes when nothing does. Its commands go through the
//! same list as the room transport's (`Game::cmds`): `lobby_open` and
//! `lobby_close`.

use crate::browse::{self, gives_way, Ad, Relays};
use crate::game::{Game, Screen};
use crate::online::{holds, SEATS};
use std::collections::BTreeSet;

/// Where Quick match has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Quick {
    /// reading the list since this page clock
    Listening { since_us: i64 },
    /// joined lobby `code` at this page clock, not yet seated
    Joining { code: String, since_us: i64 },
    /// hosting a lobby it found nobody to join, which may give way
    Hosting,
}

impl Game {
    /// Find a game: the list of open lobbies.
    pub(crate) fn find_game(&mut self) {
        self.leave_online();
        self.quick = None;
        self.open_list();
    }

    /// Quick match: join the best open lobby, or host one.
    pub(crate) fn quick_match(&mut self) {
        self.leave_online();
        self.quick = Some(Quick::Listening {
            since_us: self.now_us,
        });
        self.open_list();
    }

    fn open_list(&mut self) {
        self.listed_since_us = self.now_us;
        self.want_relays();
        let now = self.now_us;
        if let Some(r) = self.relays.as_mut() {
            r.listen(now);
        }
        self.show(Screen::Browse);
        self.ui_version += 1;
    }

    /// A lobby pressed in the list: today's Join with its code.
    pub(crate) fn join_listed(&mut self, code: &str) {
        self.leave_online();
        self.from_list = Some(code.to_string());
        self.open_room(false, code);
    }

    fn want_relays(&mut self) {
        if !self.lobby_link {
            self.lobby_link = true;
            self.command("lobby_open", false, "");
        }
    }

    fn close_relays(&mut self) {
        if self.lobby_link {
            if let Some(mut r) = self.relays.take() {
                r.unlisten();
                // the last record (a closed one) goes before the link closes
                self.lobby_left.extend(r.take_outbox());
            }
            self.lobby_link = false;
            self.command("lobby_close", false, "");
        }
    }

    /// The page has opened its link: `bytes` it drew for the key (numbers
    /// 0 to 255, comma-separated), its wall clock in milliseconds, and its
    /// clock's minutes behind UTC.
    pub fn lobby_ready(&mut self, bytes: &str, wall_ms: &str, behind: &str) {
        if !self.lobby_link {
            return;
        }
        if self.lobby_seed.is_empty() {
            self.lobby_seed = bytes
                .split(',')
                .filter_map(|b| b.trim().parse::<u8>().ok())
                .collect();
            // never an empty key, even from a page that drew nothing
            self.lobby_seed.extend(self.seed.to_le_bytes());
        }
        let wall: i64 = wall_ms.trim().parse().unwrap_or(0);
        let behind: i32 = behind.trim().parse().unwrap_or(0);
        let mut r = Relays::new(&self.lobby_seed, wall, behind, self.now_us);
        if self.screen == Screen::Browse || self.quick.is_some() {
            r.listen(self.now_us);
        }
        self.relays = Some(r);
        self.ui_version += 1;
    }

    /// Relay `k` (its place in the config's list) has opened.
    pub fn lobby_opened(&mut self, k: i32) {
        let now = self.now_us;
        if let Some(r) = self.relays.as_mut() {
            r.opened(k, now);
            self.ui_version += 1;
        }
    }

    /// Text from a relay.
    pub fn lobby_message(&mut self, text: &str) {
        let (now, build) = (self.now_us, self.build.clone());
        if let Some(r) = self.relays.as_mut() {
            if r.received(text, now, &build) {
                self.ui_version += 1;
            }
        }
    }

    /// What the link should send, as JSON `[[relay, text], ...]`; relay -1
    /// is every relay that is open.
    pub fn lobby_outbox(&mut self) -> String {
        let mut out = std::mem::take(&mut self.lobby_left);
        if let Some(r) = self.relays.as_mut() {
            out.extend(r.take_outbox());
        }
        serde_json::to_string(&out).unwrap_or_else(|_| "[]".into())
    }

    /// The record this browser should have out: a public host's lobby,
    /// waiting, with a seat free.
    fn lobby_ad(&self, region: u8) -> Option<(String, Ad)> {
        let v = self.versus.as_ref()?;
        if !v.host || !self.public || v.ended() || v.session().is_some() {
            return None;
        }
        let seats = v.seats();
        let people = (0..SEATS).filter(|&k| holds(seats, k)).count() as u8;
        if people as usize >= SEATS {
            return None;
        }
        let empty = v.empty();
        let bots = (0..SEATS)
            .filter(|&k| !holds(seats, k) && !holds(empty, k))
            .count() as u8;
        Some((
            v.code.clone(),
            Ad {
                protocol: crate::online::PROTOCOL,
                build: self.build.clone(),
                open: true,
                people,
                bots,
                maps: if self.playlist == 0 {
                    Vec::new()
                } else {
                    self.playlist_maps(self.playlist)
                },
                region,
                quick: self.quick == Some(Quick::Hosting),
            },
        ))
    }

    /// Each frame: post or renew the record, take Quick match a step, and
    /// close the link when nothing needs it.
    pub(crate) fn follow_lobby(&mut self) {
        let now = self.now_us;
        let region = self.relays.as_ref().map(|r| r.region).unwrap_or(0);
        let want = self.lobby_ad(region);
        if want.is_some() {
            self.want_relays();
        }
        if let Some(r) = self.relays.as_mut() {
            match &want {
                Some((code, ad)) => {
                    r.post(code, ad, now);
                }
                None => {
                    if let Some(mut mine) = r.mine().filter(|m| m.ad.open) {
                        mine.ad.open = false;
                        r.post(&mine.code, &mine.ad, now);
                    }
                }
            }
        }
        self.follow_quick();
        let needed = want.is_some() || self.screen == Screen::Browse || self.quick.is_some();
        if !needed {
            self.close_relays();
        }
        // the list's clock, a second at a time
        if self.screen == Screen::Browse {
            let secs = (now - self.listed_since_us) / 1_000_000;
            if secs != self.listed_secs_seen {
                self.listed_secs_seen = secs;
                self.ui_version += 1;
            }
        }
    }

    fn follow_quick(&mut self) {
        let now = self.now_us;
        match self.quick.clone() {
            Some(Quick::Listening { since_us }) => {
                let heard = self.relays.as_ref().is_some_and(|r| r.opened > 0);
                let waited = now - since_us;
                if !(heard && waited >= browse::QUICK_LISTEN_US || waited >= browse::NO_RELAY_US) {
                    return;
                }
                let best = self
                    .relays
                    .as_ref()
                    .and_then(|r| r.list(now, &self.dead).first().map(|l| l.code.clone()));
                match best {
                    Some(code) => {
                        self.join_listed(&code);
                        self.quick = Some(Quick::Joining { code, since_us: now });
                    }
                    None => {
                        self.act("online.host", "").ok();
                        self.quick = Some(Quick::Hosting);
                    }
                }
            }
            Some(Quick::Joining { code, since_us }) => {
                let seated = self.versus.as_ref().is_some_and(|v| v.seat().is_some());
                if seated {
                    self.quick = None;
                } else if now - since_us >= browse::QUICK_JOIN_US {
                    self.dead.insert(code);
                    self.from_list = None;
                    self.leave_online();
                    self.quick = Some(Quick::Listening {
                        since_us: now - browse::QUICK_LISTEN_US,
                    });
                    self.open_list();
                }
            }
            Some(Quick::Hosting) => {
                let Some(r) = self.relays.as_ref() else { return };
                let Some(mine) = r.mine().filter(|m| m.ad.open) else {
                    return;
                };
                let other = r
                    .list(now, &self.dead)
                    .into_iter()
                    .find(|o| gives_way(&mine, o))
                    .map(|o| o.code.clone());
                if let Some(code) = other {
                    self.join_listed(&code);
                    self.quick = Some(Quick::Joining { code, since_us: now });
                    // this browser's own record is closed on the next frame
                }
            }
            None => {}
        }
    }

    /// Leaving a room joined from the list before its host answered: the
    /// lobby is left off the list for the rest of this visit.
    pub(crate) fn forget_dead_lobby(&mut self) {
        let unanswered = self
            .versus
            .as_ref()
            .is_some_and(|v| !v.host && v.links().host_peer().is_none());
        if let Some(code) = self.from_list.take() {
            if unanswered {
                self.dead.insert(code);
            }
        }
    }

    /// The lobbies to show now.
    pub fn listed(&self) -> Vec<&browse::Listed> {
        match &self.relays {
            Some(r) => r.list(self.now_us, &self.dead),
            None => Vec::new(),
        }
    }
}

/// Lobbies left off the list this visit, by code.
pub type Dead = BTreeSet<String>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::tests::game;
    use crate::nostr::Event;
    use std::collections::BTreeMap;

    /// The wall clock every page reads, at page clock 0.
    const WALL_MS: i64 = 1_800_000_000_000;
    /// A frame of 100 ms: the lobby list moves in seconds.
    const FRAME_US: i64 = 100_000;

    /// One relay that keeps each host's latest record per room, as an
    /// addressable kind is kept, and passes records to every page that has
    /// asked for the list.
    #[derive(Default)]
    struct Relay {
        kept: BTreeMap<(String, String), Event>,
        asking: Vec<bool>,
    }

    fn frame(e: &Event) -> String {
        format!(
            "[\"EVENT\",\"{}\",{}]",
            browse::SUB,
            serde_json::to_string(e).unwrap()
        )
    }

    impl Relay {
        /// Each page in turn: open its link if it asked, then carry what it
        /// sends.
        fn carry(&mut self, pages: &mut [&mut Game]) {
            self.asking.resize(pages.len(), false);
            for i in 0..pages.len() {
                if pages[i].lobby_link && pages[i].relays.is_none() {
                    let bytes = format!("{i},{i},{i}");
                    let wall = (WALL_MS + pages[i].now_us / 1000).to_string();
                    pages[i].lobby_ready(&bytes, &wall, "0");
                    pages[i].lobby_opened(0);
                }
                if !pages[i].lobby_link {
                    self.asking[i] = false;
                }
                let out: Vec<(i32, String)> = serde_json::from_str(&pages[i].lobby_outbox()).unwrap();
                for (_, text) in out {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                    match v[0].as_str().unwrap() {
                        "REQ" => {
                            self.asking[i] = true;
                            for e in self.kept.values() {
                                pages[i].lobby_message(&frame(e));
                            }
                        }
                        "CLOSE" => self.asking[i] = false,
                        "EVENT" => {
                            let e: Event = serde_json::from_value(v[1].clone()).unwrap();
                            let at = (e.pubkey.clone(), e.tag("d").unwrap().to_string());
                            self.kept.insert(at, e.clone());
                            for (p, asking) in pages.iter_mut().zip(&self.asking) {
                                if *asking {
                                    p.lobby_message(&frame(&e));
                                }
                            }
                        }
                        other => panic!("a frame the relay does not know: {other}"),
                    }
                }
            }
        }

        fn run(&mut self, pages: &mut [&mut Game], from_us: i64, for_us: i64) -> i64 {
            let mut now = from_us;
            while now < from_us + for_us {
                now += FRAME_US;
                for p in pages.iter_mut() {
                    p.advance(now, 0, 0, 0, 0);
                }
                self.carry(pages);
            }
            now
        }
    }

    /// Two pages' room transports linked: `h` hosts, `g` joins.
    fn link(h: &mut Game, g: &mut Game) {
        h.online_connected(2);
        g.online_connected(1);
        for _ in 0..4 {
            let to_g: Vec<(u32, String)> = serde_json::from_str(&h.online_outbox()).unwrap();
            for (_, t) in to_g {
                g.online_message(1, &t);
            }
            let to_h: Vec<(u32, String)> = serde_json::from_str(&g.online_outbox()).unwrap();
            for (_, t) in to_h {
                h.online_message(2, &t);
            }
        }
    }

    fn pages() -> (Game, Game) {
        let (h, mut g) = (game(), game());
        // two pages draw different seeds, so different room codes
        g.seed = 7;
        (h, g)
    }

    fn code(p: &Game) -> String {
        p.versus.as_ref().unwrap().code.clone()
    }

    #[test]
    fn a_public_hosts_lobby_is_listed_and_a_searcher_joins_it_by_its_code() {
        let (mut h, mut s) = pages();
        let mut relay = Relay::default();
        h.act("online.host", "").unwrap();
        let now = relay.run(&mut [&mut h, &mut s], 0, 5 * FRAME_US);
        s.act("browse.open", "").unwrap();
        let now = relay.run(&mut [&mut h, &mut s], now, 5 * FRAME_US);
        assert_eq!(s.screen, Screen::Browse);
        let ui = crate::api::ui(&s);
        let joins: Vec<&str> = ui.panels[0]
            .actions
            .iter()
            .filter(|a| a.name == "browse.join")
            .map(|a| a.value.as_str())
            .collect();
        assert_eq!(joins, [code(&h).as_str()], "the list shows the host's lobby");
        s.act("browse.join", &code(&h)).unwrap();
        assert_eq!(code(&s), code(&h));
        assert!(!s.versus.as_ref().unwrap().host);
        // once the host starts, its lobby leaves everyone's list
        let mut t = game();
        t.seed = 11;
        t.act("browse.open", "").unwrap();
        let now = relay.run(&mut [&mut h, &mut s, &mut t], now, 5 * FRAME_US);
        assert_eq!(t.listed().len(), 1);
        h.act("online.start", "").unwrap();
        relay.run(&mut [&mut h, &mut s, &mut t], now, 5 * FRAME_US);
        assert!(t.listed().is_empty(), "a started match is not listed");
        assert!(!h.lobby_link, "and the host's link to the relays has closed");
    }

    #[test]
    fn a_host_set_to_friends_only_never_reaches_the_relays() {
        let mut h = game();
        h.act("online.public", "-1").unwrap();
        h.act("online.host", "").unwrap();
        let mut relay = Relay::default();
        relay.run(&mut [&mut h], 0, 10 * FRAME_US);
        assert!(h.cmds.iter().all(|c| !c.act.starts_with("lobby")));
        assert!(relay.kept.is_empty());
    }

    #[test]
    fn quick_match_joins_an_open_lobby_rather_than_hosting_one() {
        let (mut h, mut q) = pages();
        let mut relay = Relay::default();
        h.act("online.host", "").unwrap();
        let now = relay.run(&mut [&mut h, &mut q], 0, 5 * FRAME_US);
        q.act("browse.quick", "").unwrap();
        let now = relay.run(&mut [&mut h, &mut q], now, browse::QUICK_LISTEN_US + 2 * FRAME_US);
        assert_eq!(code(&q), code(&h));
        assert!(!q.versus.as_ref().unwrap().host);
        assert!(
            q.cmds.iter().all(|c| !(c.act == "open" && c.host)),
            "it joined from the list without ever hosting"
        );
        link(&mut h, &mut q);
        relay.run(&mut [&mut h, &mut q], now, 2 * FRAME_US);
        assert_eq!(q.quick, None, "seated, Quick match is done");
        assert_eq!(q.versus.as_ref().unwrap().seat(), Some(1));
    }

    #[test]
    fn two_players_pressing_quick_match_at_once_end_in_one_lobby() {
        for (a_seed, b_seed) in [(1, 7), (7, 1), (3, 4)] {
            let (mut a, mut b) = (game(), game());
            a.seed = a_seed;
            b.seed = b_seed;
            let mut relay = Relay::default();
            a.act("browse.quick", "").unwrap();
            b.act("browse.quick", "").unwrap();
            let now = relay.run(&mut [&mut a, &mut b], 0, browse::QUICK_LISTEN_US + 2 * FRAME_US);
            // the list was empty, so both host, and each sees the other
            let now = relay.run(&mut [&mut a, &mut b], now, 3 * FRAME_US);
            let hosts = [&a, &b]
                .iter()
                .filter(|p| p.versus.as_ref().unwrap().host)
                .count();
            assert_eq!(hosts, 1, "exactly one gives way ({a_seed}, {b_seed})");
            assert_eq!(code(&a), code(&b), "and joins the other's room");
            let (h, g) = if a.versus.as_ref().unwrap().host {
                (&mut a, &mut b)
            } else {
                (&mut b, &mut a)
            };
            link(h, g);
            relay.run(&mut [h, g], now, 3 * FRAME_US);
            assert_eq!(g.versus.as_ref().unwrap().seat(), Some(1));
            assert_eq!(g.quick, None);
        }
    }

    #[test]
    fn a_listed_lobby_whose_host_never_answers_leaves_the_list() {
        let (mut h, mut s) = pages();
        let mut relay = Relay::default();
        h.act("online.host", "").unwrap();
        let now = relay.run(&mut [&mut h], 0, 5 * FRAME_US);
        // the host's tab is gone; its record is still on the relay
        s.act("browse.open", "").unwrap();
        let now = relay.run(&mut [&mut s], now, 3 * FRAME_US);
        assert_eq!(s.listed().len(), 1);
        s.act("browse.join", &code(&h)).unwrap();
        let now = relay.run(&mut [&mut s], now, 30 * FRAME_US);
        s.act("screen.menu", "").unwrap();
        s.act("browse.open", "").unwrap();
        relay.run(&mut [&mut s], now, 3 * FRAME_US);
        assert!(s.listed().is_empty(), "the lobby that never answered is left off");
    }

    #[test]
    fn quick_match_drops_a_lobby_that_never_seats_it_and_hosts_instead() {
        let (mut h, mut q) = pages();
        let mut relay = Relay::default();
        h.act("online.host", "").unwrap();
        let now = relay.run(&mut [&mut h], 0, 5 * FRAME_US);
        q.act("browse.quick", "").unwrap();
        let now = relay.run(&mut [&mut q], now, browse::QUICK_LISTEN_US + 2 * FRAME_US);
        assert_eq!(code(&q), code(&h), "it tries the listed lobby");
        relay.run(&mut [&mut q], now, browse::QUICK_JOIN_US + 2 * FRAME_US);
        assert!(q.dead.contains(&code(&h)));
        assert!(q.versus.as_ref().unwrap().host, "and then hosts its own");
        assert_eq!(q.quick, Some(Quick::Hosting));
    }
}
