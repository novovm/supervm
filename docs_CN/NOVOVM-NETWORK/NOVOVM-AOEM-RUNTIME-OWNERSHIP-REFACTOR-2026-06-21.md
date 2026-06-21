# NOVOVM AOEM Runtime Ownership Refactor

Date: 2026-06-21

## Decision

NOVOVM must stop treating AOEM as a semantic precommit helper followed by a host-managed RocksDB execution store.

The target architecture is:

```text
Network / NovoRUDP
  -> pending ingress / ordering envelope
  -> AOEM runtime ABI
  -> AOEM-owned execution/state/persistence
  -> AOEM receipt/canonical/durable-close proof
  -> SUPERVM readback/report/consensus validation
```

RocksDB is an AOEM backend extension in the target model. It must not be independently scheduled by the SUPERVM hot path as the authoritative transaction execution store.

## Why This Refactor Is Required

The 30min NovoRUDP gate proved the network repair loop can close a small sustained run. The 2h gate exposed a deeper architectural problem:

```text
A sender ~= 8 TPS
B UDP ingress ~= 8 TPS
B primary drain/canonical/ledger ~= 5 TPS
```

The live evidence showed the bottleneck was not sender, ACK, UDP delivery, or canonical summary. The hot path still materializes/clones and commits a host-native execution store after AOEM precommit:

```text
model = post_aoem_deterministic_dirty_store_commit
precommit_store_materialized = true
materialization_risk = rocksdb_full_receipt_materialization_before_dirty_commit
```

That means AOEM is not the runtime owner. SUPERVM is currently re-implementing runtime persistence and lifecycle closure outside AOEM.

## AOEM Source Evidence

AOEM already exposes the required direction:

```text
crates/storage-backend:
  RocksDBStorage
  write_batch
  production persistent storage backend

crates/runtime/aoem-runtime-core:
  StateBackend
  RocksDbBackend
  batch_write

crates/core/aoem-state-kv:
  PersistentKvState
  write_batch_with_version
  persist_batch_with_version

crates/ffi/aoem-ffi:
  aoem_execute_batch
  aoem_execute_ops_wire_v1
  aoem_state_write_v1
  aoem_state_read_v1
  aoem_state_snapshot_v1
  AOEM_PERSISTENCE_PATH / persist delegate
```

AOEM documentation also defines the hosted production route:

```text
host
  -> aoem_execute_ops_wire_v1
  -> AOEM state namespace
  -> aoem_state_read_v1
```

## Current SUPERVM Mismatch

SUPERVM currently binds only the partial AOEM runtime surface needed for compute/readback:

```text
aoem_execute_ops_wire_v1
aoem_state_read_v1
```

It does not yet bind/use the full runtime ownership surface:

```text
aoem_state_write_v1
aoem_state_snapshot_v1
AOEM persistence ownership
NOVOVM transaction lifecycle proof readback
```

The current native transaction path still does this:

```text
raw tx batch
  -> execute_native_raw_tx_batch_chunks_via_aoem_semantic_ingress_v1
  -> load_nov_native_execution_store_v1
  -> clone previous store
  -> dispatch_nov_execution_request_into_loaded_store_v1
  -> mutate host NovNativeExecutionStoreV1
  -> save_nov_native_execution_store_with_previous_v1
```

This is acceptable only as a transitional compatibility path. It is not the final NOVOVM/AOEM architecture.

## Non-Negotiable Ownership Rules

1. AOEM owns execution state mutation.
2. AOEM owns RocksDB-backed persistence for runtime state.
3. AOEM owns receipt/canonical proof production for executed transactions.
4. SUPERVM must not materialize/clone the full receipt store in the hot path.
5. SUPERVM must not independently close durable ledger state without AOEM proof.
6. NovoRUDP ACK/durable-missing logic may consume AOEM lifecycle proof, but must not manufacture execution completion.
7. SUPERVM may keep network pending buffers, diagnostics, consensus validation, and read-only mirrors.

## Target ABI Contract

Introduce or standardize a NOVOVM-specific AOEM runtime wire profile:

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_V1
```

Input:

```text
chain_id
batch_id
sequence range
raw tx bytes
tx hash
execution subject
requested execution behavior
ordering metadata
```

AOEM output:

```text
batch_id
per-tx execution status
per-tx receipt
canonical inclusion proof
state delta root
durable close proof
version / snapshot metadata
failure classification
```

The output must be sufficient for SUPERVM to update NovoRUDP durable ledger without writing the execution store itself.

## Migration Plan

### Phase 0: Freeze Host-Store Expansion

Stop adding new host-side RocksDB scheduling fixes for the 2h gate. Keep the signed 30min tag as historical baseline only.

### Phase 1: Binding Surface

Extend `crates/aoem-bindings` to bind and test:

```text
aoem_state_write_v1
aoem_state_snapshot_v1
AOEM persistence capability flags
AOEM_PERSISTENCE_PATH startup behavior
```

Add a smoke test proving SUPERVM can create AOEM with persistence enabled and read back state through AOEM.

### Phase 2: AOEM Native Tx Wire Profile

Define the NOVOVM transaction batch wire envelope under AOEM ownership.

Do not route it through host `NovNativeExecutionStoreV1` mutation.

### Phase 3: SUPERVM Host Compatibility Adapter

Replace the current host-native execution commit path with:

```text
raw tx batch
  -> AOEM native tx wire batch
  -> AOEM result/proof readback
  -> SUPERVM pending/canonical mirror update
  -> NovoRUDP durable ledger close by AOEM proof
```

The old host store can remain as a read-only mirror or compatibility snapshot, not as the writer of truth.

### Phase 4: Gates

Run gates in this order:

```text
AOEM persistent tx batch smoke
SUPERVM local 64 tx AOEM-owned path
NovoRUDP 5min / 2400 AOEM-owned path
NovoRUDP 30min / 14400 regression
NovoRUDP 2h / 57600
fault profile
```

## Explicitly Deprecated Path

The following path is deprecated for production:

```text
AOEM semantic ingress
  -> host load/clone NovNativeExecutionStoreV1
  -> host mutate receipts/module_state
  -> host RocksDB commit
  -> host canonical projection
```

It may remain behind a compatibility flag for replay and migration, but must not be the default NovoRUDP sustained path.

## Immediate Next Task

Start with:

```text
NOVOVM AOEM Runtime Ownership Phase 1:
Bind AOEM state_write/state_snapshot/persistence and prove AOEM-owned persistent state roundtrip from SUPERVM.
```

Do not resume the 2h NovoRUDP gate until Phase 1 and Phase 2 prove that AOEM is the runtime state owner.
