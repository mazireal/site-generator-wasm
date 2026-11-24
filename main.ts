// The harness runs `node /bot/main.ts` (node22). It reads every case from
// /input/cases.json (a JSON array of {id, input}), answers each one alone
// with the WebAssembly module behind solve.js, and writes
// /output/answers.json: a JSON array of the answers, one per case and in the
// cases' order. Outside a container MAKEFUN_INPUT_DIR and MAKEFUN_OUTPUT_DIR
// stand in for /input and /output.
//
// The driver is tuned for the timed heat, where every run is a fresh
// process: V8's incremental marking is off (the rules allow an entry to tune
// V8's flags this way), and the raw input bytes go straight to the module
// and the raw answer bytes straight to the file — JavaScript never parses
// or stringifies the big payloads.

import { readFileSync, openSync, writeSync, closeSync } from 'node:fs';
import { join } from 'node:path';
import { setFlagsFromString } from 'node:v8';

// V8's incremental marking off, set before solve.js is imported so all of its
// work runs under it.
setFlagsFromString('--no-incremental-marking');

/** solve.js, imported by its URL (a .js module carries no types) and typed by its contract. */
const { runAll } = (await import(new URL('./solve.js', import.meta.url).href)) as {
  runAll: (input: Uint8Array) => Uint8Array;
};

const INPUT_DIR = process.env.MAKEFUN_INPUT_DIR ?? '/input';
const OUTPUT_DIR = process.env.MAKEFUN_OUTPUT_DIR ?? '/output';

const bytes = readFileSync(join(INPUT_DIR, 'cases.json'));
const out = runAll(bytes);
const fd = openSync(join(OUTPUT_DIR, 'answers.json'), 'w');
writeSync(fd, out);
closeSync(fd);
