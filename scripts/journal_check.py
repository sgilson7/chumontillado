#!/usr/bin/env python3
"""Check that the design journal exists and can be read the way the procedure reads it.

The journal is where the person reads the agent's work: an HTML page of dated
entries, kept beside the project and never pushed. It is written for a person,
so this checks its form and its links to the repository's records, and nothing
about whether an entry is right:

- once BRIEF.md holds the person's description, the journal named in the
  brief's Journal section exists (`default` is ../<this folder>-journal/), unless
  that section says `none`;
- it holds an opening entry (data-kind="reading"), and the entry is no longer
  the template's blank one (no data-blank attribute). The opening entry is the
  agent reading Builder and the brief back to the person before it plans;
- every entry has an id and a kind, so it can be cited and its removal seen;
- every SECOND-ORDER.md row with status *person* is named in the journal, so
  an item the person must do reaches the page they read;
- every notebook row (R2.3) and triage row (T4) the journal cites exists, so
  the journal cannot report a record that is not there;
- no entry present in the last commit of the journal's repository is gone.
  Entries are history: they are added and corrected, never removed.

On CI the journal is usually absent, since it is never pushed; a missing
journal is then reported and not failed. Everywhere else it fails.

    python3 scripts/journal_check.py
"""

import html
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from notebook_check import cells, rows  # noqa: E402

ENTRY = re.compile(r"<article\b[^>]*>", re.I)
ATTR = re.compile(r'([\w-]+)(?:\s*=\s*"([^"]*)")?')
NOTE_ID = re.compile(r"(?<![\w.-])R([1-9]\d*\.[1-9]\d*)(?![\w]|\.\d)")
TRIAGE_ID = re.compile(r"(?<![\w.-])T([1-9]\d*)(?![\w])")


def section(text, heading):
    """The lines under '## heading' up to the next '## ' heading, or None."""
    out, inside = [], False
    for line in text.splitlines():
        if line.startswith("## "):
            if inside:
                break
            inside = line[3:].strip().lower().startswith(heading)
            continue
        if inside:
            out.append(line)
    return out if inside else None


def described(brief):
    """True once the brief's word-for-word section quotes the person."""
    for line in brief.splitlines():
        if line.startswith("## ") and "word for word" in line.lower():
            body = section(brief, line[3:].strip().lower()) or []
            return any(l.startswith(">") and l.lstrip("> ").strip() for l in body)
    return False


def setting(root, brief):
    """('none', None), or ('path', Path) for the journal the brief names."""
    body = section(brief, "journal")
    value = "default"
    if body:
        m = re.search(r"`([^`]+)`", "\n".join(body))
        if m:
            value = m.group(1).strip()
    if value.lower() == "none":
        return "none", None
    if value.lower() == "default":
        return "path", root.parent / f"{root.name}-journal"
    return "path", (root / value).resolve()


def entries(page):
    """Yield a dict of attributes for each <article class="entry">."""
    for m in ENTRY.finditer(page):
        attrs = {k.lower(): (v if v is not None else "") for k, v in ATTR.findall(m.group(0)[8:-1])}
        if "entry" in attrs.get("class", "").split():
            yield attrs


def text_of(page):
    page = re.sub(r"<(script|style)\b.*?</\1>", " ", page, flags=re.S | re.I)
    return html.unescape(re.sub(r"<[^>]+>", " ", page))


def triage_numbers(root):
    path = root / "TRIAGE.md"
    found, header = set(), None
    if not path.exists():
        return found
    for line in path.read_text().splitlines():
        if not line.lstrip().startswith("|"):
            header = None
            continue
        c = cells(line)
        if header is None:
            header = c
            continue
        if set("".join(c)) <= set("-: ") or not any(c[1:]):
            continue
        m = re.fullmatch(r"T?([1-9]\d*)", c[0])
        if m:
            found.add(m.group(1))
    return found


def committed_ids(index):
    """Entry ids in the last commit of the repository holding the journal, or None."""
    def git(*args):
        return subprocess.run(["git", "-C", str(index.parent), *args], capture_output=True, text=True)
    top = git("rev-parse", "--show-toplevel")
    if top.returncode != 0:
        return None
    rel = index.resolve().relative_to(Path(top.stdout.strip()).resolve()).as_posix()
    old = git("show", f"HEAD:{rel}")
    if old.returncode != 0:
        return None
    return {e["id"] for e in entries(old.stdout) if e.get("id")}


def main(root):
    root = Path(root).resolve()
    brief = (root / "BRIEF.md").read_text()
    kind, where = setting(root, brief)
    if kind == "none":
        print("journal: turned off in BRIEF.md's Journal section")
        return 0
    if not described(brief):
        print("journal: not started yet (BRIEF.md has no description word for word)")
        return 0
    index = where / "index.html"
    if not index.exists():
        if os.environ.get("CI"):
            print(f"journal: {where.name}/ is not on this machine (CI); it is checked where it is kept")
            return 0
        print(f"FAIL the journal is missing: BRIEF.md names {where}, and it has no index.html. "
              "Start it with python3 tools/journal/new_journal.py, or write `none` under ## Journal in BRIEF.md")
        return 1

    page = index.read_text()
    found = list(entries(page))
    problems = []
    readings = [e for e in found if e.get("data-kind") == "reading"]
    if not readings:
        problems.append('the journal has no opening entry: add an <article class="entry" data-kind="reading"> '
                        "reading Builder and the brief back to the person before the plan")
    elif all("data-blank" in e for e in readings):
        problems.append("the opening entry is still the template's blank one (data-blank): fill it in, then remove data-blank")
    for e in found:
        if not e.get("id"):
            problems.append(f"an entry of kind {e.get('data-kind', '?')!r} has no id; give every entry an id so it can be cited")
        if not e.get("data-kind"):
            problems.append(f"entry {e.get('id', '?')} has no data-kind")

    text = text_of(page)
    notebook = root / "SECOND-ORDER.md"
    numbers = set()
    if notebook.exists():
        for number, _, status in rows(notebook.read_text()):
            numbers.add(number)
            if status == "person" and not re.search(rf"(?<![\w.]){re.escape(number)}(?![\w]|\.\d)", text):
                problems.append(f"SECOND-ORDER.md: {number} is the person's to do and the journal does not name it")
    for n in sorted(set(NOTE_ID.findall(text))):
        if f"R{n}" not in numbers:
            problems.append(f"the journal cites R{n}, and SECOND-ORDER.md has no such row")
    triage = triage_numbers(root)
    for n in sorted(set(TRIAGE_ID.findall(text)), key=int):
        if n not in triage:
            problems.append(f"the journal cites T{n}, and TRIAGE.md has no such row")

    before = committed_ids(index)
    if before:
        now = {e["id"] for e in found if e.get("id")}
        for gone in sorted(before - now):
            problems.append(f"entry {gone} was in the journal's last commit and is gone: entries are corrected, never removed")

    for p in problems:
        print(f"FAIL {p}")
    if problems:
        return 1
    print(f"journal: {len(found)} entries at {where.name}/, an opening entry, "
          "person rows named, cited rows exist" + ("" if before is None else ", none removed since the last commit"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent))
