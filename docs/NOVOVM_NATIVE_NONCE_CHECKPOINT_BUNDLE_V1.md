# NOVOVM Native Nonce Checkpoint Bundle V1

Status: size-limited offline evidence export and verification for the
[Native Nonce Identity V2](NOVOVM_NATIVE_NONCE_IDENTITY_V2.md) migration preflight.
This is not state import, protocol activation, execution replay, QC verification,
or permission to upgrade a running legacy chain.

## Purpose and trust boundary

The pure migration planner requires a complete legacy Host-store JSON snapshot
and the complete ordered, signed `Execute` transaction history. A collection of
hashed nonce keys cannot reconstruct that history. The checkpoint bundle makes
those exact inputs portable between development machines and binds them to a
specific local ledger checkpoint.

Verification has two separate prerequisites:

1. The operator obtains the expected chain ID, namespace digest, legacy protocol
   commitment, tip block hash and snapshot digest from an independently known
   checkpoint or an authoritative cross-device handoff.
2. The verifier checks the supplied bundle against those expected anchors and
   checks its internal ledger, snapshot and nonce commitments.

`inspect` only reports what its input files say. Copying those observed values
straight back into `export` or `verify` checks local consistency, not independent
provenance. A bundle digest received together with an untrusted bundle does not
authenticate its source. Even an independently pinned checkpoint is not a QC or
a proof that the ledger's business execution was correct.

No command opens AOEM execution, changes a protocol pin, modifies balances or
nonces, writes a ledger, activates V2, or restarts a running node. NOV-specific
checkpoint and migration semantics remain in SUPERVM; AOEM's generic runtime
and DLL are unchanged.

## Inputs and safe offline workflow

Use an explicitly selected, trusted, coherent stopped-node copy of the legacy
Host-store JSON and its matching ledger RocksDB. Do not point these commands
at a running node's live directory or an untrusted database download. Creating
a valid snapshot/database copy is an operator step; this CLI does not stop
services or make a coordinated live backup.

Source paths have no automatic defaults and do not depend on a particular
drive, parent-directory name, or another machine's checkout location. The
examples below use paths relative to the current working directory. Replace
the angle-bracket placeholders with independently established values; they
are not literal shell arguments. Digest/hash arguments are canonical lowercase
32-byte hex strings without a `0x` prefix.

Inspect a copied checkpoint:

```text
novovmctl native-nonce-migration inspect --snapshot ./checkpoint/legacy-store.json --ledger ./checkpoint/ledger --chain-id <chain-id>
```

Export after establishing the expected anchors independently:

```text
novovmctl native-nonce-migration export --snapshot ./checkpoint/legacy-store.json --ledger ./checkpoint/ledger --chain-id <chain-id> --namespace-digest <namespace-digest> --legacy-protocol-commitment <legacy-protocol-commitment> --tip-block-hash <tip-block-hash> --snapshot-digest <snapshot-digest> --bundle-out ./checkpoint/nonce-checkpoint-v1.bin
```

Transfer the bundle and communicate its expected digest and checkpoint anchors
through the authoritative handoff. Verify on another machine without access to
the original database:

```text
novovmctl native-nonce-migration verify --bundle ./checkpoint/nonce-checkpoint-v1.bin --bundle-digest <bundle-digest> --chain-id <chain-id> --namespace-digest <namespace-digest> --legacy-protocol-commitment <legacy-protocol-commitment> --tip-block-hash <tip-block-hash> --snapshot-digest <snapshot-digest>
```

The exporter uses a read-only ledger wrapper that rejects mutations. A fresh
CLI process opens RocksDB read-only; an in-process library caller can reuse an
existing registered database handle, including one originally opened for
writing. The wrapper still cannot write, but this does not freeze other
handles or provide snapshot isolation against a concurrent writer. A coherent
stopped-node copy remains required.

Source preservation is checked at the ledger key/value and exact snapshot-byte
level, not as byte-identical physical RocksDB directory files. Engine diagnostic
files are outside that assertion.

Missing databases, absent ownership or domain metadata, and an outstanding
prepared slot reject instead of being created, repaired, resumed, or treated
as empty genesis. Export creates a new output file exclusively and rejects an
existing destination; it never overwrites an earlier bundle. The output's
parent directory must already exist and the destination must be outside the
ledger directory. The output path must be Unicode-representable in the JSON
report; unsupported paths reject before output creation. Directory resolution
assumes an operator-controlled filesystem, not hostile concurrent directory
replacement. A failed or interrupted export is not a usable bundle:
verification must succeed before any handoff is accepted.

## Bundle and verification contract

The deterministic framed binary format starts with `NVNCPK1\0` and carries the
exact source snapshot bytes, ledger-head JSON and full block JSON records in
height order. Each payload has a little-endian `u32` byte length; the block
count is a little-endian `u32` after the head frame, and trailing bytes reject.
It does not replace the source snapshot with a reserialized or
silently upgraded store. Framing lengths and aggregate sizes are checked
before admitting payloads. The supported limits are:

