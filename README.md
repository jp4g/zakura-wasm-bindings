# Zakura private network bindings

A `publish = false` production Rust binding crate for the bounded network-document
operation related to zcash.js #4. Exact dependencies are `zcash_protocol 0.10.6`
(no default features) and genuine `wasm-bindgen 0.2.128`, with the full Cargo lock.

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
occurs. Seed an owned scratch Cargo registry with the 17 exact lock-checksummed
crate archives and matching existing registry index (this run records that copy
in `/home/jack/zakura-bindings-network-logs/cache-provenance.json`). Never write to
another worker's or global cache. All output/cache/log paths must be external.

```sh
python3 build.py /home/jack/zakura-bindings-network-scratch /home/jack/zakura-bindings-network-scratch/packet-1
```

The destination must be new. Scope, tool identity, and committed-script guards
remain active under Python `-O` and `-OO`. Run the nonexecuting guard regression
with `python3 tests/test_build_guards.py` (also with `-O` and `-OO`). The script builds committed HEAD from a Git archive,
uses fresh targets with `--offline --locked`, runs native and generated Node
checks, and emits genuine web-target glue/WASM/declarations plus `network.mjs`.
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
