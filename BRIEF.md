# Brief

Stage 1. The person's own words come first; the rest is written by the agent from them. `C1 Frame the problem and set the rules`, `B1 Establish context`.

## The description, word for word

> Now this is amazing so far. I want to use this base of chumworld arena, and now make the game into a multiplayer online boomer shooter / movement shooter, playing something like ultrakill mixed with straftat, with momentum maintaining tech, a wide variety of guns, cool melee weapons like the katana from straftat. This can be an entirely different repo entirely, I just want to use the first person and artstyle we have developed here to build this boomer shooter. First, start by creating a new design log, starting with a tab on a deep dive research analysis on straftat, the only james bond games wiht local multiplayer, and ultrakill, on how their mechanics can be used to make a fun online arena based shooter, using 1 death per round mechanics like cs and straftat. then, the second design tab will be on  how you will use what we've learend in chumworld and chumworld arena to create this boomer / multiplayer shooter, supporting 4 player online multplater with roll back, with an emphasis on reducing lag and increasing framerate and smootheness of gameplay, as chumworld arena is really smooth currently. then, create a third tab with the design for the playable characters and guns in the game with 4 different playable character skins in the chumworld-arena style, but in a gritty weird artstyle, like a picasso painting or those old spanish masters, and there should be 10 guns and 3 melee weapons in the mvp to start, half of them with a realistic artsyle and the other half with the spanish / picasso artstyle. then the fourth tab will be the full mockup of the game as you will build it, with movement mechanics and flowing movement like surfing / bhopping a top priortiy, with a mix of hitscan and non hitscan guns. walljumps must also be in the game. then expose all design decision i need to make in a 5th tab, and then stop and wait for my instruction.

Sam, 2026-11-04. Then, after answering the journal's nineteen questions (page 5):

> ok i answered, start building

The answers are in the published journal's store and under "Rules agreed" in `CLAUDE.md`. CS-Q1's note: "Give it a more Spanish sounding chum based name, some play on the cask of amontillado (which I know is Italian and not Spanish)". CS-Q14's note: "but I want to see how the kuwahara filter looks so record a video of the game running with it".

## The tool

A first-person movement shooter in the browser for up to four players online, one life a round, built on Chumworld Arena's engine and its primitive-built figures, painted after the Spanish masters and Picasso. Desktop browsers with a keyboard and mouse.

## Done means

For the MVP (PLAN.md M0 to M6):

- Hopping, surfing, sliding, slide jumps, three wall jumps a flight and the slam work as the journal's Mockup page describes, at 60 ticks a second, with the view turned every display frame.
- Ten guns (five hitscan, five projectile) and three melee weapons, seven drawn true and six painted.
- Four seats online with rollback, by room code or link; empty seats can be bots; a dropped seat's bot continues it.
- One-life rounds, first to 5, on three maps in turn; at 75 s every position shows through walls.
- Four skins inside one shared hit envelope.
- Published on GitHub Pages at each milestone.

For M1: the practice room, playable in a browser, where every move above can be tried, with a speed readout.

## Decisions that stay with the person

- The name (Chumontillado, R1.1).
- How the movement and every weapon feel: the numbers are starting points to tune by play.
- The skins' and weapons' names (CS-Q19: A, as drawn).

## Storage

Allowed by Sam on 2026-11-07 (T32), asked "Allow it?" and answering "Allow it": the page keeps the map playlists and the choices a match is planned from (the playlist, its order, the weapons) in this browser between visits. Nothing else is stored, and nothing leaves the browser.

## Accessibility

The HUD's speed, wall jumps and state are text. Settings change the field of view and the look speed. No information is carried by colour alone.

## Style

The journal's roster palette (primed canvas, ivory black, ochre, vermilion, umber), figures from primitives as in Chumworld Arena, no post pass (CS-Q14: C).

## Words

Hop, surf ramp, slope, slide, slide jump, wall jump, slam, practice room: `GLOSSARY.md`.

## Autonomy

The plan is approved in advance by the answers to the journal's questions. Where this brief is silent, the agent decides, records the decision in `SECOND-ORDER.md`, and continues. The agent may not loosen a Builder requirement or a boundary test. CS-Q1: B grants a new public repository under sgilson7 named `chumontillado`, published to GitHub Pages with `./scripts/publish.sh` at each milestone once `./scripts/check.sh` passes. Online play may reach only the relays the online kit lists (M4).

## Journal

../chumontillado-journal/
