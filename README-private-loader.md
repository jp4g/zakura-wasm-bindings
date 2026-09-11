# Internal verified primitive package

This package adds a confined executable seam for the accepted stateless network
context and transaction byte/identity bindings. It is **not H1**, `runtime_init`,
a public `createRuntime`, network registration, consensus/proof validation, or a
wallet/storage/signing/proving service. The SDK loader remains internal, with no
root export. New Rust is unnecessary and none is added.

The selected producing commit is `63d08edb99b1d88e4899c27e61cc955fca5e35d6`.
It consumes the exact accepted `acaf7069e466c82ce1be2c45ebafb75a92b59ee9`
packet, whose metadata SHA256 is
`09ae852de689eb47fba35dfefaae81397d280f5c2542bccdee9747954efad575`.
Later test/documentation commits are not the selected producing revision.

`primitive/package.mjs` authenticates that packet and its complete physical
inventory, checked-entry source/lock/tree, and the committed packaging sources.
It feeds owned snapshots through the already installed read-only Rolldown 1.2.7.
Esbuild is absent here. No dependencies are installed or package files changed.
The virtual input map denies imports outside the authenticated source inventory.
The output must be one module with no static/dynamic dependencies or ambient
fetch/eval. Tree shaking removes the unused generated default URL initializer;
**generated glue is never text-patched**. The worker is the committed transport
wrapper; its only ambient import is the Node native `node:worker_threads` service.

The canonical, frozen-API-compatible `zcash-artifact/1` manifest has exactly three
assets: `primitive.mjs` (module), `worker.mjs` (worker), and `bindings_bg.wasm`.
The complete canonical manifest is independently pinned by the paired SDK loader:
`49b8d1b68cb851c473c184bb7986842718d652cdd31995a47ae82bb23565499b`.
Build metadata SHA256 is
`b6a930766ec48f2d3b669b7302ad97964eda3a41bd4edf0c1fad2019aac8af78`.
Metadata binds the producer revision/tree, every packaging source hash, accepted
packet identity, compiler JS/native files, module input mapping, Node version,
and accepted locked dependency graph. WASM is copied from the authenticated
accepted packet; this packaging run does not repeat its accepted Rust compilation.

Reproduce from the selected revision, using an external new output directory:

```sh
REPRO=$(mktemp -d /home/jack/zcash-primitive-loader-scratch/repro.XXXXXX)
git worktree add --detach "$REPRO/source" 63d08edb99b1d88e4899c27e61cc955fca5e35d6
node "$REPRO/source/primitive/package.mjs" /home/jack/zakura-transaction-bindings-scratch/packet-final "$REPRO/packet"
```

Two actual producer runs under the recorded installed compiler produced identical
bytes in `packet-63d08ed` and `packet-63d08ed-repro` under the assigned scratch root.
The paired SDK test pin is `tests/runtime-primitive-loader/pin.json`.
Source documentation is `docs/planning/primitive-loader.md` in that worktree.

The worker imports the module only from the paired loader's verified temporary
file/Blob URL and receives owned verified WASM bytes explicitly. Its request
schema is exact: init `{type,moduleUrl,wasm}`, context
`{type,id,format,bytes,value}`, transaction `{type,id,bytes,value}`. IDs are
increasing positive safe integers. Results are `{type:'result',id,result}` or
`{type:'invalid',id}`; readiness/fatal are `{type:'ready'}` / `{type:'fatal'}`.
Only the checked private entries call Rust. A trap/unexpected exception marks the
wrapper terminal and asks the owner to terminate it. This bootstrap is a trusted
internal component, not an independently exposed URL-configurable backend.

Run `node --test --test-isolation=none tests/primitive/package.test.mjs` after the
selected packet is present at its recorded path. Tests cover actual checked Node
execution, corrupt packet/inventory rejection without executable output, and a
diagnostic WASM unreachable instruction after a real codec call. That last test
proves wrapper invalidation under injected WASM failure, not a production Rust
panic trigger or panic recovery. The loader's Node tests prove terminal teardown.

Browser execution, CSP and realm destruction are **pending parent execution**.
This sandbox returns `listen EPERM` before Firefox startup. The exact immutable
strict-HTTPS parent package and command are in the SDK document and
`/home/jack/zcash-primitive-loader-logs/REPORT.md`. No TLS/confinement policy was
changed. Independent HIGH review and parent host verification remain required.
