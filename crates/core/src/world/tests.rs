//! The movement's behaviour, through the world as a player meets it. Each
//! expected value is worked out by hand from `movement::TUNING` in the
//! test's comment.

use super::*;
use crate::fx::int;
use crate::geom::yaw_of;
use crate::input::{CROUCH, FORWARD, JUMP, LEFT, RIGHT};
use crate::movement::{Mover, TUNING};

fn keys(k: u32) -> Input {
    Input {
        keys: k,
        ..Input::default()
    }
}

fn run(w: &mut World, input: Input, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[input]);
    }
}

/// A body alone in the practice room's map, for the mover directly.
fn body_at(p: V3) -> Body {
    Body::at(p)
}

fn near(a: Fx, b: Fx, within: Fx) -> bool {
    (a - b).abs() <= within
}

#[test]
fn a_player_at_the_spawn_comes_to_rest_on_the_floor() {
    let mut w = World::practice();
    run(&mut w, Input::default(), 60);
    let b = w.players[0].body;
    assert!(b.on_ground, "{b:?}");
    assert_eq!(b.velocity, V3::ZERO);
    // the floor's top is y = 0; a trace stops 1/1024 m short at most
    assert!(b.origin.y >= 0 && b.origin.y < ONE / 100, "{}", b.origin.y);
}

#[test]
fn running_forward_reaches_the_run_speed_and_no_more() {
    let mut w = World::practice();
    // the spawn faces −z (yaw 0) down the open middle of the room
    run(&mut w, keys(FORWARD), 90);
    let s = w.players[0].body.speed();
    // friction 6 against acceleration 10 settles at the run speed: each tick
    // adds 10 × 8 / 60 = 1.33 m/s, more than friction's 6 × 8 / 60 = 0.8 takes,
    // and the add is capped where the speed meets 8
    // T3: the run speed is 9
    assert!(near(s, int(9), ONE / 20), "{}", fx::decimal(s, 3));
}

#[test]
fn air_strafing_with_the_wish_square_to_the_velocity_adds_the_cap_at_right_angles() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    // high above the room, nothing to hit for a second
    let mut b = body_at(v3(0, int(200), 0));
    b.velocity = v3(int(10), 0, 0);
    for _ in 0..60 {
        let v = b.velocity;
        let yaw = yaw_of(v3(-v.z, 0, v.x));
        m.tick(&mut b, &keys(FORWARD), yaw, yaw);
    }
    // two sub-steps a tick: the first adds the whole cap, 0.8, square to the
    // velocity; the second finds 0.8 already along the wish and adds nothing.
    // So speed² grows by 0.64 a tick: √(100 + 60 × 0.64) = 11.77 m/s
    let s = b.speed();
    assert!(near(s, ONE * 1177 / 100, ONE / 20), "{}", fx::decimal(s, 3));
}

#[test]
fn above_the_run_speed_a_wish_already_met_along_its_way_adds_nothing() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    let mut b = body_at(v3(0, int(200), 0));
    // 12 m/s north; D held with the view at 30°, so the wish points 60° off
    // the velocity: 12 × cos 60° = 6 m/s along it already. Past the air's
    // cap (0.8), nothing is added; without the cap 8 − 6 = 2 would be
    b.velocity = v3(0, 0, -int(12));
    for _ in 0..60 {
        m.tick(&mut b, &keys(RIGHT), fx::deg(30), fx::deg(30));
    }
    assert_eq!(b.speed(), int(12));
}

#[test]
fn holding_jump_hops_again_on_every_landing() {
    let mut w = World::practice();
    run(&mut w, Input::default(), 30);
    let mut jumps = 0;
    for _ in 0..180 {
        w.step(&[keys(JUMP)]);
        if w.players[0].events.jumped {
            jumps += 1;
        }
    }
    // 7.92 m/s up under 18 m/s² (T1): 0.88 s, 53 ticks a hop, so 180 ticks
    // hold hops starting at about 0, 53, 106 and 159
    assert_eq!(jumps, 4);
}

#[test]
fn hopping_and_strafing_builds_speed_past_the_run_and_landing_late_does_not() {
    let hop = |late: usize| {
        // a bare floor 400 m across: the room's ramps would get in the way of
        // the circles a strafer turns
        let mut w = World::practice();
        w.map = crate::map::Map {
            brushes: vec![crate::map::block(
                v3(-int(200), -ONE, -int(200)),
                v3(int(200), 0, int(200)),
                crate::map::Surface::Floor,
            )],
            spawns: vec![(V3::ZERO, 0)],
        };
        w.players[0].body.origin = V3::ZERO;
        w.players[0].prev = V3::ZERO;
        run(&mut w, Input::default(), 10);
        // run up to speed first: a jump held from a standstill never meets
        // the ground, so only the air's cap is ever added
        run(&mut w, keys(FORWARD), 60);
        let mut on = 0;
        for _ in 0..150 {
            let b = w.players[0].body;
            let v = b.velocity;
            // the wish square to the velocity, turning left
            w.players[0].aim.yaw = yaw_of(v3(-v.z, 0, v.x));
            let k = if b.on_ground {
                on += 1;
                if on > late {
                    on = 0;
                    FORWARD | JUMP
                } else {
                    FORWARD
                }
            } else {
                FORWARD
            };
            w.step(&[keys(k)]);
        }
        w.players[0].body.speed()
    };
    let on_time = hop(0);
    let late = hop(2);
    // 2.5 s of hops on time: each tick in the air adds 0.64 to speed², from
    // 64: about √(64 + 0.64 × 140) ≈ 12.4 m/s, a little less for the ticks
    // on the ground
    // with T1's longer flights a late hop still gains a little in the air,
    // but the two ticks of friction at each landing give most of it back
    assert!(on_time > int(11), "on time: {}", fx::decimal(on_time, 2));
    assert!(
        on_time - late > int(2),
        "on time {}, late {}",
        fx::decimal(on_time, 2),
        fx::decimal(late, 2)
    );
}

