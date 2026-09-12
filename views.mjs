// Private worker-local account/address primitive. The enclosing host authenticates
// this packaged executable closure; this is not the public WalletClient factory.
import { initializeStorage } from './wallet.mjs';
import * as binding from './bindings.js';
import { copyBytes } from './bytes.mjs';
const aborted = Object.getOwnPropertyDescriptor(AbortSignal.prototype, 'aborted').get;
const operations = new Set(['account_balance','account_import','account_import_hd','account_create_hd','account_import_mnemonic','account_list','account_get','address_current','address_next','address_list','address_at']);
const queries = new Set(['wallet_history','wallet_transaction','wallet_notes','wallet_utxos']);
const syncs = new Set(['scan_state','scan_block_hash','scan_rewind','scan_complete']);
const enhancements = new Set(['enhancement_requests','enhancement_apply']);
const scans = new Set(['scan_plan','scan_ingest_batch',...syncs,...enhancements,...queries]);
const writes = new Set(['scan_plan','scan_ingest_batch','scan_rewind','scan_complete','enhancement_apply','account_import','account_import_hd','account_create_hd','account_import_mnemonic','address_next','address_at']);
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
function scanFields(value, allowed) {
    if (!value || Object.getPrototypeOf(value)!==Object.prototype) throw TypeError('INVALID_ARGUMENT');
    const result=Object.create(null);
    for (const key of Reflect.ownKeys(value)) {
      const property=Object.getOwnPropertyDescriptor(value,key);
      if (!allowed.includes(key)||!property||!('value' in property)) throw TypeError('INVALID_ARGUMENT');
      result[key]=property.value;
    }
    return result;
}
function scanHeight(value) {
  if (!Number.isInteger(value)||value<0||value>0xffffffff) throw TypeError('INVALID_ARGUMENT');
  return value;
}
function scanPoint(value) {
  const target=scanFields(value,['height','hash']);
  if (!Number.isInteger(target.height)||target.height<0||target.height>=0xffffffff) throw TypeError('INVALID_ARGUMENT');
  if (typeof target.hash!=='string'||! /^[0-9a-f]{64}$/.test(target.hash)) throw TypeError('INVALID_ARGUMENT');
  return target;
}
function scanHex(bytes) {
    const ascii=new Uint8Array(bytes.length*2);
    for(let i=0;i<bytes.length;i++) {
      const high=bytes[i]>>>4, low=bytes[i]&15;
      ascii[2*i]=high+(high<10?48:87);
      ascii[2*i+1]=low+(low<10?48:87);
    }
    return new TextDecoder().decode(ascii);
}
function lowerScan(args, operation) {
  if (queries.has(operation)) {
    const inventory=operation==='wallet_notes'||operation==='wallet_utxos';
    const input=scanFields(args,inventory?['accountId','cursor','limit','spendState','locked','uneconomic','signal',...(operation==='wallet_notes'?['pool']:[])]:operation==='wallet_history'?['accountId','cursor','limit','signal']:['txid','signal']);delete input.signal;
    if(operation==='wallet_history'||inventory) {
      if(typeof input.accountId!=='string'||input.accountId.length!==36)throw TypeError('INVALID_ARGUMENT');
      if(Object.hasOwn(input,'cursor')&&(typeof input.cursor!=='string'||input.cursor.length>(inventory?2048:1024)))throw TypeError('INVALID_ARGUMENT');
      if(Object.hasOwn(input,'limit')&&(!Number.isInteger(input.limit)||input.limit<1||input.limit>200))throw TypeError('INVALID_ARGUMENT');
    }else if(typeof input.txid!=='string'||! /^[0-9a-f]{64}$/.test(input.txid))throw TypeError('INVALID_ARGUMENT');
    if(inventory) {
      for(const key of ['locked','uneconomic'])if(Object.hasOwn(input,key)&&typeof input[key]!=='boolean')throw TypeError('INVALID_ARGUMENT');
      if(Object.hasOwn(input,'spendState')&&!['unspent','pendingSpend','spent','unknown'].includes(input.spendState))throw TypeError('INVALID_ARGUMENT');
      if(Object.hasOwn(input,'pool')&&!['sapling','ironwood'].includes(input.pool))throw TypeError('INVALID_ARGUMENT');
    }
    return input;
  }
  if (enhancements.has(operation)) return lowerEnhancement(args,operation);
  if (operation==='scan_complete') {
    const input=scanFields(args,['revision','target','treeState','signal']);delete input.signal;
    if(typeof input.revision!=='string'||input.revision.length>128)throw TypeError('INVALID_ARGUMENT');
    input.target=scanPoint(input.target);input.treeState=scanHex(copyBytes(input.treeState,65536,'RESOURCE_LIMIT'));
    return input;
  }
  const keys=operation==='scan_state'?[]:operation==='scan_block_hash'?['height']:operation==='scan_rewind'?['revision','requestedPoint']:
    operation==='scan_plan'?['target']:['target','revision','priorTreeState','blocks'];
  const input=scanFields(args,[...keys,'signal']);delete input.signal;
  if (operation==='scan_state') return input;
  if (operation==='scan_block_hash') {input.height=scanHeight(input.height);return input;}
  if (operation!=='scan_plan'&&(typeof input.revision!=='string'||input.revision.length>128)) throw TypeError('INVALID_ARGUMENT');
  if (operation==='scan_rewind') {input.requestedPoint=scanPoint(input.requestedPoint);return input;}
  input.target=scanPoint(input.target);
  if (operation==='scan_ingest_batch') {
    input.priorTreeState=scanHex(copyBytes(input.priorTreeState,65536,'INVALID_ARGUMENT'));
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
      input.blocks.push(scanHex(bytes));
    }
  }
  return input;
}
function lowerEnhancement(args, operation) {
  const input=scanFields(args,operation==='enhancement_requests'?['signal']:['revision','request','result','signal']);delete input.signal;
  if (operation==='enhancement_requests') return input;
  if (typeof input.revision!=='string'||input.revision.length>128) throw TypeError('INVALID_ARGUMENT');
  const request=scanFields(input.request,['kind','txid','address','start','endExclusive','requestAt','txStatus','outputStatus']);
  if (request.kind==='address') {
    scanFields(input.request,['kind','address','start','endExclusive','requestAt','txStatus','outputStatus']);
    if (typeof request.address!=='string'||request.address.length>128||!request.address.length
      || !['mined','mempool','all'].includes(request.txStatus)||!['unspent','all'].includes(request.outputStatus)) throw TypeError('INVALID_ARGUMENT');
    scanHeight(request.start);if(request.endExclusive!==null)scanHeight(request.endExclusive);
    if(request.requestAt!==null&&(!Number.isSafeInteger(request.requestAt)||request.requestAt<0))throw TypeError('INVALID_ARGUMENT');
  } else {
    scanFields(input.request,['kind','txid']);
    if (!['enhancement','status'].includes(request.kind)||typeof request.txid!=='string'||! /^[0-9a-f]{64}$/.test(request.txid)) throw TypeError('INVALID_ARGUMENT');
  }
  input.request=request;
  const result=scanFields(input.result,['transactions','asOfHeight','complete','status','height']);
  if (Object.hasOwn(result,'status')) {
    scanFields(input.result,['status','height']);
    if (!['notRecognized','notInMainChain','mined'].includes(result.status)) throw TypeError('INVALID_ARGUMENT');
    if(result.status==='mined')scanHeight(result.height);else if(Object.hasOwn(result,'height'))throw TypeError('INVALID_ARGUMENT');
  } else {
    scanFields(input.result,request.kind==='address'?['transactions','asOfHeight','complete']:['transactions']);
    if(request.kind==='address'&&typeof result.complete!=='boolean')throw TypeError('INVALID_ARGUMENT');
    if(Object.hasOwn(result,'asOfHeight'))scanHeight(result.asOfHeight);
    const items=result.transactions;
    if(!Array.isArray(items)||items.length>16||Reflect.ownKeys(items).length!==items.length+1)throw TypeError('RESOURCE_LIMIT');
    let remaining=2*1024*1024;
    result.transactions=[];
    for(let i=0;i<items.length;i++) {
      const property=Object.getOwnPropertyDescriptor(items,String(i));
      if(!property||!('value'in property))throw TypeError('INVALID_ARGUMENT');
      const item=scanFields(property.value,['bytes','minedHeight']);
      if(item.minedHeight!==null)scanHeight(item.minedHeight);
      const bytes=copyBytes(item.bytes,remaining,'RESOURCE_LIMIT');remaining-=bytes.length;
      result.transactions.push({bytes:scanHex(bytes),minedHeight:item.minedHeight});
    }
  }
  input.result=result;
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
          result=queries.has(operation)?binding.query_call(token,operation,input):enhancements.has(operation)?binding.enhancement_call(token,operation,input):syncs.has(operation)?binding.sync_call(token,operation,input):
            scans.has(operation)?binding.scan_call(token,operation,input):binding.views_call(token,operation,input);
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
      if(queries.has(operation)&&value!==null) {
        for(const row of value.items??value.accounts)for(const key of operation==='wallet_notes'||operation==='wallet_utxos'?['value']:['balanceDelta','totalReceived','totalSpent','fee'])if(row[key]!==null)row[key]=BigInt(row[key]);
        if(operation==='wallet_transaction') {
          const bytes=hex=>{const result=new Uint8Array(hex.length/2);for(let i=0;i<result.length;i++)result[i]=parseInt(hex.slice(2*i,2*i+2),16);return result;};
          if(value.raw!==null)value.raw=bytes(value.raw);
          for(const output of value.outputs){if(output.value!==null)output.value=BigInt(output.value);if(output.memo.kind==='binary')output.memo.bytes=bytes(output.memo.bytes);}
        }
      }
      return value;
    },
    close(token,owner) {
      if(poisoned)throw Error('DOMAIN_INVALID');
      storage.close(token,owner);
    },
  });
}
