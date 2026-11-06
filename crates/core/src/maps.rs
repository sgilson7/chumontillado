//! The three maps of a match (M5), as the journal's Mockup page drew them,
//! each with one trick: the Plaza's stands are a ring of surf ramps, the
//! Catedral's buttresses are ramps and its bell tower a wall-jump shaft,
//! the Galería's tall rooms are climbed wall to wall. A match plays them in
//! turn, round by round (CS-Q11: A).
//!
//! Every map is held to the rule of CS-Q24: A (the journal's Feel mockups
//! page): no place on its floor is more than 6 m from something to move
//! off, a wall, a surf face or a pillar (`tests` below). Each also has a
//! post or pillar to circle near its middle, from the talk's account of how
//! enemies lose you round one.
//!
//! T13 added the Mercado, the Puerto and the Acueducto; T16 the Molino
//! and the Alcázar, whose ramparts stand for the puñal's jump.
//!
//! Lengths in metres, y up. A seat's spawn faces the map's middle.

use crate::fx::{self, int, Fx, ONE};
use crate::geom::{v3, yaw_of, V3};
use crate::map::{block, prism, turned, Map, Surface};
use crate::weapons::Weapon;

/// The maps in the order a match plays them.
pub fn all() -> [Map; COUNT] {
    [
        plaza(),
        catedral(),
        galeria(),
        mercado(),
        puerto(),
        acueducto(),
        molino(),
        alcazar(),
    ]
}

/// How many maps a match goes round.
pub const COUNT: usize = 8;

/// The map at place `k` in the order, built alone.
pub fn map(k: usize) -> Map {
    match k % COUNT {
        0 => plaza(),
        1 => catedral(),
        2 => galeria(),
        3 => mercado(),
        4 => puerto(),
        5 => acueducto(),
        6 => molino(),
        _ => alcazar(),
    }
}

/// A spawn at (x, z) facing the point (fx, fz).
fn spawn(x: i32, z: i32, face_x: i32, face_z: i32) -> (V3, i32) {
    let at = v3(int(x), 0, int(z));
    (at, yaw_of(v3(int(face_x - x), 0, int(face_z - z))))
}

fn b(x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, s: Surface) -> crate::map::Brush {
    block(v3(int(x0), int(y0), int(z0)), v3(int(x1), int(y1), int(z1)), s)
}

/// A square of floor `half` m each way with walls `high` m round it.
fn yard(half_x: i32, half_z: i32, high: i32) -> Vec<crate::map::Brush> {
    let (x, z) = (half_x, half_z);
    vec![
        b(-x - 1, -1, -z - 1, x + 1, 0, z + 1, Surface::Floor),
        b(-x - 1, 0, -z - 1, -x, high, z + 1, Surface::Wall),
        b(x, 0, -z - 1, x + 1, high, z + 1, Surface::Wall),
        b(-x, 0, -z - 1, x, high, -z, Surface::Wall),
        b(-x, 0, z, x, high, z + 1, Surface::Wall),
    ]
}

/// The Plaza: a bullring 40 m across, the stands round it a ring of surf
/// ramps, wooden shields to wall jump from, and a low plinth in the middle
/// with the Toro on it. T17: the barrier at the foot of the stands is gone;
/// a surf round the ring caught on it. T28: the ring stands in the sky, with
/// no walls round it and no floor behind the stands: over their top is a
/// fall off the map.
pub fn plaza() -> Map {
    // the floor: a square 36 m across, its corners under the stands, and
    // under each stand a strip reaching to its foot and a metre on
    let mut bs = vec![b(-18, -1, -18, 18, 0, 18, Surface::Floor)];
    for k in 0..32 {
        let strip = block(
            v3(-(ONE * 28 / 10), -ONE, int(14)),
            v3(ONE * 28 / 10, 0, int(22)),
            Surface::Floor,
        );
        bs.push(turned(strip, fx::TURN * k / 32, V3::ZERO));
    }
    // the stands: 32 surf ramps from 21 m out to 27, 8 m high (53°)
    for k in 0..32 {
        let a = fx::TURN * k / 32;
        let ramp = prism(
            [(int(21), 0), (int(27), 0), (int(27), int(8))],
            false,
            -(ONE * 28 / 10),
            ONE * 28 / 10,
            Surface::Ramp,
        );
        bs.push(turned(ramp, a, V3::ZERO));
    }
    // eight shields (burladeros) at 12 m, 1.9 m high, to wall jump off
    for k in 0..8 {
        let a = fx::TURN * (2 * k + 1) / 16;
        let shield = block(
            v3(-(ONE * 12 / 10), 0, -(ONE * 15 / 100)),
            v3(ONE * 12 / 10, ONE * 19 / 10, ONE * 15 / 100),
            Surface::Block,
        );
        bs.push(turned(
            shield,
            a,
            v3(fx::mul(int(12), fx::sin(a)), 0, fx::mul(int(12), fx::cos(a))),
        ));
    }
    // the plinth: 3 m square, 1.2 m high, to hop onto or circle
    bs.push(block(
        v3(-ONE * 3 / 2, 0, -ONE * 3 / 2),
        v3(ONE * 3 / 2, ONE * 12 / 10, ONE * 3 / 2),
        Surface::Block,
    ));
    let on = |k: i32, r: i32| {
        let a = fx::TURN * k / 8;
        v3(fx::mul(int(r), fx::sin(a)), 0, fx::mul(int(r), fx::cos(a)))
    };
    Map {
        brushes: bs,
        spawns: vec![
            spawn(0, 16, 0, 0),
            spawn(16, 0, 0, 0),
            spawn(0, -16, 0, 0),
            spawn(-16, 0, 0, 0),
        ],
        name: "map_plaza",
        pickups: vec![
            (v3(0, ONE * 12 / 10, 0), Weapon::Toro),
            (on(1, 7), Weapon::Shotgun),
            (on(3, 7), Weapon::Drum),
            (on(5, 7), Weapon::Rifle),
            (on(7, 7), Weapon::Katana),
            (on(1, 16), Weapon::MachinePistol),
            (on(3, 16), Weapon::Collage),
            (on(5, 16), Weapon::Guadana),
            (on(7, 16), Weapon::Trabuco),
            (on(5, 11), Weapon::Punal),
        ],
        bot_loop: vec![(8, 0), (0, 8), (-8, 0), (0, -8)],
    }
}