#[test]
fn a_surf_ramp_gives_no_footing_and_speeds_a_surfer_along_it() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    // the west face of the ramp whose ridge is at x = 0 (7 up over 5.25
    // across), half way up, moving north along it at 10 m/s
    let (x, y) = (-ONE * 2625 / 1000, ONE * 35 / 10);
    let mut b = body_at(v3(x - HALF - ONE * 3 / 10, y, int(10)));
    // arriving at the face: 6 m/s into it (east), 10 along it (north)
    b.velocity = v3(int(6), 0, -int(10));
    // the ramp's west face, and the box's corner that meets it first
    let face = *w
        .map
        .brushes
        .iter()
        .filter(|r| r.surface == crate::map::Surface::Ramp && r.mins.x > int(-10))
        .flat_map(|r| r.planes.iter())
        .find(|p| p.n.x < 0 && p.n.y > 0)
        .unwrap();
    // outside the brush is n · p > d, so the gap is how far past d
    let gap = |b: &Body| face.n.dot(b.origin.add(v3(crate::movement::HALF_WIDTH, 0, 0))) - face.d;
    let mut grounded = 0;
    let mut widest = 0;
    for t in 0..30 {
        // a surfer's way: the view north along the ramp, D held into it (east)
        m.tick(&mut b, &keys(RIGHT), 0, 0);
        if b.on_ground {
            grounded += 1;
        }
        // from the third tick, once it has reached the face
        if t >= 3 {
            widest = widest.max(gap(&b));
        }
    }
    // held on the face: the box never more than 5 cm off it (a face that
    // threw back what went into it would bounce the surfer away)
    assert!(widest < ONE / 20, "{}", fx::decimal(widest, 3));
    assert_eq!(grounded, 0, "a ramp face at 53° is never ground");
    assert!(b.velocity.len() > int(10), "{}", fx::decimal(b.velocity.len(), 2));
    // still on the ramp's face, not thrown off it: within a metre of it
    assert!(b.origin.x > x - int(2), "{}", fx::decimal(b.origin.x, 2));
}

/// Half the box's width plus a hair: a box beside a face at x touches it at
/// x − HALF.
const HALF: Fx = crate::movement::HALF_WIDTH + ONE / 50;

#[test]
fn crouching_at_a_run_slides_at_eleven_metres_a_second_and_slows_slowly() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    w.step(&[keys(FORWARD | CROUCH)]);
    assert!(w.players[0].events.slid);
    // 1.4 × 9 = 12.6 (T3), less one tick of slide friction 0.6 × 12.6 / 60 = 0.13
    let s = w.players[0].body.speed();
    assert!(near(s, ONE * 1247 / 100, ONE / 20), "{}", fx::decimal(s, 3));
    run(&mut w, keys(CROUCH), 30);
    let after = w.players[0].body.speed();
    // half a second at friction 0.6: about 11.1 × (1 − 0.01)^30 = 8.2
    // half a second at friction 0.6: about 12.47 × (1 − 0.01)^30 = 9.2
    assert!(after > int(9), "{}", fx::decimal(after, 3));
}

#[test]
fn a_second_slide_within_a_quarter_second_gives_no_boost() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    run(&mut w, keys(FORWARD | CROUCH), 5);
    run(&mut w, keys(FORWARD), 5);
    let before = w.players[0].body.speed();
    w.step(&[keys(FORWARD | CROUCH)]);
    assert!(w.players[0].events.slid);
    // 10 ticks after the first slide began, inside slide_rest's 15
    assert!(w.players[0].body.speed() <= before, "no boost inside slide_rest");
}

#[test]
fn a_jump_out_of_a_slide_adds_speed_along_and_up() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    run(&mut w, keys(FORWARD | CROUCH), 4);
    let before = w.players[0].body.speed();
    w.step(&[keys(CROUCH | JUMP)]);
    assert!(w.players[0].events.jumped);
    let gain = w.players[0].body.speed() - before;
    // + slide_jump 2.5 (T1): the jump comes before friction (Source's
    // order), so that tick takes none
    assert!(near(gain, ONE * 35 / 10, ONE / 50), "{}", fx::decimal(gain, 3));
    // up: 7.92 + 1.5 = 9.42. Crouch is held, so the body is ducked, and a
    // ducked jump sets the speed up rather than adding to it: the half of
    // gravity before the jump is dropped, and the two after it (in the jump
    // and at the tick's end) take 0.15 each: 9.12
    let up = w.players[0].body.velocity.y;
    // T3: 7.92 + 0.5 = 8.42, less the two halves after: 8.12
    assert!(near(up, ONE * 812 / 100, ONE / 50), "{}", fx::decimal(up, 3));
}

#[test]
fn three_wall_jumps_a_flight_each_pushing_up_and_out_and_a_fourth_does_nothing() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    // beside the west wall (its face at x = −32), well off the floor
    let mut b = body_at(v3(int(-32) + HALF, int(6), 0));
    b.velocity = v3(0, 0, -int(6));
    for k in 0..4 {
        b.origin.x = int(-32) + HALF;
        b.velocity.x = 0;
        let ev = m.tick(&mut b, &keys(JUMP), 0, 0);
        if k < 3 {
            assert!(ev.wall_jumped, "jump {k}");
            // 5 m/s out (east), up at 6 less half a tick's gravity, along the
            // wall kept
            assert!(
                near(b.velocity.x, int(5), ONE / 100),
                "{}",
                fx::decimal(b.velocity.x, 3)
            );
            assert!(
                near(b.velocity.y, int(6) - int(18) / 120, ONE / 50),
                "{}",
                fx::decimal(b.velocity.y, 3)
            );
            assert!(near(b.velocity.z, -int(6), ONE / 50));
        } else {
            assert!(!ev.wall_jumped, "a fourth");
        }
        // let go of jump so the next press is fresh
        m.tick(&mut b, &Input::default(), 0, 0);
    }
    assert_eq!(b.wall_jumps, 3);
}

