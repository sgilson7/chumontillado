# Handoff

Stage 9, written by the agent. `C5 Consolidate in your own words`. A fresh session starts from this file, not from memory.

## What this is

Chumontillado is a first-person movement shooter for up to four players online, one life a round, built in the browser on Chumworld Arena's engine. M1 is the practice room's movement (hop, surf, slide, slide jump, wall jump, slam); M2 adds the five hitscan guns, straw targets and a bot that hops round the room, or shoots back in a duel. Every map surface is textured (T4). It is published at https://sgilson7.github.io/chumontillado/. The design is the journal's (../chumontillado-journal/, https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ).

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
| `crates/core/src/world.rs` | the players, targets and shots, and one tick; tests in `world/tests.rs` |
| `crates/core/src/weapons.rs`, `arms.rs` | the guns' numbers and rays; what a player holds and fires |
| `crates/core/src/pilot.rs` | the bot: a record made from the world |
| `crates/core/src/game.rs`, `api.rs` | screens, settings, clock and frame; the view model and `act` |
| the arena's modules | `fx`, `geom`, `rng`, `ticks`, `mesh`, `prim`, `render`, `look`, `sky`, `camera`, `input`, `flicker`, `page`, `glossary` |
| `web/` | the page, drawing what the core returns |

## Commands

- `cargo test`: the core's tests. `python3 scripts/break_tests.py`: 28 of 28 caught.
- `./scripts/check.sh`: BUILDER CHECK PASSED (recorded below).
- `./scripts/publish.sh`, with `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME` set (this machine has no git identity), then `git push origin main`: the push runs `.github/workflows/pages.yml`, which deploys the gh-pages branch (R2.3). Pages' own branch builder hung, so it is not used.

## What will bite within the hour

- The room's spawn faces −z; tests that run need open floor ahead of them (R1.10).
- `cargo fmt` moves break rows' anchors: check `tests/breaks.json` finds each one after formatting.
- `.venv` is a link to Chumworld Arena's, for Playwright.

## The journal

../chumontillado-journal/, published at https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ. Newest entry: M1, the practice room.

## Where work stopped (2026-11-04)

M2 is live with T1 to T6. Next, in order:

1. **M3:** the five projectile guns (Drum launcher, Trabuco, Toro, Campana, Collage), the three melee weapons (katana with its lunge and its 6-tick parry of projectiles, CS-Q15; navaja; guadaña with its hook), and weapon jumps at 30% self-damage (CS-Q16). Projectiles need their own deterministic flight in `world.rs` and a test like the hitscan ones.
2. **M4:** four seats online with rollback. Bring the arena's `online.rs` and `rollback.rs` (e78945b) and the shim's online calls back. This is also where a hit shown and then taken back (R5.2, CS-Q5: B) and the exact aim at the click (R5.6) belong.
3. **M5:** rounds, the three maps (every surface textured, T4), the lobby, positions through walls at 75 s (CS-Q18: C).
4. **M6:** the four skins inside the 0.96 m envelope (R6.1), sound, and the Kuwahara video (R1.13).
5. **Final pass:** animations (T7, R6.4).

Before trusting a test run, check it compiled: filter for `error` lines as well as failures (R6.5).

## Open for the person

- R1.1 (person): the name, Chumontillado.
- R1.14 (person): how the movement feels, from play.
- R5.10 (person): how each gun feels, from play (`weapons::GUNS`).

## Check

`./scripts/check.sh` on 2026-11-04 after T5 and T6: BUILDER CHECK PASSED (format, lint, 179 tests, the WebAssembly build and import audit, the art check, the notebook, the journal, and the browser check in Chromium, Firefox and WebKit).
