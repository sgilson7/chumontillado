//! tool_core: every rule of Chumontillado. No input or output.
//!
//! Builder requirement: this crate performs no input or output. It takes
//! values and returns values, so `cargo test` reaches every rule without a
//! browser. `tests/boundary.rs` enforces that, along with the dependency
//! allowlist and the reference ban on floats, hash maps and clocks.
//!
//! M0: Builder's template with Chumworld Arena's reusable modules (commit
//! e78945b: fixed-point maths, vectors, rng, the clock, meshes, the
//! renderer, the look, the sky, the strings and the glossary, the input
//! record and the view), CS-Q2: A. M1: CSSurf's Source movement (commit
//! 91d04b3) ported to fixed point, with the journal's moves on top. Where to
//! start reading: `api.rs` is the door the shim calls through, `game.rs`
//! the screens and the clock, `world.rs` the order of one tick,
//! `movement.rs` how a player moves.

pub mod api;
pub mod arms;
pub mod camera;
pub mod flicker;
pub mod fx;
pub mod game;
pub mod geom;
pub mod glossary;
pub mod input;
pub mod look;
pub mod map;
pub mod mesh;
pub mod movement;
pub mod page;
pub mod pilot;
pub mod prim;
pub mod render;
pub mod rng;
pub mod sky;
pub mod ticks;
pub mod trace;
pub mod weapons;
pub mod world;
