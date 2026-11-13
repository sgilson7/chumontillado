#!/usr/bin/env python3
"""Online check (M4): three browsers meet by room code and play one match.

After Chumworld Arena's tests/browser/online.py (e78945b), which met by
pasted codes. This game meets only by room code, so this check goes through
the public relays in web/net/config.js and needs the internet; it sits
beside the browser check, which fails on any off-origin request and must
keep doing so.

The host opens a room and reads its invite link from the page. One guest
opens that link; the other types the room code into Join. The host starts;
the fourth seat is a bot's. All three play for ten seconds, and their
checksums for every tick all of them summed must match.

`?ice=none` drops the STUN servers, so the browsers, all on this machine,
link on local addresses; the relays still introduce them.

It fails on: a page error, a browser that never takes its seat or never
plays, the seats not being the ones the host gave, the browsers disagreeing,
or a browser reporting anything but playing.

    python3 tests/browser/online.py [chromium firefox ...]   (default: chromium)
    BUILDER_ORIGIN=https://sgilson7.github.io/chumontillado python3 tests/browser/online.py
"""
import functools
import http.server
import json
import pathlib
import re
import sys
import threading

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[2]
DIST = ROOT / "dist"
STRINGS = json.loads((ROOT / "data" / "strings.json").read_text())
TIMEOUT_MS = 30000
# every browser plays until it has summed this tick (10 s of play)
SUM_TICK = 600

failures = []


def check(ok, what):
    print(("  PASS " if ok else "  FAIL ") + what)
    if not ok:
        failures.append(what)


class Quiet(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
    }

    def log_message(self, *args):
        pass


def serve():
    handler = functools.partial(Quiet, directory=str(DIST))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


LAUNCH = {
    "chromium": {"args": ["--disable-features=WebRtcHideLocalIpsWithMdns"]},
    "firefox": {"firefox_user_prefs": {"media.peerconnection.ice.obfuscate_host_addresses": False}},
}


def open_page(browser, url, who, seen):
    page = browser.new_context().new_page()
    # a relay that refuses its socket is not the game's fault: the
    # transport tries every relay in config.js, and the match only needs
    # one (R8.12). Chromium and Firefox word it differently; every other
    # error still counts
    def relay_down(text):
        return "wss://" in text and ("WebSocket connection" in text or "establish a connection" in text)

    page.on("pageerror", lambda e: None if relay_down(str(e)) else seen.append(f"{who}: {e}"))
    page.on(
        "console",
        lambda m: seen.append(f"{who}: {m.text}") if m.type == "error" and not relay_down(m.text) else None,
    )
    page.goto(url)
    page.wait_for_selector("body[data-ready='true']", state="attached", timeout=TIMEOUT_MS)
    return page


def sums(page):
    out = {}
    for pair in (page.text_content("#online-sums") or "").split():
        tick, value = pair.split(":")
        out[int(tick)] = value
    return out


def run(p, engine, origin):
    print(f"\n[{engine}] online: a host and two guests by room code, a bot in the fourth seat")
    browser = getattr(p, engine).launch(**LAUNCH.get(engine, {}))
    seen = []
    q = "?ice=none&pointer=free"
    host = open_page(browser, f"{origin}/index.html{q}", "host", seen)
    host.click("#title button")
    host.click('button[data-action="online.host"]')
    host.wait_for_selector("#invite-link", state="attached", timeout=TIMEOUT_MS)
    code = host.input_value("#room-code")
    link = host.input_value("#invite-link")
    check(re.fullmatch(r"[A-HJ-NP-Z]{5}", code) is not None, f"the host shows a room code ({code})")
    check(link.endswith(f"?join={code}"), f"the host shows an invite link with it ({link})")

    # guest one opens the invite link (with this check's own settings)
    linked = open_page(browser, link.replace("?", q + "&", 1), "linked", seen)
    # guest two types the code into Join
    typed = open_page(browser, f"{origin}/index.html{q}", "typed", seen)
    typed.click("#title button")
    typed.click('button[data-action="online.join"]')
    typed.fill("#room-in", code.lower())
    typed.click('button[data-action="online.room"]')

    you = STRINGS["seat_you"]
    # T25: the lobby's seats are cards, each saying who holds it
    held = STRINGS["seat_player"]
    try:
        host.locator("#seat-2 .who", has_text=held).wait_for(state="attached", timeout=TIMEOUT_MS * 2)
        ok = True
    except Exception:
        ok = False
    check(ok, "both guests take a seat in the host's lobby")
    if not ok:
        # what each guest was told, for the failure's record
        for who, page in (("linked", linked), ("typed", typed)):
            print(f"    {who} says: {page.inner_text('#online p') if page.query_selector('#online p') else '(no panel)'}")
    seats = [host.inner_text(f"#seat-{k} .who") for k in range(4)]
    want = [you, held, held, STRINGS["seat_bot"]]
    check([x.lower() for x in seats] == [x.lower() for x in want], f"the host holds seat 1, the guests 2 and 3, and seat 4 is a bot's ({seats})")
    for who, page in (("linked", linked), ("typed", typed)):
        try:
            page.locator(".card[data-kind='you'] .who", has_text=you).wait_for(state="attached", timeout=TIMEOUT_MS)
            ok = True
        except Exception:
            ok = False
        check(ok, f"the {who} guest is told its seat")

    host.click('button[data-action="online.start"]')
    pages = (("host", host), ("linked", linked), ("typed", typed))
    for who, page in pages:
        try:
            page.wait_for_selector("body[data-screen='play']", state="attached", timeout=TIMEOUT_MS)
            ok = True
        except Exception:
            ok = False
        check(ok, f"the {who} is playing")

    # each walks and fires on its own keys; the core suite plays long lagged
    # matches, this shows three real pages agree
    for k, (who, page) in enumerate(pages):
        page.click("#view")
        page.keyboard.down("KeyW")
        page.keyboard.down(["KeyA", "KeyD", "Space"][k])
        page.mouse.down()
        page.mouse.up()
    for who, page in pages:
        try:
            page.locator("#online-sums", has_text=f"{SUM_TICK}:").wait_for(state="attached", timeout=TIMEOUT_MS * 2)
            ok = True
        except Exception:
            ok = False
        check(ok, f"the {who} summed the match at tick {SUM_TICK}")
    all_sums = [sums(page) for _, page in pages]
    both = sorted(set(all_sums[0]) & set(all_sums[1]) & set(all_sums[2]))
    check(SUM_TICK in both, f"all three summed tick {SUM_TICK} ({both})")
    check(
        all(all_sums[0][t] == all_sums[1][t] == all_sums[2][t] for t in both),
        f"all three reach the same checksum at every tick all summed {[(t, [s[t] for s in all_sums]) for t in both]}",
    )
    check(not seen, f"no page error ({seen})")
    browser.close()


