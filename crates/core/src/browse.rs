//! T38: the lobby list. A host whose room is public posts a lobby record
//! to the online kit's relays and renews it; a player who presses Find a
//! game or Quick match reads every record there is, and the core keeps
//! the ones that pass its checks. Joining one is the room-code path that
//! already runs (`versus.rs`): a record only introduces.
//!
//! The design and its sources are in the research analysis "Finding a
//! game without a server" (journal, T38). Its one idea from Countless
//! Days' ledger: a rule that every browser applies to the same records
//! reaches the same answer without asking anyone. Quick match leans on it
//! (`gives_way`).
//!
//! Nothing here reaches a relay. `Relays` leaves frames in an outbox for
//! the page's `web/net/lobby.js` to send, and takes what comes back as
//! text, so `cargo test` plays hosts and searchers against a simulated
//! relay.

use crate::nostr::{self, Event, Key};
use crate::online::{room_name, APP_ID, PROTOCOL, SEATS};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// NIP-78's kind for an app's own data: addressable, so a relay keeps only
/// a host's latest record for each room.
pub const KIND: u32 = 30078;
/// Leading zero bits of work on each record (NIP-13): a few thousand
/// hashes, a few milliseconds, and a cost to flooding the list.
pub const WORK_BITS: u32 = 12;
/// A host renews its record this often, and at once when it changes.
pub const RENEW_US: i64 = 30_000_000;
/// A record expires this many seconds after it is made, so a closed tab's
/// lobby leaves the list on its own.
pub const LIFETIME_S: i64 = 90;
/// A record made further than this from this browser's clock, either way,
/// is turned away: clocks differ, but not by minutes.
pub const SKEW_S: i64 = 120;
/// Quick match listens this long before it joins or hosts.
pub const QUICK_LISTEN_US: i64 = 3_000_000;
/// A lobby Quick match joined that has not seated it by now is dropped,
/// and Quick match looks again.
pub const QUICK_JOIN_US: i64 = 12_000_000;
/// With no relay open by now, the list says so.
pub const NO_RELAY_US: i64 = 15_000_000;
/// The subscription's name on every relay.
pub const SUB: &str = "lobbies";

/// The tag every record carries and every list asks for: the app and the
/// protocol, so a build that cannot play this one never lists its lobbies.
pub fn tag() -> String {
    format!("{APP_ID}-p{PROTOCOL}")
}

/// The region band a clock offset falls in, from the page's
/// `getTimezoneOffset()` (minutes behind UTC): 0 the Americas, 1 Europe
/// and Africa, 2 Asia and the Pacific. It stands in for a latency
/// estimate (Agarwal and Lorch 2009) without measuring anything.
pub fn region(minutes_behind_utc: i32) -> u8 {
    let east = -minutes_behind_utc;
    if east <= -180 {
        0
    } else if east <= 240 {
        1
    } else {
        2
    }
}

pub const REGIONS: u8 = 3;

/// What a lobby record says. No typed text: every field is a number or a
/// flag the core can check, so a stranger's record shows nothing a
/// stranger wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ad {
    pub protocol: u32,
    pub build: String,
    /// false: the lobby has filled, started or closed
    pub open: bool,
    /// seats people hold, the host's among them
    pub people: u8,
    /// seats bots will play
    pub bots: u8,
    /// the host's planned maps by number; none for every map in turn
    pub maps: Vec<u8>,
    pub region: u8,
    /// hosted by Quick match, so it may give way (`gives_way`)
    pub quick: bool,
}

/// A record that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub code: String,
    pub host: String,
    pub made: i64,
    pub id: String,
    pub ad: Ad,
}

impl Listed {
    pub fn free(&self) -> bool {
        self.ad.open && (self.ad.people as usize) < SEATS
    }
}

/// Why a record was turned away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    Kind,
    Tag,
    Room,
    Signature,
    Work,
    Expired,
    Clock,
    Content,
    Build,
}

