# Measurements

Stage 4, recorded by the agent. `B2 Require step-by-step justification`.

| What | Command | Result | When |
| --- | --- | --- | --- |
| Toolchain | `rustc --version; wasm-bindgen --version` | rustc 1.99.0, wasm-bindgen 0.2.127 | 2026-11-04, Apple M2 Max |
| Tests (M1) | `./scripts/count-tests.sh` | 140 passed, 0 failed, 1 ignored (the probe) | 2026-11-04, Apple M2 Max |
| Breaks (M1) | `python3 scripts/break_tests.py` | 30 of 30 caught, after R1.9's two misses were fixed | 2026-11-04, Apple M2 Max |
| WebAssembly size (M1) | printed by `./scripts/build.sh` | 380 KB (Chumworld Arena's: 1 415 KB, with the sword and online play) | 2026-11-04, Apple M2 Max |
| A tick, four players moving in the practice room (the journal's Engine page targets 100 µs) | `cargo test --release -p tool_core --test tick_probe -- --ignored --nocapture` | mean 5 µs, 99th percentile 7 and 12 µs, worst 47 and 46 µs over two runs of 3 600 ticks, native. Chumworld Arena's duel tick, for comparison: mean 125 µs | 2026-11-04, Apple M2 Max |
| Tick, four seats moving and shooting (M2: one person firing 12 a second, three bots) | `cargo test --release -p tool_core --test tick_probe -- --ignored --nocapture` | mean 7 and 5 µs, 99th percentile 13 and 8 µs, worst 27 and 13 µs over two runs of 3 600 ticks | 2026-11-04, Apple M2 Max |
| WebAssembly size (M2) | printed by `./scripts/build.sh` | 437 KB | 2026-11-04, Apple M2 Max |
| Tick, four seats moving and shooting (M3, the same probe) | `cargo test --release -p tool_core --test tick_probe -- --ignored --nocapture` | mean 8 µs, 99th percentile 18 µs, worst 28 µs over 3 600 ticks | 2026-11-04, Apple M2 Max |
| Tick, four seats with the air full: 96 grenades bouncing, the most it holds (M3) | the same command, second probe | mean 27 µs, 99th percentile 59 µs, worst 90 µs over 3 600 ticks; under the 100 µs target even at its worst | 2026-11-04, Apple M2 Max |

## Not measured

- A frame in WebAssembly in a browser; the online frame (M4).