/// The Catedral: a nave and transept open to the sky, with columns down the
/// nave; outside, flying buttresses that are surf ramps, and a bell tower
/// that is a wall-jump shaft.
pub fn catedral() -> Map {
    // the yard was 40 m by 44 each way; the probe test found it empty, 11 m
    // from anything, so it is narrower and a cloister of columns stands in it
    let mut bs = yard(32, 40, 16);
    let w = Surface::Wall;
    let column = |x: i32, z: i32, high: i32| {
        block(
            v3(int(x) - ONE * 6 / 10, 0, int(z) - ONE * 6 / 10),
            v3(int(x) + ONE * 6 / 10, int(high), int(z) + ONE * 6 / 10),
            Surface::Block,
        )
    };
    for x in [-28, -20, 20, 28] {
        for z in [-36, -28, -20, -12, 12, 20, 28, 36] {
            bs.push(column(x, z, 8));
        }
    }
    for x in [-12, -4, 4, 12] {
        for z in [-36, 36] {
            bs.push(column(x, z, 8));
        }
    }
    // the altar at the crossing, to circle, the Pincel on it
    bs.push(b(-1, 0, -1, 1, 1, 1, Surface::Block));
    // the nave's walls, with the crossing open and side doors at 18 to 22
    for (z0, z1) in [(-30, -6), (6, 18), (22, 30)] {
        bs.push(b(-8, 0, z0, -7, 12, z1, w));
        bs.push(b(7, 0, z0, 8, 12, z1, w));
    }
    // the transept's walls, its ends with doors
    for side in [-1, 1] {
        let (x0, x1) = if side < 0 { (-22, -8) } else { (8, 22) };
        bs.push(b(x0, 0, -7, x1, 12, -6, w));
        bs.push(b(x0, 0, 6, x1, 12, 7, w));
        let (e0, e1) = if side < 0 { (-23, -22) } else { (22, 23) };
        bs.push(b(e0, 0, -7, e1, 12, -2, w));
        bs.push(b(e0, 0, 2, e1, 12, 7, w));
    }
    // the apse wall to the north, the west front to the south with a door
    bs.push(b(-8, 0, -31, 8, 12, -30, w));
    bs.push(b(-8, 0, 30, -2, 12, 31, w));
    bs.push(b(2, 0, 30, 8, 12, 31, w));
    // columns down the nave
    for z in [-24, -18, -12, 12, 18, 24] {
        for x in [-4, 4] {
            bs.push(block(
                v3(int(x) - ONE / 2, 0, int(z) - ONE / 2),
                v3(int(x) + ONE / 2, int(10), int(z) + ONE / 2),
                Surface::Block,
            ));
        }
    }
    // flying buttresses: surf ramps falling away from the nave, 9 m high
    // over 8 m (48°)
    for (z0, z1) in [(-24, -20), (-16, -12), (10, 14), (24, 28)] {
        bs.push(prism(
            [(int(16), 0), (int(8), 0), (int(8), int(9))],
            true,
            int(z0),
            int(z1),
            Surface::Ramp,
        ));
        bs.push(prism(
            [(-int(16), 0), (-int(8), 0), (-int(8), int(9))],
            true,
            int(z0),
            int(z1),
            Surface::Ramp,
        ));
    }
    // the bell tower: two walls 3 m apart, 14 m high, to climb
    bs.push(b(26, 0, 24, 27, 14, 34, w));
    bs.push(b(30, 0, 24, 31, 14, 34, w));
    Map {
        brushes: bs,
        spawns: vec![
            spawn(0, -26, 0, 0),
            spawn(18, 0, 0, 0),
            spawn(0, 26, 0, 0),
            spawn(-18, 0, 0, 0),
        ],
        name: "map_catedral",
        pickups: vec![
            (v3(0, ONE, 0), Weapon::Pincel),
            (v3(-int(15), 0, 0), Weapon::Shotgun),
            (v3(int(15), 0, 0), Weapon::MachinePistol),
            (v3(0, 0, -int(18)), Weapon::Rifle),
            (v3(0, 0, int(18)), Weapon::Trabuco),
            (v3(-int(24), 0, int(32)), Weapon::Campana),
            (v3(int(24), 0, -int(32)), Weapon::Drum),
            (v3(int(8), 0, -int(34)), Weapon::Collage),
            (v3(-int(8), 0, int(34)), Weapon::Guadana),
            (v3(ONE * 285 / 10, 0, int(29)), Weapon::Katana),
            (v3(-ONE * 285 / 10, 0, -int(29)), Weapon::Punal),
        ],
        bot_loop: vec![
            (0, -20),
            (0, -3),
            (15, 0),
            (0, 3),
            (0, 20),
            (0, 3),
            (-15, 0),
            (0, -3),
        ],
    }
}

