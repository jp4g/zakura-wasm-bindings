// Ordinary page-owned module; WebDriver only observes this test.
const active = new Set(), results = [];
const diagnostics = [], ownedScripts = [];
const root = `private-views-test-${crypto.randomUUID()}`;
const parameters = new TextEncoder().encode('{"encoding":"regtest","Overwinter":10,"Sapling":20,"Blossom":30,"Heartwood":40,"Canopy":50,"Nu5":60,"Nu6":70,"Nu6_1":80,"Nu6_2":90,"Nu6_3":100}');
const initialize = create => ({ op: 'initialize', root, create, format: 'zcash-js-network/1', parameters, genesis: new Uint8Array(32).fill(3) });
function check(value, message) { if (!value) throw Error(message); }
async function observe(body) {
  const response = await fetch('/lifecycle', { method: 'POST', body: JSON.stringify(body), signal: AbortSignal.timeout(15000) });
  const data = await response.json();
  if (!response.ok) throw Error(JSON.stringify(data));
  return data;
}
function start() {
  const token = crypto.randomUUID();
  const script = `/tests/wallet-views-worker.mjs?owner=${token}`;
  ownedScripts.push(script);
  diagnostics.push({script,phase:'constructed',pageMs:performance.now()});
  const worker = new Worker(script, { type: 'module' }); active.add(worker);
  let error, instance;
  worker.addEventListener('error', e => { error = Error(`${e.message} (${e.filename}:${e.lineno}:${e.colno})`); diagnostics.push({script,phase:'page-worker-error',message:error.message,pageMs:performance.now()}); });
  worker.addEventListener('message', e => { if(e.data.diagnostic)diagnostics.push({script,pageMs:performance.now(),...e.data.diagnostic}); });
  return {
    async call(request) {
      if (error) throw error;
      return new Promise((resolve, reject) => {
        const done = (error, value) => {
          clearTimeout(timer); worker.removeEventListener('message', message); worker.removeEventListener('error', failed);
          error ? reject(error) : resolve(value);
        };
        const timer = setTimeout(() => { worker.terminate(); done(Error(`worker deadline: ${request.op}; last phase=${diagnostics.filter(d=>d.script===script).at(-1)?.phase}`)); }, 30000);
        const message = e => {
          if(e.data.diagnostic) { if(['module-error','worker-error','unhandled-rejection'].includes(e.data.diagnostic.phase))done(Error(JSON.stringify(e.data.diagnostic))); return; }
          if (e.data.ok && e.data.instance) instance = e.data.instance; done(null, e.data);
        };
        const failed = e => done(Error(e.message));
        diagnostics.push({script,phase:`post:${request.op}`,pageMs:performance.now()});
        worker.addEventListener('message', message); worker.addEventListener('error', failed); worker.postMessage('instance' in request ? request : { ...request, instance });
      });
    },
    async destroy() {
      const creation = await observe({ script, state: 'created' });
      worker.terminate();
      const destruction = await observe({ script, state: 'destroyed', realm: creation.realm });
      active.delete(worker); results.push(destruction);
    },
  };
}
let outcome;
try {
  const first = start();
  const opened = await first.call(initialize(true));
  check(opened.ok, JSON.stringify(opened));
  check(opened.secure, 'secure dedicated OPFS worker');
  const fixture=await (await fetch('/tests/views-fixture.json')).json();
  const input=fixture.import;
  for(const k of ['parameters','genesis','priorTreeState'])input.birthday[k]=Uint8Array.from(input.birthday[k].match(/../g),b=>parseInt(b,16));
  const imported=await first.call({op:'account_import',generation:opened.generation,args:input});check(imported.ok,JSON.stringify(imported));
  const account=imported.result, args={accountId:account.id};
  const listed=await first.call({op:'address_list',generation:opened.generation,args});check(listed.ok && listed.result[0].address===fixture.defaultAddress.address,'native default address');
  const allocated=await first.call({op:'address_next',generation:opened.generation,args});check(allocated.ok,JSON.stringify(allocated,(_,v)=>typeof v==='bigint'?v.toString():v));
  const exact=await first.call({op:'address_at',generation:opened.generation,args:{...args,index:309485009821345068724781055n,request:{format:'unified',transparent:'omit',sapling:'omit',ironwood:'require'}}});check(exact.ok && exact.result.index===309485009821345068724781055n,'88 bit exact index');
  const transparent=await first.call({op:'address_next',generation:opened.generation,args:{...args,request:{format:'transparent'}}});check(transparent.ok && transparent.result.receiverTypes.join()==='p2pkh','transparent only');
  const records=(await first.call({op:'address_list',generation:opened.generation,args})).result;
  const encode=v=>JSON.stringify(v,(_,x)=>typeof x==='bigint'?x.toString():x);
  results.push({case:'account import and unified/transparent address allocation',accountId:account.id,addresses:records.length});
  const contender = start();
  const busy = await contender.call(initialize(false));
  check(!busy.ok && busy.error === 'STORAGE_BUSY', `contention: ${JSON.stringify(busy)}`);
  await contender.destroy();
  check((await first.call({ op: 'close', generation: opened.generation })).ok, 'close');
  check(!(await first.call({ op: 'account_list', generation: opened.generation })).ok, 'stale operation');
  await first.destroy();
  // No replacement until the server observed the original BiDi realm destroyed.
  const wrong = start(), wrongRequest = initialize(false); wrongRequest.genesis[0] = 4;
  check((await wrong.call(wrongRequest)).error === 'NETWORK_MISMATCH', 'wrong network');
  await wrong.destroy();
  const reopened = start();
  const again = await reopened.call(initialize(false)); check(again.ok, JSON.stringify(again));
  const stored=await reopened.call({op:'account_get',generation:again.generation,args:{accountId:account.id}});check(stored.ok&&encode(stored.result)===encode(account),'persistent UUID/account');
  const addresses=await reopened.call({op:'address_list',generation:again.generation,args});check(addresses.ok&&encode(addresses.result)===encode(records),'persistent verified address list');
  check((await reopened.call({op:'account_list',generation:again.generation,instance:opened.instance})).error==='WRONG_INSTANCE','cross-worker handle');
  results.push({case:'same DB account/address after observed destruction',accountId:account.id,addresses:addresses.result.length});
  check((await reopened.call({ op: 'close', generation: again.generation })).ok, 'reopened close');
  await reopened.destroy();
  outcome = { pass: true, root, results, userAgent: navigator.userAgent, actualQuotaExhaustion: false, uaEviction: false };
} catch (e) { outcome = { pass: false, root, results, error: { name: e.name, message: e.message, stack: e.stack } }; }
finally { for (const worker of active) worker.terminate(); }
Object.assign(outcome,{diagnostics,ownedScripts});
document.querySelector('#result').textContent = JSON.stringify(outcome, null, 2);
await fetch('/result', { method: 'POST', body: JSON.stringify(outcome) });
