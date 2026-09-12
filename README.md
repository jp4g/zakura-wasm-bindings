# Zakura private bindings

A `publish = false` production Rust binding crate for bounded network documents
and exact transaction bytes/identity, related to zcash.js #3/#4/#6. The crate name
`zakura-network-bindings` is retained for compatibility. Exact dependencies include
`zcash_protocol 0.10.6`, `zakura-primitives 1.0.0` (no defaults, std), and genuine
`wasm-bindgen 0.2.128`, with the full Cargo lock.

The JS entry exposes `consensusContext(parametersFormat, parameters, height)` and
returns a frozen `{ height, branchId }`. It validates `zcash-js-network/1` and an
ordinary, attached Uint8Array (Node Buffer/subviews included), snapshots at most
256 bytes, and checks integer uint32 heights before generated-glue coercion.
Shared buffers and foreign-realm inputs are unsupported. Rust independently
validates the canonical document and calls the actual upstream
`BranchId::for_height`. Invalid documents/formats fail with fixed strings;
JS admission fails with TypeError. These are private binding errors, not H1 errors.

All ten explicit upgrade heights through Nu6_3 are required, nondecreasing, with
null-only suffixes. Equal heights use upstream order. The encoding selector never
imports a built-in schedule. The maximum valid document is under the 256-byte cap.
The parser adapts the independently accepted component at SDK revision
`a280d31d9e1c3c624cb9a340089b3ad79117e011`; its qualification crate stays intact.
No upstream workspace, branch constants, or live-chain schedules are copied into
production. The stateless operation needs no registry and retains no caller bytes.

## Explicit build

Use installed stable Rust with wasm32-unknown-unknown, Node, and the approved
bindgen binary pinned by SHA-256 in `build.py`. No tool installation or download
occurs. Seed an owned scratch Cargo registry with the 143 exact lock-checksummed
crate archives and matching existing registry index (this run records that copy
in `/home/jack/zakura-transaction-bindings-logs/cache-provenance.json`). Never write to
another worker's or global cache. All output/cache/log paths must be external.

```sh
REPRO=$(mktemp -d /home/jack/zakura-transaction-bindings-scratch/producer-repro.XXXXXX)
git worktree add --detach "$REPRO/source" acaf7069e466c82ce1be2c45ebafb75a92b59ee9
python3 "$REPRO/source/build.py" /home/jack/zakura-transaction-bindings-scratch "$REPRO/packet"
```

These commands reproduce the selected producing revision from a detached checkout,
with a new, nonexistent packet destination. Later documentation commits do not
produce the selected binaries. Exact reproduction requires the recorded installed
tools, locked inputs and same Cargo cache root
`/home/jack/zakura-transaction-bindings-scratch/cargo`: dependency panic paths embed
that location in WASM. Moving the cache can change WASM bytes even with identical
sources and tools (independent review S1). The selected normal/`-OO` equality is
for this same cache root. Reuse it only within its authorized ownership.

The destination must be new. Scope, tool identity, and committed-script guards
remain active under Python `-O` and `-OO`. Run the nonexecuting guard regression
with `python3 tests/test_build_guards.py` (also with `-O` and `-OO`). The script builds committed HEAD from a Git archive,
uses fresh targets with `--offline --locked`, runs native and generated Node
checks, and emits genuine web-target glue/WASM/declarations plus `network.mjs`, `transaction.mjs`, and their shared `bytes.mjs` guard.
`build.json` records producing commit/tree, lock, tools, graph and exact output
lengths/hashes. It is local build provenance; independently pin its hash in the
consumer. Two fresh builds can be compared byte-for-byte. Dev native checks use
`CARGO_HOME` and `CARGO_TARGET_DIR` pointing to owned external directories.

Initialize the generated entry using verified bytes:

```js
const bindings = await import('./network.mjs'); // verified local copy
bindings.initialize(verifiedWasmBytes);
const context = bindings.consensusContext('zcash-js-network/1', documentBytes, 20);
```

The generated `bindings.js` export is internal and assumes already checked
arguments. Initialization here is wasm-bindgen instantiation, with verified bytes
supplied explicitly (attached Uint8Array, at most 1 MiB; no URL/default fetch).
It is not `runtime_init` or host negotiation.

This packet supplies a lower-level branch context, not a public `ConsensusContext`
with registered `Network`, a runtime profile, or complete G4. Identity/genesis
registration, public defineNetwork, H1 executable loader/worker/ABI, full-client
genesis verification, wallet/storage/signing/proving/outbox remain separate gates.
The SDK integration stays entirely in `qualification/private-bindings-consumer`.

## Internal transaction codec

