// Private worker-local account/address primitive. The enclosing host authenticates
// this packaged executable closure; this is not the public WalletClient factory.
import { initializeStorage } from './wallet.mjs';
import * as binding from './bindings.js';
import { copyBytes } from './bytes.mjs';
const aborted = Object.getOwnPropertyDescriptor(AbortSignal.prototype, 'aborted').get;
const operations = new Set(['account_balance','account_import','account_import_hd','account_create_hd','account_import_mnemonic','account_list','account_get','address_current','address_next','address_list','address_at']);
const scans = new Set(['scan_plan','scan_ingest_batch']);
const writes = new Set(['scan_plan','scan_ingest_batch','account_import','account_import_hd','account_create_hd','account_import_mnemonic','address_next','address_at']);
function abort(signal, commit) {
  if (signal !== undefined && aborted.call(signal)) throw Object.assign(Error('ABORTED'), { commit });
}
function lower(value, name = '', depth = 0) {
  if (depth > 5) throw TypeError('INVALID_ARGUMENT');
  if (['parameters','genesis','priorTreeState'].includes(name)) {
    const bytes=copyBytes(value,name==='parameters'?256:name==='genesis'?32:65536,'INVALID_ARGUMENT');
    return Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');
  }
  if (name==='index') {
    if (typeof value!=='bigint'||value<0n||value>=(1n<<88n))throw TypeError('INVALID_ARGUMENT');
    return value.toString();
  }
  if(typeof value==='string'&&value.length>140000)throw TypeError('INVALID_ARGUMENT');
  if (value===null||typeof value==='boolean'||typeof value==='string') return value;
  if (typeof value==='number'&&Number.isSafeInteger(value)) return value;
  if (Array.isArray(value)) {if(name!=='enabledPools'||value.length<1||value.length>3)throw TypeError('INVALID_ARGUMENT');return value.map(v=>lower(v,'',depth+1));}
  if (!value||Object.getPrototypeOf(value)!==Object.prototype) throw TypeError('INVALID_ARGUMENT');
  const allowed=depth===0?['confirmations','accountIndex','accountId','viewingKey','birthday','name','viewOnly','enabledPools','request','index','signal']:name==='confirmations'?['trusted','untrusted','allowZeroConfirmationShielding']:name==='birthday'?['parameters','genesis','firstScanHeight','priorTreeState','recoverUntilExclusive','source']:name==='request'?['format','transparent','sapling','ironwood']:[];
  const result=Object.create(null);
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key!=='string'||!allowed.includes(key))throw TypeError('INVALID_ARGUMENT');
    const property=Object.getOwnPropertyDescriptor(value,key);
    if (!property||!('value' in property))throw TypeError('INVALID_ARGUMENT');
    if (depth===0&&key==='signal')continue;
    result[key]=lower(property.value,key,depth+1);
  }
  return result;
}
// Scan protobufs are copied and hex encoded only; Rust owns their interpretation.
function lowerScan(args, operation) {
  function fields(value, allowed) {
    if (!value || Object.getPrototypeOf(value)!==Object.prototype) throw TypeError('INVALID_ARGUMENT');
    const result=Object.create(null);
    for (const key of Reflect.ownKeys(value)) {
      const property=Object.getOwnPropertyDescriptor(value,key);
      if (!allowed.includes(key)||!property||!('value' in property)) throw TypeError('INVALID_ARGUMENT');
      result[key]=property.value;
    }
    return result;
  }
  const input=fields(args,operation==='scan_plan'?['target','signal']:['target','revision','priorTreeState','blocks','signal']);
  delete input.signal;
  const target=fields(input.target,['height','hash']);
  if (!Number.isInteger(target.height)||target.height<0||target.height>=0xffffffff) throw TypeError('INVALID_ARGUMENT');
  const hex=bytes=>Array.from(bytes,b=>b.toString(16).padStart(2,'0')).join('');
  if (typeof target.hash!=='string'||! /^[0-9a-f]{64}$/.test(target.hash)) throw TypeError('INVALID_ARGUMENT');
  input.target=target;
  if (operation==='scan_ingest_batch') {
    if (typeof input.revision!=='string'||input.revision.length>128) throw TypeError('INVALID_ARGUMENT');
    input.priorTreeState=hex(copyBytes(input.priorTreeState,65536,'INVALID_ARGUMENT'));
    const blocks=input.blocks;
    if (!Array.isArray(blocks)||blocks.length<1||blocks.length>16) throw TypeError('RESOURCE_LIMIT');
    if (Reflect.ownKeys(blocks).length!==blocks.length+1) throw TypeError('INVALID_ARGUMENT');
    let remaining=2*1024*1024;
    input.blocks=[];
    for (let i=0;i<blocks.length;i++) {
      const property=Object.getOwnPropertyDescriptor(blocks,String(i));
      if (!property||!('value' in property)) throw TypeError('INVALID_ARGUMENT');
      const bytes=copyBytes(property.value,remaining,'RESOURCE_LIMIT');
      remaining-=bytes.length;
      input.blocks.push(hex(bytes));
    }
  }
  return input;
}
function lift(value) {
  if (Array.isArray(value)) return value.map(lift);
  if (value&&typeof value==='object'&&typeof value.index==='string') value.index=BigInt(value.index);
  return value;
}
function liftAmounts(value) {
  if (value===null) return null;
  for (const [key,amount] of Object.entries(value)) {
    if (['total','spendable','locked','changePendingConfirmation','pendingSpendability','uneconomic','observedTotal'].includes(key)) value[key]=BigInt(amount);
    else if (amount&&typeof amount==='object') liftAmounts(amount);
  }
  return value;
}
export async function initializeViews(wasm,backend,format,parameters,genesis) {
  // The sole storage owner stays private. It initializes the SAME binding module.
  const storage=await initializeStorage(wasm,backend,format,parameters,genesis);
  return viewsForStorage(storage);
}
/** Bind account/query operations to the already opened worker-local storage. */
export function viewsForStorage(storage) {
  const {generation,instance}=storage;
  let poisoned=false;
  return Object.freeze({
    generation,instance,
    call(token,owner,operation,args={},seed,mnemonic,passphrase) {
      if(poisoned)throw Error('DOMAIN_INVALID');
      let signal,input;
      try {
        storage.binding(token,owner); // actual Rust generation + owned JS instance
        if(!operations.has(operation)&&!scans.has(operation))throw TypeError('INVALID_ARGUMENT');
        const descriptor=Object.getOwnPropertyDescriptor(args,'signal');
        if(descriptor&&!('value' in descriptor))throw TypeError('INVALID_ARGUMENT');
        signal=descriptor?.value;
        abort(signal,'none');
        input=JSON.stringify(scans.has(operation)?lowerScan(args,operation):lower(args));
        abort(signal,'none');
      } catch(error) {
        if(scans.has(operation))throw Object.assign(error instanceof Error?error:Error('INVALID_ARGUMENT'),{commit:'none'});
        throw error;
      }
      let result;
      let ownedSeed,ownedMnemonic,ownedPassphrase;
      try {
        if(operation==='account_import_mnemonic') {
          if(seed!==undefined)throw 'INVALID_ARGUMENT';
          try {
            ownedMnemonic=copyBytes(mnemonic,4096,'INVALID_ARGUMENT');
            ownedPassphrase=passphrase===undefined?new Uint8Array():copyBytes(passphrase,65536,'INVALID_ARGUMENT',0);
          }catch {throw 'INVALID_ARGUMENT';}
          result=binding.views_mnemonic_call(token,input,ownedMnemonic,ownedPassphrase);
        } else if(mnemonic!==undefined||passphrase!==undefined) {
          throw 'INVALID_ARGUMENT';
        } else if(operation==='account_import_hd'||operation==='account_create_hd') {
          try {ownedSeed=copyBytes(seed,64,'INVALID_ARGUMENT');}catch {throw 'INVALID_ARGUMENT';}
          if(ownedSeed.length!==32&&ownedSeed.length!==64)throw 'INVALID_ARGUMENT';
          result=binding.views_seed_call(token,operation,input,ownedSeed);
        } else {
          if(seed!==undefined)throw 'INVALID_ARGUMENT';
          result=scans.has(operation)?binding.scan_call(token,operation,input):binding.views_call(token,operation,input);
        }
      }
      catch(error) {
        // Infallible upstream entropy/clock paths can trap. Never reuse that owner.
        if(typeof error!=='string'){poisoned=true;throw Error('DOMAIN_INVALID');}
        throw Object.assign(Error(error),scans.has(operation)?{commit:'none'}:{});
      }
      finally {ownedSeed?.fill(0);ownedMnemonic?.fill(0);ownedPassphrase?.fill(0);}
      abort(signal,writes.has(operation)?'committed':'none');
      const value=lift(JSON.parse(result));
      if(operation==='account_balance')value.amounts=liftAmounts(value.amounts);
      return value;
    },
    close(token,owner) {
      if(poisoned)throw Error('DOMAIN_INVALID');
      storage.close(token,owner);
    },
  });
}