#[test]
fn holding_jump_against_a_wall_does_not_spend_the_wall_jumps() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    let mut b = body_at(v3(int(-32) + HALF, int(6), 0));
    b.old_jump = true;
    for _ in 0..10 {
        b.origin.x = int(-32) + HALF;
        m.tick(&mut b, &keys(JUMP), 0, 0);
    }
    assert_eq!(b.wall_jumps, 0);
}

#[test]
fn a_slam_drops_at_thirty_and_a_jump_off_its_landing_goes_higher_than_a_jump() {
    let mut w = World::practice();
    w.players[0].body.origin = v3(int(-28), int(8), int(-28));
    w.players[0].body.since_slam = 1_000;
    w.step(&[keys(CROUCH)]);
    assert!(w.players[0].events.slammed);
    // −30, less a tick's gravity in its two halves (18 / 60 = 0.3)
    let vy = w.players[0].body.velocity.y;
    assert!(
        near(vy, -int(30) - int(18) / 60, ONE / 50),
        "{}",
        fx::decimal(vy, 3)
    );
    let mut landed = false;
    for _ in 0..40 {
        w.step(&[keys(CROUCH)]);
        if w.players[0].events.landed {
            landed = true;
            break;
        }
    }
    assert!(landed);
    w.step(&[keys(JUMP)]);
    let ev = w.players[0].events;
    assert!(ev.jumped && ev.bounced, "{ev:?}");
    // fell about 8 m: 60% is 4.8 m, √(2 × 18 × 4.8) = 13.1 m/s up, well over a
    // jump's 5.6
    let up = w.players[0].body.velocity.y;
    assert!(up > int(12), "{}", fx::decimal(up, 2));
}

#[test]
fn the_slope_is_walked_up_to_the_balcony() {
    let mut w = World::practice();
    // at the slope's foot, facing south (yaw a half turn) up it
    w.players[0].body.origin = v3(int(28), 0, int(4));
    w.players[0].aim.yaw = fx::HALF_TURN;
    run(&mut w, keys(FORWARD), 240);
    let p = w.players[0].body.origin;
    assert!(p.y >= map::BALCONY_Y - ONE / 100, "{}", fx::decimal(p.y, 2));
}

#[test]
fn r_puts_the_player_back_at_the_spawn() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    w.step(&[keys(crate::input::RESET)]);
    assert_eq!(w.players[0].body.origin, w.map.spawns[0].0);
    assert_eq!(w.players[0].body.velocity, V3::ZERO);
}

#[test]
fn the_same_records_give_the_same_world() {
    let script = |t: u64| Input {
        keys: [FORWARD, FORWARD | JUMP, LEFT | JUMP, CROUCH][(t / 23 % 4) as usize],
        dx: ((t * 37) % 400) as i32 - 200,
        dy: ((t * 11) % 60) as i32 - 30,
        ..Input::default()
    };
    let play = || {
        let mut w = World::practice();
        for t in 0..600 {
            w.step(&[script(t)]);
        }
        w
    };
    assert_eq!(play(), play());
}

#[test]
fn speed_reads_with_one_decimal() {
    assert_eq!(speed_text(ONE * 173 / 10), "17.3");
    assert_eq!(speed_text(int(8)), "8.0");
    assert_eq!(speed_text(0), "0.0");
}

#[test]
fn the_tuning_is_the_mockup_pages_numbers() {
    // the journal's Mockup page, 2026-11-04: run 8, gravity 18, jump 5.6,
    // cap 0.8, air accel 40, slide 1.4× run, three wall jumps
    // T1 doubled the jump's height: 5.6 × √2 = 7.92 m/s
    assert_eq!(
        (
            TUNING.run,
            TUNING.gravity,
            TUNING.jump,
            TUNING.air_cap,
            TUNING.air_accelerate
        ),
        (int(9), int(18), ONE * 792 / 100, ONE * 8 / 10, int(40))
    );
    assert_eq!((TUNING.slide_boost_pct, TUNING.wall_jumps), (140, 3));
}

#[test]
fn a_jump_rises_twice_as_high_as_the_mockups() {
    let w = World::practice();
    let height = |jump: Fx| {
        let mut m = Mover::new(&w.map);
        m.t.jump = jump;
        let mut b = body_at(v3(int(8), 0, int(16)));
        for _ in 0..30 {
            m.tick(&mut b, &Input::default(), 0, 0);
        }
        let floor = b.origin.y;
        m.tick(&mut b, &keys(JUMP), 0, 0);
        let mut top = floor;
        for _ in 0..60 {
            m.tick(&mut b, &Input::default(), 0, 0);
            top = top.max(b.origin.y);
        }
        top - floor
    };
    let (before, now) = (height(ONE * 56 / 10), height(TUNING.jump));
    // height goes with the square of the speed: (7.92 / 5.6)² = 2.0 (T1);
    // each about v² / 2g, 0.87 and 1.74 m, less the ticks' steps
    let ratio = fx::div(now, before);
    assert!(
        near(ratio, int(2), ONE / 20),
        "{} over {}: {}",
        fx::decimal(now, 3),
        fx::decimal(before, 3),
        fx::decimal(ratio, 3)
    );
    assert!(near(now, ONE * 1742 / 1000, ONE / 10), "{}", fx::decimal(now, 3));
}

#[test]
fn a_slide_jump_counts_up_to_a_fifth_of_a_second_after_the_slide_ends() {
    let after = |ticks: usize| {
        let mut w = World::practice();
        run(&mut w, keys(FORWARD), 60);
        run(&mut w, keys(FORWARD | CROUCH), 4);
        // let go of crouch: the slide ends, then wait before jumping
        run(&mut w, keys(FORWARD), ticks);
        w.step(&[keys(FORWARD | JUMP)]);
        w.players[0].events.slide_jumped
    };
    // slide_jump_ticks is 12 (T1): the slide ends on the first tick without
    // crouch, so a jump 11 ticks later is the 12th tick since
    assert!(after(11), "200 ms after");
    assert!(!after(15), "250 ms after");
}

