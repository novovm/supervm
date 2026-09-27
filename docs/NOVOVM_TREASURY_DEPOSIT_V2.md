# Balance-backed treasury deposits V2

## Two separate operations

- `treasury.deposit_reserve`: transfer an explicitly specified, positive amount
  of an existing on-chain asset from the execution subject's own account balance
  into treasury reserves. No mint, external deposit or withdrawal entitlement
  is created by this operation. Account ownership/authentication is enforced by
  the existing transaction admission and execution-policy layer, as for swaps.
- `governance.set_reserve_proof`: existing authorized manual attestation only.
  It updates reserve proof metadata, not treasury reserves or account balances.
  Existing labels retain `automated_verification=false` and no mint/redemption
  claim. Governance configuration is bound by the production protocol pin.

External-chain deposits are NOT implemented by either operation. A report hash,
an arbitrary source account in args or governance permission is not funding.
External inclusion/finality verification and unique-event consumption remain
required before adding a bridge deposit credit path.

## Implementation and failure behavior

The actual dispatcher calls shared `deposit_from_balance_transition_v2` in the
protocol crate. This computes both outputs without writes. It retains reserve
proof status/capacity checks and the existing u64 encoding ceiling, checks the
source balance, then publishes the debit and matching reserve increase through
the existing host/AOEM persistence flow. AOEM itself is unchanged.

Missing/invalid asset or amount, zero, insufficient balance and unsupported
amounts reject. There is no fee-budget fallback. The fee owner is not the deposit
source: the authenticated execution subject is. Execution args cannot override
that source. Failed business checks do not debit principal or increase reserves;
existing fee/nonce processing and failure counters retain their separate roles.
Success receipts identify funding source, source account and remaining balance.

## Compatibility and rollout gate

This is an intentional execution-rule change, not a backward-compatible proof
refactor. `compiled_defaults.treasury_deposit_contract=balance_backed_v2` is now
part of the protocol config commitment. Old pins and bound stores must fail on
drift. Do NOT clear a stored commitment, erase a database, or re-pin a running
network automatically. A coordinated activation/state migration is still needed.
No running service, existing authority database or AOEM binary was changed.

Legacy demos/soaks that created reserves from an unfunded account must be migrated
to explicitly funded test/genesis fixtures. Never reintroduce a production mint
shortcut or silently credit test funds during transaction execution to make them
pass. Until those integrations and activation are validated, this slice is NOT
a mainnet-ready or four-device-ready release.

## Local evidence and limits

- 1,080 combinations: 648 positive-amount cases compare the unchanged proof
  checks with explicitly specified V2 debit/receipt changes; 432 zero-amount
  cases reject. This is NOT unmodified V1/V2 state-root parity.
- Dispatcher negatives: missing funds, forged source/proof args, malformed args,
  single/cumulative encoding overflow, manual attestation without funds.
- Local persistence read-back: USDT balance 10 -> 3, treasury 0 -> 7;
  repeated reads recover the same complete state and successful receipt.
- Internal dispatcher repeats are NOT a replay API. Existing signed-ingress
  restart/nonce regression is separate and passed; no new funded signed-ingress
  multi-process crash acceptance is claimed.
- Governance end-to-end regression passed after correcting a stale delta count:
  proof metadata changes both policy and full committed-state V3 projections.
- Configuration drift and reserve-proof fee/redeem regressions passed.

No full business execution zkVM proof, network upgrade, crash-injection campaign
or complete legacy fixture migration is accepted by this slice.