/// Checks a record against this browser's clock (`now_s`, seconds since
/// 1970) and build.
pub fn accept(e: &Event, now_s: i64, build: &str) -> Result<Listed, Refused> {
    if e.kind != KIND {
        return Err(Refused::Kind);
    }
    if e.tag("t") != Some(tag().as_str()) {
        return Err(Refused::Tag);
    }
    let prefix = room_name("");
    let code = e
        .tag("d")
        .and_then(|d| d.strip_prefix(prefix.as_str()))
        .and_then(crate::versus::read_code)
        .ok_or(Refused::Room)?;
    if e.tag("d") != Some(room_name(&code).as_str()) {
        return Err(Refused::Room);
    }
    let until: i64 = e
        .tag("expiration")
        .and_then(|x| x.parse().ok())
        .ok_or(Refused::Expired)?;
    if until <= now_s {
        return Err(Refused::Expired);
    }
    if (e.created_at - now_s).abs() > SKEW_S {
        return Err(Refused::Clock);
    }
    // the work the record commits to, and the work it has
    let committed: u32 = e
        .tags
        .iter()
        .find(|t| t.first().map(String::as_str) == Some("nonce"))
        .and_then(|t| t.get(2))
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let done = nostr::unhex(&e.id).map(|id| nostr::work(&id)).unwrap_or(0);
    if committed < WORK_BITS || done < WORK_BITS {
        return Err(Refused::Work);
    }
    if !nostr::check(e) {
        return Err(Refused::Signature);
    }
    let ad: Ad = serde_json::from_str(&e.content).map_err(|_| Refused::Content)?;
    let sane = (ad.people as usize) <= SEATS
        && (ad.people + ad.bots) as usize <= SEATS
        && ad.people >= 1
        && ad.region < REGIONS
        && ad.maps.len() <= 64
        && ad.maps.iter().all(|m| (*m as usize) < crate::maps::COUNT);
    if !sane {
        return Err(Refused::Content);
    }
    if ad.protocol != PROTOCOL || ad.build != build {
        return Err(Refused::Build);
    }
    Ok(Listed {
        code,
        host: e.pubkey.clone(),
        made: e.created_at,
        id: e.id.clone(),
        ad,
    })
}

/// Whether `a` is the newer record of the same host's (a later time; at
/// the same second the smaller id), so every browser keeps the same one.
fn newer(a: &Listed, b: &Listed) -> bool {
    (a.made, std::cmp::Reverse(&a.id)) > (b.made, std::cmp::Reverse(&b.id))
}

/// Quick match's rule, the ledger's any-order merge put to pairing: a
/// lobby Quick match hosted, with nobody in it yet, gives way to any other
/// open lobby hosted on purpose, and to a quick one whose host's key is
/// smaller. Two lone quick hosts that see each other's records both apply
/// it, and exactly one gives way. A lobby hosted on purpose never does.
pub fn gives_way(mine: &Listed, other: &Listed) -> bool {
    mine.ad.quick
        && mine.ad.people == 1
        && other.host != mine.host
        && other.free()
        && (!other.ad.quick || other.host < mine.host)
}

/// The list's order: this browser's region first, then fuller lobbies
/// (so matches fill), then the older, then by host so the order is fixed.
pub fn order(a: &Listed, b: &Listed, region: u8) -> std::cmp::Ordering {
    let key = |l: &Listed| {
        (
            l.ad.region != region,
            std::cmp::Reverse(l.ad.people),
            l.made,
            l.host.clone(),
        )
    };
    key(a).cmp(&key(b))
}

/// The link to the relays, as the core keeps it: the key, the clock, the
/// records heard, and the record this browser has posted.
#[derive(Debug, Clone)]
pub struct Relays {
    key: Key,
    /// microseconds to add to the page's clock for the wall clock
    wall_from_page_us: i64,
    pub region: u8,
    /// the best record heard from each host
    heard: BTreeMap<String, Listed>,
    outbox: Vec<(i32, String)>,
    /// whether the list is wanted (Find a game, Quick match)
    pub listening: bool,
    /// the record posted, the page clock it was posted at, and what it said
    posted: Option<(Event, i64, String, Ad)>,
    /// relays that have opened
    pub opened: u32,
    pub since_us: i64,
}

