import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
const out = process.argv[2];
const { initialize, consensusContext } = await import(pathToFileURL(`${out}/network.mjs`));
for (const value of [undefined, './bindings_bg.wasm', {}, new Uint8Array(0), new Uint8Array(1048577)]) {
  assert.throws(() => initialize(value), /invalid wasm bytes/);
}
await initialize(new Uint8Array(await readFile(`${out}/bindings_bg.wasm`)));
const bytes = new TextEncoder().encode('{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}');
assert.deepEqual(consensusContext('zcash-js-network/1', bytes, 20), { height: 20, branchId: 0x76b809bb });
for (const height of [-1, 4294967296, 1.1, NaN, Infinity, '20', 20n, null, {}, -Infinity]) {
  assert.throws(() => consensusContext('zcash-js-network/1', bytes, height), /invalid height/);
}
for (const input of [[], {}, new Uint16Array(10), new DataView(bytes.buffer), null, new Uint8Array(257)]) {
  assert.throws(() => consensusContext('zcash-js-network/1', input, 20), /invalid parameters/);
}
assert.throws(() => consensusContext('unknown', bytes, 20), /unsupported network format/);
const detached = bytes.slice(); structuredClone(detached.buffer, { transfer: [detached.buffer] });
assert.throws(() => consensusContext('zcash-js-network/1', detached, 20), /invalid parameters/);
assert.throws(() => consensusContext('zcash-js-network/1', new Uint8Array(new SharedArrayBuffer(10)), 20), /invalid parameters/);
console.log('Node generated binding: 25 checks passed');
