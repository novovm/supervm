# NOVOVM Generic Native Transaction AOEM State Ownership v1

Status: production candidate, explicitly enabled by the product package. The
host Adapter owns product semantics and domain-neutral AOEM Semantic Graph V3
owns canonical state and receipt persistence. This ownership gate is not a
consensus proof or a proof-sealed block-finality claim.

## Ownership boundary

NOVOVM/SUPERVM owns:

- native ingress authentication, chain-domain and nonce policy;
- transaction semantics and all product business policy;
- deterministic lowering from native execution results to opaque atomic
  key/value writes;
- query projection and verification.

AOEM owns:

- Semantic Graph V3 scheduling;
- bounded atomic write admission;
- the RocksDB writer lifecycle;
- durable ordered events;
- completion-write publication;
- canonical state and receipt persistence.

AOEM does not contain NOVOVM transaction, account, asset, receipt, route, or
module logic. Task kinds, payloads, keys, values, graph identities, and event
payloads remain opaque to AOEM.

## Commit protocol

The host Adapter serializes the authenticated, deterministically computed
state envelope and splits it into values no larger than the AOEM V3 fixed
limit. At most four chunks are emitted by one graph step. Every step carries
one durable event.

AOEM persists all accepted step writes. Only after those writes complete does
the mandatory completion-write callback publish the compact head record. The
head binds:

- the v2 `production_accepted=true` marker and pre-publish gate contract;
- batch result identity;
- envelope length and chunk count;
- envelope digest;
- state root;
- receipt root.

Recovery reads the head and chunks through
`aoem_storage_provider_wire_v1`, reconstructs the envelope, verifies its
digest and roots, and rejects partial or mismatched state. Legacy v1 heads do
not contain the production-acceptance marker and are rejected rather than
silently promoted; development databases must be cleared or explicitly
migrated before using this build.

## Production activation

Generic native transaction ingress keeps the AOEM-owned production gate off by
default for development and library callers. The product Linux package turns
it on explicitly with
`NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE=true` and requires the raw
AOEM semantic precommit with
`NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED=true`. This prevents differences
between machines from silently selecting a production owner.

The default FULLMAX core runtime uses its generic storage-provider RocksDB
surface. An explicit `NOVOVM_AOEM_PERSIST_BACKEND=none` is treated as an
operator override and fails the AOEM ownership gate closed. The separate
FULLMAX persist sidecar remains supported and is exercised by the restart
recovery gate.

## Required AOEM contract

The production owner requires the generic AOEM capability boundary:

```text
semantic_graph_v3 = true
semantic_graph_v3_domain_agnostic = true
semantic_graph_v3_opaque_task_payload = true
semantic_graph_v3_host_business_policy_owner = host
semantic_graph_v3_atomic_step_commit = true
semantic_graph_v3_durable_completion_boundary = true
```

The corresponding V3 and storage-provider symbols must also be exported. No
NOVOVM-specific AOEM opcode or capability is required.

## Runtime evidence

A successful production-owned batch reports:

```text
business_semantic_planner = supervm_host_adapter
business_policy_owner = SUPERVM_host
host_adapter_lowering_completed = true
semantic_graph_v3_ready = true
semantic_graph_v3_domain_agnostic = true
aoem_domain_specific_logic = false
canonical_state_transition_owned_by_aoem = true
receipt_owner = AOEM
legacy_host_canonical_write = false
```

The host-side JSON/RocksDB save that follows a successful AOEM commit is a
query projection, not a second canonical owner.

## Paths

No drive letter or workspace name is required. For development and tests, the
AOEM-owned database path is resolved in this order:

1. environment variable `NOVOVM_AOEM_OWNED_STATE_DB_PATH`;
2. request parameter `aoem_owned_state_db_path`;
3. the configured native execution-store path plus
   `.aoem-owned.rocksdb`.

Production-owned ingress rejects request-level persistence-path and namespace
overrides. Those values are node configuration, so every machine may use its
own local paths while sharing the same chain and AOEM state namespace.

The default is derived from the configured native execution-store path on every
development machine. It never depends on a drive letter or workspace parent
name.

The AOEM runtime persistence path used by recovery gates is derived from the
same temporary/store path. Gate execution therefore does not require
machine-specific absolute-path configuration.

## Fail-closed rules

The production owner is rejected when:

- the domain-neutral V3 capability or symbols are missing;
- host lowering did not complete;
- a graph step, event, key, or value exceeds AOEM bounds;
- graph execution or the completion write fails;
- readback is missing or differs;
- state-root or receipt-root parity fails;
- any AOEM domain-specific business logic is reported;
- a legacy host canonical write is detected.

## AOEM ownership seal

The v1 ownership seal requires all of the following with the AOEM-owned path
enabled and legacy fallback disabled. It does not establish block consensus or
execution-validity proof finality:

- AOEM-owned Semantic Graph V3 state reopens after process exit, with state
  root, receipt root, cumulative receipt count, and capability contract
  verified;
- host query/lifecycle projection reopens independently;
- restart performs no duplicate execution or canonical inclusion;
- pending-crash recovery preserves already committed transactions;
- remote NovoRUDP reentry cannot duplicate receipts, state-head advances, or
  canonical inclusion;
- packet loss, duplication, delay, and reordering converge without a legacy
  canonical write;
- an idle tick after successful execution is neutral and cannot overwrite the
  last valid AOEM ownership evidence.