def run_list(p, engine, origin):
    """T38: a public host's lobby is listed, and a player who presses Find a
    game sees it and joins it from the list; then two players press Quick
    match at once and end in one lobby."""
    print(f"\n[{engine}] online: Find a game lists a host's lobby; two Quick matches meet")
    browser = getattr(p, engine).launch(**LAUNCH.get(engine, {}))
    seen = []
    q = "?ice=none&pointer=free"
    host = open_page(browser, f"{origin}/index.html{q}", "host", seen)
    host.click("#title button")
    host.click('button[data-action="online.host"]')
    host.wait_for_selector("#room-code", state="attached", timeout=TIMEOUT_MS)
    code = host.input_value("#room-code")
    finder = open_page(browser, f"{origin}/index.html{q}", "finder", seen)
    finder.click("#title button")
    finder.click('button[data-action="browse.open"]')
    listed = f'button[data-action="browse.join"][data-value="{code}"]'
    try:
        finder.wait_for_selector(listed, state="attached", timeout=TIMEOUT_MS)
        ok = True
    except Exception:
        ok = False
    check(ok, f"Find a game lists the host's lobby ({code})")
    if ok:
        finder.click(listed)
        try:
            host.locator("#seat-1 .who", has_text=STRINGS["seat_player"]).wait_for(state="attached", timeout=TIMEOUT_MS * 2)
            ok = True
        except Exception:
            ok = False
        check(ok, "the player who found it takes a seat in the host's lobby")
    # both leave as a player would, so their records and links close
    for page in (host, finder):
        page.click('button[data-action="screen.menu"]')

    a = open_page(browser, f"{origin}/index.html{q}", "quick a", seen)
    b = open_page(browser, f"{origin}/index.html{q}", "quick b", seen)
    for page in (a, b):
        page.click("#title button")
    for page in (a, b):
        page.click('button[data-action="browse.quick"]')
    # one hosts, the other gives way and joins it: the host sees a player
    joined = []
    for who, page in (("a", a), ("b", b)):
        try:
            page.locator("#seat-1 .who", has_text=STRINGS["seat_player"]).wait_for(state="attached", timeout=TIMEOUT_MS * 2)
            joined.append(who)
        except Exception:
            pass
    codes = {a.input_value("#room-code") if a.query_selector("#room-code") else "", b.input_value("#room-code") if b.query_selector("#room-code") else ""}
    check(len(joined) >= 1 and len(codes) == 1, f"two Quick matches pressed at once end in one lobby ({joined}, {codes})")
    check(not seen, f"no page error ({seen})")
    browser.close()


def main(engines):
    # BUILDER_ORIGIN, as for the browser check: the live site, whose invite
    # link is then the real one
    import os
    server = None if os.environ.get("BUILDER_ORIGIN") else serve()
    origin = os.environ.get("BUILDER_ORIGIN") or f"http://127.0.0.1:{server.server_address[1]}"
    with sync_playwright() as p:
        for engine in engines:
            run(p, engine, origin)
            run_list(p, engine, origin)
    if server:
        server.shutdown()
    print()
    if failures:
        print(f"ONLINE CHECK FAILED: {len(failures)} problem(s)")
        sys.exit(1)
    print("ONLINE CHECK PASSED in " + ", ".join(engines))


if __name__ == "__main__":
    main(sys.argv[1:] or ["chromium"])
