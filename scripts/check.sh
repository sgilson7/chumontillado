#!/usr/bin/env bash
# The full local pre-deploy check. CI runs this same file.
#
#   ./scripts/check.sh                 everything
#   BUILDER_ENGINES=chromium ./scripts/check.sh   one engine while iterating
#
# Each step prints its name first, so a failure names the step that failed.
set -euo pipefail
cd "$(dirname "$0")/.."

PY="${BUILDER_PYTHON:-}"
if [ -z "$PY" ]; then
  if [ -x .venv/bin/python ]; then PY=.venv/bin/python; else PY=python3; fi
fi

step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

step "1/11 format (cargo fmt --check)"
cargo fmt --all -- --check

step "2/11 lint (cargo clippy, warnings are errors)"
cargo clippy --workspace --all-targets --quiet -- -D warnings

step "3/11 core and boundary tests (cargo test)"
cargo test --workspace --quiet

step "4/11 every break row still finds its code (scripts/break_tests.py --rotted)"
"$PY" scripts/break_tests.py --rotted

step "5/11 release WebAssembly build (scripts/build.sh)"
./scripts/build.sh >/dev/null
echo "dist/ assembled"

step "6/11 WebAssembly import audit (scripts/wasm_imports.py)"
"$PY" scripts/wasm_imports.py dist

step "7/11 art pipeline checks, no TeX needed (tests/art_check.py)"
if [ -f tests/art_check.py ]; then "$PY" tests/art_check.py; else echo "no art pipeline in this project"; fi

step "8/11 lifelike clips: nothing clips, and the check fails on planted defects (scripts/check_clips.py, tests/clip_defect.py)"
python3 scripts/check_clips.py
python3 tests/clip_defect.py

step "9/11 notebook form (scripts/notebook_check.py)"
"$PY" scripts/notebook_check.py

step "10/11 design journal (scripts/journal_check.py)"
"$PY" scripts/journal_check.py

step "11/11 browser check (tests/browser/check.py)"
"$PY" tests/browser/check.py

printf '\n\033[1mBUILDER CHECK PASSED\033[0m\n'
