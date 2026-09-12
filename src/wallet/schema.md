# Pinned host schema admission

The contract is the schema produced by the existing host from an empty database,
with its own version-1 binding marker and the locked `zakura-client-sqlite
0.1.0-rc4` migrations, Orchard and transparent inputs enabled. Native SQLite is
3.50.2 from locked rusqlite 0.37.0. The WASM build uses the same vendored SQLite
with the approved host compile flags; its real Node/Firefox checks remain a
separate acceptance gate. This is structural admission, not general wallet
import, authentication of wallet data, or a guarantee that populated migrations
can finish without a seed. Backend network/key/data checks remain unchanged.

## Contract and ordering

`schema.rs` reads only `user_version`, the full main-schema inventory, the exact
host marker and its singleton typed row, and the exact migration metadata and
all IDs (bounded by the pinned count). A supported, structurally valid marker
with different parameters/genesis yields `NETWORK_MISMATCH`. Malformed marker
parameters yield `SCHEMA_MISMATCH`. Version and canonical bootstrap states,
metadata BLOB type/length/uniqueness, known IDs and dependency closure are checked
before the expected full schema is composed. An empty applied set requires
version 0; a nonempty set requires version 8.

The only bootstrap states are empty/version 0, marker-only/version 0, and marker
plus empty canonical metadata/version 0. Metadata absence in a populated wallet
is not bootstrap. Every table/index/view/trigger and automatic SQLite object
must match. There is no `ext_*`/`sqlite_*` exemption. Physical root pages, schema
cookies and row values are not schema definitions. Comparison is exact stored
SQL, including literals, comments and quoting; it performs no unsafe whitespace
or quoted-string normalization and does not prepare views. These are canonical
producer contracts, not a SQL-equivalence/import API.

The existing shared `validate_schema` call precedes writable policy PRAGMAs and
is repeated by binding. Its implementation delegates to this one contract.
Migration execution and per-migration transactions have not changed. Opening a
wallet with corrupt populated data may still reach the backend's existing
migration/key/seed errors after structural admission; this change does not claim
whole-file rollback for every data-dependent migration failure or hot-journal
recovery scenario.

## Source effects, not sampled schemas

`tests/wallet-schema-source.json` records all 66 pinned source hashes, IDs,
direct dependencies, source SQL block lines and the 414 projected statements.
The source checker verifies those source bytes, IDs, edges and rendered
statements. Its lexical extraction is a consistency check for the frozen audited
blocks, **not a completeness proof from a regex inventory**. A new source hash
requires another source/control-flow audit, not automatic catalog regeneration.

The audited schema path is each successful `RusqliteMigration::up`. Direct
`execute` DDL is included (`add_transparent_value_index`); module constants are
included (pool migration tables/nullifiers); the nullifier constant is executed
after the three column renames, at its actual call position. Conditional
transparent DDL in `transparent_gap_limit_handling` is enabled by the pinned
feature graph. `wallet_summaries::block_deltas` is temporary, not a main-schema
object. Its UPDATE and other DML are not schema effects. `user_version` is
handled explicitly by admission. The migrator disables foreign keys before
replacement transactions; the native projector does so too, and retains each
source `legacy_alter_table` setting.

SQL bodies are retained, including CHECK/UNIQUE/FK clauses, partial indexes,
views and literals. The native test executes this source projection only in
in-memory **test** connections, taking complete before/after schema inventories
for each migration's dependency closure. It derives 294 replacement/tombstone
entries, including automatic index removal/recreation and `sqlite_sequence`.
Thus it includes SQLite's actual indirect rewrites rather than merely names
following CREATE/DROP/ALTER. For example, the three pool-migration column
renames change the dependency table's foreign-key target as well as its own
columns; both complete resulting definitions are captured. Production only
folds the static `schema_catalog.rs` entries; it never replays source SQL or
backend migrations into a reference database during admission.

All schema-changing blocks in the listed migrations are unconditional on a
successful transaction, subject to the fixed feature graph and the explicit
anchor-history exception below. The other conditional operations decode keys,
transactions and PCZTs, backfill/repair rows, derive addresses, manage tree
contents, or error/roll back. The helper paths from `ensure_default_transparent_address`
(`insert_initial_transparent_addrs`), `fix_broken_commitment_trees`
(`truncate_to_height` and tree truncation), `witness_stabilized_notes`
(`mark_stabilized_notes`), address insertion and queue/tree stores change rows,
not persistent schema. In particular, seed-required `ufvk_support` and
`full_account_ids` and PCZT backfill errors are not bypassed by this projection.

