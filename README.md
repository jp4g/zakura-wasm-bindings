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
python3 build.py /home/jack/zakura-transaction-bindings-scratch /home/jack/zakura-transaction-bindings-scratch/packet-1
```

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

Current producer: `70d1c0ec44b56333b84adf7d57e455b291cd85ff`; build metadata SHA256
`d59dc6af3108dc68e70b5060e1e83b6a6dcbb7c35943724774593d53eb38fa9f`.
The two fresh normal/optimized builds are byte-identical. For this pin, build a
detached checkout of that exact revision; later documentation commits produce a
different provenance revision. Native and real generated Node checks pass.
Combined SDK Node consumption passes; new Firefox execution is pending parent
loopback permission (`listen EPERM` in this worker). Browser support remains required.
Reports/commands: `/home/jack/zakura-transaction-bindings-logs/REPORT.md`,
`CLIresult.md`, and `checkpoint.md`. No push, merge, publication or deployment.

## Private wallet storage lifecycle candidate

The optional `wallet-storage` feature and `wallet.mjs` implement an internal schema lifecycle primitive for a later H1 owner. This is **not `wallet_open`, `WalletClient`, or complete recovery**. The branch starts from transaction candidate `acaf706`, which remains under independent review.

`initializeStorage(verifiedWasmBytes, ownedBackend, format, parameters, genesis)` returns an instance-bound storage owner with `binding(generation, instance)` and `close(generation, instance)`. The enclosing host must supply an already validated `zcash-js-network/1` registration and checked genesis; this primitive persists and compares that identity, without making network requests to authenticate it. Parameters lower through the unchanged frozen parser. The backend performs real WalletDb migrations, loads rusqlite's array module and queries the resulting account schema. There are no account/signing/send or arbitrary SQL commands.

The deliberate bound is **one initialization attempt and one active DB per dedicated worker**, with one bundled SQLite instance, no shared memory, a 16 MiB SQLite allocator and a 256 MiB WASM memory maximum. Use a fresh worker for reopening. The raw Rust generation is worker-local; JS also checks a fresh CSPRNG instance identity. Inputs are checked and copied before generated glue and outputs are independent copies. No memory storage fallback exists.

Linux Node uses `wallet-host/node-worker.mjs`, worker-tracked descriptors and `/usr/bin/flock` for cross-process exclusion. Other Node operating systems are unsupported by this bounded adapter. Browser `wallet-host/browser-worker.mjs` uses dedicated-worker OPFS exclusive synchronous access handles. Both enforce TRUNCATE journals, FULL synchronization, temporary memory and a single storage owner. The private packaged worker entries load local artifact bytes; integration with the H1 verified-byte loader and request envelopes remains required before public use.

The host bootstrap commits `ext_wallet_storage` identity before backend migration. Only this primitive's marked databases (or genuinely empty new databases) are admitted; unmarked existing wallets require a separately reviewed adoption interface. Unknown host versions and unknown applied backend migrations reject. Migration errors fail closed; no secret-on-open path is added. Earlier migration transactions can remain committed after interruption; the interrupted transaction rolls back and reopening resumes on the same bound network.

Close errors remain errors, including VFS close errors SQLite itself may ignore. Admission stops, stale operations reject, and the lease remains held until worker destruction after a close failure. Successful close is idempotent at the JS wrapper; raw repeated Rust close rejects. An initialization failure, trap, OOM or worker loss requires domain destruction. Terminating a worker during mutation has an unknown commit outcome and requires reopening, not automatic replay.

Build from committed sources, with the existing cache copied into the assigned scratch `cargo` directory:

```sh
python3 -O build-wallet.py /home/jack/zakura-wallet-storage-scratch/NEW_BUILD
```

The builder runs actual native migrations, unchanged primitive Node tests, actual generated wallet WASM lifecycle/fault tests, process-crash recovery and an import/memory audit. It preserves failed stages and complete provenance. Browser execution requires the ordinary host command `node tests/wallet-firefox.mjs NEW_BUILD/bundle` with the existing packaged Firefox/geckodriver. Workspace loopback denial is not browser evidence; see the external `REPORT.md` and `checkpoint.md` in `/home/jack/zakura-wallet-storage-logs` for exact run status and the hash-pinned parent command. Injected I/O/quota faults, native quota exhaustion and UA eviction are separate claims; the latter two are not established here.

[Source/backend/VFS provenance](wallet-host/PROVENANCE.md). This bounded candidate stops for independent review, without claiming acceptance of issues #2, #4 or #5.