impl Relays {
    /// `bytes`: what the page drew for the key; `wall_ms`: its wall clock;
    /// `behind`: its clock's minutes behind UTC; `now_us`: the page clock.
    pub fn new(bytes: &[u8], wall_ms: i64, behind: i32, now_us: i64) -> Relays {
        Relays {
            key: Key::from_bytes(bytes),
            wall_from_page_us: wall_ms * 1000 - now_us,
            region: region(behind),
            heard: BTreeMap::new(),
            outbox: Vec::new(),
            listening: false,
            posted: None,
            opened: 0,
            since_us: now_us,
        }
    }

    pub fn me(&self) -> String {
        self.key.public()
    }

    /// Seconds since 1970 by this browser's clock.
    pub fn wall_s(&self, now_us: i64) -> i64 {
        (now_us + self.wall_from_page_us).div_euclid(1_000_000)
    }

    fn filter(&self, now_us: i64) -> serde_json::Value {
        json!({"kinds": [KIND], "#t": [tag()], "since": self.wall_s(now_us) - LIFETIME_S})
    }

    /// Start asking every relay for records (Find a game, Quick match).
    pub fn listen(&mut self, now_us: i64) {
        if !self.listening {
            self.listening = true;
            let f = nostr::subscribe(SUB, self.filter(now_us));
            self.outbox.push((-1, f));
        }
    }

    pub fn unlisten(&mut self) {
        if self.listening {
            self.listening = false;
            self.outbox.push((-1, nostr::unsubscribe(SUB)));
            self.heard.clear();
        }
    }

    /// Relay `k` has opened: it hears the subscription and the record
    /// posted, if any, which went to the relays open before it.
    pub fn opened(&mut self, k: i32, now_us: i64) {
        self.opened += 1;
        if self.listening {
            self.outbox.push((k, nostr::subscribe(SUB, self.filter(now_us))));
        }
        if let Some((e, ..)) = &self.posted {
            self.outbox.push((k, nostr::publish(e)));
        }
    }

    /// Posts a record for room `code` if it says something new or is due
    /// to be renewed. Returns whether it posted.
    pub fn post(&mut self, code: &str, ad: &Ad, now_us: i64) -> bool {
        let due = match &self.posted {
            Some((_, at, c, said)) => c != code || said != ad || now_us - at >= RENEW_US,
            None => true,
        };
        if !due {
            return false;
        }
        let made = self.wall_s(now_us);
        let tags = vec![
            vec!["d".into(), room_name(code)],
            vec!["t".into(), tag()],
            vec!["expiration".into(), (made + LIFETIME_S).to_string()],
        ];
        let content = serde_json::to_string(ad).unwrap_or_default();
        let e = nostr::sign(&self.key, made, KIND, tags, &content, WORK_BITS);
        self.outbox.push((-1, nostr::publish(&e)));
        self.posted = Some((e, now_us, code.to_string(), ad.clone()));
        true
    }

    /// Whether this browser has a record out that says its lobby is open.
    pub fn posted_open(&self) -> bool {
        self.posted.as_ref().is_some_and(|(.., ad)| ad.open)
    }

    /// The record posted last, as `accept` would list it.
    pub fn mine(&self) -> Option<Listed> {
        let (e, _, code, ad) = self.posted.as_ref()?;
        Some(Listed {
            code: code.clone(),
            host: e.pubkey.clone(),
            made: e.created_at,
            id: e.id.clone(),
            ad: ad.clone(),
        })
    }

    /// Text from a relay. Returns whether the list changed.
    pub fn received(&mut self, text: &str, now_us: i64, build: &str) -> bool {
        let Some((sub, e)) = nostr::received(text) else {
            return false;
        };
        if sub != SUB || !self.listening || e.pubkey == self.me() {
            return false;
        }
        let Ok(l) = accept(&e, self.wall_s(now_us), build) else {
            return false;
        };
        match self.heard.get(&l.host) {
            Some(old) if !newer(&l, old) => false,
            _ => {
                self.heard.insert(l.host.clone(), l);
                true
            }
        }
    }