/// The Galería: a long hall with four rooms of paintings off each side.
/// Two rooms have a chimney 3 m wide to climb wall to wall, one corner room
/// a surf ramp; pedestals stand in the rooms.
pub fn galeria() -> Map {
    let mut bs = yard(36, 24, 14);
    let w = Surface::Wall;
    // the hall's walls at z = ±3, a door 3 m wide into each room
    let mut edges = vec![-int(36)];
    for d in [-26, -9, 9, 26] {
        edges.push(int(d) - ONE * 3 / 2);
        edges.push(int(d) + ONE * 3 / 2);
    }
    edges.push(int(36));
    for pair in edges.chunks(2) {
        bs.push(block(v3(pair[0], 0, -int(4)), v3(pair[1], int(12), -int(3)), w));
        bs.push(block(v3(pair[0], 0, int(3)), v3(pair[1], int(12), int(4)), w));
    }
    // the rooms' dividers
    for x in [-18, 0, 18] {
        bs.push(block(
            v3(int(x) - ONE / 2, 0, -int(24)),
            v3(int(x) + ONE / 2, int(12), -int(4)),
            w,
        ));
        bs.push(block(
            v3(int(x) - ONE / 2, 0, int(4)),
            v3(int(x) + ONE / 2, int(12), int(24)),
            w,
        ));
    }
    // chimneys: a wall 3 m from a divider, in a north room and a south room
    bs.push(b(3, 0, -20, 4, 12, -8, w));
    bs.push(b(-4, 0, 8, -3, 12, 20, w));
    // a surf ramp in the north-east room: 8 m high over 7 m (49°)
    bs.push(prism(
        [(-int(23), 0), (-int(16), 0), (-int(23), int(8))],
        false,
        int(20),
        int(32),
        Surface::Ramp,
    ));
    // pedestals in the rooms, and paintings on the walls
    for (x, z) in [
        (-26, -14),
        (-9, 14),
        (26, 14),
        (-26, 14),
        (9, -14),
        (-9, -14),
        (9, 14),
    ] {
        bs.push(block(
            v3(int(x) - ONE, 0, int(z) - ONE),
            v3(int(x) + ONE, ONE * 11 / 10, int(z) + ONE),
            Surface::Block,
        ));
    }
    for x in [-30, -22, -13, -5, 5, 13, 22, 30] {
        bs.push(block(
            v3(int(x) - ONE, int(2), -int(24)),
            v3(int(x) + ONE, ONE * 35 / 10, -int(24) + ONE / 20),
            Surface::Trim,
        ));
        bs.push(block(
            v3(int(x) - ONE, int(2), int(24) - ONE / 20),
            v3(int(x) + ONE, ONE * 35 / 10, int(24)),
            Surface::Trim,
        ));
    }
    Map {
        brushes: bs,
        spawns: vec![
            spawn(-26, -10, -26, 0),
            spawn(26, 10, 26, 0),
            spawn(-26, 10, -26, 0),
            spawn(26, -10, 26, 0),
        ],
        name: "map_galeria",
        pickups: vec![
            (V3::ZERO, Weapon::Pincel),
            (v3(-int(9), ONE * 11 / 10, -int(14)), Weapon::Toro),
            (v3(int(9), ONE * 11 / 10, int(14)), Weapon::Campana),
            (v3(-int(26), 0, 0), Weapon::Shotgun),
            (v3(int(26), 0, 0), Weapon::Rifle),
            (v3(int(9), 0, -int(8)), Weapon::Katana),
            (v3(-int(9), 0, int(8)), Weapon::Trabuco),
            (v3(int(30), 0, -int(10)), Weapon::Collage),
            (v3(-int(30), 0, -int(20)), Weapon::Drum),
            (v3(-int(30), 0, int(20)), Weapon::Guadana),
            (v3(int(30), 0, int(20)), Weapon::MachinePistol),
            (v3(-int(9), 0, -int(8)), Weapon::Punal),
        ],
        bot_loop: vec![(-30, 0), (30, 0)],
    }
}

