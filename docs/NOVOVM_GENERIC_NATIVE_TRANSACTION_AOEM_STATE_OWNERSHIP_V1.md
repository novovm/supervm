# NOVOVM Generic Native Transaction AOEM State Ownership v1

Status: production candidate, explicitly enabled by the product package. The
host Adapter owns product semantics and computes every NOV business-state
transition. Domain-neutral AOEM Semantic Graph V3 executes the resulting opaque
atomic-write graph and owns authoritative state and receipt persistence. This
ownership gate is not a consensus proof or a proof-sealed block-finality claim.

## Ownership boundary

NOVOVM/SUPERVM owns:

- native ingress authentication, chain-domain and nonce policy;
- transaction semantics and all product business policy;
- deterministic computation of balances, fees, receipts, policy effects, and
  every other NOV business-state transition;
- deterministic lowering of those computed results to opaque atomic key/value
  writes;
- query projection and verification.

AOEM owns:

- Semantic Graph V3 scheduling;
- bounded atomic write admission;
- the RocksDB writer lifecycle;
- durable ordered events;
- completion-write publication;
- authoritative state and receipt persistence and readback.

AOEM does not contain NOVOVM transaction, account, asset, receipt, route, or
module logic. Task kinds, payloads, keys, values, graph identities, and event
payloads remain opaque to AOEM. Executing the domain-neutral graph does not make
AOEM the owner of NOV business-transition computation; the Host has already
computed that transition before lowering it to the graph.

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

New production commits use `novovm-consensus-native-state-wire/v1` and
`novovm-consensus-receipt-wire/v1`. The state root binds the compact AOEM
semantic sequence/head and a canonical commitment to every environment value
that can select Host business policy plus its compiled defaults. Once this
protocol-config commitment is present in authoritative state, configuration
drift fails closed. A legacy JSON state root remains loadable only for explicit
compatibility/recovery and cannot be committed into a new durable block.

## Production activation

Generic native transaction ingress keeps the AOEM-owned production gate off by
default for development and library callers. The product Linux package turns
it on explicitly with
`NOVOVM_AOEM_NATIVE_TX_BATCH_PRODUCTION_CANDIDATE=true` and requires the raw
AOEM semantic precommit with
`NOVOVM_NATIVE_AOEM_SEMANTIC_INGRESS_REQUIRED=true`. This prevents differences
between machines from silently selecting a production owner.

Before enabling that owner, each machine must run
`NOVOVM_NODE_MODE=native_protocol_config_commitment novovm-node` against the
same binary/configuration and set the reported value as
`NOVOVM_NATIVE_PROTOCOL_CONFIG_EXPECTED_COMMITMENT`. The four pins must match;
the command does not depend on a shared drive letter or open AOEM state.

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
business_transition_computation_owner = SUPERVM_host
host_adapter_lowering_completed = true
semantic_graph_v3_ready = true
semantic_graph_v3_domain_agnostic = true
aoem_domain_specific_logic = false
domain_neutral_graph_execution_owned_by_aoem = true
authoritative_state_persistence_owned_by_aoem = true
receipt_owner = AOEM
legacy_host_canonical_write = false
```

The former phrase `canonical_state_transition_owned_by_aoem` is not an accurate
description of this boundary and must not be used as a business-computation
claim. AOEM owns execution and durable publication of the opaque graph; the Host
owns the NOV transition that produced that graph. Likewise, `receipt_owner =
AOEM` means authoritative receipt persistence and verified readback, not that
AOEM interprets or constructs NOV receipt semantics.

The host-side JSON/RocksDB save that follows a successful AOEM commit is a
query projection, not a second authoritative persistence owner.

AOEM ownership is a durable one-way latch for an initialized authority domain.
The Host projection binding and block-ledger ownership record are rechecked
while holding the Host write lock. Turning the gate off later, or setting
`NOVOVM_LEGACY_HOST_TRANSITIONAL_FALLBACK=true` after an AOEM commit failure,
cannot restore direct Host mutation. The legacy path may still compute shadow
or A/B diagnostics, but a rejected AOEM production batch advances neither the
AOEM authority nor the Host projection.

The compatibility input name `full_business_compute_required` is accepted only
as a deprecated alias that requests the Semantic Graph V3 production gate. It
does not mean AOEM computes NOV business semantics. Runtime evidence reports
the effective `semantic_graph_v3_required` value separately; the old
`full_business_compute_required` claim remains `false` and is marked
deprecated.

## Paths

No drive letter or workspace name is required. For development and tests, the
AOEM-owned database path is resolved in this order:

1. environment variable `NOVOVM_AOEM_OWNED_STATE_DB_PATH`;
2. request parameter `aoem_owned_state_db_path`;
3. the configured native execution-store path plus
   `.aoem-owned.rocksdb`.

Production-owned ingress rejects request-level persistence-path and namespace
overrides. Those values are node configuration, so every machine may use its
own local paths and local AOEM storage namespace. Namespace identity enforces
local store isolation but is excluded from consensus batch identity; chain,
parent state, block context, ordered transactions, and protocol configuration
remain consensus-bound.

The default is derived from the configured native execution-store path on every
development machine. It never depends on a drive letter or workspace parent
name.

The AOEM runtime persistence path used by recovery gates is derived from the
same temporary/store path. Gate execution therefore does not require
machine-specific absolute-path configuration.

## Fail-closed rules

The production owner is rejected when:

- the domain-neutral V3 capability or symbols are missing;
- the Host is not reported as business-policy and transition-computation owner;
- host lowering did not complete;
- the persisted native protocol-config commitment is missing or differs from
  the current/pinned commitment;
- a graph step, event, key, or value exceeds AOEM bounds;
- graph execution or the completion write fails;
- readback is missing or differs;
- state-root or receipt-root parity fails;
- any AOEM domain-specific business logic is reported;
- a legacy host canonical write is detected.

The exceptional Host-to-AOEM migration is also fail closed. A non-empty legacy
Host projection may initialize a missing AOEM authority only when
`NOVOVM_ALLOW_AOEM_STATE_BOOTSTRAP_FROM_HOST=true` and
`NOVOVM_AOEM_STATE_BOOTSTRAP_HOST_ANCHOR_COMMITMENT` equals the canonical
commitment of the exact Host snapshot, chain, and AOEM namespace. A Host store
that already carries an AOEM authority/protocol binding cannot bootstrap a
missing AOEM authority again.

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