Rendered schema constants are pinned source semantics: account-kind codes 0/1,
pool codes 0/2/3, external key scope 0, foreign key scope and legacy address
sentinel -1, shard height 16, and Scanned priority 10. Parameter-dependent
persistent SQL occurs in the three shard scan-range views: Sapling, NU5 and
NU6.3 activation, respectively, or SQL NULL. Those exact positions alone become
runtime activation placeholders. Birthday/fallback heights occurring only in
DML are omitted with that DML; no migration activation data is rewritten in a
fixture or production database.

## Composition scope and checks

Direct dependencies order every overlapping persistent writer in the native
net-effect inventory: all 297 incomparable pairs have disjoint replacement/
tombstone names. Table replacements carry their automatic index effects. ALTER
renames operate on the fixed source identifiers and their stored references;
the whole-inventory delta includes the affected FK/view/index definitions.
The native composition test deliberately applies every incomparable pair in
both orders, in both the minimal union of its ancestor closures and the maximal
closed predecessor context excluding both migrations and their descendants.
It compares both inventories and the static-effect fold (594 contexts).

The compositional argument depends on source effect completeness and context
invariance, not just these test counts. For the fixed CREATE/DROP/ALTER paths,
every optional object's introduction/replacement is itself an enumerated writer;
the pair checks exercise its interaction with the other writer's alterations.
No source branch chooses a different persistent definition based on arbitrary
wallet data in the supported history. Disjoint fixed effects commute, and
swapping adjacent incomparable migrations connects topological orders. This is
the implementation's bounded source argument plus native counterexample checks;
it is not an independent HIGH review or an exhaustive enumeration of all closed
sets. Observed backend prefixes never determine catalog membership.

## Historical variants and remaining exceptions

The producer scope is explicit: the host's locked RC4-from-empty history. Its
binding marker is established before the same pinned backend creates tables;
there is no general historical-wallet import/binding operation. Previously
accepting arbitrary marker-shaped schemas was the admission bug, not an import
contract. The source comments documenting other upstream producers are not
silently generalized into this contract:

- `orchard_ironwood_migration_tables:57-73` documents earlier released creators
  without `DEFAULT 144`. This RC4 creator includes it. The anchor repair tests
  column presence, so it is a schema no-op on the supported history. It does not
  replace a missing default in an earlier producer's existing column.
- `orchard_ironwood_migration_anchor_interval:1-17,58-94` also repairs an absent
  column. The abbreviated upstream test predecessor lacks the production FK;
  it is not a complete historical producer contract and is not admitted.
- `ufvk_support` branches on an older `sent_notes.output_pool` input for DML,
  then replaces it with the fixed table. The host's actual predecessor is
  `initial_setup`; arbitrary older input columns are not implicitly permitted.

There are no intentionally omitted persistent effects for the stated producer.
Broader historical producer admission remains outside this contract and would
need explicit full source-backed predecessor contracts and separate review.
The six empty effects are the five data-only migrations listed in the inventory
and the RC4-specific anchor repair no-op. Empty effects are not fabricated
applied IDs: real backend migration transactions still record their own UUIDs.

## Qualification and reproduction

Use the commands and exact outcomes in the authorized-r3 REPORT/CLIresult.
The source-native test writes a native-effect receipt; `wallet-schema-catalog.py`
serializes it and verifies the pinned migration source hashes. The generated
catalog must remain byte-identical on regeneration. The source checker runs
before generation. Tests under `tests/wallet-schema/` are compiled as private
unit tests so fixture generation uses the exact private `Document::parse`, with
no public testing hook, alternative network codec or dependency modification.

The retained-fixture test is explicitly ignored by default because it needs
external immutable fixture receipts. It is mandatory in qualification and is
run with `WALLET_SCHEMA_INVENTORY=...` and `--include-ignored` (or its exact filter
with `--ignored`). The ordinary builder's native pass alone therefore does not
qualify that retained corpus. The full final worker run includes it; the parent
command includes it too.

All original 67 fixtures remain unchanged: 54 are explicit parameter-contradiction
negatives, 13 are not certified from the absence of that counterexample. New
correctly parameterized controls use unchanged backend transactions and real
commit veto rollback. Their hashes, marker/parameter bytes, DDL hashes and
applied sets are retained. The corrected boundary 13 in the first GREEN corpus
has Orchard but no account birthdays; native SELECT preparation fails there,
but schema admission and real recovery succeed.

The unchanged build04 WASM is RED evidence only. The parent must compose this
commit with its separately owned inode fix, build from the original producer
root, run the real Node schema/corpus and existing crash/admission/lifecycle
checks, run ordinary Firefox OPFS plus the derived schema harness, retain final
process/cleanup evidence, and obtain independent HIGH review. Preparing the
browser harness or running native code does not satisfy those gates.