/// A column `w` m square and `high` m tall, its middle at (x, z).
fn pillar(x: Fx, z: Fx, w: Fx, high: Fx, s: Surface) -> crate::map::Brush {
    block(v3(x - w / 2, 0, z - w / 2), v3(x + w / 2, high, z + w / 2), s)
}

/// The Mercado (T13): a market square ringed by arcades, its stalls'
/// steep gable roofs short surf ramps to hop between, a fountain in the
/// middle to circle.
pub fn mercado() -> Map {
    let mut bs = yard(30, 30, 14);
    // the arcades: columns every 4 m 20 m out on each side, a roof over
    // them to climb onto
    for k in -5..=5 {
        let t = int(k * 4);
        for (x, z) in [(t, -int(20)), (t, int(20)), (-int(20), t), (int(20), t)] {
            bs.push(pillar(x, z, ONE, int(6), Surface::Block));
        }
    }
    for side in [-1, 1] {
        bs.push(block(
            v3(-int(30), int(6), int(20) * side - ONE),
            v3(int(30), ONE * 66 / 10, int(20) * side + ONE),
            Surface::Trim,
        ));
        bs.push(block(
            v3(int(20) * side - ONE, int(6), -int(19)),
            v3(int(20) * side + ONE, ONE * 66 / 10, int(19)),
            Surface::Trim,
        ));
    }
    // four stalls: posts and a gable roof 3 m high over 2 m (56°)
    for (cx, cz) in [(-9, -9), (9, -9), (-9, 9), (9, 9)] {
        let (cx, cz) = (int(cx), int(cz));
        for (dx, dz) in [(-3, -1), (3, -1), (-3, 1), (3, 1)] {
            bs.push(pillar(
                cx + int(dx) + ONE / 4 * -dx.signum(),
                cz + int(dz) * 3 / 2,
                ONE * 3 / 10,
                ONE * 24 / 10,
                Surface::Block,
            ));
        }
        bs.push(prism(
            [
                (cz - int(2), ONE * 24 / 10),
                (cz + int(2), ONE * 24 / 10),
                (cz, ONE * 54 / 10),
            ],
            false,
            cx - int(4),
            cx + int(4),
            Surface::Ramp,
        ));
    }
    // the fountain: a basin and its pillar
    bs.push(pillar(0, 0, int(3), ONE * 8 / 10, Surface::Block));
    bs.push(pillar(0, 0, ONE, int(3), Surface::Block));
    Map {
        brushes: bs,
        spawns: vec![
            spawn(0, -25, 0, 0),
            spawn(25, 0, 0, 0),
            spawn(0, 25, 0, 0),
            spawn(-25, 0, 0, 0),
        ],
        name: "map_mercado",
        pickups: vec![
            (v3(0, 0, int(4)), Weapon::Pincel),
            (v3(-int(9), 0, -int(14)), Weapon::Shotgun),
            (v3(int(9), 0, int(14)), Weapon::Rifle),
            (v3(int(14), 0, -int(9)), Weapon::Toro),
            (v3(-int(14), 0, int(9)), Weapon::Drum),
            (v3(int(25), 0, int(25)), Weapon::Katana),
            (v3(-int(25), 0, -int(25)), Weapon::Guadana),
            (v3(int(25), 0, -int(25)), Weapon::Campana),
            (v3(-int(25), 0, int(25)), Weapon::Collage),
            (v3(0, 0, -int(14)), Weapon::Trabuco),
            (v3(0, 0, int(14)), Weapon::MachinePistol),
            (v3(-int(14), 0, -int(9)), Weapon::Punal),
        ],
        bot_loop: vec![(14, 0), (0, 14), (-14, 0), (0, -14)],
    }
}

