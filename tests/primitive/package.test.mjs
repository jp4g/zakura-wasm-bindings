import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Worker } from 'node:worker_threads';
const scratch = '/home/jack/zcash-primitive-loader-scratch';
const producer = new URL('../../primitive/package.mjs', import.meta.url);
test('packages real accepted checked entries into a closed module and executable worker', async () => {
  assert.ok(existsSync(producer), 'primitive producer must exist');
  const { buildPrimitive } = await import(producer);
  const root = mkdtempSync(join(scratch, 'package-test-'));
  const out = join(root, 'packet');
  const result = await buildPrimitive('/home/jack/zakura-transaction-bindings-scratch/packet-final', out);
  const manifest = JSON.parse(readFileSync(join(out, 'manifest.json')));
  assert.equal(manifest.contractRevision, 'zakura-private-primitive/1');
  assert.deepEqual(manifest.files.map(f => f.kind).sort(), ['module', 'wasm', 'worker']);
  assert.ok(!/\bfetch\s*\(|\bimport\s*\(/.test(readFileSync(join(out, 'primitive.mjs'), 'utf8')));
  const worker = new Worker(pathToFileURL(join(out, 'worker.mjs')));
  const receive = () => new Promise((resolve, reject) => { worker.once('message', resolve); worker.once('error', reject); });
  try {
    const ready = receive();
    worker.postMessage({ type: 'init', moduleUrl: pathToFileURL(join(out, 'primitive.mjs')).href,
      wasm: new Uint8Array(readFileSync(join(out, 'bindings_bg.wasm'))) });
    assert.deepEqual(await ready, { type: 'ready' });
    const answer = receive();
    worker.postMessage({ type: 'context', id: 1, format: 'zcash-js-network/1', bytes: new TextEncoder().encode('{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}'), value: 20 });
    assert.deepEqual(await answer, { type: 'result', id: 1, result: { height: 20, branchId: 1991772603 } });
    assert.match(result.manifestSha256, /^[a-f0-9]{64}$/);
  } finally { await worker.terminate(); }
});

test('packet corruption and inventory additions leave no executable destination', async () => {
  const { buildPrimitive } = await import(producer);
  const { cpSync, writeFileSync } = await import('node:fs');
  const root = mkdtempSync(join(scratch, 'package-negative-'));
  for (const member of ['build.json', 'bindings.js', 'bindings_bg.wasm', 'network.mjs', 'transaction.mjs', 'bytes.mjs', 'extra.mjs']) {
    const packet = join(root, member + '-input'), out = join(root, member + '-out');
    cpSync('/home/jack/zakura-transaction-bindings-scratch/packet-final', packet, { recursive: true });
    writeFileSync(join(packet, member), 'invalid');
    await assert.rejects(buildPrimitive(packet, out));
    assert.equal(existsSync(out), false);
  }
});

test('actual diagnostic WASM trap poisons the real wrapper; later requests never execute', async () => {
  // Fault injection only: real accepted initialization and codec, then an actual
  // WASM unreachable instruction. This is not evidence of a Rust panic trigger.
  const { writeFileSync } = await import('node:fs');
  const packet = join(scratch, 'packet-63d08ed');
  const root = mkdtempSync(join(scratch, 'trap-test-'));
  const fixture = join(root, 'fault.mjs');
  writeFileSync(fixture, `import * as real from ${JSON.stringify(pathToFileURL(join(packet, 'primitive.mjs')).href)};
export const initialize = real.initialize;
export const consensusContext = real.consensusContext;
export function decodeTransaction(raw, branch) {
  real.decodeTransaction(raw, branch);
  const module = new WebAssembly.Module(Uint8Array.from([0,97,115,109,1,0,0,0,1,4,1,96,0,0,3,2,1,0,7,8,1,4,116,114,97,112,0,0,10,5,1,3,0,0,11]));
  new WebAssembly.Instance(module).exports.trap();
}`);
  const worker = new Worker(pathToFileURL(join(packet, 'worker.mjs')));
  const messages = []; worker.on('message', m => messages.push(m));
  const next = () => new Promise((resolve, reject) => { worker.once('message', resolve); worker.once('error', reject); });
  try {
    const ready = next(); worker.postMessage({ type: 'init', moduleUrl: pathToFileURL(fixture).href, wasm: new Uint8Array(readFileSync(join(packet, 'bindings_bg.wasm'))) });
    assert.deepEqual(await ready, { type: 'ready' });
    const vector = JSON.parse(readFileSync(new URL('../transaction-vectors.json', import.meta.url)))[0];
    const trapped = next(); worker.postMessage({ type: 'transaction', id: 1, bytes: new Uint8Array(Buffer.from(vector.hex, 'hex')), value: vector.branch });
    assert.deepEqual(await trapped, { type: 'fatal' });
    worker.postMessage({ type: 'context', id: 2, format: 'zcash-js-network/1', bytes: Uint8Array.of(1), value: 0 });
    await new Promise(resolve => setTimeout(resolve, 30));
    assert.equal(messages.length, 2);
  } finally { await worker.terminate(); }
  assert.equal(worker.threadId, -1);
});
