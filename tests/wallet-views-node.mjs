import assert from 'node:assert/strict';
import fs from 'node:fs';
import {Worker} from 'node:worker_threads';
import {pathToFileURL} from 'node:url';
import {initialize} from './wallet-support.mjs';
const bundle=process.argv[2], fixture=JSON.parse(fs.readFileSync(`${bundle}/tests/views-fixture.json`));
const show=v=>JSON.stringify(v,(_,x)=>typeof x==='bigint'?x.toString():x);
for(const k of ['parameters','genesis','priorTreeState'])fixture.import.birthday[k]=Uint8Array.from(fixture.import.birthday[k].match(/../g),b=>parseInt(b,16));
fixture.defaultAddress.index=BigInt(fixture.defaultAddress.index);
const root=fs.mkdtempSync(`${process.env.WALLET_TEST_ROOT}/views-node-`);
function start(create=false,ownedRoot=root) {
  const worker=new Worker(pathToFileURL(`${bundle}/tests/wallet-views-worker.mjs`),{workerData:{root:ownedRoot,create},trackUnmanagedFds:true});
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
    async destroy(){let timer;try{await Promise.race([worker.terminate(),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('destruction deadline')),5000);})]);}finally{clearTimeout(timer);}assert.ok(stopped,'worker exit observed');},
  };
}
let owner=start(true), account,list,token;
const policyAccounts=[];
try {
  token=await owner.call('initialize');assert.equal(token.ok,true,show(token));
  const imported=await owner.call('account_import',fixture.import);assert.equal(imported.ok,true,show(imported));account=imported.result;
  assert.equal(account.viewOnly,false);assert.equal(account.signerAttached,false);
  for(const input of fixture.policyImports) {
    for(const k of ['parameters','genesis','priorTreeState'])input.birthday[k]=Uint8Array.from(input.birthday[k].match(/../g),b=>parseInt(b,16));
    const result=await owner.call('account_import',input);assert.equal(result.ok,true,show(result));
    assert.equal(result.result.viewOnly,input.viewOnly);assert.equal(result.result.signerAttached,false);policyAccounts.push(result.result);
  }

  const args={accountId:account.id};
  const initial=await owner.call('address_list',args);assert.deepEqual(initial.result,[fixture.defaultAddress]);
  assert.equal((await owner.call('address_current',args)).result,fixture.defaultAddress.address);
  const next=await owner.call('address_next',args);assert.equal(next.ok,true,show(next));
  assert.notEqual(next.result.address,fixture.defaultAddress.address);
  const high=await owner.call('address_at',{...args,index:309485009821345068724781055n,request:{format:'unified',transparent:'omit',sapling:'omit',ironwood:'require'}});
  assert.equal(high.ok,true,show(high));assert.equal(high.result.index,309485009821345068724781055n);
  const transparent=await owner.call('address_next',{...args,request:{format:'transparent'}});assert.equal(transparent.ok,true,show(transparent));assert.deepEqual(transparent.result.receiverTypes,['p2pkh']);
  list=(await owner.call('address_list',args)).result;
  const aborted=await owner.call('address_next',args,{abort:'before'});assert.equal(aborted.error,'ABORTED');assert.equal(aborted.commit,'none');
  assert.deepEqual((await owner.call('address_list',args)).result,list);
  const failed=await owner.call('address_next',args,{fault:'quota'});assert.equal(failed.ok,false);
  assert.deepEqual((await owner.call('address_list',args)).result,list,'failed write no consumed address');
  const commitFailure=await owner.call('address_at',{...args,index:309485009821345068724781054n,request:{format:'unified',transparent:'omit',sapling:'omit',ironwood:'require'}},{fault:'commit'});assert.equal(commitFailure.ok,false);
  assert.deepEqual((await owner.call('address_list',args)).result,list,'failed commit sync no exposure');
  const post=await owner.call('address_next',args,{abort:'duringSync'});assert.equal(post.error,'ABORTED');assert.equal(post.commit,'committed');
  const after=(await owner.call('address_list',args)).result;assert.equal(after.length,list.length+1);list=after;
  assert.equal((await owner.call('account_import',{...fixture.import,viewingKey:fixture.uivk})).error,'INCOMING_ONLY_WALLET_UNSUPPORTED');
  assert.equal((await owner.call('account_list',{}, {generation:token.generation+1})).error,'STALE_HANDLE');
  assert.equal((await owner.call('account_list',{}, {instance:'wrong'})).error,'WRONG_INSTANCE');
  assert.equal((await owner.call('close')).ok,true);
  assert.equal((await owner.call('account_list')).error,'STALE_HANDLE');
}finally{await owner.destroy();}
owner=start();
try {
  assert.equal((await owner.call('initialize')).ok,true);
  assert.deepEqual((await owner.call('account_list')).result.map(a=>a.id).sort(),[account,...policyAccounts].map(a=>a.id).sort());
  for(const a of policyAccounts)assert.deepEqual((await owner.call('account_get',{accountId:a.id})).result,a);
  assert.deepEqual((await owner.call('address_list',{accountId:account.id})).result,list);
  assert.equal((await owner.call('account_list',{}, {instance:token.instance})).error,'WRONG_INSTANCE');
  assert.equal((await owner.call('close')).ok,true);
}finally{await owner.destroy();}
console.log(show({pass:true,case:'actual FS account UUID/address close/destroy/reopen; instance/generation',root}));

