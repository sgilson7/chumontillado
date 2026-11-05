"""The game's own browser interactions, run by check.py in every engine.

check.py has already loaded the page (with ?pointer=free, so the keyboard
drives it without a pointer lock), waited for it to report ready, and let
it idle. This walks a person's first minute: the title, the menu, the
practice room, a run and a jump read off the HUD, the pause with its
controls, and the field of view in the settings.

Expected values are worked out by hand from BRIEF.md and the journal's
Mockup page, as T3 changed it: the run speed is 9 m/s (movement::TUNING), the HUD reads one
decimal, and there are three wall jumps a flight.
"""

import json
import pathlib

STRINGS = json.loads((pathlib.Path(__file__).resolve().parents[2] / "data" / "strings.json").read_text())


def hud(page, id):
    return page.inner_text(f"#hud-{id}")


def until(page, read, want, ms=15000):
    """Poll until `read()` gives `want` (the page's CSP forbids evaluating a
    string, so wait_for_function cannot be used)."""
    for _ in range(ms // 50):
        if read() == want:
            return True
        page.wait_for_timeout(50)
    return read() == want


def run(page, check, engine):
    check(page.get_attribute("body", "data-screen") == "title", "the title screen shows first")
    check(page.inner_text("#title h1") == STRINGS["title"], "the game's name from data/strings.json")
    page.click("#title button")
    page.wait_for_selector('body[data-screen="menu"]', timeout=10000)
    page.click('#menu button[data-action="menu.practice"]')
    page.wait_for_selector('body[data-screen="play"]', timeout=10000)
    check(hud(page, "speed") == "0.0 m/s", "standing still reads 0.0 m/s")
    check(hud(page, "walls") == STRINGS["hud_walls"].replace("{n}", "3"), "three wall jumps to start")

    # hold forward until the HUD reads the run speed (the room's spawn faces
    # the open floor; 9 m/s is reached in under half a second)
    page.keyboard.down("KeyW")
    check(until(page, lambda: hud(page, "speed"), "9.0 m/s"), "holding W reaches the run speed, 9.0 m/s (T3)")
    page.keyboard.down("Space")
    check(until(page, lambda: hud(page, "doing"), STRINGS["hud_air"]), "Space leaves the ground")
    # M6: the jump (and the landing) is heard: the page started sounds
    check(
        until(page, lambda: page.evaluate("() => window.__builderSounds > 0"), True),
        "the jump is heard: Web Audio plays the core's sounds",
    )
    page.keyboard.up("Space")
    page.keyboard.up("KeyW")

    page.keyboard.press("Escape")
    page.wait_for_selector('body[data-screen="paused"]', timeout=10000)
    rows = page.locator("#paused table.controls tr").count()
    # api::CONTROLS: move, look, fire, reload, guns, jump, crouch, wall jump,
    # start again, pause (M2)
    check(rows == 12, f"the pause lists the twelve controls ({rows})")
    page.click('#paused button[data-action="screen.settings"]')
    page.wait_for_selector('body[data-screen="settings"]', timeout=10000)
    page.click('#setting-fov button[data-value="1"]')
    check(until(page, lambda: page.inner_text("#setting-fov output"), "110°"), "Wider takes the field of view from 105° to 110°")
    # T15: the painted filter starts off, and turning it on changes the drawing
    check(page.inner_text("#setting-filter output") == STRINGS["filter_off"], "the painted filter starts off")
    shot = lambda: page.evaluate("() => document.getElementById('view').toDataURL()")
    plain = shot()
    page.wait_for_timeout(300)
    # WebKit's canvas reads differently frame to frame with nothing moving,
    # so there the comparison would prove nothing and is left out (R14.2)
    steady = shot() == plain
    check(steady or engine == "webkit", f"in the pause the scene holds still, WebKit excepted ({engine}: {'steady' if steady else 'not steady'})")
    page.click('#setting-filter button[data-value="1"]')
    check(until(page, lambda: page.inner_text("#setting-filter output"), STRINGS["filter_on"]), "the painted filter turns on")
    page.wait_for_timeout(300)
    if steady:
        check(shot() != plain, "with the filter on, the scene is drawn differently")
    else:
        print(f"  NOTE {engine}: the canvas is not steady, so the filter's drawing is not compared")
    page.click('#setting-filter button[data-value="-1"]')
    check(until(page, lambda: page.inner_text("#setting-filter output"), STRINGS["filter_off"]), "and off again")
    page.click('#settings button[data-action="settings.back"]')
    page.wait_for_selector('body[data-screen="paused"]', timeout=10000)
    check(page.inner_text("#message") == "", "no error message is shown")
