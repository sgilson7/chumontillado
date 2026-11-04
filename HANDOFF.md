# Handoff

Stage 9, written by the agent. `C5 Consolidate in your own words`. A fresh session starts from this file, not from memory.

## What this is

Chumontillado is a first-person movement shooter for up to four players online, one life a round, built in the browser on Chumworld Arena's engine. M1 is the practice room: every move (hop, surf, slide, slide jump, wall jump, slam) with a speed readout, and no weapons yet. It is published at https://sgilson7.github.io/chumontillado/. The design is the journal's (../chumontillado-journal/, https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ).

## Rules that hold it up

- Every rule is in the core in Q16.16 (`boundary.rs` bans floats there); rollback at M4 depends on it.
- The movement's numbers are in `movement::TUNING`, one place; `world/tests.rs` holds each move to a hand-worked value, 28 break rows in `tests/breaks.json`.
- The view turns every display frame by the mouse movement not yet given to a tick (`World::camera`'s `pending`); test `the_mouse_turns_the_view_on_the_frame_it_moves_before_any_tick_takes_it`.
- A wall jump needs a fresh press and a slam a fresh crouch (R1.7), because a held jump hops.

## Shape of the code

| Where | What |
| --- | --- |
| `crates/core/src/movement.rs` | Source's movement (CSSurf's port) and the journal's moves |
| `crates/core/src/trace.rs`, `map.rs` | the box sweep; brushes as planes; the practice room |
| `crates/core/src/world.rs` | the players and one tick; tests in `world/tests.rs` |
| `crates/core/src/game.rs`, `api.rs` | screens, settings, clock and frame; the view model and `act` |
| the arena's modules | `fx`, `geom`, `rng`, `ticks`, `mesh`, `prim`, `render`, `look`, `sky`, `camera`, `input`, `flicker`, `page`, `glossary` |
| `web/` | the page, drawing what the core returns |

## Commands

- `cargo test`: the core's tests. `python3 scripts/break_tests.py`: 28 of 28 caught.
- `./scripts/check.sh`: BUILDER CHECK PASSED (recorded below).
- `./scripts/publish.sh`, with `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME` set (this machine has no git identity).

## What will bite within the hour

- The room's spawn faces −z; tests that run need open floor ahead of them (R1.10).
- `cargo fmt` moves break rows' anchors: check `tests/breaks.json` finds each one after formatting.
- `.venv` is a link to Chumworld Arena's, for Playwright.

## The journal

../chumontillado-journal/, published at https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ. Newest entry: M1, the practice room.

## Open for the person

- R1.1 (person): the name, Chumontillado.
- R1.14 (person): how the movement feels, from play.

## Check

`./scripts/check.sh` on 2026-11-04 at M1: BUILDER CHECK PASSED (format, lint, 140 tests, the WebAssembly build and import audit, the art check, the notebook, the journal, and the browser check in Chromium, Firefox and WebKit).
