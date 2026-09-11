import assert from 'node:assert/strict';
import fs from 'node:fs';
import { start, initialize, parameters } from './wallet-support.mjs';
const bundle = process.argv[2];
const root = fs.mkdtempSync(`${process.env.WALLET_TEST_ROOT}/node-`);
let owner = start(bundle, root, true);
try {
  const opened = await owner.call(initialize());
  assert.equal(opened.ok, true, JSON.stringify(opened));
  assert.deepEqual((await owner.call({ op: 'binding', generation: opened.generation })).bytes, parameters);
  assert.equal((await owner.call({ op: 'close', generation: opened.generation })).ok, true);
} finally { await owner.destroy(); }
assert.ok(fs.statSync(`${root}/wallet.db`).size > 100000);
owner = start(bundle, root);
try {
  const opened = await owner.call(initialize());
  assert.equal(opened.ok, true, JSON.stringify(opened));
  assert.deepEqual((await owner.call({ op: 'binding', generation: opened.generation })).bytes, parameters);
  assert.equal((await owner.call({ op: 'close', generation: opened.generation })).ok, true);
} finally { await owner.destroy(); }
console.log(JSON.stringify({ pass: true, case: 'Node create/migrate/close/destroy/reopen', root }));
