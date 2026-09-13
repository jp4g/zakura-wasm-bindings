import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
const bytes = fs.readFileSync(`${process.argv[2]}/bindings_bg.wasm`);
const module = new WebAssembly.Module(bytes);
const imports = WebAssembly.Module.imports(module);
// time's pinned wasm-bindgen clock uses Date construction and getTime.
const generated = ['__wbg_getTime_65922ba0b59d55a7', '__wbg_new_0_35540e542ba689d2', '__wbg_getRandomValues_a678b7300e8ed57f', '__wbg_getRandomValues_436a51d0629d84e1', '__wbg___wbindgen_throw_5d9e815e6fdf150f', '__wbindgen_init_externref_table', '__wbindgen_generic_0000000000000001'];
const host = ['entropy', 'utc_ms', 'sleep', 'host_error', 'file_open', 'file_close', 'file_read', 'file_write', 'file_truncate', 'file_sync', 'file_size', 'file_lock', 'file_unlock', 'file_reserved', 'file_delete', 'file_access'];
for (const entry of imports) {
  assert.equal(entry.kind, 'function');
  assert.ok(entry.module === './bindings_bg.js' && generated.includes(entry.name) || entry.module === './wallet-host/storage-host.mjs' && host.includes(entry.name), JSON.stringify(entry));
}
// Read the memory limits from these same validated module bytes.
let at = 8, limits;
function uint() { let n = 0, shift = 0, byte; do { byte = bytes[at++]; n += (byte & 127) * 2 ** shift; shift += 7; assert.ok(shift <= 35); } while (byte & 128); return n; }
while (at < bytes.length) {
  const id = bytes[at++], length = uint(), end = at + length;
  if (id === 5) {
    assert.equal(uint(), 1, 'one memory');
    const flags = uint(), initial = uint();
    assert.equal(flags, 1, 'nonshared memory must have an explicit maximum');
    limits = { initial, maximum: uint() }; assert.ok(limits.maximum <= 4096, '256 MiB cap');
  }
  at = end;
}
assert.ok(limits);
console.log(JSON.stringify({ pass: true, sha256: createHash('sha256').update(bytes).digest('hex'), imports, exports: WebAssembly.Module.exports(module), memoryPages: limits }));