After the same explicit initialization, import `decodeTransaction(raw, branch)`
from `transaction.mjs`. `branch` is a primitive uint32 with an explicit known
upstream branch value. There is no default or guessed context. Input must be an
ordinary attached same-realm Uint8Array (Buffer/subviews supported), 1 through
2,097,152 bytes. The 2 MiB cap is this private binding's admission policy, not a
consensus maximum. The captured ArrayBuffer intrinsic rejects shared backing
stores even when their prototype is spoofed; length/type checks precede glue.

The frozen result record is `{ bytes: Uint8Array, txid: Uint8Array, display: string }`.
Both arrays are independently owned and intentionally mutable; no Rust handle or
WASM-memory view escapes. `bytes` is the admitted snapshot, returned only after
actual `Transaction::write` is byte-identical to it. `txid` contains the 32 actual
`Transaction::txid()` bytes in internal order, copied by unmodified wasm-bindgen;
`display` is their reversed lowercase hex form. Mutating inputs/results cannot
change other results. Generated `transaction_id` and shared `copyBytes` are
internal implementation exports; consumers use the checked entry.

Rust also bounds input, performs actual `Transaction::read` with full consumption,
requires V3–V6, explicit/embedded branch equality and upstream `valid_in_branch`,
then writes and rejects any changed bytes. This preserves the accepted R1 lossy
V4 rejection. The 13 test-only vectors are exact copies of accepted SDK
`qualification/transaction-codec/fixtures/vectors.json`, SHA256
`26cb21c3733ff8a57b4c99310cfb73331d6376a80a065513932349b497cfbd74`.
Common source is published 1.0.0 at `f4526b0fa86406589732c8fb3849855fb92c43a2`;
the independent qualification report is
`/home/jack/zcash-transaction-codec-logs/review/REPORT-r2.md`.

This is a byte/identity codec. It does not validate signatures, proofs, consensus,
activation, fee/expiry policy or spendability. It does not construct/sign/edit
transactions or implement the public SDK transaction factories. V1/V2 are
unsupported; parsing V3/V4 fixture components is not Sprout wallet support.
V5/V6 identities are effect digests, not hashes of every serialized byte.
Input bounding is not a complete execution-time/memory or panic-recovery budget.

Selected producing commit: `acaf7069e466c82ce1be2c45ebafb75a92b59ee9`;
build metadata SHA256
`09ae852de689eb47fba35dfefaae81397d280f5c2542bccdee9747954efad575`.
The packets `/home/jack/zakura-transaction-bindings-scratch/packet-final` and
`/home/jack/zakura-transaction-bindings-scratch/packet-final-OO` are byte-identical,
including metadata. Native and actual generated Node checks pass.

The selected consumer package is
`/home/jack/zakura-transaction-bindings-scratch/coordinator-final-1`, produced from
SDK runtime source `2c6c9c23b954662750d664aed323d48c19174213`, with SHA256SUMS digest
`dbf2c6891e73f69ae256f1e3ce6f756b7d7a68bb67c47123fc79a5e01cd89e18`.
SDK Node and actual parent Firefox execution passed: 196 network cases,
13 transaction vectors, 208 baseline adapter calls and 1,472 truncated-prefix
rejections. Firefox 155.0.1 also passed 28 network admissions and 23 pre-init
transaction controls. Its nonisolated page did not expose SharedArrayBuffer;
the six shared-store controls are Node evidence.

Actual Firefox receipt:
`/home/jack/zakura-transaction-bindings-logs/browser/firefox-1789157899032.json`;
parent log: `/home/jack/zakura-transaction-bindings-logs/coordinator-browser-final.log`.
Session deletion, driver-group disappearance and server closure are all recorded
true, with no cleanup error and `acceptInsecureCerts: false`.
The SDK consumer README's fresh `mktemp` verification/preparation commands select
`packet-final`; parent verified them in `coordinator-doc-repro.log`.

Final coordinator R1 documentation closure and command record:
`/home/jack/zakura-transaction-bindings-logs/fixes/REPORT.md` and
`/home/jack/zakura-transaction-bindings-logs/fixes/CLIresult.md`.
The original worker `REPORT.md` and `checkpoint.md`, obsolete packets and failed
browser attempt remain immutable historical evidence; its final `CLIresult.md`
is absent and its exit status is unknown. No worker restart is inferred or needed.
Independent review under `/home/jack/zakura-transaction-bindings-logs/review/`
qualified the bounded runtime checks but held final acceptance for R1.
This documentation fix still requires fresh independent review for R1 acceptance.
No push, merge, publication or deployment.

The private stateless lightwallet protobuf production codec is documented in [lightwire/README.md](lightwire/README.md).
