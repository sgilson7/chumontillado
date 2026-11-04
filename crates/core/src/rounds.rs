//! A match's rounds (M5): one life each (CS-Q3: A), the last one standing
//! takes the round, the first to 5 wins (CS-Q4: A), each round on the next
//! map (CS-Q11: A), and from 75 s every seat sees where the others are
//! through walls (CS-Q18: C). Part of the world, so every browser runs the
//! same rounds from the same records; `world.rs` steps them.

/// A round's three seconds' count before it goes.
pub const COUNT_TICKS: i32 = 180;
/// The four seconds after a round, showing who took it.
pub const OVER_TICKS: i32 = 240;
/// From 75 s into a round, positions show through walls (CS-Q18: C).
pub const XRAY_TICKS: i32 = 75 * 60;
/// A round still undecided at two and a half minutes ends with no one
/// taking it (R10.4).
pub const LIMIT_TICKS: i32 = 150 * 60;
/// Rounds to win the match.
pub const TO_WIN: i32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// the count: nobody moves or fires
    Count,
    Live,
    /// a round is over; `Rounds::winner` took it (or nobody)
    Over,
    /// the match is over; `Rounds::winner` won it
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rounds {
    /// the map being played, by its place in `maps::all`
    pub map: usize,
    /// the round, from 1
    pub round: i32,
    pub scores: [i32; 4],
    pub phase: Phase,
    /// ticks in this phase
    pub ticks: i32,
    pub winner: Option<usize>,
}

impl Rounds {
    pub fn new() -> Rounds {
        Rounds {
            map: 0,
            round: 1,
            scores: [0; 4],
            phase: Phase::Count,
            ticks: 0,
            winner: None,
        }
    }

    /// Positions show through walls.
    pub fn xray(&self) -> bool {
        self.phase == Phase::Live && self.ticks >= XRAY_TICKS
    }

    /// Nobody moves or fires.
    pub fn frozen(&self) -> bool {
        self.phase != Phase::Live
    }

    /// The seconds left in the count, 3 to 1.
    pub fn count_left(&self) -> i32 {
        (COUNT_TICKS - self.ticks + 59) / 60
    }
}

impl Default for Rounds {
    fn default() -> Rounds {
        Rounds::new()
    }
}

/// What a tick of rounds asks of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Stay,
    /// set up the next round on this map
    Round(usize),
}

/// One tick: `standing` is how many seats in the match are alive, `alone`
/// the one if only one is, `present` how many seats are in it at all.
pub fn tick(r: &mut Rounds, standing: usize, alone: Option<usize>, present: usize, maps: usize) -> Next {
    r.ticks += 1;
    match r.phase {
        Phase::Count => {
            if r.ticks >= COUNT_TICKS {
                r.phase = Phase::Live;
                r.ticks = 0;
            }
        }
        Phase::Live => {
            // a round of one seat never ends by a death
            let decided = present >= 2 && standing <= 1;
            if decided || r.ticks >= LIMIT_TICKS {
                r.winner = if decided { alone } else { None };
                if let Some(w) = r.winner {
                    r.scores[w] += 1;
                }
                r.phase = Phase::Over;
                r.ticks = 0;
            }
        }
        Phase::Over => {
            if r.ticks >= OVER_TICKS {
                if let Some(w) = r.winner.filter(|&w| r.scores[w] >= TO_WIN) {
                    r.phase = Phase::Done;
                    r.winner = Some(w);
                    r.ticks = 0;
                } else {
                    r.map = (r.map + 1) % maps.max(1);
                    r.round += 1;
                    r.phase = Phase::Count;
                    r.ticks = 0;
                    r.winner = None;
                    return Next::Round(r.map);
                }
            }
        }
        Phase::Done => {}
    }
    Next::Stay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_counts_three_seconds_then_goes_and_the_last_standing_takes_it() {
        let mut r = Rounds::new();
        for _ in 0..COUNT_TICKS - 1 {
            tick(&mut r, 4, None, 4, 3);
        }
        assert_eq!(r.phase, Phase::Count);
        tick(&mut r, 4, None, 4, 3);
        assert_eq!(r.phase, Phase::Live);
        tick(&mut r, 2, None, 4, 3);
        assert_eq!(r.phase, Phase::Live, "two still standing");
        tick(&mut r, 1, Some(2), 4, 3);
        assert_eq!(
            (r.phase, r.winner, r.scores),
            (Phase::Over, Some(2), [0, 0, 1, 0])
        );
        // four seconds on, the next round on the next map
        for _ in 0..OVER_TICKS - 1 {
            assert_eq!(tick(&mut r, 1, Some(2), 4, 3), Next::Stay);
        }
        assert_eq!(tick(&mut r, 1, Some(2), 4, 3), Next::Round(1));
        assert_eq!((r.phase, r.round, r.map, r.winner), (Phase::Count, 2, 1, None));
    }

    #[test]
    fn the_first_to_five_wins_the_match_and_the_maps_go_round_in_turn() {
        let mut r = Rounds::new();
        let mut maps = vec![r.map];
        for _ in 0..5 {
            r.phase = Phase::Live;
            r.ticks = 0;
            tick(&mut r, 1, Some(1), 4, 3);
            for _ in 0..OVER_TICKS {
                if let Next::Round(m) = tick(&mut r, 1, Some(1), 4, 3) {
                    maps.push(m);
                }
            }
        }
        assert_eq!((r.phase, r.winner, r.scores[1]), (Phase::Done, Some(1), 5));
        assert_eq!(maps, vec![0, 1, 2, 0, 1]);
    }

    #[test]
    fn a_round_where_everyone_falls_at_once_or_time_runs_out_scores_nobody() {
        let mut r = Rounds::new();
        r.phase = Phase::Live;
        tick(&mut r, 0, None, 4, 3);
        assert_eq!((r.phase, r.winner, r.scores), (Phase::Over, None, [0; 4]));
        let mut r = Rounds::new();
        r.phase = Phase::Live;
        for _ in 0..LIMIT_TICKS {
            tick(&mut r, 3, None, 4, 3);
        }
        assert_eq!((r.phase, r.winner), (Phase::Over, None));
    }

    #[test]
    fn positions_show_through_walls_from_seventy_five_seconds() {
        let mut r = Rounds::new();
        r.phase = Phase::Live;
        r.ticks = XRAY_TICKS - 1;
        assert!(!r.xray());
        r.ticks = XRAY_TICKS;
        assert!(r.xray());
        assert_eq!(XRAY_TICKS, 4500);
    }
}