    /// The lobbies to show: open, with a seat free, not expired by now
    /// and not in `dead`, in the list's order.
    pub fn list(&self, now_us: i64, dead: &std::collections::BTreeSet<String>) -> Vec<&Listed> {
        let now_s = self.wall_s(now_us);
        let mut out: Vec<&Listed> = self
            .heard
            .values()
            .filter(|l| l.free() && l.made + LIFETIME_S > now_s && !dead.contains(&l.code))
            .collect();
        out.sort_by(|a, b| order(a, b, self.region));
        out
    }

    pub fn take_outbox(&mut self) -> Vec<(i32, String)> {
        std::mem::take(&mut self.outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD: &str = "b1";
    const NOW_S: i64 = 1_800_000_000;

    fn ad(people: u8, quick: bool) -> Ad {
        Ad {
            protocol: PROTOCOL,
            build: BUILD.into(),
            open: true,
            people,
            bots: SEATS as u8 - people,
            maps: vec![],
            region: 1,
            quick,
        }
    }

    fn record(key: u8, code: &str, made: i64, a: &Ad) -> Event {
        let mut r = Relays::new(&[key; 32], made * 1000, 0, 0);
        r.post(code, a, 0);
        let (e, ..) = r.posted.clone().unwrap();
        e
    }

    fn listed(key: u8, a: &Ad) -> Listed {
        accept(&record(key, "KQXRT", NOW_S, a), NOW_S, BUILD).unwrap()
    }

    #[test]
    fn a_hosts_record_passes_every_check() {
        let l = listed(1, &ad(2, false));
        assert_eq!(l.code, "KQXRT");
        assert_eq!(l.ad.people, 2);
    }

    #[test]
    fn each_kind_of_bad_record_is_turned_away() {
        let good = record(1, "KQXRT", NOW_S, &ad(1, false));
        let resign = |mut e: Event, work: u32| {
            let key = Key::from_bytes(&[1; 32]);
            e.tags.retain(|t| t[0] != "nonce");
            nostr::sign(&key, e.created_at, e.kind, e.tags, &e.content, work)
        };
        let try_ = |e: &Event| accept(e, NOW_S, BUILD).err();
        assert_eq!(try_(&good), None);
        let mut e = good.clone();
        e.kind = 1;
        assert_eq!(try_(&resign(e, WORK_BITS)), Some(Refused::Kind));
        let mut e = good.clone();
        e.tags[1][1] = "chumontillado-p1".into();
        assert_eq!(try_(&resign(e, WORK_BITS)), Some(Refused::Tag));
        let mut e = good.clone();
        e.tags[0][1] = "chumontillado-p2-KQXR0".into();
        assert_eq!(try_(&resign(e, WORK_BITS)), Some(Refused::Room));
        assert_eq!(
            accept(&good, NOW_S + LIFETIME_S, BUILD).err(),
            Some(Refused::Expired)
        );
        assert_eq!(
            accept(&good, NOW_S - SKEW_S - 1, BUILD).err(),
            Some(Refused::Clock)
        );
        assert_eq!(try_(&resign(good.clone(), 0)), Some(Refused::Work));
        let mut e = good.clone();
        e.content = e.content.replace("\"people\":1", "\"people\":3");
        assert_eq!(try_(&e), Some(Refused::Signature));
        let mut e = good.clone();
        e.content = e.content.replace("\"people\":1", "\"people\":9");
        assert_eq!(try_(&resign(e, WORK_BITS)), Some(Refused::Content));
        let mut e = good.clone();
        e.content = e
            .content
            .replace("\"quick\":false", "\"quick\":false,\"name\":\"x\"");
        assert_eq!(try_(&resign(e, WORK_BITS)), Some(Refused::Content));
        assert_eq!(accept(&good, NOW_S, "b2").err(), Some(Refused::Build));
    }

    #[test]
    fn a_hosts_newer_record_replaces_its_older_and_a_closed_one_leaves_the_list() {
        let mut r = Relays::new(&[9; 32], NOW_S * 1000, 0, 0);
        r.listen(0);
        let frame = |e: &Event| format!("[\"EVENT\",\"{SUB}\",{}]", serde_json::to_string(e).unwrap());
        let none = Default::default();
        assert!(r.received(&frame(&record(1, "KQXRT", NOW_S - 5, &ad(1, false))), 0, BUILD));
        assert_eq!(r.list(0, &none)[0].ad.people, 1);
        assert!(r.received(&frame(&record(1, "KQXRT", NOW_S, &ad(2, false))), 0, BUILD));
        assert!(!r.received(&frame(&record(1, "KQXRT", NOW_S - 9, &ad(3, false))), 0, BUILD));
        assert_eq!(r.list(0, &none)[0].ad.people, 2);
        let mut shut = ad(2, false);
        shut.open = false;
        assert!(r.received(&frame(&record(1, "KQXRT", NOW_S + 1, &shut)), 0, BUILD));
        assert!(r.list(0, &none).is_empty());
        // and a lobby that never answered a join is left out
        assert!(r.received(&frame(&record(2, "QXRTK", NOW_S, &ad(1, false))), 0, BUILD));
        let dead = ["QXRTK".to_string()].into_iter().collect();
        assert!(r.list(0, &dead).is_empty());
        // a record outlives its lifetime on the list's clock too
        assert!(r.list((LIFETIME_S + 1) * 1_000_000, &none).is_empty());
    }

    #[test]
    fn the_list_puts_this_region_first_then_fuller_lobbies() {
        let mut far = ad(3, false);
        far.region = 2;
        let near_one = listed(1, &ad(1, false));
        let near_two = listed(2, &ad(2, false));
        let far_three = listed(3, &far);
        let mut v = [&far_three, &near_one, &near_two];
        v.sort_by(|a, b| order(a, b, 1));
        assert_eq!(v.iter().map(|l| l.ad.people).collect::<Vec<_>>(), vec![2, 1, 3]);
    }

    #[test]
    fn of_two_lone_quick_hosts_exactly_one_gives_way() {
        let a = listed(1, &ad(1, true));
        let b = listed(2, &ad(1, true));
        assert!(gives_way(&a, &b) != gives_way(&b, &a));
        // a lobby hosted on purpose never gives way, and a quick one does
        let kept = listed(3, &ad(1, false));
        assert!(!gives_way(&kept, &a) && gives_way(&a, &kept) && gives_way(&b, &kept));
        let also_kept = listed(5, &ad(1, false));
        assert!(!gives_way(&kept, &also_kept) && !gives_way(&also_kept, &kept));
        // nor does a quick host someone has joined
        let joined = listed(4, &ad(2, true));
        assert!(!gives_way(&joined, &a) && !gives_way(&joined, &b));
    }

    #[test]
    fn region_bands_follow_the_clock_offset() {
        // getTimezoneOffset: New York 300, London 0, Lagos -60, Tokyo -540
        assert_eq!(region(300), 0);
        assert_eq!(region(180), 0);
        assert_eq!(region(0), 1);
        assert_eq!(region(-60), 1);
        assert_eq!(region(-240), 1);
        assert_eq!(region(-540), 2);
        assert_eq!(region(-330), 2);
    }

    #[test]
    fn a_relay_that_opens_late_hears_the_list_request_and_the_record() {
        let mut r = Relays::new(&[1; 32], NOW_S * 1000, 0, 0);
        r.listen(0);
        assert!(r.post("KQXRT", &ad(1, false), 0));
        r.take_outbox();
        r.opened(3, 0);
        let out = r.take_outbox();
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(k, _)| *k == 3));
        assert!(out[0].1.starts_with("[\"REQ\"") && out[1].1.starts_with("[\"EVENT\""));
    }

    #[test]
    fn a_record_is_renewed_when_due_or_changed_and_not_otherwise() {
        let mut r = Relays::new(&[1; 32], NOW_S * 1000, 0, 0);
        assert!(r.post("KQXRT", &ad(1, false), 0));
        assert!(!r.post("KQXRT", &ad(1, false), RENEW_US - 1));
        assert!(r.post("KQXRT", &ad(2, false), RENEW_US - 1));
        assert!(r.post("KQXRT", &ad(2, false), 2 * RENEW_US));
    }
}
