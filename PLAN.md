# Plan

Stage 2, written by the agent from `BRIEF.md` and the journal's Mockup page (2026-11-04), whose order of work the answers approved. `B2 Require step-by-step justification`.

| # | Decision | Reason | Test that shows it holds |
| --- | --- | --- | --- |
| M0 | Builder's template, with Chumworld Arena's reusable modules copied in at e78945b: fixed-point maths, vectors, rng, the clock, meshes, the renderer, the look, the sky, strings, glossary, the input record, the view, the flicker scan, the shim and the page | CS-Q2: A; these are the parts that make the arena smooth, and none holds a sword rule | the copied modules' own tests; `boundary.rs`; `copy.rs` |
| M1 | Movement: CSSurf's Source movement (91d04b3) ported to Q16.16 metres, the Mockup page's numbers, and the slide, slide jump, wall jump and slam on top; the practice room; a speed readout. A stop for the person to feel it | Movement is the priority (the brief); surfing and hopping depend on Source's exact order of operations, which CSSurf already ported and tuned | `world/tests.rs`: run speed, strafe gain, autohop, hop on time against late, surf, slide, slide jump, wall jumps, slam bounce, slope, determinism |
| M2 | The five hitscan guns; one bot; hit marks shown at once and taken back on a miss (CS-Q5: B) | Hitscan needs the aim at the click carried in the record (CS-Q13: A) | a shot at a still target hits at the recorded aim; a re-run with the target moved misses |
| M3 | The five projectile guns, the katana (lunge, parry of projectiles in 6 ticks, CS-Q15: A), the navaja, the guadaña; weapon jumps at 30% self-damage (CS-Q16: A) | The weapons' tricks are movement too | each weapon's damage and its movement effect |
| M4 | Four seats with rollback (the arena's design, two seats to four), a mesh with the host relaying pairs that cannot link (CS-Q6: A); bots in seats, online too (CS-Q17: A); a dropped seat continued by its bot | CS-Q6, CS-Q17 | the arena's lockstep and rollback tests at four seats; two browsers' checksums agree |
| M5 | One-life rounds (CS-Q3: A), first to 5 (CS-Q4: A), three maps in turn (CS-Q11: A), the pistol and navaja at spawn (CS-Q10: A), positions through walls at 75 s (CS-Q18: C) | the answers | a round ends when one is left; the scores; the map order |
| M6 | The four skins in one envelope (CS-Q12: A), the weapons as drawn (CS-Q19: A), sound; a video of the game with a Kuwahara pass for the person to see (CS-Q14: C and its note), the pass itself not shipped | the answers | each skin's body inside the envelope in every pose |

Each milestone is published when `./scripts/check.sh` passes (BRIEF.md, Autonomy) and gets a journal entry with shots.

## Open questions for the person

None blocking. The name (R1.1) and every feel judgement stay open.

## Status

- [x] Approved in advance by the journal's answers, 2026-11-04.