#[test]
fn holding_crouch_and_jump_chains_slide_jumps_landing_after_landing() {
    let mut w = World::practice();
    // a bare floor, room to run straight
    w.map = crate::map::Map {
        brushes: vec![crate::map::block(
            v3(-int(400), -ONE, -int(400)),
            v3(int(400), 0, int(400)),
            crate::map::Surface::Floor,
        )],
        spawns: vec![(V3::ZERO, 0)],
    };
    w.players[0].body.origin = V3::ZERO;
    run(&mut w, keys(FORWARD), 60);
    let mut chained = 0;
    for _ in 0..300 {
        w.step(&[keys(FORWARD | CROUCH | JUMP)]);
        if w.players[0].events.slide_jumped {
            chained += 1;
        }
    }
    // each landing slides (crouch held) and jumps out of it at once: five
    // seconds of 0.88 s flights hold five; each adds 2.5 m/s, from the
    // boost's 11.2 on the first: about 11.2 + 5 × 2.5 = 23.7 m/s
    assert!(chained >= 5, "{chained} slide jumps in a row");
    let s = w.players[0].body.speed();
    // T3: from the boost's 12.6, + 5 × 3.5 = 30.1 m/s
    assert!(s > int(25), "{}", fx::decimal(s, 2));
}

#[test]
fn crouching_on_the_way_down_from_a_jump_readies_a_slide_and_does_not_slam() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    w.step(&[keys(FORWARD | JUMP)]);
    run(&mut w, keys(FORWARD), 40);
    // at the top of a jump, 1.74 m up: under slam_above's 3 m
    w.step(&[keys(FORWARD | CROUCH)]);
    assert!(!w.players[0].events.slammed);
    let mut slid = false;
    for _ in 0..30 {
        w.step(&[keys(FORWARD | CROUCH)]);
        slid |= w.players[0].events.slid;
    }
    assert!(slid, "landing with crouch held slides");
}

#[test]
fn a_slide_a_third_of_a_second_after_the_last_boosts_again() {
    let mut w = World::practice();
    run(&mut w, keys(FORWARD), 60);
    run(&mut w, keys(FORWARD | CROUCH), 3);
    // back to running: friction takes the 11.2 back down to 8 within a few
    // ticks, and acceleration holds it there
    run(&mut w, keys(FORWARD), 17);
    let before = w.players[0].body.speed();
    assert!(near(before, int(9), ONE / 10), "{}", fx::decimal(before, 2));
    // 20 ticks since the first slide began: past slide_rest's 15 (T1)
    w.step(&[keys(FORWARD | CROUCH)]);
    let s = w.players[0].body.speed();
    assert!(s > int(11), "boosted again: {}", fx::decimal(s, 2));
}

#[test]
fn under_the_run_speed_the_keys_steer_outright_in_the_air() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    // standing in the air, D held: the velocity goes straight toward the
    // run speed east at 60 m/s² (T2), so 9 m/s (T3) in 9 / 60 = 0.15 s, 9 ticks
    let mut b = body_at(v3(0, int(200), 0));
    for _ in 0..9 {
        m.tick(&mut b, &keys(RIGHT), 0, 0);
    }
    assert!(
        near(b.velocity.x, int(9), ONE / 20),
        "{}",
        fx::decimal(b.velocity.x, 3)
    );
    assert!(b.velocity.z.abs() < ONE / 100);
    // moving 6 m/s north, S held: north to 9 m/s south is 15 m/s of change,
    // 15 / 60 = 0.25 s, so after 15 ticks it moves south at 9
    let mut c = body_at(v3(0, int(200), 0));
    c.velocity = v3(0, 0, -int(6));
    for _ in 0..15 {
        m.tick(&mut c, &keys(crate::input::BACK), 0, 0);
    }
    assert!(
        near(c.velocity.z, int(9), ONE / 20),
        "{}",
        fx::decimal(c.velocity.z, 3)
    );
}

#[test]
fn above_the_run_speed_w_alone_turns_toward_the_view_and_keeps_the_speed() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    let mut b = body_at(v3(0, int(200), 0));
    // 15 m/s north, the view 45° to the north-east, W held
    b.velocity = v3(0, 0, -int(15));
    let view = yaw_of(v3(ONE, 0, -ONE));
    for _ in 0..30 {
        m.tick(&mut b, &keys(FORWARD), view, view);
    }
    let s = b.speed();
    assert!(near(s, int(15), ONE / 20), "{}", fx::decimal(s, 3));
    // turned most of the way to the view: the east part at least 9 of the
    // 10.6 that 45° would give
    assert!(b.velocity.x > int(9), "{}", fx::decimal(b.velocity.x, 3));
}

#[test]
fn once_on_a_ramp_holding_w_into_it_costs_no_speed_along_it() {
    let w = World::practice();
    let mut m = Mover::new(&w.map);
    let (x, y) = (-ONE * 2625 / 1000, ONE * 35 / 10);
    let mut b = body_at(v3(x - HALF - ONE * 3 / 10, y, int(10)));
    b.velocity = v3(int(6), 0, -int(10));
    let east = yaw_of(v3(ONE, 0, 0));
    let mut along_at_contact = None;
    let mut grounded = 0;
    for _ in 0..30 {
        m.tick(&mut b, &keys(FORWARD), east, east);
        grounded += b.on_ground as i32;
        if b.since_touch == 0 && along_at_contact.is_none() {
            along_at_contact = Some(-b.velocity.z);
        }
    }
    // W steers toward the view in the air, so before the face it turns the
    // velocity a little toward the ramp (R3.1). On the face the air
    // control waits (T2), and the speed along the ramp (north) holds
    let at = along_at_contact.expect("reached the face");
    assert_eq!(grounded, 0);
    assert!(
        -b.velocity.z >= at - ONE / 100,
        "{} at contact, {} after",
        fx::decimal(at, 2),
        fx::decimal(-b.velocity.z, 2)
    );
}

// ---- M2: the guns ----------------------------------------------------------

fn fire() -> Input {
    Input {
        buttons: crate::input::FIRE,
        ..Input::default()
    }
}

