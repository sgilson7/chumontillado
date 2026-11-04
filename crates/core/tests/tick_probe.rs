//! The tick's cost with four players moving in the practice room: the
//! journal's Engine page targets 100 µs (native, release) so that
//! rollback's worst frame, nine ticks, stays near 1 ms. A probe: it prints
//! numbers and asserts a loose bound only. Record its output in
//! MEASUREMENTS.md.
//!
//!     cargo test --release -p tool_core --test tick_probe -- --ignored --nocapture

use std::time::Instant;

use tool_core::fx::int;
use tool_core::geom::v3;
use tool_core::input::{Input, CROUCH, FORWARD, JUMP, LEFT, RIGHT};
use tool_core::world::World;

#[test]
#[ignore]
fn four_players_moving_tick_in_well_under_a_millisecond() {
    let mut w = World::practice();
    let one = w.players[0].clone();
    for k in 1..4 {
        let mut p = one.clone();
        p.body.origin = v3(int(-28 + 10 * k), 0, int(-20));
        p.prev = p.body.origin;
        w.players.push(p);
    }
    // each seat hops, strafes, turns and slides in its own rhythm
    let record = |seat: usize, t: u64| Input {
        keys: [
            FORWARD | JUMP,
            FORWARD | LEFT | JUMP,
            RIGHT | JUMP,
            FORWARD | CROUCH,
        ][((t / 40 + seat as u64) % 4) as usize],
        dx: (((t * 7 + seat as u64 * 13) % 300) as i32) - 150,
        ..Input::default()
    };
    let mut times = Vec::with_capacity(3600);
    for t in 0..3600u64 {
        let inputs: Vec<Input> = (0..4).map(|s| record(s, t)).collect();
        let start = Instant::now();
        w.step(&inputs);
        times.push(start.elapsed().as_nanos() as u64);
    }
    times.sort_unstable();
    let mean = times.iter().sum::<u64>() / times.len() as u64;
    let p99 = times[times.len() * 99 / 100];
    let worst = *times.last().unwrap();
    println!(
        "4 players, 3600 ticks: mean {} µs, 99th {} µs, worst {} µs",
        mean / 1000,
        p99 / 1000,
        worst / 1000
    );
    assert!(mean < 1_000_000, "a tick averaged {mean} ns");
}
