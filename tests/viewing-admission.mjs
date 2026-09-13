// Facade lifetime/admission only. tests/viewing.rs exercises the real upstream keys.
import assert from 'node:assert/strict';
import vm from 'node:vm';
import fs from 'node:fs';
import {copyBytes} from '../bytes.mjs';
let opens=0,frees=0,initializations=0;
const native=()=>({describe:()=>'{"kind":"ufvk"}',export:()=> 'viewing-key',to_incoming:native,derive:()=>'{"index":"0"}',find:()=>'{"index":"1"}',free(){frees++;}});
const context=vm.createContext({Uint8Array,JSON,Object,Number,TypeError,Error,parseInt});
const facade=new vm.SourceTextModule(fs.readFileSync(new URL('../network.mjs',import.meta.url),'utf8'),{context});
await facade.link(specifier=>{
  const exports=specifier==='./bytes.mjs'?{copyBytes}:{validate_birthday(){},initSync(){initializations++;},consensus_branch(){return 0;},viewing_open(){opens++;return native();},viewing_decode_address(){return '{"encoded":"address","knownReceivers":[],"unknownTypecodes":[100]}';},viewing_select_receiver(){return '{"pool":"sapling","type":"sapling","bytes":"00ff"}';}};
  return new vm.SyntheticModule(Object.keys(exports),function(){for(const [key,value]of Object.entries(exports))this.setExport(key,value);},{context});
});
await facade.evaluate();
const api=facade.namespace,parameters=new Uint8Array([1]);
const full=api.openViewingAuthority(parameters,'ufvk','key','["sapling"]'),alias=full,incoming=full.toIncoming();
assert.equal(full.export('ufvk','discloses-viewing-authority'),'viewing-key');
assert.equal(full.derive('0','{}').index,'0');assert.equal(full.find('0','{}',1).index,'1');
full.dispose();full.dispose();assert.equal(frees,1);
for(const fn of [()=>alias.describe(),()=>alias.export('ufvk','discloses-viewing-authority'),()=>alias.derive('0','{}'),()=>alias.find('0','{}',1),()=>alias.toIncoming()])assert.throws(fn,/CLOSED/);
assert.equal(incoming.describe().kind,'ufvk');incoming.dispose();assert.equal(frees,2);
let coerced=0;const bad={toString(){coerced++;return 'key';}};
assert.throws(()=>api.openViewingAuthority(parameters,'ufvk',bad,'[]'));
assert.equal(coerced,0);assert.equal(opens,1);
assert.throws(()=>api.selectViewingReceiver(parameters,'address','sapling',-1,0));
assert.throws(()=>api.selectViewingReceiver(parameters,'address','sapling',0,2**32));
assert.deepEqual([...api.selectViewingReceiver(parameters,'address','sapling',0,0).bytes],[0,255]);
console.log('standalone viewing facade admission, independent ownership and idempotent release passed');

api.initialize(new Uint8Array(3 * 1048576));
assert.equal(initializations,1);
assert.throws(()=>api.initialize(new Uint8Array(3 * 1048576 + 1)));
assert.equal(initializations,1);
