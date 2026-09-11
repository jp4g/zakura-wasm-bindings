import assert from 'node:assert/strict';
import fs from 'node:fs';
import {Worker} from 'node:worker_threads';
import {pathToFileURL} from 'node:url';
import {initialize} from './wallet-support.mjs';
const bundle=process.argv[2], fixture=JSON.parse(fs.readFileSync(`${bundle}/tests/views-fixture.json`));
const root=fs.mkdtempSync(`${process.env.WALLET_TEST_ROOT}/views-node-`);
function start(create=false) {
  const worker=new Worker(pathToFileURL(`${bundle}/tests/wallet-views-worker.mjs`),{workerData:{root,create},trackUnmanagedFds:true});
  let token,stopped=false; worker.on('exit',()=>{stopped=true;});
  return {
    async call(op,args={},override={}) {
      return new Promise((resolve,reject)=>{
        const done=(e,v)=>{clearTimeout(timer);worker.off('message',message);worker.off('error',error);worker.off('exit',exit);e?reject(e):resolve(v);};
        const timer=setTimeout(()=>{void worker.terminate();done(Error('deadline'));},30000);
        const message=v=>{if(v.ok&&v.instance)token=v;done(null,v);},error=e=>done(e),exit=n=>done(Error(`unexpected worker exit ${n}`));
        worker.once('message',message);worker.once('error',error);worker.once('exit',exit);
        worker.postMessage(op==='initialize'?{...initialize(),...override}:{op,args,generation:token?.generation,instance:token?.instance,...override});
      });
    },
    async destroy(){await worker.terminate();assert.ok(stopped,'worker exit observed');},
  };
}
let owner=start(true), account,list,token;
try {
  token=await owner.call('initialize');assert.equal(token.ok,true,JSON.stringify(token));
  const imported=await owner.call('account_import',fixture.import);assert.equal(imported.ok,true,JSON.stringify(imported));account=imported.result;
  assert.equal(account.viewOnly,false);assert.equal(account.signerAttached,false);
  const args={accountId:account.id};
  const initial=await owner.call('address_list',args);assert.deepEqual(initial.result,[fixture.defaultAddress]);
  assert.equal((await owner.call('address_current',args)).result,fixture.defaultAddress.address);
  const next=await owner.call('address_next',args);assert.equal(next.ok,true,JSON.stringify(next));
  assert.notEqual(next.result.address,fixture.defaultAddress.address);
  const high=await owner.call('address_at',{...args,index:309485009821345068724781055n,request:{format:'unified',transparent:'omit',sapling:'omit',ironwood:'require'}});
  assert.equal(high.ok,true,JSON.stringify(high));assert.equal(high.result.index,309485009821345068724781055n);
  list=(await owner.call('address_list',args)).result;
  assert.equal((await owner.call('account_list',{}, {generation:token.generation+1})).error,'STALE_HANDLE');
  assert.equal((await owner.call('account_list',{}, {instance:'wrong'})).error,'WRONG_INSTANCE');
  assert.equal((await owner.call('close')).ok,true);
  assert.equal((await owner.call('account_list')).error,'STALE_HANDLE');
}finally{await owner.destroy();}
owner=start();
try {
  assert.equal((await owner.call('initialize')).ok,true);
  assert.deepEqual((await owner.call('account_list')).result,[account]);
  assert.deepEqual((await owner.call('address_list',{accountId:account.id})).result,list);
  assert.equal((await owner.call('account_list',{}, {instance:token.instance})).error,'WRONG_INSTANCE');
  assert.equal((await owner.call('close')).ok,true);
}finally{await owner.destroy();}
console.log(JSON.stringify({pass:true,case:'actual FS account UUID/address close/destroy/reopen; instance/generation',root}));
