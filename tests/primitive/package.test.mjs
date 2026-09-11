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