/// The room, settled, the player still at the spawn facing north.
fn settled(bot: Bot) -> World {
    let mut w = World::room(bot);
    run(&mut w, Input::default(), 30);
    w
}

/// The pitch (angle units) that aims from the spawn's eye at a height
/// `y` m on the straw target 20 m north (at (8, −4)).
fn pitch_at(y_hundredths: i32) -> i32 {
    // atan2 gives 0 to a whole turn; a pitch is signed
    fx::wrap(fx::atan2(y_hundredths * ONE / 100 - ONE * 165 / 100, int(20)))
}

#[test]
fn a_pistol_shot_takes_fifty_from_a_straw_head_and_twenty_five_from_its_body() {
    // level from the eye (1.65 m), 20 m north: the target's head, at 1.65
    let mut w = settled(Bot::Moves);
    w.step(&[fire()]);
    assert_eq!(w.dummies[0].health, 50, "a headshot: 25 × 2");
    assert!(w.players[0].hit_mark > 0);
    // aimed at 1.1 m, the body
    let mut w = settled(Bot::Moves);
    w.players[0].aim.pitch = pitch_at(110);
    w.step(&[fire()]);
    assert_eq!(w.dummies[0].health, 75);
}

#[test]
fn a_rifle_headshot_fells_a_straw_target_and_it_stands_again_full() {
    let mut w = settled(Bot::Moves);
    w.step(&[keys(crate::input::SLOT_4)]);
    run(&mut w, Input::default(), crate::arms::RAISE as usize);
    w.step(&[fire()]);
    // 70 × 2 = 140, past its 100
    assert_eq!(w.dummies[0].down, TARGET_DOWN);
    run(&mut w, Input::default(), TARGET_DOWN as usize);
    assert_eq!((w.dummies[0].down, w.dummies[0].health), (0, HEALTH));
}

#[test]
fn a_block_between_stops_the_shot() {
    let mut w = settled(Bot::Moves);
    // behind the block at (10, −12), 2 m high, crouched low, aiming level
    // north at the target at (14, −24) would pass; from (10, 0.3 below the
    // block's top) the block is in the way of one at (10, −20)
    w.dummies[0].feet = v3(int(10), 0, int(-20));
    w.players[0].body.origin = v3(int(10), 0, int(-6));
    w.players[0].aim.pitch = fx::wrap(fx::atan2(-(ONE * 65 / 100), int(14)));
    w.step(&[fire()]);
    assert_eq!(w.dummies[0].health, HEALTH);
    assert_eq!(w.marks.len(), 1, "the shot marks the block");
}

#[test]
fn the_shotgun_close_up_nearly_empties_a_target_and_throws_you_up_off_the_floor() {
    let mut w = settled(Bot::Moves);
    w.players[0].body.origin = v3(int(8), 0, -ONE);
    w.step(&[keys(crate::input::SLOT_2)]);
    run(&mut w, Input::default(), crate::arms::RAISE as usize);
    // 3 m from the target, aimed at its chest (1.15 m): the ring's pellets
    // are 3 × tan 5° = 0.26 m out, inside the body's 0.32, so all eight meet
    // it: at least 8 × 12 = 96
    w.players[0].aim.pitch = fx::wrap(fx::atan2(-(ONE / 2), int(3)));
    w.step(&[fire()]);
    let d = &w.dummies[0];
    assert!(d.down > 0 || d.health <= 4, "{} left", d.health);
    // in the air, aimed straight down: up by the kick, 6 m/s
    let mut w = settled(Bot::Moves);
    w.step(&[keys(crate::input::SLOT_2)]);
    run(&mut w, Input::default(), crate::arms::RAISE as usize);
    w.players[0].body.origin.y = int(3);
    w.players[0].body.on_ground = false;
    w.players[0].body.velocity = V3::ZERO;
    w.players[0].aim.pitch = crate::camera::PITCH_MIN;
    w.step(&[fire()]);
    // + 6 × sin 89° ≈ 6.0, less a tick's gravity (0.3)
    let vy = w.players[0].body.velocity.y;
    assert!(near(vy, ONE * 57 / 10, ONE / 10), "{}", fx::decimal(vy, 2));
}

#[test]
fn in_a_duel_the_bot_shoots_the_player_and_in_the_practice_room_it_never_does() {
    let mut duel = settled(Bot::Shoots);
    let mut hurt = false;
    for _ in 0..900 {
        duel.step(&[Input::default()]);
        hurt |= duel.players[0].health < HEALTH || !duel.players[0].alive();
    }
    assert!(hurt, "fifteen seconds in its sight");
    let mut room = settled(Bot::Moves);
    for _ in 0..900 {
        room.step(&[Input::default()]);
        assert_eq!(room.players[0].health, HEALTH);
    }
}

#[test]
fn a_player_who_goes_down_stands_again_at_the_spawn_full() {
    let mut w = settled(Bot::Moves);
    w.players[0].body.origin = v3(int(20), 0, int(-10));
    w.players[0].health = 0;
    w.players[0].dead = RESPAWN;
    run(&mut w, Input::default(), RESPAWN as usize);
    let p = &w.players[0];
    assert!(p.alive());
    assert_eq!((p.health, p.body.origin), (HEALTH, w.map.spawns[0].0));
}

#[test]
fn the_walls_keep_at_most_forty_eight_marks() {
    let mut w = settled(Bot::Moves);
    // straight up at the sky: nothing to mark; at the floor, a mark a shot
    w.players[0].aim.pitch = crate::camera::PITCH_MIN;
    w.step(&[keys(crate::input::SLOT_3)]);
    run(&mut w, Input::default(), crate::arms::RAISE as usize);
    // twelve shots a second: five seconds, sixty shots
    for _ in 0..300 {
        w.players[0].arms.rounds[2] = 30;
        w.step(&[fire()]);
    }
    assert_eq!(w.marks.len(), MARKS_MOST);
}

