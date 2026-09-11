import { initSync, lightwire_encode, lightwire_decode } from './wasm/zakura_lightwire.js';

const MAX_MESSAGE = 4 * 1024 * 1024, MAX_FIELD = 1024 * 1024, MAX_ITEMS = 8192, MAX_DEPTH = 16;
const bufferLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get;
const typed = Object.getPrototypeOf(Uint8Array.prototype);
const typedBuffer = Object.getOwnPropertyDescriptor(typed, 'buffer').get;
const typedLength = Object.getOwnPropertyDescriptor(typed, 'byteLength').get;
const typedOffset = Object.getOwnPropertyDescriptor(typed, 'byteOffset').get;
const typedTag = Object.getOwnPropertyDescriptor(typed, Symbol.toStringTag).get;
const descriptor = Object.getOwnPropertyDescriptor;
const prototype = Object.getPrototypeOf;
const stringify = JSON.stringify, parse = JSON.parse;
const unary = new Set(['GetLatestBlock','GetLightdInfo','GetTransaction','GetAddressUtxos','GetTaddressBalance','GetTreeState','SendTransaction']);
const stream = new Set(['GetSubtreeRoots','GetBlockRange','GetTaddressTransactions','GetMempoolStream']);
let initialized = false;
function bytes(value, max) {
  // Intrinsic getters reject proxies and disguised shared backing stores before generated glue.
  if (typedTag.call(value) !== 'Uint8Array') throw new TypeError('expected Uint8Array');
  const buffer = typedBuffer.call(value), size = typedLength.call(value), offset = typedOffset.call(value);
  bufferLength.call(buffer);
  if (size > max) throw new RangeError('byte limit');
  return new Uint8Array(new Uint8Array(buffer, offset, size));
}
function method(name, kind) {
  if (typeof name !== 'string' || !(kind === 'item' ? stream.has(name) : kind === 'response' ? unary.has(name) : unary.has(name) || stream.has(name))) throw new TypeError('unknown or wrong-kind method');
  return name;
}
function dto(value) {
  let items = 0, chars = 0;
  const seen = new Set();
  function copy(v, depth) {
    if (++items > MAX_ITEMS || depth > MAX_DEPTH) throw new RangeError('DTO resource limit');
    if (typeof v === 'string') {
      chars += v.length;
      if (v.length > 2 * MAX_FIELD || chars > 2 * MAX_MESSAGE) throw new RangeError('DTO string limit');
      // Reject lone surrogates rather than allowing JSON to normalize ill-formed Unicode.
      if (!v.isWellFormed()) throw new TypeError('invalid Unicode');
      return v;
    }
    if (v === null || typeof v === 'boolean') return v;
    if (typeof v === 'number' && Number.isSafeInteger(v) && !Object.is(v, -0)) return v;
    if (typeof v !== 'object' || v === null || seen.has(v)) throw new TypeError('invalid DTO value');
    const array = Array.isArray(v), proto = prototype(v);
    if (array ? proto !== Array.prototype : proto !== Object.prototype && proto !== null) throw new TypeError('expected plain DTO record');
    seen.add(v);
    const length = array ? descriptor(v, 'length').value : 0;
    if (array && length > MAX_ITEMS) throw new RangeError('DTO array limit');
    const keys = Reflect.ownKeys(v);
    if (keys.length > MAX_ITEMS) throw new RangeError('DTO key limit');
    const result = array ? [] : Object.create(null);
    if (array && (keys.length !== length + 1)) throw new TypeError('expected dense DTO array');
    for (const key of keys) {
      if (array && key === 'length') continue;
      const p = descriptor(v, key);
      if (!p) throw new TypeError('DTO source changed');
      if (typeof key !== 'string' || !p.enumerable || !Object.hasOwn(p, 'value') || (array && (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= length))) throw new TypeError('invalid DTO property');
      if (key.length > 128) throw new RangeError('DTO key limit');
      result[key] = copy(p.value, depth + 1);
    }
    seen.delete(v);
    return result;
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new TypeError('expected DTO record');
  const text = stringify(copy(value, 0));
  if (text.length > MAX_MESSAGE * 2) throw new RangeError('DTO size limit');
  return text;
}
/** One synchronous, module-owned stateless codec. All operation bytes/results are owned copies. */
export function createLightwire(wasmBytes) {
  if (initialized) throw new Error('lightwire module already initialized');
  const owned = bytes(wasmBytes, 16 * 1024 * 1024);
  initSync({ module: owned });
  initialized = true;
  return Object.freeze({
    encodeRequest(name, request) {
      method(name, 'request');
      return lightwire_encode(name, dto(request));
    },
    decodeResponse(name, payload) {
      method(name, 'response');
      return parse(lightwire_decode(name, bytes(payload, MAX_MESSAGE), false));
    },
    decodeItem(name, payload) {
      method(name, 'item');
      return parse(lightwire_decode(name, bytes(payload, MAX_MESSAGE), true));
    },
  });
}
