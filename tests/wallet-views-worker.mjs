// Test driver only: production APIs and real FS/OPFS backend, synthetic fixtures.
const node = typeof process !== 'undefined' && process.versions?.node;
function diagnose(phase, error) {
  if (!node) self.postMessage({diagnostic:{phase,atMs:performance.now(),source:import.meta.url,...(error?{error:{name:error.name,message:error.message??String(error),stack:error.stack}}:{})}});
}
if (!node) {
  self.addEventListener('error',e=>diagnose('worker-error',e.error??e));
  self.addEventListener('unhandledrejection',e=>diagnose('unhandled-rejection',e.reason));
  self.addEventListener('message',()=>diagnose('message-arrived'));
}
diagnose('module-import-start');
const { initializeViews } = await import('../views.mjs').catch(e=>{diagnose('module-error',e);throw e;});
diagnose('module-import-complete');
let port, backend, wasm, options;
if (node) {
  const { parentPort, workerData } = await import('node:worker_threads'); port=parentPort; options=workerData;
  const fs=await import('node:fs'); wasm=new Uint8Array(fs.readFileSync(new URL('../bindings_bg.wasm',import.meta.url)));
} else { port={postMessage:v=>self.postMessage(v)}; }
let owner, attempted=false;
async function receive(request) {
  diagnose(`receive:${request.op}`);
  try {
    if (request.op==='initialize') {
      if (attempted) throw Error('DOMAIN_USED'); attempted=true;
      const {acquire}=await import(node?'../wallet-host/node-fs.mjs':'../wallet-host/opfs.mjs');
      diagnose('backend-acquire-start');
      backend=await acquire(node?options.root:request.root,{create:node?options.create:request.create});
      diagnose('backend-acquire-complete');
      diagnose('wasm-read-start');
      if (!node) {const r=await fetch(new URL('../bindings_bg.wasm',import.meta.url)); if(!r.ok)throw Error('WASM_UNAVAILABLE'); wasm=new Uint8Array(await r.arrayBuffer());}
      diagnose('wasm-read-complete');
      diagnose('initialize-views-start');
      owner=await initializeViews(wasm,backend,request.format,request.parameters,request.genesis);
      diagnose('initialize-views-complete');
      port.postMessage({ok:true,generation:owner.generation,instance:owner.instance,secure:!node&&isSecureContext});
    } else if (request.op==='close') {
      owner.close(request.generation,request.instance); port.postMessage({ok:true});
    } else {
      const args=request.args??{}, controller=new AbortController();
      if(request.abort){args.signal=controller.signal;if(request.abort==='before')controller.abort();}
      const sync=backend.sync,write=backend.write;
      if(request.abort==='duringSync')backend.sync=(...a)=>{const result=sync(...a);controller.abort();return result;};
      if(request.fault==='commit'){let calls=0;backend.sync=(...a)=>{if(++calls===2)throw Object.assign(Error('synthetic commit sync fault'),{code:'EIO'});return sync(...a);};}
      const cryptoObject=globalThis.crypto,random=cryptoObject.getRandomValues;
      if(request.fault==='entropy')cryptoObject.getRandomValues=()=>{throw Error('synthetic entropy unavailable');};
      if(request.fault==='quota')backend.write=()=>{throw Object.assign(Error('synthetic quota fault'),{code:'ENOSPC'});};
      try {const result=owner.call(request.generation,request.instance,request.op,args);port.postMessage({ok:true,result});}
      finally {backend.sync=sync;backend.write=write;cryptoObject.getRandomValues=random;}
    }
  } catch (e) { diagnose(`operation-error:${request.op}`,e); port.postMessage({ok:false,error:typeof e==='string'?e:e.code==='EBUSY'||e.name==='NoModificationAllowedError'?'STORAGE_BUSY':e.message,commit:e.commit}); }
}
if(node)port.on('message',receive); else self.onmessage=e=>receive(e.data);
diagnose('handler-installed');
