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
- Internal dispatcher repeats are NOT a replay API. The signed-ingress
  restart/nonce regression now explicitly seeds 100 USDT in its isolated test
  store BEFORE submitting the signed transaction, pays fees in NOV, and asserts
  a successful receipt plus USDT balances of account=73 and treasury=27.
  Clearing the runtime nonce registry and resubmitting must leave the complete
  persisted state unchanged. This is a simulated restart, not multi-process
  crash acceptance. Its test profile disables AOEM semantic ingress and the
  production-candidate gate; it is not an AOEM production-mode acceptance test.
- Governance end-to-end regression passed after correcting a stale delta count:
  proof metadata changes both policy and full committed-state V3 projections.
- Configuration drift and reserve-proof fee/redeem regressions passed.

No full business execution zkVM proof, network upgrade, crash-injection campaign
or complete legacy fixture migration is accepted by this slice.

## Fixture migration status

The funded signed-ingress regression, deposit/attestation boundary tests and
production configuration pin regression are now explicit CI steps. Local funded
replay and Clippy passed; adding CI steps does not establish remote CI success.

Previously reproduced blocker (fixture migration described below):
`candidate_workspace_execution_competing_results_match_authority_and_survive_parent_gc`
fails at the successful first receipt assertion. Its successor spends from a
zero-funded parent. The success assertion is retained; do not reclassify a failed
deposit as successful execution coverage. The shared `genesis_plan` fixture binds
the default empty account state. Candidate/soak migration requires an explicit
agreed initial allocation represented in the actual parent snapshot AND its
state root, not a test-only bypass inside business execution.

No active database was funded, no service was restarted, and this branch remains
unmerged while that bootstrap and full integration regression are unfinished.

### Funded candidate parent fixture

Candidate execution fixtures now reuse the existing production one-shot
Host-to-AOEM snapshot bootstrap, not a new mint or an execution-time credit:

1. Require absent AOEM authority and an empty isolated Host store.
2. Allocate exactly 1,000 NOV to each explicitly enumerated test signer before
   executing transactions. Never use these known test seeds on a real network.
3. Compute the full Host snapshot anchor, including chain and namespace binding.
4. Derive the expected initial state root from the same allocation with the
   protocol config commitment bound, and put that root in the execution plan.
5. Enable import only inside the test's scoped initialization call; execute the
   ordinary AOEM production path, then read back the unchanged initial balances
   and check the block's pre-state root. The initial fixture transaction may be
   rejected; successful *funded successor deposits* remain mandatory assertions.

Every fixture checks rejection of a changed balance, wrong chain, wrong
namespace and already protocol-bound import image. These checks invoke the real
bootstrap verifier. The same helper supplies the competing-candidate, batch
authentication, checkpoint recovery and three-process recovery fixtures.

This is test initialization using an existing import mechanism, NOT a deployed
genesis allocation tool, a network-approved issuance schedule or a migration of
running nodes. The namespace-specific import anchor is distinct from the shared
business state root. Cross-machine operators still need independently agreed
chain, protocol configuration and initial allocations. Public startup/soak
configuration migration remains pending; do not merge solely on these fixtures.

Local funded candidate suite: 4 passed, 0 failed in 129.96 seconds. The worker
entry is ignored in ordinary enumeration but is explicitly launched by the
passing three-process parent (execute, recover/abort, recover-aborted). Competing
branches, whole-batch authentication and all four checkpoint interruptions also
passed without relaxing successful deposit assertions. The six treasury
transition tests and node library/test Clippy with warnings denied passed.
CI now includes the funded candidate suite; remote Linux execution is not claimed
by these Windows-local results.

### Dual-node lifecycle fixture migration

The dual-node gate now prepares identical explicit initial allocations in its
fresh per-process Host RocksDB stores before any child starts. Identity i owns
i USDT plus 10,000 NOV; the fixture transaction deposits exactly i USDT and uses
NOV for fees. Both identity and amount advance across sender rounds. Namespaces
have distinct exact-snapshot import anchors, while the allocation is identical.
Preparation refuses existing stores, reads the written snapshot back, and uses
the existing one-shot Host-to-AOEM bootstrap authorization. No running node or
operator-selected authority database is funded. Public startup tooling and
general long-run soak fixture migration remain outside this slice.

The gate now requires successful persisted deposit receipts on every receiver
and checks USDT conservation, not only receipt counts. Local acceptance:

- One sender, one receiver, one round: 8 successes, receiver reserve 36 USDT,
  account USDT remaining 0; durable block and reverse-index checks passed.
- One sender, three receivers, two sender processes: each receiver independently
  has 8 successes, reserve 36 USDT and account USDT remaining 0.
- Fixture determinism/bounds and existing-store refusal tests passed, as did
  gate Clippy with warnings denied.

Reports: `artifacts/native-pipeline/funded-v2-local-gate.json` and
`artifacts/native-pipeline/funded-v2-fanout-gate.json` (local generated evidence).
These are loopback processes, NOT four physical machines or long-run acceptance.

Linux CI run 36352667760 on the preceding 4f51504 revision failed at
`native_nonce_upgrade_authorization_cli_verifies_quorum_without_source_mutation_or_activation`:
the frozen authorization fixture's target protocol commitment no longer matches
the binary after V2 activation was added to compiled defaults. The rejection
must remain enforced; regenerate test-only certificates with valid signatures
for the new intended target rather than editing a signed commitment or weakening
verification. Format, Clippy and security checks passed in that run; later
pipeline stages were not reached. The current branch is not merge-ready.
