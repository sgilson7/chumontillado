# Instructions for a coding agent in a Builder project

`AGENTS.md` holds the same rules for agents that read that file instead.

## Where code goes

1. Read `BRIEF.md`, `BUILDER-SPEC.md` and `GLOSSARY.md` before changing code.
2. The core (`crates/core`) owns every application decision and performs no input or output. Scripts and tools that need a rule read the core's output; they do not recompute it.
3. The shim (`crates/shim`) translates values and decides no application behaviour.
4. The page (`web/`) draws what the core returns and does not reproduce a core rule. The core describes the screen (`ui`) and takes settings, screens and flags through one function (`act`). Presentation numbers go in `data/*.json`; shaders go in `data/shaders/` and take rule values as uniforms.

## How to work

5. Run `cargo test` after each meaningful core change.
6. For each new behaviour test, add a row to `tests/breaks.json` and run `python3 scripts/break_tests.py`: the test must fail with its behaviour broken. Work expected values out by hand; never compare a value with the function under test. When a test needs a workaround to pass, the workaround is the finding: record it.
7. Keep `SECOND-ORDER.md` while you work: one row per assumption, surprise, divergence from the plan, or item the person must do, written when you notice it. It is how the person sees what you decided alone. A row that produced a rule names where the rule now lives; a row with status *person* is named in `HANDOFF.md`.
8. A scripted edit that replaces text asserts that it matched. A count in a document comes from `./scripts/count-tests.sh`.
9. Do not deploy unless `BRIEF.md`'s Autonomy paragraph says you may. Deploying is otherwise the person's decision (stage 8).
10. Before handoff, run `./scripts/check.sh` and record the result in `HANDOFF.md`.

## Words

11. Use each term in the sense `GLOSSARY.md` gives it, and never a word from its `Not:` line. A new term gets an entry before it appears in a string.
12. Reader-visible text follows `TONE.md`. A string that looks wrong is reported in `SECOND-ORDER.md`, not repaired. A string you write yourself is listed at the foot of `SECOND-ORDER.md` for the person to read. A list you generate that a reader will see (names, labels, word lists) is a worklist row for the person.

## The design journal

13. Keep the design journal unless `BRIEF.md`'s Journal section says `none`. Start it with `python3 tools/journal/new_journal.py`; it lives beside the project (by default `../<this folder>-journal/`) as its own repository and is never pushed. `tools/journal/ENTRIES.md` says what each kind of entry holds.
14. Before writing `PLAN.md`, write the journal's opening entry: read Builder and the brief back to the person, with numbered rules and what holds each, the question prefix, where you stop, and your questions. When the person answers, each rule that applies to every later session goes under "Rules agreed for this project" below, and each rule the core can enforce becomes a test.
15. Every decision put to the person, visible change, divergence from the plan and deploy gets a journal entry when it happens, citing its rows (T4, R2.3, XX-Q5). Never remove an entry. `scripts/journal_check.py` checks the form; commit in the journal's repository after each change.

## Rules agreed for this project

Agreed 2026-11-04 in the journal (page 5, `#2026-11-04-decisions`; answers in the published journal's store).

- CS-1. Every rule lives in the core, in Q16.16 integers; the page draws instances and reads input (as Chumworld Arena's CA-1).
- CS-2. Chumworld Arena's modules are copied in at e78945b and CSSurf's movement at 91d04b3 before they are changed; each change to a copied number is a `SECOND-ORDER.md` row naming the old value.
- CS-3. A bot moves only through the same input record a player makes, so rollback and a dropped seat stay in step.
- CS-4. Movement first: every move in the journal's Mockup page has a test with its value worked out by hand, and the movement's numbers live in `movement::TUNING`, one place.
- CS-5. One hit envelope for every skin (CS-Q12: A), with a test that each body stays inside it.
- CS-6. No name, likeness or asset from STRAFTAT, ULTRAKILL, the James Bond games, or any painting; painters' styles only.
- CS-7. Publishing is granted for each milestone (CS-Q1: B, BRIEF.md); every visible change and deploy gets a journal entry with shots.
- Answers: CS-Q1 B (public repository, Pages, each milestone; the name a Spanish-sounding play on "The Cask of Amontillado"); CS-Q2 A; CS-Q3 A (one life); CS-Q4 A (first to 5); CS-Q5 B (hits shown at once, taken back on a miss); CS-Q6 A (mesh, host relays); CS-Q7 A (melee arcs); CS-Q8 A (slide jump, slam, weapon jumps; no dash, no wall run); CS-Q9 A (held jump hops); CS-Q10 A (pistol and navaja at spawn); CS-Q11 A (next map each round); CS-Q12 A; CS-Q13 A (60 Hz, aim at the click); CS-Q14 C (no post pass; record a video with a Kuwahara filter for the person); CS-Q15 A (katana turns projectiles, 6 ticks); CS-Q16 A (30% self-damage); CS-Q17 A (bots online); CS-Q18 C (positions through walls at 75 s); CS-Q19 A (names and split as drawn).

## Stopping

Write the journal's opening entry, then `PLAN.md` from the brief, and stop until the person has read it, unless `BRIEF.md`'s Autonomy paragraph approves the plan in advance. When a rule in `BUILDER-SPEC.md` seems to block the work, stop and say so in a worklist row; do not loosen a check to make it pass. When a request is refused on safety grounds, leave the feature out and say so; do not reword the request to get past the refusal.
