# Timing — site-generator-wasm (2026-11-24, this VM, Node v24.22.1)

Method: 7 fresh-process runs each (`node main.ts` on the kit practice set,
3 cases), CPU time = user+sys via bash `time`. Median reported.
Null program = empty main.ts in the same runtime.

## Full program (median CPU ms)
| program  | raw | net of null (215 ms) |
|----------|-----|----------------------|
| WASM     | 954 | 739                  |
| baseline | 1706| 1491                 |
| v3 (site-generator-fast) | 774 | 559          |

- vs baseline: raw 1.79x, net-of-null 2.02x.
- vs v3: raw 0.81x, net-of-null 0.76x — WASM is SLOWER than v3.
- Run lists: wasm [871,899,907,954,961,967,1070]; baseline
  [1499,1562,1598,1706,1747,1772,1781]; v3 [750,751,758,774,815,823,825];
  null [208,212,214,215,221,223,227].

## In-process (median CPU ms, 5 runs, warmed up)
| program | ms |
|---------|----|
| WASM runAll (3 cases) | 235 |
| baseline run x3 | 652 |
| v3 run x3 | 163 |

WASM compile+instantiate of the 452 KB module costs ~2 ms — the gap is
compute, not startup: the WASM JSON parse + generation (235 ms) is slower
than v3's optimised JS (163 ms). The current entry is v4 (faster than v3),
so the gap to the entry that would be replaced is larger still.

## Conclusion
Do NOT submit: the gate "faster than the current entry" fails.
