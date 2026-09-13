import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdtempSync, mkdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { buildWalletPackage, profile } from './package.mjs';

// Supply the accepted native build and the actual SDK bootstrap bundle.
const scratch = mkdtempSync(join(tmpdir(), 'wallet-package-test-'));
const options = { nativeBuild: process.argv[2], workerPath: process.argv[3], output: join(scratch, 'package') };
const result = await buildWalletPackage(options);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
for (const file of result.manifest.files) {
  const bytes = readFileSync(join(options.output, file.url));
  assert.equal(bytes.length, file.byteLength);
  assert.equal(digest(bytes), file.sha256);
}
assert.deepEqual(result.manifest.files.map(file => file.kind), ['module', 'worker', 'wasm', 'glue', 'glue']);
const api = await import(pathToFileURL(join(options.output, 'wallet.mjs')));
assert.deepEqual(Object.keys(api).sort(), ['consensusContext', 'decodeTransaction', 'initializeWalletRuntime', 'runtimeIdentity', 'viewsForStorage']);
assert.equal(api.runtimeIdentity.buildSha256, digest(readFileSync(join(options.output, 'build.json'))));
assert.equal(api.runtimeIdentity.dependencyGraphSha256, digest(readFileSync(join(options.output, 'dependency-graph.json'))));
assert.equal(api.runtimeIdentity.contractRevision, profile.contractRevision);
assert.deepEqual(api.runtimeIdentity.schemas, result.manifest.schemas);
assert.equal(api.runtimeIdentity.schemas.operations.walletViews, '4');
assert.equal(api.runtimeIdentity.schemas.operations.walletSigner, '2');
assert.equal(api.runtimeIdentity.schemas.operations.walletProposals, '1');
assert.equal(api.runtimeIdentity.schemas.operations.walletScan, '1');
assert.equal(api.runtimeIdentity.schemas.operations.walletSync, '2');
assert.equal(api.runtimeIdentity.schemas.operations.walletEnhancement, '1');
assert.equal(api.runtimeIdentity.schemas.operations.walletQueries, '2');
assert.equal(api.runtimeIdentity.schemas.database, 'wallet-storage/3');
assert.deepEqual(api.runtimeIdentity.memory, { initialPages: 307, maximumPages: 4096, shared: false });
assert.equal(result.manifestSha256, digest(readFileSync(join(options.output, 'manifest.json'))));
await assert.rejects(buildWalletPackage(options), { code: 'EEXIST' });
const bad = join(scratch, 'bad-native'); mkdirSync(bad);
writeFileSync(join(bad, 'build.json'), '{}');
const badOutput = join(scratch, 'bad-output');
await assert.rejects(buildWalletPackage({ ...options, nativeBuild: bad, output: badOutput }), /native receipt mismatch/);
assert.equal(existsSync(badOutput), false);
console.log(JSON.stringify({ pass: true, output: options.output, scope: 'asset closure and identity; supplied worker protocol remains SDK-owned' }));
