#!/usr/bin/env python3
"""T34: join each lifelike skin's bake (data/skins/<id>.json, written by
tools/mesh/bake_game.py) into data/skins.json, the table the core reads, with
every number as the core holds it (the reach, for the envelope test, in data/skins_reach.json): metres and matrix values as Q16 integers,
tilts in angle units (a turn is 65536). Run after a bake.

    python3 scripts/join_skins.py
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORDER = ["hidalgo", "infanta", "harlequin", "sordo", "fraile", "torero", "knight_a", "knight_b"]
ONE = 65536


def q(x):
    return int(round(x * ONE))


def main():
    out = []
    reach = {}
    for skin in ORDER:
        s = json.loads((ROOT / "data" / "skins" / f"{skin}.json").read_text())
        out.append({
            "skin": skin,
            "lower": s["lower"],
            "upper": s["upper"],
            "boundary": [[q(x) for x in m] for m in s["boundary"]],
            "tilt": [int(round(t * ONE / 360)) for t in s["tilt"]],
            "hold": [[q(x) for x in m] for m in s["hold"]],
        })
        reach[skin] = {pair: {piece: [q(x) for x in r] for piece, r in pieces.items()} for pair, pieces in s["reach"].items()}
    text = json.dumps({"_about": "T34: the lifelike skins' clip table for the core, joined from data/skins/*.json by scripts/join_skins.py. Q16 integers; tilts in angle units.", "skins": out}, separators=(",", ":"))
    (ROOT / "data" / "skins.json").write_text(text + "\n")
    print(f"data/skins.json: {len(out)} skins, {len(text)} bytes")
    # the reach is for the envelope test alone, so it stays out of the game
    (ROOT / "data" / "skins_reach.json").write_text(json.dumps({"_about": "T34: each lifelike skin's reach, by lower and upper clip and piece: furthest from the upright line, highest, lowest (Q16 m). Read by the envelope test only.", "skins": reach}, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