#[test]
fn the_same_records_with_shots_give_the_same_world() {
    let script = |t: u64| Input {
        keys: [FORWARD, FORWARD | JUMP, LEFT | JUMP, CROUCH][(t / 23 % 4) as usize],
        buttons: if t % 7 < 2 { crate::input::FIRE } else { 0 },
        dx: ((t * 37) % 400) as i32 - 200,
        ..Input::default()
    };
    let play = || {
        let mut w = World::room(Bot::Shoots);
        for t in 0..900 {
            w.step(&[script(t)]);
        }
        w
    };
    assert_eq!(play(), play());
}

#[test]
fn the_drawn_bot_and_straw_target_stay_inside_the_hit_envelope() {
    // CS-5: nothing solid drawn outside what a shot meets: within the body's
    // radius across, and no higher than the top of the head
    use crate::render::Pass;
    use crate::weapons::{BODY_R, HEAD_R};
    let look = crate::look::Look::parse(include_str!("../../../../data/look.json")).unwrap();
    let mut shapes = crate::render::Shapes::default();
    draw_figure(&mut shapes, V3::ZERO, 0, false, false, &look.palette, look.ink());
    // the bot alone must fill most of the envelope (R6.2)
    let bot_parts = shapes.list.len();
    let dummy = Dummy {
        feet: V3::ZERO,
        health: HEALTH,
        down: 0,
        flash: 0,
    };
    draw_dummy(&mut shapes, &dummy, &look.palette, look.ink());
    let top = ONE * 165 / 100 + HEAD_R;
    // every vertex of every solid part, where it is drawn: the meshes' own
    // shapes, not their boxes (the capsule fills a third of its box, R6.2)
    let table = crate::prim::MeshTable::default();
    let mut widest = 0;
    for (k, sh) in shapes
        .list
        .iter()
        .enumerate()
        .filter(|(_, s)| s.pass == Pass::Solid)
    {
        let v = table.vertices(sh.mesh);
        for p in v.chunks(crate::prim::VERTEX_VALUES) {
            let at = sh.at.apply(v3(p[0], p[1], p[2]));
            if k < bot_parts {
                widest = widest.max(at.len_xz());
            }
            assert!(
                at.len_xz() <= BODY_R,
                "a part reaches {} out",
                fx::decimal(at.len_xz(), 3)
            );
            assert!(at.y <= top, "a part reaches {} up", fx::decimal(at.y, 3));
        }
    }
    // and the bot fills most of it: its body at least 0.4 m out
    assert!(
        widest >= ONE * 4 / 10,
        "the widest part reaches only {}",
        fx::decimal(widest, 3)
    );
}

// ---- M3: projectiles, bursts and melee -------------------------------------

/// A bare floor 400 m across with walls across the way north and south at
/// `-north` and `south` metres (0 for none), 20 m high, and the player alone
/// at the origin facing north (heading 0, −z), settled.
fn bare(north: i32, south: i32) -> World {
    use crate::map::{block, Map, Surface};
    let mut w = World::practice();
    let mut brushes = vec![block(
        v3(-int(200), -ONE, -int(200)),
        v3(int(200), 0, int(200)),
        Surface::Floor,
    )];
    if north > 0 {
        brushes.push(block(
            v3(-int(200), 0, -int(north) - ONE),
            v3(int(200), int(20), -int(north)),
            Surface::Wall,
        ));
    }
    if south > 0 {
        brushes.push(block(
            v3(-int(200), 0, int(south)),
            v3(int(200), int(20), int(south) + ONE),
            Surface::Wall,
        ));
    }
    w.map = Map {
        brushes,
        spawns: vec![(V3::ZERO, 0)],
    };
    w.players.truncate(1);
    w.dummies.clear();
    w.players[0] = Player::at((V3::ZERO, 0));
    run(&mut w, Input::default(), 10);
    w
}

/// The player holding `g`, raised and ready.
fn holding(w: &mut World, g: Weapon) {
    w.players[0].arms.weapon = g;
    w.players[0].arms.raise = 0;
}

fn alt() -> Input {
    Input {
        buttons: crate::input::ALT,
        ..Input::default()
    }
}

#[test]
fn a_toro_rocket_takes_about_a_second_to_cross_twenty_metres_and_fells_a_straw_target() {
    let mut w = settled(Bot::Moves);
    holding(&mut w, Weapon::Toro);
    w.players[0].aim.pitch = pitch_at(110);
    w.step(&[fire()]);
    assert_eq!(w.flying.len(), 1);
    // from the muzzle, 0.7 m ahead of the eye, to the target's front, 20 −
    // 0.48 − the rocket's 0.15: 18.67 m at a third of a metre a tick, so
    // the 57th tick's move (counting the firing tick's) reaches it
    run(&mut w, Input::default(), 55);
    assert_eq!(w.dummies[0].health, HEALTH, "still in the air");
    run(&mut w, Input::default(), 4);
    assert_eq!(
        w.dummies[0].down,
        TARGET_DOWN - 3,
        "100 direct, felled three ticks ago"
    );
    assert!(w.flying.is_empty());
    assert_eq!(w.bursts.len(), 1);
}

#[test]
fn a_toro_rocket_at_your_feet_throws_you_higher_than_a_jump_and_costs_thirty_percent() {
    let mut w = bare(0, 0);
    holding(&mut w, Weapon::Toro);
    w.players[0].aim.pitch = crate::camera::PITCH_MIN;
    w.step(&[fire()]);
    let mut top = 0;
    for _ in 0..90 {
        w.step(&[Input::default()]);
        top = top.max(w.players[0].body.origin.y);
    }
    // the burst is within a body's radius of the feet: its edge is 0 away,
    // so 70 × 30% = 21, and the whole 10 m/s push, nearly straight up:
    // about 9.7 m/s, a rise of 9.7² / 36 = 2.6 m; a jump rises 7.92² / 36
    // = 1.74
    assert_eq!(w.players[0].health, HEALTH - 21);
    assert!(top > ONE * 23 / 10, "rose {}", fx::decimal(top, 2));
}

