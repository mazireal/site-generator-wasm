# Verification — site-generator-wasm (2026-11-24)

## Practice set (kit practice/, 3 cases)
- Ran `node main.ts` end to end with MAKEFUN_INPUT_DIR/OUTPUT_DIR pointed
  at the kit practice set, and compared per the real scorer
  (program/scorer/main.ts: canonical key order, strings code unit for code
  unit): **3/3 MATCH**.
- Note: a raw byte `cmp` of answers.json vs expected.json differs only because
  expected.json is a wrapper object and our answers are a plain array with
  sorted object keys; the scorer canonicalises keys, so this is a pass.

## Stress (stress/run.mjs, ours.run vs baseline run)
- 23 hand-crafted edge cases + 3 practice cases + 18 cases drawn from 6 fresh
  generator seeds: **44 cases, 0 failures**.
- Extra sets drawn with program/generator/draw.ts from fresh sha256 seeds
  (tune1/2/3, final4/5/6), outputs under /tmp/wasm-verify/.

## Constancy cache (stress/constancy.mjs, ours.run vs baseline run)
- 7 targeted cases for the constant-template pre-render: a constant partial
  included at depth 1, one template used as a layout AND included, a
  constant partial reading a loop element, a constant template including a
  missing template, `recent: 0` gating a constant partial, a constant
  template two includes deep, and a constant partial inside a `posts` loop:
  **7 passed, 0 failed**.

## Verdict
Correctness is perfect on everything tested (51 cases, 0 failures), and the
entry is now faster than the current JS entry (see TIMING.md), so it may
replace it.