/// The Puerto (T13): a harbour, a ship's hull in the middle a long ridge to
/// surf down either side, crates stacked on the quays to wall jump
/// between, a shed with a door.
pub fn puerto() -> Map {
    // 28 m each way: at 32 the probe test found the quays' far edges 9 m
    // from anything
    let mut bs = yard(28, 28, 14);
    // the hull: 36 m long, 7 m high over 6 m each side (49°)
    for side in [-1, 1] {
        bs.push(prism(
            [(int(6) * side, 0), (0, 0), (0, int(7))],
            true,
            -int(18),
            int(18),
            Surface::Ramp,
        ));
    }
    // its mast and the bow and stern posts
    bs.push(pillar(0, 0, ONE, int(16), Surface::Block));
    bs.push(pillar(0, -int(19), ONE * 3 / 2, int(4), Surface::Block));
    bs.push(pillar(0, int(19), ONE * 3 / 2, int(4), Surface::Block));
    // crates: stacks one to three high on the quays
    let crates = [
        (-16, -10, 3),
        (-14, 2, 1),
        (-18, 14, 2),
        (16, -14, 2),
        (14, 0, 1),
        (18, 12, 3),
        (-10, -24, 1),
        (10, 24, 2),
        (-24, 0, 2),
        (24, -2, 2),
        (19, -23, 2),
        (21, 21, 1),
        (-21, 22, 2),
        (21, 6, 1),
    ];
    for (x, z, high) in crates {
        bs.push(pillar(int(x), int(z), int(2), int(2 * high), Surface::Block));
    }
    // the shed in the north-west corner, its door facing the ship
    bs.push(b(-17, 0, -28, -16, 6, -24, Surface::Wall));
    bs.push(b(-17, 0, -21, -16, 6, -16, Surface::Wall));
    bs.push(b(-28, 0, -17, -16, 6, -16, Surface::Wall));
    // bollards along the quays
    for z in [-26, -8, 8, 26] {
        bs.push(pillar(-int(9), int(z), ONE * 6 / 10, ONE * 8 / 10, Surface::Trim));
        bs.push(pillar(int(9), int(z), ONE * 6 / 10, ONE * 8 / 10, Surface::Trim));
    }
    Map {
        brushes: bs,
        spawns: vec![
            spawn(-22, 8, 0, 8),
            spawn(22, -8, 0, -8),
            spawn(0, 25, 0, 0),
            spawn(0, -25, 0, 0),
        ],
        name: "map_puerto",
        pickups: vec![
            (v3(-int(22), 0, -int(22)), Weapon::Pincel),
            (v3(int(12), 0, -int(22)), Weapon::Toro),
            (v3(-int(12), 0, int(22)), Weapon::Campana),
            (v3(-int(10), 0, 0), Weapon::Shotgun),
            (v3(int(10), 0, 0), Weapon::Rifle),
            (v3(int(24), 0, int(25)), Weapon::Drum),
            (v3(-int(25), 0, int(25)), Weapon::Katana),
            (v3(int(25), 0, -int(25)), Weapon::Guadana),
            (v3(int(12), 0, int(12)), Weapon::Trabuco),
            (v3(-int(12), 0, -int(12)), Weapon::Collage),
            (v3(int(24), 0, int(10)), Weapon::MachinePistol),
            (v3(-int(24), 0, -int(10)), Weapon::Punal),
        ],
        bot_loop: vec![(12, -22), (12, 22), (-12, 22), (-12, -22)],
    }
}

/// The Acueducto (T13): a valley whose two banks are long surf ramps,
/// crossed by an aqueduct whose top is a bridge between the banks' crests
/// and whose piers stand 3 m apart, to climb between; boulders in the
/// valley.
pub fn acueducto() -> Map {
    let mut bs = yard(32, 36, 16);
    // the banks: 10 m high over 8 m (51°), the length of the valley
    for side in [-1, 1] {
        bs.push(prism(
            [
                (int(22) * side, 0),
                (int(14) * side, 0),
                (int(22) * side, int(10)),
            ],
            true,
            -int(36),
            int(36),
            Surface::Ramp,
        ));
        // the ground behind each crest, level with it
        bs.push(block(
            v3(if side < 0 { -int(32) } else { int(22) }, 0, -int(36)),
            v3(if side < 0 { -int(22) } else { int(32) }, int(10), int(36)),
            Surface::Floor,
        ));
    }
    // the aqueduct: piers 2 m square every 5 m, 3 m apart, and its deck
    for k in -3..3 {
        let x = int(k * 5) + ONE * 5 / 2;
        bs.push(pillar(x, 0, int(2), ONE * 94 / 10, Surface::Wall));
    }
    bs.push(block(
        v3(-int(22), ONE * 94 / 10, -ONE * 3 / 2),
        v3(int(22), int(10), ONE * 3 / 2),
        Surface::Trim,
    ));
    // boulders down the valley
    for (x, z) in [
        (-7, -9),
        (7, 9),
        (-7, 27),
        (7, -27),
        (-7, -27),
        (7, 27),
        (0, -18),
        (0, 18),
        (7, -9),
        (-7, 9),
        (0, 33),
        (0, -33),
    ] {
        bs.push(pillar(int(x), int(z), int(2), ONE * 15 / 10, Surface::Block));
    }
    Map {
        brushes: bs,
        spawns: vec![
            spawn(0, -26, 0, 0),
            spawn(0, 26, 0, 0),
            spawn(-9, -3, 0, 0),
            spawn(9, 3, 0, 0),
        ],
        name: "map_acueducto",
        pickups: vec![
            (v3(0, int(10), 0), Weapon::Pincel),
            (v3(-int(27), int(10), 0), Weapon::Rifle),
            (v3(int(27), int(10), 0), Weapon::Toro),
            (v3(0, 0, -int(12)), Weapon::Shotgun),
            (v3(0, 0, int(12)), Weapon::Drum),
            (v3(-int(10), 0, -int(20)), Weapon::Katana),
            (v3(int(10), 0, int(20)), Weapon::Guadana),
            (v3(-int(27), int(10), -int(25)), Weapon::Campana),
            (v3(int(27), int(10), int(25)), Weapon::Trabuco),
            (v3(int(10), 0, -int(20)), Weapon::Collage),
            (v3(-int(10), 0, int(20)), Weapon::MachinePistol),
            (V3::ZERO, Weapon::Punal),
        ],
        bot_loop: vec![(0, -24), (0, 24)],
    }
}