#[test]
fn a_drum_grenade_bounces_along_the_floor_and_bursts_at_its_fuse() {
    let mut w = bare(0, 0);
    holding(&mut w, Weapon::Drum);
    w.step(&[fire()]);
    let start = w.flying[0].bounces;
    let mut low = int(10);
    // its fuse is 90 ticks, the firing tick the first
    for _ in 0..88 {
        w.step(&[Input::default()]);
        if let Some(q) = w.flying.first() {
            low = low.min(q.at.y);
        }
    }
    let q = w.flying[0];
    assert!(q.bounces < start, "it has bounced");
    assert!(low >= 0, "never below the floor: {}", fx::decimal(low, 3));
    assert!(w.bursts.is_empty());
    w.step(&[Input::default()]);
    assert!(w.flying.is_empty());
    assert_eq!(w.bursts.len(), 1);
}

#[test]
fn a_collage_shot_bounces_off_one_wall_and_stops_at_the_next() {
    // walls 10 m north and 10 m south; level from the eye, north: 9.3 m to
    // the first, back 20 m to the second, 29 m at 40 m/s, 44 ticks
    let mut w = bare(10, 10);
    holding(&mut w, Weapon::Collage);
    w.step(&[fire()]);
    run(&mut w, Input::default(), 20);
    let q = w.flying[0];
    assert!(q.velocity.z > 0 && q.bounces == 0, "turned back south");
    assert_eq!(q.velocity.len(), int(40), "keeping all its speed");
    run(&mut w, Input::default(), 30);
    assert!(w.flying.is_empty());
    assert_eq!(w.marks.len(), 1, "it marks the second wall");
}

#[test]
fn a_trabuco_nail_pins_a_body_with_a_wall_close_behind_it() {
    let pinned_after = |wall: i32, at: Fx| {
        let mut w = bare(wall, 0);
        let mut other = Player::at((v3(0, 0, -at), 0));
        other.pilot = None;
        w.players.push(other);
        run(&mut w, Input::default(), 5);
        holding(&mut w, Weapon::Trabuco);
        // level at the chest
        w.players[0].aim.pitch = fx::wrap(fx::atan2(-(ONE / 2), at));
        w.step(&[fire(), Input::default()]);
        run2(&mut w, 15);
        (w.players[1].pinned, w.players[1].health)
    };
    // 9.2 m off with a wall at 10: its back 0.32 m from the wall, its front
    // 1.28 m, inside the 1.5
    let (pinned, health) = pinned_after(10, ONE * 92 / 10);
    assert!(pinned > 0, "pinned");
    assert!(health < HEALTH && (HEALTH - health) % 15 == 0, "{health}");
    // the same with the wall 6 m further back: hit, not pinned
    let (pinned, health) = pinned_after(16, ONE * 92 / 10);
    assert_eq!(pinned, 0);
    assert!(health < HEALTH);
}

fn run2(w: &mut World, ticks: usize) {
    for _ in 0..ticks {
        w.step(&[Input::default(), Input::default()]);
    }
}

#[test]
fn a_katana_swing_hits_a_straw_target_within_reach_once_and_misses_one_beyond() {
    let swing_from = |z_hundredths: i32| {
        let mut w = settled(Bot::Moves);
        w.players[0].body.origin = v3(int(8), 0, z_hundredths * ONE / 100);
        holding(&mut w, Weapon::Katana);
        w.step(&[fire()]);
        run(&mut w, Input::default(), 12);
        w.dummies[0].health
    };
    // the target at z = −4: from −2, its edge 1.52 m off, within the 1.6;
    // one swing, 50, however many of its active ticks it is in the arc
    assert_eq!(swing_from(-200), 50);
    // from −1.8, 1.72 m: out of reach
    assert_eq!(swing_from(-180), HEALTH);
}

#[test]
fn a_navaja_from_behind_fells_a_straw_target_and_from_the_front_takes_thirty_five() {
    // the target at (8, −4) faces the spawn, +z
    let mut w = settled(Bot::Moves);
    w.players[0].body.origin = v3(int(8), 0, -(ONE * 53 / 10));
    w.players[0].aim.yaw = fx::HALF_TURN;
    holding(&mut w, Weapon::Navaja);
    w.step(&[fire()]);
    run(&mut w, Input::default(), 8);
    assert!(w.dummies[0].down > 0, "150 from behind");
    let mut w = settled(Bot::Moves);
    w.players[0].body.origin = v3(int(8), 0, -(ONE * 27 / 10));
    holding(&mut w, Weapon::Navaja);
    w.step(&[fire()]);
    run(&mut w, Input::default(), 8);
    assert_eq!(w.dummies[0].health, HEALTH - 35);
}

#[test]
fn the_navaja_held_runs_ten_percent_faster_and_the_katana_allows_a_fourth_wall_jump() {
    let mut w = World::practice();
    holding(&mut w, Weapon::Navaja);
    run(&mut w, keys(FORWARD), 90);
    // 9 × 1.1 = 9.9
    let s = w.players[0].body.speed();
    assert!(near(s, ONE * 99 / 10, ONE / 20), "{}", fx::decimal(s, 3));
    holding(&mut w, Weapon::Katana);
    assert_eq!(w.players[0].tuning().wall_jumps, 4);
    holding(&mut w, Weapon::Pistol);
    assert_eq!(w.players[0].tuning().wall_jumps, 3);
}

/// A rocket from seat 1, 2.5 m in front of the player's eye at the spawn,
/// flying at them at 20 m/s.
fn rocket_at_the_player(w: &mut World) {
    let eye = w.players[0].eye_now();
    let at = eye.add(v3(0, 0, -(ONE * 25 / 10)));
    w.flying.push(Projectile {
        weapon: Weapon::Toro,
        owner: 1,
        at,
        prev: at,
        velocity: v3(0, 0, int(20)),
        age: 0,
        bounces: 0,
    });
}

