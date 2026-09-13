// Facade admission/receipts; native fixture tests actual construction and persistence.
import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs';
import {copyBytes} from '../bytes.mjs';
let calls=0,after=()=>{},failure,missing=false;
const context=vm.createContext({Object,Array,Number,Uint8Array,TextDecoder,AbortSignal,TypeError,Error,Set,Reflect,JSON,BigInt});
const facade=new vm.SourceTextModule(fs.readFileSync(new URL('../views.mjs',import.meta.url),'utf8'),{context});
await facade.link(async name=>{
  const exports=name==='./wallet.mjs'?{initializeStorage(){throw Error('unused');}}:name==='./bytes.mjs'?{copyBytes}:{pczt_build_call(g,op,input){
    calls++;assert.equal(g,1);assert.deepEqual(JSON.parse(input),op==='pczt_build'?request:{operationId:request.operationId});
    if(failure)throw failure;after();
    return missing?'null':JSON.stringify({bytes:'0001feff',outputs:[{amount:'10000',memo:'00ff'},{amount:'5000',memo:null}],proofsComplete:false,authorizationComplete:false});
  }};
  return new vm.SyntheticModule(Object.keys(exports),function(){for(const [key,value]of Object.entries(exports))this.setExport(key,value);},{context});
});
await facade.evaluate();
const storage={generation:1,instance:{},binding(){},run:fn=>fn()};
const owner=facade.namespace.viewsForStorage(storage),call=(op,args)=>owner.call(1,storage.instance,op,args);
const request={operationId:'01'.repeat(32),proposalId:'02'.repeat(32),reviewCommitment:'03'.repeat(32)};
for(const args of [{...request,proposalId:'bad'},{...request,extra:true},{...request,get reviewCommitment(){throw Error('getter');}}]) {
  assert.throws(()=>call('pczt_build',args),{message:'INVALID_ARGUMENT',commit:'none'});
}
assert.equal(calls,0);
const value=call('pczt_build',request);
assert.deepEqual(value.bytes,new Uint8Array([0,1,254,255]));
assert.equal(value.outputs[0].amount,10000n);assert.deepEqual(value.outputs[0].memo,new Uint8Array([0,255]));
assert.equal(value.outputs[1].memo,null);
missing=true;assert.equal(call('pczt_get_artifact',{operationId:request.operationId}),null);missing=false;
failure='STALE_PROPOSAL';assert.throws(()=>call('pczt_build',request),{message:'STALE_PROPOSAL',commit:'none'});failure=undefined;
const controller=new AbortController();after=()=>controller.abort();
assert.throws(()=>call('pczt_build',{...request,signal:controller.signal}),{message:'ABORTED',commit:'committed'});
after=()=>{};assert.equal(call('pczt_build',request).outputs[0].amount,10000n);
console.log('PCZT build facade admission, byte conversion and commit receipts passed');
