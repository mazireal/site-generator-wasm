# Timing — site-generator-wasm (2026-11-24, this VM, Node v24.22.1)

Method: 9 fresh-process runs each (`node main.ts` on the kit practice set,
3 cases), interleaved round-robin, CPU time = user+sys via getrusage.
Median reported. Null program = empty main.ts in the same runtime.

## Full program (median CPU ms)
| program  | raw | net of null (220 ms) |
|----------|-----|----------------------|
| WASM (tuned) | 557 | 337              |
| v4 (site-generator-fast) | 763 | 543  |
| baseline | 1656 | 1436              |
| null     | 220 | —                   |

- vs baseline: raw 2.97x, net-of-null 4.25x.
- vs v4: raw 1.37x, net-of-null 1.61x — the WASM entry is now FASTER than
  the current JS entry (v5, the current entry, was reported at 755 ms
  full-program, the same ballpark as the v4 measured here).
- Run lists: wasm [524,535,535,536,557,563,574,576,583]; v4
  [723,736,747,751,763,765,773,826,844]; baseline
  [1582,1642,1646,1654,1656,1663,1692,1739,1746]; null
  [204,207,212,214,220,220,235,244,248].

## In-process (median, warmed up)
| program | runAll (bytes in/out) | run x3 (per case, no JSON) |
|---------|----------------------|----------------------------|
| WASM    | 122 ms               | 203 ms                     |
| v4      | 269 ms *             | 108 ms                     |
| baseline| 836 ms *             | 598 ms                     |

\* v4/baseline export no `runAll`; their runAll number is the fallback
(JSON.parse the cases + run per case + JSON.stringify the answers), which is
what their drivers do. The WASM driver's `runAll` parses and serializes
inside the module, so its runAll is the fair driver-path comparison. The
per-case `run` column shows the opposite side: the WASM `run` pays a JS
stringify + parse per case, so it is slower than v4's `run` — but the driver
uses `runAll`, not `run`.

Native (same Rust code compiled for the host, for reference): `run_all`
87 ms (v1: 186 ms), split roughly parse 8 ms / build 69 ms / serialize
10 ms.

## Where the time went, and what fixed it

v1's WASM was slower than the JS entry (954 ms vs 774 ms full-program)
because its compute was slower (235 ms in-process vs 163 ms). Tuning:

1. `Val` in `Rc` (strings/arrays/objects), direct-write template rendering,
   no-copy JSON parse/serialize, moved file contents, ASCII fast paths for
   `cmp_u16`/`slugify`, single-buffer Markdown rendering: native 186 → 107 ms.
2. Constant-template pre-rendering (templates reading only
   `site`/`tags`/`recent`, analysed conservatively): layouts render roughly
   halved per case.
3. Byte-run fast paths in the inline scanner, `push_escaped` and `words_of`:
   native 107 → 87 ms.
4. **The big one — V8's wasm tiering.** In a fresh process the first `runAll`
   cost ~533 ms CPU while warmed calls cost ~157 ms. The gap is TurboFan
   compiling hot functions on background threads mid-call — CPU time the
   heat counts, for code that never runs again. Raising the tiering budget
   past anything one run executes (`--wasm-tiering-budget=1000000000`, set
   with `setFlagsFromString` before importing solve.js, the mechanism the
   rules allow) keeps every function on Liftoff: first call 533 → 265 ms,
   full program 777 → 544 ms. Adding the v4 entry's JS flags
   (`--no-opt --no-lazy --max-semi-space-size=8`) brings it to ~524–557 ms.
   A tiny warmup call does NOT pay (its cost exceeds the tiering it buys);
   `--no-wasm-tier-up` is worse (480 ms first call); eager TurboFan
   (`--no-wasm-tier-up --no-wasm-lazy-compilation`) barely helps (481 ms).
   WASM SIMD (`RUSTFLAGS="-C target-feature=+simd128"`) made no measurable
   difference (528 vs 524 ms full-program); it is kept in the build command
   as harmless.

## Conclusion
SUBMIT: the gate "faster than the current entry" now passes with a real
margin (557 ms vs ~755 ms full-program, 1.37x raw / 1.61x net-of-null).