## Per-migration source/effect inventory

Full hashes and UUIDs are in the source JSON. Lines refer to the pinned migration module.

| Migration | Source SQL block lines | Net entries |
|---|---|---:|
| `initial_setup` | 38 | 11 |
| `utxos_table` | 35 | 2 |
| `ufvk_support` | 63, 196, 206, 292 | 2 |
| `addresses_table` | 51, 171 | 3 |
| `add_utxo_account` | 53, 91, 117 | 1 |
| `sent_notes_to_internal` | 39, 69 | 1 |
| `add_transaction_views` | 63, 139, 165, 191 | 4 |
| `v_transactions_net` | 56, 61, 107 | 4 |
| `received_notes_nullable_nf` | 41, 62, 77, 124, 216 | 9 |
| `shardtree_support` | 63, 71, 90, 246 | 11 |
| `orchard_shardtree` | 47, 66, 80, 113, 133 | 9 |
| `receiving_key_scopes` | 110 | 1 |
| `add_account_birthdays` | 41 | 1 |
| `sapling_memo_consistency` | 130 | 1 |
| `v_transactions_transparent_history` | 40, 140 | 2 |
| `v_tx_outputs_use_legacy_false` | 41 | 1 |
| `v_transactions_shielding_balance` | 40 | 1 |
| `v_transactions_note_uniqueness` | 40 | 1 |
| `v_sapling_shard_unscanned_ranges` | 45, 78 | 2 |
| `wallet_summaries` | 42, 48, 65 | 2 |
| `full_account_ids` | 69, 249, 411 | 24 |
| `orchard_received_notes` | 41, 83, 121, 140, 270 | 11 |
| `ensure_orchard_ua_receiver` | data only | 0 |
| `utxos_to_txos` | 36 | 14 |
| `ephemeral_addresses` | 108 | 2 |
| `spend_key_available` | 38 | 1 |
| `nullifier_map` | 44 | 6 |
| `tx_retrieval_queue` | 72 | 7 |
| `tx_retrieval_queue_expiry` | 41 | 1 |
| `support_legacy_sqlite` | 35 | 1 |
| `fix_broken_commitment_trees` | data only | 0 |
| `fix_bad_change_flagging` | data only | 0 |
| `add_account_uuids` | 57, 139 | 7 |
| `v_transactions_additional_totals` | 40 | 1 |
| `transparent_gap_limit_handling` | 174, 295, 437, 476, 621, 656 | 14 |
| `ensure_default_transparent_address` | data only | 0 |
| `fix_transparent_received_outputs` | 46 | 1 |
| `support_zcashd_wallet_import` | 41 | 5 |
| `fix_v_transactions_expired_unmined` | 38 | 1 |
| `v_received_output_spends_account` | 38 | 1 |
| `v_tx_outputs_return_addrs` | 38 | 1 |
| `tx_observation_height` | 58 | 1 |
| `add_transaction_trust_marker` | 41 | 2 |
| `account_delete_cascade` | 50, 100, 135, 157, 193, 215, 246, 268, 292, 323, 346, 367 | 48 |
| `standalone_p2sh` | 47 | 6 |
| `add_transparent_receiver_address_index` | 309 | 1 |
| `add_transparent_value_index` | 61 | 1 |
| `ironwood_shardtree` | 53, 60, 79, 96, 129, 149 | 11 |
| `witness_stabilized_notes` | 53 | 4 |
| `orchard_note_version` | 48 | 1 |
| `ironwood_received_notes` | 49 | 11 |
| `fix_bad_ironwood_change_flagging` | data only | 0 |
| `ironwood_pool_code_views` | 50 | 2 |
| `ivk_item_cache` | 106, 184 | 5 |
| `note_locking` | 45 | 4 |
| `orchard_ironwood_migration_tables` | 65 | 15 |
| `orchard_ironwood_migration_anchor_interval` | 83 | 0 |
| `orchard_ironwood_migration_unsatisfiability` | 105, 256, 278, 353 | 5 |
| `tree_retained_checkpoints` | 41 | 2 |
| `tx_status_observation_intent` | 91 | 2 |
| `v_address_uses_ironwood` | 64 | 1 |
| `v_transactions_pool_crossing` | 70 | 1 |
| `zip318_classification` | 69 | 1 |
| `v_transactions_zip318_kind` | 60 | 1 |
| `v_tx_outputs_key_scopes` | 37 | 1 |
| `v_tx_outputs_transparent_addresses` | 63 | 1 |