/// The Molino (T16): a hill of La Mancha in the middle, its four sides
/// surf ramps up to a flat top; a windmill in each corner, its sails a
/// cross on the face toward the hill, to wall jump off; dry-stone walls
/// and haystacks across the fields.
pub fn molino() -> Map {
    // 28 m each way: at 30 the probe test found the fields by the walls
    // 7 m from anything
    let mut bs = yard(28, 28, 14);
    // the hill: a top 8 m square, 6 m up, each side a surf ramp down 5 m
    // out (50°)
    bs.push(b(-4, 0, -4, 4, 6, 4, Surface::Block));
    for k in 0..4 {
        let ramp = prism(
            [(int(4), 0), (int(9), 0), (int(4), int(6))],
            false,
            -int(4),
            int(4),
            Surface::Ramp,
        );
        bs.push(turned(ramp, fx::TURN * k / 4, V3::ZERO));
    }
    // the windmills: towers 4 m square, 11 m high, the sails a cross 8 m
    // across on the face toward the hill
    for (x, z) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
        let (cx, cz) = (int(19 * x), int(19 * z));
        let toward = yaw_of(v3(-cx, 0, -cz));
        // turned so a face, not a corner, looks at the hill
        bs.push(turned(
            block(
                v3(-int(2), 0, -int(2)),
                v3(int(2), int(11), int(2)),
                Surface::Wall,
            ),
            toward,
            v3(cx, 0, cz),
        ));
        // the sails stand just off that face, 2.2 m from the middle
        let d = v3(cx, 0, cz).len();
        let k = ONE * 22 / 10;
        let out = v3(
            cx - fx::mul(k, fx::div(cx, d)),
            0,
            cz - fx::mul(k, fx::div(cz, d)),
        );
        bs.push(turned(
            block(
                v3(-ONE / 4, int(4), -ONE / 5),
                v3(ONE / 4, int(12), ONE / 5),
                Surface::Trim,
            ),
            toward,
            out,
        ));
        bs.push(turned(
            block(
                v3(-int(4), ONE * 77 / 10, -ONE / 5),
                v3(int(4), ONE * 83 / 10, ONE / 5),
                Surface::Trim,
            ),
            toward,
            out,
        ));
    }
    // dry-stone walls, 1.2 m high, to hop or wall jump off, one on each
    // side of the hill
    bs.push(b(-5, 0, 15, 5, 1, 16, Surface::Block));
    bs.push(b(-5, 0, -16, 5, 1, -15, Surface::Block));
    bs.push(b(15, 0, -5, 16, 1, 5, Surface::Block));
    bs.push(b(-16, 0, -5, -15, 1, 5, Surface::Block));
    // haystacks
    for (x, z) in [
        (-12, -24),
        (12, 24),
        (24, -12),
        (-24, 12),
        (-12, 12),
        (12, -12),
        (24, 24),
        (-24, -24),
        (-22, -8),
        (8, -22),
        (22, 8),
        (-8, 22),
        (11, 11),
        (-11, -11),
    ] {
        bs.push(pillar(int(x), int(z), int(2), ONE * 16 / 10, Surface::Block));
    }
    Map {
        brushes: bs,
        spawns: vec![
            spawn(0, -23, 0, 0),
            spawn(23, 0, 0, 0),
            spawn(0, 23, 0, 0),
            spawn(-23, 0, 0, 0),
        ],
        name: "map_molino",
        pickups: vec![
            (v3(0, int(6), 0), Weapon::Pincel),
            (v3(-int(11), 0, 0), Weapon::Shotgun),
            (v3(int(11), 0, 0), Weapon::Rifle),
            (v3(0, 0, -int(11)), Weapon::Toro),
            (v3(0, 0, int(11)), Weapon::Drum),
            (v3(int(14), 0, int(14)), Weapon::Katana),
            (v3(-int(14), 0, -int(14)), Weapon::Guadana),
            (v3(int(14), 0, -int(14)), Weapon::Campana),
            (v3(-int(14), 0, int(14)), Weapon::Punal),
            (v3(int(24), 0, int(6)), Weapon::Trabuco),
            (v3(-int(24), 0, -int(6)), Weapon::Collage),
            (v3(int(6), 0, -int(24)), Weapon::MachinePistol),
        ],
        bot_loop: vec![(12, 0), (0, 12), (-12, 0), (0, -12)],
    }
}

/// How high the Alcázar's ramparts stand (m): under the 3.92 m the puñal
/// jumps (T16), over the 1.74 m a jump does, so the walk is reached by the
/// puñal, a wall jump off a tower, or the surf ramps against it.
pub const RAMPART: Fx = ONE * 35 / 10;

