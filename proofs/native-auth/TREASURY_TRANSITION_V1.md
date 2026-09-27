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
