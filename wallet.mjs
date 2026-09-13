// Private worker-local primitive. The future H1 owner supplies verified bytes and
// a validated genesis/parameter registration. This is not a public WalletClient.
import { initSync } from './bindings.js';
import * as binding from './bindings.js';
import { copyBytes } from './bytes.mjs';
import * as host from './wallet-host/storage-host.mjs';
let attempted = false;
function uint(value) {
  if (!Number.isInteger(value) || value < 1 || value > 0xffffffff) throw TypeError('INVALID_ARGUMENT');
  return value;
}
export async function initializeStorage(wasm, backend, format, parameters, genesis) {
  // Preserve pre-initialization admission for existing worker-local consumers.
  const { params, identity } = network(format, parameters, genesis);
  return initializeWalletRuntime(wasm).open(backend, format, params, identity);
}
function network(format, parameters, genesis) {
  const params = copyBytes(parameters, 256, 'invalid parameters');
  const identity = copyBytes(genesis, 32, 'invalid genesis');
  if (identity.length !== 32 || format !== 'zcash-js-network/1') throw TypeError('INVALID_ARGUMENT');
  return { params, identity };
}

/** Initialize only verified code and worker-local SQLite, without a storage lease.
 * The host can finish compatibility negotiation before acquiring/opening a DB.
 */
export function initializeWalletRuntime(wasm) {
  const code = copyBytes(wasm, 32 * 1024 * 1024, 'invalid wasm bytes');
  if (attempted) throw Error('DOMAIN_USED');
  attempted = true;
  const instance = crypto.randomUUID();
  const exports = initSync({ module: code });
  host.attachMemory(exports.memory);
  if (exports.wallet_pool_start() + exports.wallet_pool_size() > exports.__heap_base.value) throw Error('allocator overlap');
  if (exports.wallet_runtime_init() !== 0) throw Error('RUNTIME_UNAVAILABLE');
  let invalid = false;
  const run = (backend, fn) => {
    if (invalid) throw Error('DOMAIN_INVALID');
    try { return host.withBackend(backend, fn); }
    catch (error) { if (typeof error !== 'string' || error === 'STORAGE_CLOSE_FAILED') invalid = true; throw error; }
  };
  return Object.freeze({
    get invalid() { return invalid; },
    signers: Object.freeze({
      describe(token) { uint(token); return JSON.parse(run(undefined, () => binding.signer_describe(token))); },
      release(token) { uint(token); return run(undefined, () => binding.signer_release(token)); },
    }),
    openMemory(format, parameters, genesis) {
      const { params, identity } = network(format, parameters, genesis);
      return openStorage(undefined, instance, format, params, identity, run);
    },
    open(backend, format, parameters, genesis) {
      const { params, identity } = network(format, parameters, genesis);
      host.attachBackend(backend);
      return openStorage(backend, instance, format, params, identity, run);
    },
  });
}
function openStorage(backend, instance, format, params, identity, execute) {
  const run = fn => execute(backend, fn);
  const generation = run(() => backend === undefined
    ? binding.storage_initialize_memory(format, params, identity)
    : binding.storage_initialize(format, params, identity));
  let closed = false, closeError;
  return {
    generation, instance, run,
    bindSigner(token, accountId) { uint(token); if (typeof accountId !== 'string' || accountId.length !== 36) throw TypeError('INVALID_ARGUMENT'); if (closed) throw Error('STALE_HANDLE'); return run(() => binding.signer_bind(token, generation, accountId)); },
    unbindSigner(token, accountId) { uint(token); if (typeof accountId !== 'string' || accountId.length !== 36) throw TypeError('INVALID_ARGUMENT'); if (closed) throw Error('STALE_HANDLE'); return run(() => binding.signer_unbind(token, generation, accountId)); },
    binding(token, owner) {
      uint(token);
      if (owner !== instance) throw Error('WRONG_INSTANCE');
      if (token !== generation) throw Error('STALE_HANDLE');
      if (closed) throw Error('STALE_HANDLE');
      try { return run(() => new Uint8Array(binding.storage_binding(token))); }
      catch (error) { if (typeof error !== 'string') { closed = true; closeError = Error('DOMAIN_INVALID'); } throw error; }
    },
    close(token, owner) {
      uint(token);
      if (owner !== instance) throw Error('WRONG_INSTANCE');
      if (token !== generation) throw Error('STALE_HANDLE');
      if (closed) { if (closeError) throw closeError; return; }
      closed = true;
      try {
        run(() => {
          binding.storage_close(token);
          // SQLite may ignore an xClose error. The owner-local host latch is authoritative too.
          if (host.state.closeError) throw Error('STORAGE_CLOSE_FAILED');
          if (backend) backend.release();
        });
      } catch { closeError = Error('STORAGE_CLOSE_FAILED'); throw closeError; }
    },
  };
}
