# NOVOVM Candidate Workspace V1

Status: local candidate input staging and recovery API. Isolated execution is
documented separately in [Candidate Execution V1](NOVOVM_CANDIDATE_EXECUTION_V1.md);
neither API provides consensus, canonical promotion, or mainnet sign-off.

## Purpose and ownership

The common candidate execution-plan API advances a single local execution
head. It must not be used to try competing untrusted proposals against
authoritative balances and nonces. This slice introduces a bounded local
workspace for storing a complete plan and copying its locally verified parent
snapshot. The separate execution API consumes that immutable input; creating
or loading an input workspace does not execute its transactions.

Workspace layout and lifecycle belong to the NOVOVM Host. AOEM is used through
its domain-neutral persistence interface; no NOV-specific business rules,
opcodes, exports, DLL changes, or sibling AOEM repository edits are part of
this slice. The execution extension also uses existing AOEM generic precommit
operations, while the Host still computes NOV business transitions.
Local paths and storage namespaces are not network plan fields.

The local Host API is
`tx_ingress::candidate_workspace::{create_v1, load_v1, list_v1, abort_v1}`.
It requires explicit AOEM production ownership, a persistent backend and the
operator's protocol pin. It supports only an existing local AOEM authority
database and an existing AOEM-owned ledger parent; it does not create genesis
or bootstrap an empty chain. An unresolved authoritative prepared candidate
prevents parent capture.

## Lifecycle

The local lifecycle is:

```text
local parent snapshot + complete execution plan
  -> durable staging record
  -> isolated parent-state copy
  -> ready workspace
```

Calling `create_v1` with the same plan resumes interrupted staging only while
the current authoritative parent still exactly matches the captured parent.
After authority advances, an incomplete copy cannot be guaranteed recoverable:
it remains incomplete and can be aborted, but must not be completed using a
different parent. An abort is terminal for that workspace.

A ready workspace has a complete copied payload whose digest, descriptor,
plan, local ledger parent and AOEM-parent bindings have been validated. It
does not mean the transactions' signatures have been authenticated, the
transactions have executed, or their proposed output is valid. Workspace
staging, resume and abort do not advance the authoritative balance, nonce,
durable execution head or canonical ledger pointer.

The candidate input includes the full ordered raw transaction body and the
plan's protocol, chain, block context, pre-state and AOEM-parent bindings.
Parent state comes from the local authority, not from an arbitrary remote
snapshot selected by a proposer. Structural input validation is not a
substitute for ingress authentication, execution, independent proof checking
or validator authorization.

Once ready, the independent copy can be loaded or exactly replayed without
requiring the authoritative head to remain at that parent. Later authority
advancement and garbage collection of authority snapshots do not remove the
workspace's copied payload. This is not protection against deletion or loss
of the database containing the workspace itself.

## Resource and recovery boundary

V1 deliberately has fixed local resource limits per chain and storage-namespace
scope:

- At most 32 workspace slots, including staging and aborted slots.
- At most 8 MiB for each persisted input payload and 64 MiB for inputs in aggregate.
- Slots are not reused and there is no workspace garbage collection in V1.

The execution extension has a separate 8 MiB per-result and 64 MiB aggregate
output budget, attached to the same 32 input slots. Input and output limits
must not be combined into a claim of a 64 MiB total database limit. Aborting
does not refund either budget.

Ready and abort use independent, bound lifecycle markers. An abort marker
takes precedence even if a ready marker is present. Abort does not return a
slot or its reserved payload bytes to the capacity budget.

These are workspace-specific bounds, not a claim that the entire node or
AOEM database has a 64 MiB disk limit. The existing execution plan's separate
transaction-count and raw-body limits remain in force. Exhaustion must reject
new staging rather than silently overwrite an existing candidate or discard
recovery evidence.

Restart recovery must revalidate persisted inputs and copied parent state.
An incomplete or inconsistent workspace is not upgraded to completed execution.
Aborting a workspace is not a rollback of an already committed authoritative
block; this slice never commits a competing candidate in the first place.

Workspace operations use a scope-specific OS lock at a fixed location under
the canonicalized AOEM database path. Selecting a different Host projection
backend cannot select a different lock for the same workspace scope. Parent
capture separately holds the existing authority lock while reading its
consistent ledger and AOEM snapshot.

If an AOEM graph commit returns an error with an uncertain completion outcome,
the process retains the workspace OS lock until process exit. Further access
to that scope in the same process immediately rejects as poisoned; other
processes remain fenced by the OS lock and eventually fail their bounded lock
wait. The authority lock is not retained. Recovery requires the owning
process to exit before another process can reacquire the workspace lock and
inspect durable facts; a commit error must not be treated as proof that no
write occurred.

All `WorkspaceInfoV1` responses from the input API retain:

```text
execution_completed = false
transactions_authenticated = false
```

These fields describe input preparation only, even if an independently stored
execution result exists. Use `execute_v1` or `load_execution_v1` and its
`ExecutionInfoV1` response to establish authenticated, completed candidate
execution. An input `ready` marker is never execution evidence.

No workspace or execution result authorizes `proof_sealed`, `chain_canonical`,
`safe`, or `finalized` to become true. Neither API adds voting/QC, fork choice,
promotion, a network proposal scheduler, a CLI command, an RPC method, or a
new automatic node execution mode.

## Verification contract

The targeted test command is:

```sh
cargo test -p novovm-node --lib candidate_workspace -- --test-threads=1
```

The canonical mainline gate runs this filter as a required step. Its
`test_native_candidate_workspace` field is initially false and becomes true
only after the command succeeds. This filter covers input persistence/recovery
and the `candidate_workspace_execution` tests for isolated execution. The
execution extension reuses this required field. The subsequent
[nonce-identity slice](NOVOVM_NATIVE_NONCE_IDENTITY_V2.md) adds a separate
`test_native_nonce_identity_v2` field.

The serializer, preflight and node-runtime locksets now contain 45 required
fields in the same order. The frozen contract includes rejection when either
the workspace or nonce-identity evidence field is missing or false. Older
43-field or 44-field reports must be regenerated rather than reused as current
evidence.

Local targeted tests and the canonical gate must be run against the actual
checkout and available AOEM runtime before recording a pass. This document is
the verification contract, not a record that those commands have passed.
Linux CI, multiple processes, physical LAN devices, public-network operation
and long-run testing are separate evidence boundaries.

The input tests include codec/capacity/lock checks and AOEM-backed integration
tests. Their ignored worker is invoked explicitly by the parent test in three
separate process lifetimes: create ready, reopen and abort, then reopen aborted
and reject revival. A phase/PID manifest rejects accidental zero-test execution.
This covers clean process-exit input recovery, not forced termination or power
loss, and must not be relabeled as an execution-result restart test. Checkpoint
tests interrupt after acknowledged commits; the poison unit test exercises
retained OS-lock behavior, not a real uncertain AOEM commit. Execution-specific
evidence requirements are listed in the execution document. Concurrent
cross-process contention and hard-crash injection remain separate boundaries.

## Next boundary

Before remote proposals can drive the chain, remaining work includes an
explicit activation/upgrade procedure for legacy state under
[Native Nonce Identity V2](NOVOVM_NATIVE_NONCE_IDENTITY_V2.md),
independent validator execution/evidence verification, authority-bound body
acquisition, an exclusive durable scheduling owner, voting/QC, fork choice,
and recoverable canonical promotion. A persisted ready input workspace is
only the input-and-parent preparation step toward that pipeline.