const entropyRoot=fs.mkdtempSync(`${process.env.WALLET_TEST_ROOT}/views-entropy-`);
owner=start(true,entropyRoot);
try {
  assert.equal((await owner.call('initialize')).ok,true);
  assert.equal((await owner.call('account_import',fixture.import,{fault:'entropy'})).error,'DOMAIN_INVALID');
  assert.equal((await owner.call('account_list')).error,'DOMAIN_INVALID');
}finally{await owner.destroy();}
owner=start(false,entropyRoot);
try{assert.equal((await owner.call('initialize')).ok,true);assert.deepEqual((await owner.call('account_list')).result,[]);assert.equal((await owner.call('close')).ok,true);}
finally{await owner.destroy();}
console.log(JSON.stringify({pass:true,case:'entropy trap invalidates owner; destroyed owner reopens with no imported account',root:entropyRoot}));

const hdRoot=fs.mkdtempSync(`${process.env.WALLET_TEST_ROOT}/views-hd-`);
const seed=new Uint8Array(32).fill(40), hdInput={birthday:fixture.import.birthday,accountIndex:0,name:'private HD'};
let hdAccount,hdAddresses,hdGap,hdPost;
owner=start(true,hdRoot);
try {
  const opened=await owner.call('initialize');assert.equal(opened.ok,true);
  assert.equal((await owner.call('account_import_hd',hdInput,{seed,abort:'before'})).commit,'none');
  assert.deepEqual((await owner.call('account_list')).result,[]);
  for(const invalid of [new Uint8Array(31),new Uint8Array(253),[1,2,3]])assert.equal((await owner.call('account_import_hd',hdInput,{seed:invalid})).error,'INVALID_ARGUMENT');
  assert.equal((await owner.call('account_import_hd',{...hdInput,enabledPools:['sapling']},{seed})).error,'UNSUPPORTED_HD_POOLS');
  assert.equal((await owner.call('account_import_hd',hdInput,{seed,instance:'wrong'})).error,'WRONG_INSTANCE');
  assert.equal((await owner.call('account_import_hd',hdInput,{seed,generation:opened.generation+1})).error,'STALE_HANDLE');
  const imported=await owner.call('account_import_hd',hdInput,{seed});assert.equal(imported.ok,true,show(imported));hdAccount=imported.result;
  assert.equal(hdAccount.accountIndex,0);assert.equal(hdAccount.signerAttached,false);assert.equal(hdAccount.viewOnly,false);
  assert.deepEqual(seed,new Uint8Array(32).fill(40),'application-owned bytes unchanged');
  assert.equal((await owner.call('account_import_hd',hdInput,{seed})).error,'ACCOUNT_COLLISION');
  const gap=await owner.call('account_import_hd',{...hdInput,accountIndex:3,enabledPools:['ironwood','transparent','sapling']},{seed});assert.equal(gap.ok,true,show(gap));hdGap=gap.result;
  const args={accountId:hdAccount.id};
  assert.equal(typeof (await owner.call('address_current',args)).result,'string');
  assert.equal((await owner.call('address_next',args)).ok,true);
  assert.equal((await owner.call('address_at',{...args,index:309485009821345068724781055n,request:{format:'unified',transparent:'omit',sapling:'omit',ironwood:'require'}})).ok,true);
  hdAddresses=(await owner.call('address_list',args)).result;
  const before=(await owner.call('account_list')).result;
  const failed=await owner.call('account_create_hd',{birthday:hdInput.birthday},{seed,fault:'commit'});assert.equal(failed.ok,false);
  assert.deepEqual((await owner.call('account_list')).result,before,'failed native HD commit consumes no index/account');
  const post=await owner.call('account_create_hd',{birthday:hdInput.birthday},{seed,abort:'duringSync'});assert.equal(post.error,'ABORTED');assert.equal(post.commit,'committed');assert.equal('result' in post,false);
  hdPost=(await owner.call('account_list')).result.find(a=>a.accountIndex===4);assert.ok(hdPost);assert.equal(hdPost.signerAttached,false);
  assert.equal((await owner.call('close')).ok,true);
}finally{await owner.destroy();}
owner=start(false,hdRoot);
try {
  assert.equal((await owner.call('initialize')).ok,true);
  for(const account of [hdAccount,hdGap,hdPost])assert.deepEqual((await owner.call('account_get',{accountId:account.id})).result,account);
  assert.deepEqual((await owner.call('address_list',{accountId:hdAccount.id})).result,hdAddresses);
  const next=await owner.call('account_create_hd',{birthday:hdInput.birthday},{seed});assert.equal(next.ok,true,show(next));assert.equal(next.result.accountIndex,5);
  assert.equal((await owner.call('close')).ok,true);
}finally{await owner.destroy();}
for(const file of fs.readdirSync(hdRoot,{recursive:true})) {
  const path=`${hdRoot}/${file}`;if(!fs.statSync(path).isFile())continue;
  const bytes=fs.readFileSync(path);assert.equal(bytes.includes(Buffer.from(seed)),false,'synthetic seed absent from persisted files');assert.equal(bytes.includes(Buffer.from(seed).toString('hex')),false);
}
console.log(JSON.stringify({pass:true,case:'private HD explicit/gap/native next; commit rollback; abort semantics; all addresses; destroyed owner reopen; seed byte absence',root:hdRoot}));