#[test]
fn a_katana_swing_in_time_turns_a_rocket_back_and_it_fells_the_target_behind() {
    let mut w = settled(Bot::Moves);
    holding(&mut w, Weapon::Katana);
    w.players[0].aim.pitch = pitch_at(110);
    rocket_at_the_player(&mut w);
    // and one 2.5 m behind the player, flying away: out of the arc
    let mut behind = w.flying[0];
    behind.at = behind.at.add(v3(0, 0, int(5)));
    w.flying.push(behind);
    w.step(&[fire()]);
    assert_eq!(w.flying[1].owner, 1, "the one behind is not turned");
    let q = w.flying[0];
    assert_eq!(q.owner, 0, "the parrier's now");
    assert!(q.velocity.z < 0, "flying north, the way the player faces");
    // north 20 m at 20 m/s to the straw target at (8, −4)
    run(&mut w, Input::default(), 70);
    assert_eq!(w.players[0].health, HEALTH);
    assert!(w.dummies[0].down > 0, "100 direct");
}

#[test]
fn a_katana_swing_after_its_six_parry_ticks_does_not_turn_a_rocket() {
    let mut w = settled(Bot::Moves);
    holding(&mut w, Weapon::Katana);
    w.step(&[fire()]);
    // the swing's age is 1 after its tick; six more and it is past the parry
    run(&mut w, Input::default(), crate::weapons::PARRY as usize);
    rocket_at_the_player(&mut w);
    run(&mut w, Input::default(), 10);
    assert!(!w.players[0].alive(), "100 direct");
}

#[test]
fn a_katana_lunge_flings_you_about_seven_metres_before_you_land() {
    let mut w = bare(0, 0);
    holding(&mut w, Weapon::Katana);
    w.step(&[alt()]);
    assert!(!w.players[0].body.on_ground);
    let mut ticks = 0;
    while !w.players[0].body.on_ground && ticks < 120 {
        w.step(&[Input::default()]);
        ticks += 1;
    }
    // 4 m/s up under 18 m/s²: 0.44 s; at 16 m/s, 7.1 m
    let z = -w.players[0].body.origin.z;
    assert!(z > ONE * 66 / 10 && z < ONE * 76 / 10, "{}", fx::decimal(z, 2));
}

#[test]
fn the_guadana_hook_pulls_you_to_a_wall_in_reach_while_held_and_lets_go_when_released() {
    let mut w = bare(10, 0);
    holding(&mut w, Weapon::Guadana);
    w.players[0].aim.pitch = fx::deg(20);
    w.step(&[alt()]);
    assert!(w.players[0].hook.is_some());
    // lifted off the floor as it catches
    assert_eq!(w.players[0].body.velocity.y, crate::weapons::HOOK_LIFT);
    run(&mut w, alt(), 30);
    let b = w.players[0].body;
    // the hook holds 10 × tan 20° = 3.64 m above the eye, at y 5.29; the
    // line from the body's middle (0.9) rises at about 24°, so 40 m/s² of
    // pull is about 16 up. With the line bearing the body's weight, half a
    // second from 4 m/s up rises 4 × 0.5 + 16 × 0.5² / 2 = 4 m (less as the
    // angle changes); without it, 16 − 18 = −2 m/s², 1.75 m; without the
    // lift, 2 m
    assert!(b.origin.z < -ONE && b.origin.y > ONE * 25 / 10, "{:?}", b.origin);
    w.step(&[Input::default()]);
    assert!(w.players[0].hook.is_none());
    // a wall 20 m off is past the 12 m reach
    let mut w = bare(20, 0);
    holding(&mut w, Weapon::Guadana);
    w.players[0].aim.pitch = fx::deg(20);
    w.step(&[alt()]);
    assert!(w.players[0].hook.is_none());
}

#[test]
fn a_campana_at_your_feet_costs_you_thirty_percent_and_its_toll_pushes_a_player_beside_you_away() {
    let mut w = bare(0, 0);
    let mut other = Player::at((v3(int(3), 0, 0), 0));
    other.pilot = None;
    w.players.push(other);
    run2(&mut w, 5);
    holding(&mut w, Weapon::Campana);
    w.players[0].aim.pitch = crate::camera::PITCH_MIN;
    w.step(&[fire(), Input::default()]);
    run2(&mut w, 10);
    // within 1 m of your own burst: 120 × 30% = 36
    assert_eq!(w.players[0].health, HEALTH - 36);
    let o = &w.players[1];
    assert!(o.health < HEALTH, "hurt");
    assert!(o.body.origin.x > int(3), "pushed away, east");
}

#[test]
fn the_same_records_with_projectiles_and_swings_give_the_same_world() {
    use crate::input::{MELEE, SLOT_10, SLOT_6, SLOT_7, SLOT_8, SLOT_9};
    let slots = [SLOT_6, SLOT_7, SLOT_8, SLOT_9, SLOT_10, MELEE];
    let script = |t: u64| Input {
        keys: [FORWARD, FORWARD | JUMP, LEFT | JUMP, CROUCH][(t / 23 % 4) as usize]
            | if t.is_multiple_of(50) {
                slots[(t / 50 % 6) as usize]
            } else {
                0
            },
        buttons: if t % 7 < 2 { crate::input::FIRE } else { 0 }
            | if t.is_multiple_of(31) {
                crate::input::ALT
            } else {
                0
            },
        dx: ((t * 37) % 400) as i32 - 200,
        ..Input::default()
    };
    let play = || {
        let mut w = World::room(Bot::Shoots);
        for t in 0..900 {
            w.step(&[script(t)]);
        }
        w
    };
    assert_eq!(play(), play());
}

#[test]
fn a_drum_grenade_that_meets_a_body_takes_its_ninety_and_not_its_burst_as_well() {
    // the target 5 m in front: level from the eye at 24 m/s, the grenade
    // falls 9 × 0.2² = 0.36 m on the way and meets the body at 1.3 m
    let mut w = settled(Bot::Moves);
    w.players[0].body.origin = v3(int(8), 0, ONE);
    holding(&mut w, Weapon::Drum);
    w.step(&[fire()]);
    run(&mut w, Input::default(), 20);
    assert_eq!(w.dummies[0].health, HEALTH - 90);
}