| Input | Maximum |
| --- | ---: |
| Snapshot JSON | 16 MiB |
| Ledger-head JSON | 8 KiB |
| One block JSON | 16 MiB |
| Full-history blocks | 512 (at least 1 required) |
| Entire bundle | 96 MiB |
| Raw signed transactions | 65,536 |
| Raw transaction bytes, total | 64 MiB |
| One raw transaction | 1 MiB |

These are encoded input and accepted-history limits, not a hard process-memory
or RSS bound. Source RocksDB reads use the shared `DB::get` and JSON decoding
path, which can allocate a database value and deserialize it before the export
frame-size checks. The source-database path is not a resource-isolated hostile
input parser. Bundle framing checks lengths before decoding each frame, but
JSON/object allocations and verification copies still exceed encoded byte
counts. Do not interpret the 96 MiB bundle limit as a 96 MiB memory ceiling.

The verifier requires the complete block sequence from height 1 through the
pinned tip, not a partial range or a trusted-looking suffix. It checks the
parent chain, block/body/receipt commitments, exact raw transaction hashes and
signed transaction order, snapshot receipt coverage and tip-root agreement,
and ledger-head counters. State version advances once per committed
transaction, including a transaction with a rejected business-execution
receipt, not once per block: the first block's version equals its transaction
count, and each successor adds its own count to the parent version. Snapshot
V3 state and V2 cumulative receipt roots must match the tip. Unaccounted
pre-chain AOEM state is not accepted as implicit genesis.

The verifier then runs the pure migration planner to verify
signatures and subject/chain authority, reconstruct the legacy nonce maps
exactly, and derive continuous V2 signer nonce maps. Historical reuse of the
same canonical signer nonce through different aliases rejects; it is not
resolved by taking a maximum, renumbering transactions, or dropping receipts.

Successful verification establishes commitment consistency for the supplied
and pinned local checkpoint. It does not independently prove snapshot
provenance, replay historical business execution, validate AOEM execution
evidence, establish data availability for remote validators, verify QC, select
a fork, or prove chain finality. In particular:

```text
activation_ready = false
import_performed = false
```

Neither bundle export nor verification produces a new authoritative state,
V2 genesis, upgraded protocol pin, canonical promotion, or finalized block.
No automatic migration RPC, startup fallback, or old-state execution bypass
is added. A later upgrade must define and verify an explicit state transition
or an explicitly authorized new-chain/bootstrap procedure.
The subsequent [upgrade staging slice](NOVOVM_NATIVE_NONCE_UPGRADE_STAGING_V1.md)
constructs a deterministic proposed transition in a separate resumable
artifact workspace. It does not import or activate that proposal.

## Required tests and gate

Core codec, ledger binding and rejection tests run with the existing native
nonce library filter:

```sh
cargo test -p novovm-node --lib native_nonce -- --test-threads=1
```

The new required `test_native_nonce_checkpoint_bundle` gate field starts false
and becomes true only after the CLI filter succeeds:

```sh
cargo test -p novovmctl native_nonce -- --test-threads=1
```

This includes the `native_nonce_checkpoint_cli` real-binary integration test.
This slice introduced the 46-field lockset. The subsequent
[upgrade staging slice](NOVOVM_NATIVE_NONCE_UPGRADE_STAGING_V1.md) adds
`test_native_nonce_upgrade_staging`. The later
[upgrade authorization slice](NOVOVM_NATIVE_NONCE_UPGRADE_AUTHORIZATION_V1.md)
adds `test_native_nonce_upgrade_authorization`; the subsequent
[source prepare-QC slice](NOVOVM_NATIVE_NONCE_SOURCE_QC_V1.md) adds
`test_native_nonce_source_qc`. The later
[native seal new-view slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_V1.md) adds
`test_native_seal_new_view`, extending the contract to 50 fields. The subsequent
[new-view candidate admission slice](NOVOVM_NATIVE_SEAL_NEW_VIEW_ADMISSION_V1.md)
adds `test_native_seal_new_view_admission`; producer, preflight and node-runtime
locksets now contain 51 required fields in the same order.
Missing or false bundle, staging, authorization, source-QC, new-view or
new-view admission evidence rejects; older
reports must be regenerated rather than accepted as current sign-off.

Required negative cases cover tampered/truncated/oversized framing, missing or
reordered blocks, incorrect checkpoint anchors, snapshot/root/receipt/raw-wire
mismatches, duplicate or discontinuous nonce history, incomplete or prepared
ledgers, and existing output destinations. Verify must remain independent of
source database paths and must not mutate the copied source.

These are verification requirements, not a record of executed tests. Record
actual command results separately. Local success does not establish Linux CI,
physical multi-device operation, public-network readiness, long-run stability,
or completion of legacy-chain migration.
