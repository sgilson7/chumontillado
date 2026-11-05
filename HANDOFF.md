# Handoff

Stage 9, written by the agent. `C5 Consolidate in your own words`. A fresh session starts from this file, not from memory.

## What this is

Chumontillado is a first-person movement shooter for up to four players online, one life a round, built in the browser on Chumworld Arena's engine. M1 is the practice room's movement (hop, surf, slide, slide jump, wall jump, slam); M2 adds the five hitscan guns, straw targets and a bot that hops round the room, or shoots back in a duel. M3 adds the five projectile guns (drum launcher, Trabuco, Toro, Campana, Collage), the three melee weapons (katana with its lunge and parry, navaja, guadaña with its hook), nails that pin, and weapon jumps at 30% self-damage. M4 adds online play for up to four: the host shares a room code or invite link (`?join=CODE`), guests take seats 2 to 4, bots play empty seats, and the match runs on rollback over links between every pair of browsers, the host passing on what a pair cannot send each other; the host switches a bot on or off for each free seat (T10). The feel pass (journal pages 7 and 8, CS-Q20 to CS-Q24) puts every tapped fire rate and reload on a 120-a-minute beat, adds a view kick, an X on a struck head and a shooting-by-speed card, and the gun animations (T8). The four skins are in (T9), chosen in the settings or the lobby (T11). M5 adds matches: one-life rounds, first to 5, on the Plaza, the Catedral and the Galería in turn, weapons picked up off the map, positions shown through walls from 75 s; Play against bots runs one offline. T12 tells players when the browsers cannot connect; T13 adds three maps (the Mercado, the Puerto, the Acueducto), so a match goes round six. M6 adds sound and painted bursts; T15 a painted filter, off to start. T16 adds two skins (the Friar, the Torero), a fourth melee weapon (the Puñal, which jumps more than twice as high while held) and two maps (the Molino, the Alcázar), so a match goes round eight. T17 takes the barrier out of the Plaza, which caught a surf round the stands. T18 makes health a bar and shows a hit: the view shakes (a setting), the screen's edges flush red and an arc points to the shooter. T19 has bots find their way round walls and hunt in a match, so rounds with bots end. Every map surface is textured (T4). It is published at https://sgilson7.github.io/chumontillado/. The design is the journal's (../chumontillado-journal/, https://claude.ai/artifact/5F9AxuvUdChSWFCDBaygSJ).

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
| `crates/core/src/pilot.rs`, `nav.rs` | the bot: a record made from the world; T19: where it can walk on a map and the way round the walls; T20: dodging, falling back, leading, weapons by distance, and the skills (`Skill::traits`) |
| `crates/core/src/hurt.rs` | T18: a hit as the one hit feels it: the shake, the red edge, the arc, the bar's lost part |
| `crates/core/src/online.rs`, `rollback.rs`, `versus.rs` | M4: the wire and the seats; rollback for four; hosting, joining, the lobby and the invite link |
| `crates/core/src/rounds.rs`, `maps.rs` | M5: a match's rounds; the eight maps, held to the 6 m rule |
| `crates/core/src/hands.rs`, `skins.rs` | the weapon in the hands and how it moves; the six skins, each holding its weapon, each after its painter on the smooth shapes (T21: `Round`, `Rod`, `Bell` in `mesh.rs`) |
| `data/shaders/post.frag`, `web/gl.js` | T15: the painted (Kuwahara) filter, a setting off to start, drawn over the scene |
| `crates/core/src/sound.rs`, `data/sounds.json`, `web/sound.js` | M6: which sounds are made and how loud and where each is for the listener; each sound's recipe; the page's synthesis with Web Audio |
| `web/online.js`, `web/net/` | the transport: the one page file that imports it, and the relays (`config.js`), held by `online_fence.rs` |
| `crates/core/src/game.rs`, `api.rs` | screens, settings, clock and frame; the view model and `act` |
| the arena's modules | `fx`, `geom`, `rng`, `ticks`, `mesh`, `prim`, `render`, `look`, `sky`, `camera`, `input`, `flicker`, `page`, `glossary` |
| `web/` | the page, drawing what the core returns |

## Commands

- `cargo test`: the core's tests. `python3 scripts/break_tests.py`: 262 rows; run a commit's own rows by name (`break_tests.py T19`), the whole list before a milestone (a row whose broken code does not compile now fails, R7.8).
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

## Where work stopped (2026-11-04, M6)

M6 is built: sound made in the core and synthesized by the page (R13.1), heard once online even when a record comes late (R13.2), with a volume setting; the painted projectiles and bursts (R7.9); and the Kuwahara video on the journal's Build page (R1.13, R13.4). Next, in order:

1. **A friend's permanent black screen** (T14, R12.1): not reproduced; the page now names what stopped it. Ask what it says now, and their browser and device.
2. **Why a linked guest once never found the room on the live site** (R10.12): players are now told (T12); the cause is not shown. Check the relays in `web/net/config.js`.
3. **Bots on the surf ramps** (R18.3): ramps as a second kind of cell in `nav.rs`, and an air-strafe pilot along them.
4. **The hit taken back on a miss** (CS-Q5: B, R8.10), and the open M5 rows: the host's settings (R10.6) and a free camera for the dead (R10.10).
5. **Final pass:** the rest of T7: bodies that recoil, land and fall.

Before trusting a test run, check it compiled: filter for `error` lines as well as failures (R6.5).

## Open for the person

- R1.1 (person): the name, Chumontillado.
- R1.14 (person): how the movement feels, from play.
- R5.10 (person): how each gun feels, from play (`weapons::WEAPONS`).
- R8.11 (person): online play between real networks and four people at once, from the live page's invite link.
- R12.1 (person): the friend's black screen: what the page says now, and their browser and device.
- R10.11 (person): how the rounds, the three maps and the bots play: Play against bots, or host a match.
- R9.7 (person): how the animations, the kick, the skins and the fire rates on the beat feel, from play.
- R13.3 (person): how the sounds sound, from play (`data/sounds.json`).
- R15.4 (person): how the Friar, the Torero, the Puñal, the Molino and the Alcázar look and play (T16).
- R18.4 (person): how the smoothed costumes look and the bots fight at Easy, Normal and Hard (T20, T21); the bots eased after T22 (R19.1).
- R19.3 (person): how the cuts feel in play (T23).
- R16.4 (person): how the health bar, the shake, the red edge and the arc feel, and how the bots hunt (T18, T19).
- R7.10 (person): how the projectile guns, melee weapons and weapon jumps feel, from play (`weapons::WEAPONS` and the constants beside it).

## Check

`./scripts/check.sh` on 2026-11-05 after T20 and T21: see the commit's check line in the journal entry `2026-11-05-t20-t21`. `python3 scripts/break_tests.py T20 T21`: the 19 new rows caught (the T16 hood row repointed after the merge, and caught). The full run (248 rows) was last complete after T17 (214 of 214); Sam asked on 2026-11-05 for quick tests per commit, so it is not rerun every commit.

After T22 (bots eased): `./scripts/check.sh` passed; `break_tests.py EASE`: 5 of 5 caught. After T23 (cuts): `./scripts/check.sh` passed; `break_tests.py T23`: 9 of 9 caught (262 rows).
