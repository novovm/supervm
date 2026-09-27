# Treasury deposit shared transition v1

Status: production branch extraction and local differential regression only.
This is NOT a new zkVM business-execution receipt or a full transaction proof.

The V2 fixture invokes `treasury.deposit_reserve`, not an account transfer.
`novovm_protocol::native_treasury::deposit_reserve_transition_v1` now contains
the existing reserve proof status/expiry/capacity checks and reserve arithmetic.
The real node dispatcher calls this shared function. Argument decoding, state
writes, rejection counters, logs and receipts stay in the node. AOEM is unchanged.

## Compatibility and policy boundaries

This extraction deliberately preserves existing behavior:

- Host normalizes the asset; the shared function requires normalized input.
- NOV bypasses reserve proof checks. Missing non-NOV proof is permitted.
- Unknown or empty proof status normalizes to active.
- Expiry rejects only when now is strictly greater than expiry; revoked wins.
- Reserve addition saturates. This operation does not debit caller balances.
- Malformed/missing argument fallback remains in the production decoder.

These are compatibility facts, not endorsement of a public mint/deposit policy.
Caller authority, authenticated external reserve evidence and allowed argument
policy must be resolved before claiming safe permissionless deposits.

## Local verification

The frozen pre-extraction branch in `native_treasury_transition_tests.rs` is a
test-only oracle. 1,080 combinations compare full serialized stores, receipts,
semantic state digests and receipt commitments against the production dispatcher:
2 assets x 9 statuses x 4 expiry values x 5 amount cases x 3 proof capacities.
Six additional argument cases cover fallback and normalization. All passed.
Four existing reserve proof fee/redeem regression tests passed. Protocol-level
u128 saturation tests passed. Node/protocol Clippy with warnings denied passed.

Limits and discovered defects:

- Full-path comparison is limited to JSON-representable amounts through u64::MAX.
  Existing production shard `serde_json::json!` encoding panics for larger u128
  reserves/proof capacities; deposit log encoding has the same representation
  constraint. The shared arithmetic supports u128, but this does NOT establish
  full-path u128 support. Fix encoding with an explicit compatibility/version
  decision before proving such states; do not silently change consensus roots.
- A broad `reserve_proof` run stalled at
  `mainline_query_treasury_reserve_proof_product_smoke_enforces_cap` and was
  terminated. It is NOT a pass; the cause was not established in this slice.
- No new guest image/proof was generated. Existing V2 remains a parent-bound
  signature/nonce proof, not proof of this transition.

## Next required closure

Resolve the large-integer encoding defect and deposit authorization policy;
then share/bind argument decoding, fee settlement, parent-derived inputs and
canonical time, nonce update, full post-state and receipt commitments in the
proof relation. Do not prove an isolated reserve addition and label it a complete
transaction. No mainnet, multi-device, recovery or finality acceptance is added.

## Follow-up: fail-closed deposit encoding admission

The production dispatcher now calls `deposit_reserve_encodable_transition_v1`.
It retains the existing proof checks and rejection precedence, then rejects
`amount > u64::MAX` or resulting reserve `> u64::MAX` with
`reserve_encoding_limit_exceeded` BEFORE writing the reserve or success log.
The only module-state change on rejection is the existing failure counter.
Outer fee/nonce processing is unchanged; this is not full transaction rollback.
The original pure u128 arithmetic helper remains available but is not the
production admission function. No serde features or canonical encoders changed.
Successful legacy inputs keep identical state/receipt commitments; previously
crashing inputs now have a defined failure outcome. Deploy the same code on all
validators; this does not authorize mixed-version handling of those inputs.

Eight dispatcher cases (NOV and USDT, single excessive amount and accumulation
overflow) verify rejection, no reserve/balance mutation, serializable receipt,
state commitment and treasury shard encode/decode round-trip. The 1,080 legacy
combinations and six argument cases remain regression requirements.

This is containment, not a global u128 fix. It does not repair already invalid
in-memory states, validate all proof metadata, or bound unrelated fee/governance/
vault writes. A versioned full-integer encoding remains required for those paths.
The deposit amount parser still uses its legacy fallback for malformed or
unquoted out-of-range JSON numbers; strict argument decoding is not claimed.

Authorization review: `dispatch_native_module_execute_v1` does not invoke
`governance_execute_authorized_v1` for `treasury.deposit_reserve`; the deposit
branch neither debits the caller nor requires an independently verified deposit
event. Signature authentication alone is not deposit authority. This scoped
finding is not proof that a public RPC can currently exploit it. Do not declare
permissionless deposit safety. A consensus-bound authorization policy and/or
verified source event is needed; do not import a node-local environment allowlist
as a consensus rule or invent an administrator during this encoding patch.