/// The Alcázar (T16): a fortress courtyard, ramparts 3.5 m high along its
/// north and south sides with surf ramps against them, a tower in each
/// corner, arcades of columns down the east and west, and a fountain in
/// the middle among planters.
pub fn alcazar() -> Map {
    let mut bs = yard(30, 30, 16);
    // the ramparts: 5 m deep, the length of the courtyard
    for side in [-1, 1] {
        let (z0, z1) = if side < 0 {
            (-int(30), -int(25))
        } else {
            (int(25), int(30))
        };
        bs.push(block(
            v3(-int(30), 0, z0),
            v3(int(30), RAMPART, z1),
            Surface::Block,
        ));
        // two surf ramps against each, to its top over 3 m out (49°)
        for x in [-14, 14] {
            let ramp = prism(
                [(int(25), 0), (int(22), 0), (int(25), RAMPART)],
                false,
                int(x - 4),
                int(x + 4),
                Surface::Ramp,
            );
            bs.push(if side < 0 {
                turned(ramp, fx::HALF_TURN, V3::ZERO)
            } else {
                ramp
            });
        }
    }
    // the towers, 6 m square and 12 m high, in the corners
    for (x, z) in [(-1, -1), (-1, 1), (1, -1), (1, 1)] {
        bs.push(pillar(int(27 * x), int(27 * z), int(6), int(12), Surface::Wall));
    }
    // the arcades: columns every 5 m, 4 m high, under a beam
    for x in [-20, 20] {
        for k in -4..=4 {
            bs.push(pillar(int(x), int(k * 5), ONE, int(4), Surface::Block));
        }
        bs.push(block(
            v3(int(x) - ONE * 6 / 10, int(4), -int(21)),
            v3(int(x) + ONE * 6 / 10, ONE * 46 / 10, int(21)),
            Surface::Trim,
        ));
    }
    // the fountain: a basin 4 m across and a column
    bs.push(pillar(0, 0, int(4), ONE * 7 / 10, Surface::Trim));
    bs.push(pillar(0, 0, ONE, int(3), Surface::Block));
    // planters, 1 m high: the probe test found the courtyard between
    // the fountain and the arcades 7 m from anything with six
    for (x, z) in [
        (-9, -9),
        (9, 9),
        (-9, 9),
        (9, -9),
        (0, -16),
        (0, 16),
        (-10, 0),
        (10, 0),
        (0, -9),
        (0, 9),
        (-14, 14),
        (14, -14),
        (14, 14),
        (-14, -14),
    ] {
        bs.push(pillar(int(x), int(z), ONE * 15 / 10, ONE, Surface::Block));
    }
    Map {
        brushes: bs,
        spawns: vec![
            spawn(-25, 0, 0, 0),
            spawn(25, 0, 0, 0),
            spawn(0, -21, 0, 0),
            spawn(0, 21, 0, 0),
        ],
        name: "map_alcazar",
        pickups: vec![
            (v3(0, ONE * 7 / 10, -int(2)), Weapon::Pincel),
            (v3(0, RAMPART, -ONE * 275 / 10), Weapon::Rifle),
            (v3(0, RAMPART, ONE * 275 / 10), Weapon::Toro),
            (v3(-int(14), 0, 0), Weapon::Shotgun),
            (v3(int(14), 0, 0), Weapon::Drum),
            (v3(-int(25), 0, -int(12)), Weapon::Katana),
            (v3(int(25), 0, int(12)), Weapon::Guadana),
            (v3(int(25), 0, -int(12)), Weapon::Punal),
            (v3(-int(25), 0, int(12)), Weapon::Campana),
            (v3(-int(8), 0, -int(18)), Weapon::Trabuco),
            (v3(int(8), 0, int(18)), Weapon::Collage),
            (v3(int(8), 0, -int(18)), Weapon::MachinePistol),
        ],
        bot_loop: vec![(-12, -12), (12, -12), (12, 12), (-12, 12)],
    }
}

