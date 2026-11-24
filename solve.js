// site-generator-wasm: makefun-site 1.0.0's semantics, reimplemented in Rust
// and compiled to WebAssembly (src/*.rs in this repository). The .wasm file
// ships in the repository and is loaded from disk: no network, no install
// step, nothing but Node's WebAssembly.
//
// The fast path is runAll: it hands the raw cases.json bytes to the module,
// which parses the JSON, builds every case and returns the answers JSON
// bytes, so JavaScript never JSON.parse/stringifies the big payloads.
// run(input) keeps the module interface the harness expects: one site in,
// its answer out, through the same module.

import { readFileSync } from 'node:fs';

const bytes = readFileSync(new URL('./site.wasm', import.meta.url));
const instance = new WebAssembly.Instance(new WebAssembly.Module(bytes), {});
const { memory, wasm_input_ptr, wasm_run_all, wasm_run_one } = instance.exports;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Call a wasm entry with `inputBytes`, returning a copy of its output bytes. */
function callWasm(fn, inputBytes) {
  const ptr = wasm_input_ptr(inputBytes.length);
  new Uint8Array(memory.buffer).set(inputBytes, ptr);
  const r = fn(inputBytes.length);
  if (r === 0n) throw new Error('wasm run failed');
  const outPtr = Number(r & 0xffffffffn);
  const outLen = Number(r >> 32n);
  return new Uint8Array(memory.buffer, outPtr, outLen).slice();
}

/** The whole cases.json in, the answers JSON out, as bytes. */
export function runAll(inputBytes) {
  return callWasm(wasm_run_all, inputBytes);
}

/** makefun-site 1.0.0's answer for one case's input. */
export function run(input) {
  const inBytes = encoder.encode(JSON.stringify(input));
  const outBytes = callWasm(wasm_run_one, inBytes);
  return JSON.parse(decoder.decode(outBytes));
}
