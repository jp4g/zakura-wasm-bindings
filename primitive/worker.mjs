// Fixed transport bootstrap. The owner supplies only its verified file/Blob URL.
const node = typeof process !== 'undefined' && !!process.versions?.node;
const port = node ? (await import('node:worker_threads')).parentPort : globalThis;
const send = (message, transfers = []) => port.postMessage(message, transfers);
let state = 'new', api, lastId = 0;
function exact(value, keys) {
  return value && Object.getPrototypeOf(value) === Object.prototype &&
    Reflect.ownKeys(value).length === keys.length && keys.every(k => Object.hasOwn(value, k));
}
function fatal() { state = 'closed'; send({ type: 'fatal' }); }
async function receive(message) {
  if (state === 'closed') return;
  if (state === 'new') {
    state = 'starting';
    try {
      if (!exact(message, ['type', 'moduleUrl', 'wasm']) || message.type !== 'init' ||
          typeof message.moduleUrl !== 'string' || !message.moduleUrl.startsWith(node ? 'file:' : 'blob:')) throw Error();
      api = await import(message.moduleUrl);
      if (state !== 'starting') return;
      api.initialize(message.wasm);
      message.wasm.fill(0);
      state = 'ready'; send({ type: 'ready' });
    } catch { fatal(); }
    return;
  }
  if (state !== 'ready' || !exact(message, message?.type === 'context'
      ? ['type', 'id', 'format', 'bytes', 'value'] : ['type', 'id', 'bytes', 'value']) ||
      !Number.isSafeInteger(message.id) || message.id <= lastId ||
      !['context', 'transaction'].includes(message.type)) { fatal(); return; }
  lastId = message.id;
  try {
    const result = message.type === 'context'
      ? api.consensusContext(message.format, message.bytes, message.value)
      : api.decodeTransaction(message.bytes, message.value);
    send({ type: 'result', id: message.id, result }, message.type === 'transaction'
      ? [result.bytes.buffer, result.txid.buffer] : []);
  } catch (error) {
    // Checked-entry admission and Rust Result strings are recoverable. Traps and
    // unexpected exceptions permanently poison this stateless lifetime domain.
    if (typeof error === 'string' || error instanceof TypeError) send({ type: 'invalid', id: message.id });
    else fatal();
  }
}
if (node) port.on('message', receive);
else port.onmessage = event => { void receive(event.data); };
