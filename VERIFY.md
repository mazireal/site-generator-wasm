# Verification — site-generator-wasm (2026-11-24)

## Practice set (kit practice/, 3 cases)
- Ran `node main.ts` with MAKEFUN_INPUT_DIR/OUTPUT_DIR pointed at the kit practice set.
- Compared per the real scorer (program/scorer/main.ts: canonical key order,
  strings code unit for code unit): **3/3 MATCH**.
- Note: a raw byte `cmp` of answers.json vs expected.json differs only because
  expected.json is a wrapper object and our answers are a plain array with
  sorted object keys; the scorer canonicalises keys, so this is a pass.

## Stress (stress/run.mjs, ours.run vs baseline run)
- 23 hand-crafted edge cases + 3 practice cases + 9 cases drawn from 3 fresh
  generator seeds: **35 cases, 0 failures**.
- Extra sets drawn with program/generator/draw.ts from fresh sha256 seeds
  (wasm-extra-1/2/3), outputs under /tmp/wasm-verify/set1..3.

## Verdict
Correctness is perfect on everything tested. Speed is NOT better than the
current entry (see TIMING.md), so this entry must not replace it.
