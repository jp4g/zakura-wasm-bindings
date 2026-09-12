// Facade admission/dispatch only. Native scan correctness uses the real wallet tests.
import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs';
import {copyBytes} from '../bytes.mjs';
let calls=0, received, afterCall=()=>{};
const context=vm.createContext({Object,Array,Number,Uint8Array,AbortSignal,TypeError,Error,Set,Reflect,JSON,BigInt});
const facade=new vm.SourceTextModule(fs.readFileSync(new URL('../views.mjs',import.meta.url),'utf8'),{context});
await facade.link(async name=>{
  const exports=name==='./wallet.mjs'?{initializeStorage:()=>{throw Error('unused');}}:name==='./bytes.mjs'?{copyBytes}:{scan_call:(token,operation,input)=>{calls++;received={token,operation,input:JSON.parse(input)};afterCall();return '{"revision":"native:1"}';},views_call:()=>{throw Error('wrong dispatch');}};
  return new vm.SyntheticModule(Object.keys(exports),function(){for(const [key,value] of Object.entries(exports))this.setExport(key,value);},{context});
});
await facade.evaluate();
const owner={},storage={generation:1,instance:owner,binding:(token,instance)=>{assert.equal(token,1);assert.equal(instance,owner);}};
const views=facade.namespace.viewsForStorage(storage);
const target={height:7,hash:'03'.repeat(32)};
const call=(operation,args)=>views.call(1,owner,operation,args);
assert.equal(call('scan_plan',{target}).revision,'native:1');
assert.deepEqual(received,{token:1,operation:'scan_plan',input:{target:{height:7,hash:'03'.repeat(32)}}});
const batch={target,revision:'native:1',priorTreeState:new Uint8Array([0,255]),blocks:[new Uint8Array([1,2])]};
call('scan_ingest_batch',batch);
assert.deepEqual(received.input.blocks,['0102']);assert.equal(received.input.priorTreeState,'00ff');
const shared=new Uint8Array(new SharedArrayBuffer(1));
const detached=new Uint8Array(1);structuredClone(detached.buffer,{transfer:[detached.buffer]});
const accessor=[];Object.defineProperty(accessor,'0',{get(){throw Error('getter must not run');}});
for(const blocks of [[],Array(17).fill(new Uint8Array(1)),Array(1),accessor,[shared],[detached],[new Uint8Array(1048577),new Uint8Array(1048576)]]) {
  const before=calls;assert.throws(()=>call('scan_ingest_batch',{...batch,blocks}),e=>/INVALID_ARGUMENT|RESOURCE_LIMIT/.test(e.message)&&e.commit==='none');assert.equal(calls,before);
}
for(const args of [{target:{...target,hash:new Uint8Array(31)}},{target:{...target,height:0xffffffff}},{target,blocks:[]},{get target(){throw Error('getter must not run');}}])assert.throws(()=>call('scan_plan',args),/INVALID_ARGUMENT/);
assert.throws(()=>call('account_list',{target}),/INVALID_ARGUMENT/);
afterCall=()=>{throw 'STALE_REVISION';};
assert.throws(()=>call('scan_ingest_batch',batch),e=>e.message==='STALE_REVISION'&&e.commit==='none');
afterCall=()=>{};
assert.equal(call('scan_plan',{target}).revision,'native:1','native rejection leaves owner reusable');
const before=calls, controller=new AbortController();controller.abort();
assert.throws(()=>call('scan_plan',{target,signal:controller.signal}),e=>e.message==='ABORTED'&&e.commit==='none');assert.equal(calls,before);
for(const operation of ['scan_plan','scan_ingest_batch']) {
  const controller=new AbortController();afterCall=()=>controller.abort();
  assert.throws(()=>call(operation,{...(operation==='scan_plan'?{target}:batch),signal:controller.signal}),e=>e.message==='ABORTED'&&e.commit==='committed');
}
afterCall=()=>{throw new WebAssembly.RuntimeError('trap');};
assert.throws(()=>call('scan_plan',{target}),e=>e.message==='DOMAIN_INVALID'&&e.commit===undefined);
afterCall=()=>{};
assert.throws(()=>call('scan_plan',{target}),e=>e.message==='DOMAIN_INVALID'&&e.commit===undefined);
console.log('PASS: bounded scan admission, exact byte lowering, native dispatch and cancellation receipts');
