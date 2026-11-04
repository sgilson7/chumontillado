//! The tick's cost with four seats in the practice room: the journal's
//! Engine page targets 100 µs (native, release) so that rollback's worst
//! frame, nine ticks, stays near 1 ms. M2: one person hopping and firing
//! the machine pistol and three bots that shoot back, against the room's
//! straw targets, so each tick casts rays as well as moving bodies. A
//! probe: it prints numbers and asserts a loose bound only. Record its
//! output in MEASUREMENTS.md.
//!
//!     cargo test --release -p tool_core --test tick_probe -- --ignored --nocapture

use std::time::Instant;

use tool_core::fx::int;
use tool_core::geom::v3;
use tool_core::input::{Input, FIRE, FORWARD, JUMP, LEFT, SLOT_3};
use tool_core::world::{Bot, World};

#[test]
#[ignore]
fn four_seats_moving_and_shooting_tick_in_well_under_a_millisecond() {
    let mut w = World::room(Bot::Shoots);
    let bot = w.players[1].clone();
    for k in 0..2 {
        let mut p = bot.clone();
        p.body.origin = v3(int(12 + 4 * k), 0, int(-20));
        p.prev = p.body.origin;
        p.spawn.0 = p.body.origin;
        w.players.push(p);
    }
    let record = |t: u64| Input {
        keys: if t == 0 {
            SLOT_3
        } else {
            [FORWARD | JUMP, LEFT | JUMP][(t / 50 % 2) as usize]
        },
        buttons: FIRE,
        dx: ((t * 7) % 300) as i32 - 150,
        ..Input::default()
    };
    let mut times = Vec::with_capacity(3600);
    for t in 0..3600u64 {
        w.players[0].arms.rounds[2] = 30;
        let start = Instant::now();
        w.step(&[record(t)]);
        times.push(start.elapsed().as_nanos() as u64);
    }
    times.sort_unstable();
    let mean = times.iter().sum::<u64>() / times.len() as u64;
    let p99 = times[times.len() * 99 / 100];
    let worst = *times.last().unwrap();
    println!(
        "4 seats (1 firing 12/s, 3 bots), 3600 ticks: mean {} µs, 99th {} µs, worst {} µs",
        mean / 1000,
        p99 / 1000,
        worst / 1000
    );
    assert!(mean < 1_000_000, "a tick averaged {mean} ns");
}
