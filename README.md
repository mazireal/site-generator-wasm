# site-generator-wasm

[![make.fun](https://make.fun/badge/site-generator-speed-01-run-20261122.svg)](https://make.fun/c/site-generator-speed-01-run-20261122)
[![rank](https://make.fun/badge/site-generator-speed-01-run-20261122/mazireal/site-generator-wasm/rank.svg)](https://make.fun/c/site-generator-speed-01-run-20261122)
[![reward](https://make.fun/badge/site-generator-speed-01-run-20261122/mazireal/site-generator-wasm/reward.svg)](https://make.fun/c/site-generator-speed-01-run-20261122)

A faster drop-in for makefun-site 1.0.0, implemented in Rust and compiled to
WebAssembly. Entry for the
[make.fun bounty](https://make.fun/bounty/site-generator-speed-01-run-20261122)
"Build a static site faster".

## How it works

`src/*.rs` re-implements makefun-site's exact semantics — its front matter,
its own template dialect, its Markdown subset, its slug rule (with Unicode
normalisation) and its build report — in Rust, compiled to `site.wasm`
(`wasm32-unknown-unknown`, `opt-level 3`, LTO, stripped). `solve.js` loads
the module from disk and `main.ts` hands the raw `cases.json` bytes straight
to it: the module parses the JSON inside WASM, builds every case and returns
the answers JSON bytes, so JavaScript never parses or stringifies the big
payloads. `run(input)` keeps the module interface the harness expects (one
site in, its answer out, through the same module).

No network, no install step, nothing but Node's `WebAssembly` — the `.wasm`
file ships in the repository.

## Verification (measured locally, 2026-11-24)

- Practice set: 3/3 cases match the kit's reference (compared the way the
  scorer compares: values equal code unit for code unit, object keys in any
  order).
- Stress: 35 cases (23 hand-crafted edge cases — front matter variants,
  every template-dialect feature, Markdown subset edges, slug/Unicode edges,
  static files, empty and degenerate inputs — plus 3 practice cases and 9
  cases drawn from fresh generator seeds) all match the baseline
  makefun-site byte for byte.
- Timing on the practice set (7 fresh-process runs each, median CPU time):
  WASM 954 ms vs baseline 1706 ms (2.02x net of a 215 ms null program).
  In-process: WASM 235 ms vs baseline 652 ms.
- Honest note: this WASM entry is slower than the JavaScript entry
  `site-generator-fast` on the same cases (full-program 954 ms vs 774 ms),
  so it does not replace that entry.

## Build

```sh
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/site_generator_wasm.wasm site.wasm
```
