# NOVOVM AOEM Runtime Ownership Phase 1

Date: 2026-06-21

## Status

```text
Phase 1 status: IMPLEMENTATION BASELINE
NovoRUDP 2h sustained gate: PAUSED / ARCHITECTURE BLOCKED
Reason: host-native RocksDB execution store is legacy transitional path
```

The 30min NovoRUDP signed baseline remains a regression baseline. It is not the final production runtime architecture.

## Architecture Rule

SUPERVM layering must be enforced as follows:

```text
Network:
  transport, retransmission, ACK/repair, pending ingress

Consensus / business protocol:
  ordering semantics, validation, proof verification, acceptance rules

AOEM runtime:
  high-concurrency execution
  state transition
  receipt production
  canonical proof
  snapshot/readback
  RocksDB-backed persistence
```

Network must not own business consensus.

Consensus/business logic must not own concurrent compute scheduling or persistence.

AOEM must own runtime execution, state mutation, and persistence close.

## Phase 1 Scope

This phase begins the ownership correction by exposing AOEM state/persistence surface to SUPERVM.

Implemented boundary:

```text
aoem_state_write_v1 binding
aoem_state_read_v1 binding
aoem_state_snapshot_v1 binding
AOEM state surface capability checks
AOEM RocksDB/persist capability checks
host native store marked legacy_host_transitional
```

Not in this phase:

```text
full NOVOVM_AOEM_NATIVE_TX_BATCH_V1
full tx_ingress migration
consensus rewrite
NovoRUDP transport changes
2h sustained signoff
host RocksDB hot-path optimization
```

## Code Boundary

`crates/aoem-bindings` now treats the AOEM state surface as first-class FFI:

```text
aoem_state_write_v1
aoem_state_read_v1
aoem_state_snapshot_v1
```

`crates/novovm-exec` capability contract now distinguishes:

```text
execute_ops_wire_v1
state_write_v1
state_read_v1
state_snapshot_v1
state_surface_v1
rocksdb_persistence
persist_delegate_runtime
runtime_ownership_ready
```

`crates/novovm-node/src/tx_ingress.rs` now reports the current host-native store path as:

```text
runtime_ownership = legacy_host_transitional
production_target = false
replacement_target = aoem_runtime_owned_state_persistence
```

This prevents the existing host store from being mistaken for the target production runtime path.

## Acceptance Meaning

Phase 1 does not claim that transactions are already AOEM-owned end-to-end.

Phase 1 only establishes:

```text
SUPERVM can recognize and bind AOEM state/persistence ownership primitives.
The current host-native store path is explicitly demoted to transitional status.
Future tx_ingress work must target AOEM-owned state/persistence.
```

## Next Phase

Phase 2 must define:

```text
NOVOVM_AOEM_NATIVE_TX_BATCH_V1
```

The ABI must move this ownership into AOEM and must be algebraic semantic first:

```text
raw tx batch
structured execution subject
ordering metadata
receipt output
canonical proof
state delta/root
durable ledger close proof
snapshot metadata
```

The binary form used by NovoRUDP, FFI, and RocksDB is only a deterministic
carrier. The source of truth must be:

```text
canonical algebraic transaction terms
canonical term ordering
state transition operators
receipt/canonical proof terms
AOEM semantic state deltas
```

This preserves the intended SUPERVM layering:

```text
network transports canonical semantic frames
consensus validates semantic/proof terms
AOEM executes and persists algebraic runtime state
```

Only after Phase 2 and Phase 3 should NovoRUDP 2h sustained be resumed as a production architecture gate.
