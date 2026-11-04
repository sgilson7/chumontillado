# Handoff

Stage 9, written by the agent. `C5 Consolidate in your own words`. A fresh session starts from this file, not from memory.

## What this is

Chumontillado is a first-person movement shooter for up to four players online, one life a round, built in the browser on Chumworld Arena's engine. M1 is the practice room's movement (hop, surf, slide, slide jump, wall jump, slam); M2 adds the five hitscan guns, straw targets and a bot that hops round the room, or shoots back in a duel. M3 adds the five projectile guns (drum launcher, Trabuco, Toro, Campana, Collage), the three melee weapons (katana with its lunge and parry, navaja, guadaña with its hook), nails that pin, and weapon jumps at 30% self-damage. M4 adds online play for up to four: the host shares a room code or invite link (`?join=CODE`), guests take seats 2 to 4, bots play empty seats, and the match runs on rollback over links between every pair of browsers, the host passing on what a pair cannot send each other. Every map surface is textured (T4). It is published at https://sgilson7.github.io/chumontillado/. The design is the journal's (../chumontillado-journal/, https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ).

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
| `crates/core/src/world.rs` | the players, targets, shots, projectiles, bursts, swings, the hook and pins, and one tick; tests in `world/tests.rs` |
| `crates/core/src/weapons.rs`, `arms.rs` | every weapon's numbers (`WEAPONS`), rays, bursts and arcs; what a player holds, fires and swings |
| `crates/core/src/pilot.rs` | the bot: a record made from the world |
| `crates/core/src/online.rs`, `rollback.rs`, `versus.rs` | M4: the wire and the seats; rollback for four; hosting, joining, the lobby and the invite link |
| `web/online.js`, `web/net/` | the transport: the one page file that imports it, and the relays (`config.js`), held by `online_fence.rs` |
| `crates/core/src/game.rs`, `api.rs` | screens, settings, clock and frame; the view model and `act` |
| the arena's modules | `fx`, `geom`, `rng`, `ticks`, `mesh`, `prim`, `render`, `look`, `sky`, `camera`, `input`, `flicker`, `page`, `glossary` |
| `web/` | the page, drawing what the core returns |

## Commands

- `cargo test`: the core's tests. `python3 scripts/break_tests.py`: 131 of 131 caught (a row whose broken code does not compile now fails, R7.8).
- `./scripts/check.sh`: BUILDER CHECK PASSED (recorded below).
- `./scripts/publish.sh`, with `GIT_AUTHOR_NAME`/`GIT_COMMITTER_NAME` set (this machine has no git identity), then `git push origin main`: the push runs `.github/workflows/pages.yml`, which deploys the gh-pages branch (R2.3). Pages' own branch builder hung, so it is not used.

## What will bite within the hour

- The room's spawn faces −z; tests that run need open floor ahead of them (R1.10).
- `cargo fmt` moves break rows' anchors: check `tests/breaks.json` finds each one after formatting.
- `.venv` is a link to Chumworld Arena's, for Playwright.
- `tests/browser/online.py` plays three browsers through the public relays, so it needs the internet and sits outside `check.sh`, whose browser check fails on any off-origin request. Run it after an online change: `.venv/bin/python tests/browser/online.py chromium firefox`.
- The core's I/O scan (`boundary.rs`) refuses the word `net` as an identifier; the online code says `links`.

## The journal

../chumontillado-journal/, published at https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ. Newest entry: M1, the practice room.

## Where work stopped (2026-11-04, M4)

M4 is built: four seats online with rollback and an invite link. The journal's pages 7 and 8 hold the feel talk of 2026-11-04 and its mockups, with questions CS-Q20 to CS-Q24. Next, in order:

1. **The feel pass** (journal page 8), once you answer CS-Q20 to CS-Q24 or say to take the defaults: the fire cadence on a beat, the hop's air time, the headshot mark and view kick, then the hit taken back on a miss (R8.10).
2. **M5:** rounds, the three maps held to the 6 m rule (CS-Q24), the shooting-by-speed card (CS-Q22), positions through walls at 75 s (CS-Q18: C).
3. **M6:** the four skins inside the 0.96 m envelope (R6.1), sound, the painted projectiles (R7.9), and the Kuwahara video (R1.13).
4. **Final pass:** animations (T7, R6.4).

Before trusting a test run, check it compiled: filter for `error` lines as well as failures (R6.5).

## Open for the person

- R1.1 (person): the name, Chumontillado.
- R1.14 (person): how the movement feels, from play.
- R5.10 (person): how each gun feels, from play (`weapons::WEAPONS`).
- R8.11 (person): online play between real networks and four people at once, from the live page's invite link.
- R7.10 (person): how the projectile guns, melee weapons and weapon jumps feel, from play (`weapons::WEAPONS` and the constants beside it).

## Check

`./scripts/check.sh` on 2026-11-04 after M4: BUILDER CHECK PASSED (format, lint, 234 tests, the WebAssembly build and import audit, the art check, the notebook, the journal, and the browser check in Chromium, Firefox and WebKit). Live at 6c528ce: the browser check passed there, and `tests/browser/online.py` played three browsers through the live invite link.
