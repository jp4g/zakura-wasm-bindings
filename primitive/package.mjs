// Offline packager: committed private JS + exact accepted generated packet.
import { readFileSync, readdirSync, mkdtempSync, openSync, closeSync, rmSync, mkdirSync, writeFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { rolldown, VERSION } from '/home/jack/zcash.js/node_modules/rolldown/dist/index.mjs';
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const scratch = '/home/jack/zcash-primitive-loader-scratch';
export const sha = b => createHash('sha256').update(b).digest('hex');
const requireThat = (ok, why) => { if (!ok) throw Error(why); };
const canonical = x => x && typeof x === 'object' ? Array.isArray(x) ? '[' + x.map(canonical).join(',') + ']'
  : '{' + Object.keys(x).sort().map(k => JSON.stringify(k) + ':' + canonical(x[k])).join(',') + '}' : JSON.stringify(x);
function git(...args) {
  const temp = mkdtempSync(join(scratch, 'git-')), path = join(temp, 'out'), fd = openSync(path, 'wx');
  try { execFileSync('git', ['-C', root, ...args], { stdio: ['ignore', fd, 'inherit'] }); return readFileSync(path); }
  finally { closeSync(fd); rmSync(temp, { recursive: true }); }
}
export async function buildPrimitive(packet, output) {
  const revision = git('rev-parse', 'HEAD').toString().trim();
  const source = {};
  for (const name of ['entry.mjs', 'worker.mjs', 'package.mjs']) {
    const bytes = git('show', `${revision}:primitive/${name}`);
    requireThat(bytes.equals(readFileSync(join(root, 'primitive', name))), 'uncommitted producer source');
    source[name] = bytes;
  }
  const raw = readFileSync(join(packet, 'build.json'));
  requireThat(sha(raw) === '09ae852de689eb47fba35dfefaae81397d280f5c2542bccdee9747954efad575', 'accepted metadata mismatch');
  const upstream = JSON.parse(raw), inputs = {};
  requireThat(upstream.revision === 'acaf7069e466c82ce1be2c45ebafb75a92b59ee9', 'accepted revision mismatch');
  requireThat(git('rev-parse', `${upstream.revision}^{tree}`).toString().trim() === upstream.sourceTree, 'accepted source tree');
  requireThat(sha(git('show', `${upstream.revision}:Cargo.lock`)) === upstream.lockSha256, 'accepted lock');
  requireThat(JSON.stringify(readdirSync(packet).sort()) === JSON.stringify(['build.json', ...Object.keys(upstream.files)].sort()), 'packet inventory');
  for (const [name, expected] of Object.entries(upstream.files)) {
    const bytes = readFileSync(join(packet, name));
    requireThat(bytes.length === expected.bytes && sha(bytes) === expected.sha256, `packet member ${name}`);
    inputs[name] = bytes;
  }
  for (const name of ['network.mjs', 'transaction.mjs', 'bytes.mjs']) requireThat(inputs[name].equals(git('show', `${upstream.revision}:${name}`)), 'checked entry source');
  const virtual = { ...inputs, 'entry.mjs': source['entry.mjs'] };
  const build = await rolldown({ input: 'entry.mjs', cwd: '/primitive', platform: 'neutral',
    plugins: [{ name: 'verified-inputs', resolveId(id) {
      const name = id.replace(/^\.\//, ''); requireThat(Object.hasOwn(virtual, name), 'unlisted import'); return name;
    }, load(id) { return virtual[id].toString(); } }],
    onLog(level, log) { throw Error(`bundler ${level}: ${log.code}`); } });
  let chunk;
  try {
    const result = await build.generate({ format: 'es', comments: false });
    requireThat(result.output.length === 1, 'one bundle required'); chunk = result.output[0];
    requireThat(chunk.type === 'chunk' && !chunk.imports.length && !chunk.dynamicImports.length, 'closed entry required');
    requireThat(!/\b(fetch|import|eval|Function)\s*\(/.test(chunk.code), 'ambient loading in bundle');
  } finally { await build.close(); }
  const assets = { 'primitive.mjs': Buffer.from(chunk.code), 'worker.mjs': source['worker.mjs'], 'bindings_bg.wasm': inputs['bindings_bg.wasm'] };
  const compilerRoot = '/home/jack/zcash.js/node_modules';
  const compiler = {};
  for (const directory of ['rolldown', '@rolldown/binding-linux-x64-gnu']) {
    for (const name of readdirSync(join(compilerRoot, directory), { recursive: true }).sort()) {
      const path = join(compilerRoot, directory, name);
      if (name.endsWith('.mjs') || name.endsWith('.node') || name === 'package.json') compiler[directory + '/' + name] = sha(readFileSync(path));
    }
  }
  const provenance = { format: 'zakura-private-primitive-build/1', revision, sourceTree: git('rev-parse', `${revision}^{tree}`).toString().trim(),
    source: Object.fromEntries(Object.entries(source).map(([k, v]) => [k, sha(v)])),
    acceptedRevision: upstream.revision, acceptedBuildSha256: sha(raw), node: process.version,
    compiler: { name: 'rolldown', version: VERSION, files: compiler },
    moduleInputs: Object.keys(chunk.modules).sort(), dependencyGraphSha256: upstream.dependencyGraphSha256 };
  const metadata = Buffer.from(canonical(provenance));
  const manifest = { format: 'zcash-artifact/1', contractRevision: 'zakura-private-primitive/1', abiVersion: 'checked-bindgen-0.2.128/1',
    schemas: { operations: { consensusContext: '1', decodeTransaction: '1' }, protobuf: 'not-used',
      networkParameters: 'zcash-js-network/1', database: 'not-used', hostServices: {} },
    buildSha256: sha(metadata), dependencyGraphSha256: upstream.dependencyGraphSha256, mode: 'baseline',
    files: Object.entries(assets).map(([url, bytes]) => ({ url, byteLength: bytes.length, sha256: sha(bytes),
      kind: url === 'primitive.mjs' ? 'module' : url === 'worker.mjs' ? 'worker' : 'wasm',
      mediaType: url.endsWith('.wasm') ? 'application/wasm' : 'text/javascript' })) };
  const manifestBytes = Buffer.from(canonical(manifest));
  mkdirSync(output); // Exclusive destination, no partial success receipt.
  for (const [name, bytes] of Object.entries({ ...assets, 'build.json': metadata, 'manifest.json': manifestBytes })) writeFileSync(join(output, name), bytes, { flag: 'wx' });
  return { revision, buildSha256: sha(metadata), manifestSha256: sha(manifestBytes), files: manifest.files };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(JSON.stringify(await buildPrimitive(process.argv[2], process.argv[3]), null, 2));
}