/// How far `p` is from brush `br`: exact for a box (turned boxes are
/// turned back first), and for a ramp the largest of its planes'
/// distances, which never exceeds the true distance.
pub fn distance_to(br: &crate::map::Brush, p: V3) -> Fx {
    if let crate::map::Shape::Block { mins, maxs } = br.shape {
        // back into the box's own frame: the turn is a rotation and a move
        let t = br.turn;
        let d = p.sub(t.t);
        let local = v3(d.dot(t.c0), d.dot(t.c1), d.dot(t.c2));
        let gap = |v: Fx, lo: Fx, hi: Fx| (lo - v).max(v - hi).max(0);
        let g = v3(
            gap(local.x, mins.x, maxs.x),
            gap(local.y, mins.y, maxs.y),
            gap(local.z, mins.z, maxs.z),
        );
        return g.len();
    }
    br.planes
        .iter()
        .map(|pl| pl.n.dot(p) - pl.d)
        .max()
        .unwrap_or(0)
        .max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::{trace_box, Scratch};

    /// T17: a surf round the foot of the Plaza's stands meets only the
    /// ramps. Every 5° round the ring at 20.5 m, from ankle to head height,
    /// nothing but a ramp or the floor is within a metre.
    #[test]
    fn a_surf_round_the_foot_of_the_plazas_stands_meets_nothing_but_the_ramps() {
        let m = plaza();
        for k in 0..72 {
            let a = fx::TURN * k / 72;
            let (x, z) = (
                fx::mul(ONE * 205 / 10, fx::sin(a)),
                fx::mul(ONE * 205 / 10, fx::cos(a)),
            );
            for y in [ONE / 2, ONE, ONE * 3 / 2] {
                let p = v3(x, y, z);
                for br in &m.brushes {
                    if matches!(br.surface, Surface::Ramp | Surface::Floor) {
                        continue;
                    }
                    let d = distance_to(br, p);
                    assert!(
                        d > ONE,
                        "{:?} {} m from the ring at {k}",
                        br.surface,
                        fx::decimal(d, 2)
                    );
                }
            }
        }
    }

    /// CS-Q24: A. Every 4 m over each map's floor, at 1 m up, a point a
    /// player could stand at (inside no brush, floor beneath at ground
    /// level) has a wall, a surf face, a pillar or a shield within 6 m; the
    /// floors themselves do not count. A failing point is named.
    #[test]
    fn every_place_on_every_map_has_something_to_move_off_within_six_metres() {
        for m in all() {
            let mut sc = Scratch::default();
            let mut probed = 0;
            let mut far = Vec::new();
            let (lo, hi) = m.floors();
            let (x0, x1) = (fx::floor(lo.x) + 2, fx::floor(hi.x) - 2);
            let (z0, z1) = (fx::floor(lo.z) + 2, fx::floor(hi.z) - 2);
            for gx in (x0..=x1).step_by(4) {
                for gz in (z0..=z1).step_by(4) {
                    let p = v3(int(gx), ONE, int(gz));
                    if m.brushes.iter().any(|br| br.holds(p)) {
                        continue;
                    }
                    // floor beneath at ground level, not a rooftop
                    let t = trace_box(&m, &mut sc, p, p.sub(v3(0, int(2), 0)), V3::ZERO, V3::ZERO);
                    if !t.hit() || t.end.y > ONE / 10 {
                        continue;
                    }
                    probed += 1;
                    let near = m
                        .brushes
                        .iter()
                        .filter(|br| br.surface != Surface::Floor)
                        .map(|br| distance_to(br, p))
                        .min()
                        .unwrap_or(int(999));
                    if near > int(6) {
                        far.push((gx, gz, fx::decimal(near, 1)));
                    }
                }
            }
            assert!(probed > 60, "{}: only {probed} probes", m.name);
            assert!(far.is_empty(), "{}: nothing within 6 m of {far:?}", m.name);
        }
    }

    #[test]
    fn every_map_has_four_spawns_on_open_floor_and_its_pickups_on_floor_or_plinth() {
        for m in all() {
            assert_eq!(m.spawns.len(), 4, "{}", m.name);
            let mut sc = Scratch::default();
            for (at, _) in &m.spawns {
                let head = at.add(v3(0, ONE, 0));
                assert!(
                    !m.brushes.iter().any(|br| br.holds(head)),
                    "{}: spawn {at:?} in a wall",
                    m.name
                );
                let t = trace_box(&m, &mut sc, head, at.sub(v3(0, ONE, 0)), V3::ZERO, V3::ZERO);
                assert!(t.hit(), "{}: spawn {at:?} over nothing", m.name);
            }
            for (at, g) in &m.pickups {
                let mid = at.add(v3(0, ONE / 2, 0));
                assert!(
                    !m.brushes.iter().any(|br| br.holds(mid)),
                    "{}: {g:?} inside a brush",
                    m.name
                );
            }
            assert!(m.pickups.len() >= 9, "{}: most weapons lie on the map", m.name);
        }
    }

    #[test]
    fn a_match_goes_round_the_eight_maps_in_turn() {
        // T16 added the Molino and the Alcázar after the six
        let names: Vec<&str> = all().iter().map(|m| m.name).collect();
        assert_eq!(
            names,
            [
                "map_plaza",
                "map_catedral",
                "map_galeria",
                "map_mercado",
                "map_puerto",
                "map_acueducto",
                "map_molino",
                "map_alcazar"
            ]
        );
        for (k, n) in names.iter().enumerate() {
            assert_eq!(map(k).name, *n);
        }
    }

    #[test]
    fn the_distance_to_a_turned_box_is_measured_in_its_own_frame() {
        // a 2 m box turned 45°: from a point 3 m out along x, its corner
        // (√2 = 1.41 out) is 1.59 m away
        let br = turned(
            block(v3(-ONE, 0, -ONE), v3(ONE, int(2), ONE), Surface::Wall),
            fx::deg(45),
            V3::ZERO,
        );
        let d = distance_to(&br, v3(int(3), ONE, 0));
        assert!((d - ONE * 159 / 100).abs() < ONE / 50, "{}", fx::decimal(d, 3));
    }
}
