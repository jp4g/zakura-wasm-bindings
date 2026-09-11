// Internal generated glue is only called after admission. Initialization must
// receive already-verified WASM bytes; it is not host/runtime negotiation.
import { initSync, consensus_branch } from './bindings.js';
const typedArray = Object.getPrototypeOf(Uint8Array.prototype);
const byteLength = Object.getOwnPropertyDescriptor(typedArray, 'byteLength').get;
const buffer = Object.getOwnPropertyDescriptor(typedArray, 'buffer').get;
const tag = Object.getOwnPropertyDescriptor(typedArray, Symbol.toStringTag).get;
const arrayBufferByteLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get;

function copyBytes(value, limit, message) {
  if (!(value instanceof Uint8Array) || !ArrayBuffer.isView(value) ||
      tag.call(value) !== 'Uint8Array' || !(buffer.call(value) instanceof ArrayBuffer) ||
      byteLength.call(value) === 0 || byteLength.call(value) > limit) {
    throw new TypeError(message);
  }
  try {
    arrayBufferByteLength.call(buffer.call(value));
  } catch {
    throw new TypeError(message);
  }
  return new Uint8Array(value);
}

export function initialize(wasmBytes) {
  // This bounded binding accepts verified bytes only, never a URL/default fetch.
  initSync({ module: copyBytes(wasmBytes, 1048576, 'invalid wasm bytes') });
}

export function consensusContext(parametersFormat, parameters, height) {
  if (parametersFormat !== 'zcash-js-network/1') throw new TypeError('unsupported network format');
  if (!Number.isInteger(height) || height < 0 || height > 0xffffffff) throw new TypeError('invalid height');
  const owned = copyBytes(parameters, 256, 'invalid parameters');
  const branchId = consensus_branch(parametersFormat, owned, height);
  return Object.freeze({ height, branchId });
}
