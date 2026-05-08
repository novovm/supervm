# NOVOVM Account Protocol Cut C / ExecutionPolicy Implementation Checklist (2026-04-20)

Status: AUTHORITATIVE CHECKLIST (Cut C / ExecutionPolicy)  
Scope: implementation gate for the minimal `ExecutionPolicy` slice; later changes may extend execution enforcement only and must not drift into `AccountMode`, mapped assets, or a privacy asset ledger

## Goal

`Cut C` does one thing only:

`make KeyAlgo + ExecutionPolicy a non-bypassable execution admission rule in mainline`

This cut is not about:

- creating a new account-subject category
- introducing `AccountMode`
- introducing a new asset ledger or privacy asset space
- letting gateway / adapter define their own policy semantics

## Currently sealed scope

The current sealed implementation scope of `Cut C` is:

- `execution_policy`
  - `Standard`
  - `PqRequired`
  - `PrivacyRequired`
- single mainline resolve + enforcement
- gateway pass-through only
- explicit rejection paths for `PqRequired`
- explicit rejection paths for `PrivacyRequired`
- policy visibility in `receipt / trace / audit`

## Dual-track boundary

This cut allows:

- explicit `execution_policy` input
- defaulting to `Standard` when no policy is supplied
- gateway / `TxIR` pass-through into mainline
- real validation using `KeyAlgo + ExecutionPolicy` in mainline

This cut does not allow:

- recomputing or splitting `account_id` through `execution_policy`
- changing `account_balance / account_assets` through `execution_policy`
- turning policy failure into silent fallback or automatic downgrade
- growing a second resolve or enforcement layer in gateway / adapter
- introducing a new state-truth source under the name of `ExecutionPolicy`

Every expansion of `ExecutionPolicy` support must include at least one `input policy -> resolve -> enforcement -> receipt/trace/audit visible` sample showing:

- the requested `execution_policy`
- the currently bound `key_algo`
- whether enforcement passed
- if it failed, why it failed
- how the result appears in `receipt / trace / audit`

## Explicit prohibitions

The following are explicitly forbidden in this cut:

- introducing `AccountMode`
- turning `ExecutionPolicy` into a subject-splitting condition
- letting `ExecutionPolicy` touch `account_balance / account_assets`
- turning policy failure into downgraded execution
- letting gateway / adapter grow a second policy semantics layer
- smuggling `Cut B` into `Cut C`
- smuggling `Phase 4` behavior into `Cut C`
- introducing a unified asset ledger, mapped-asset protocol, privacy asset space, or `asset_root`

## Merge gate

The merge gate is one sentence:

`ExecutionPolicy controls execution enforcement only; it must not determine account_id, asset ownership, privacy asset ledgers, or unified-account subject splitting.`

Minimum PR acceptance must satisfy all of the following:

- `execution_policy` affects execution routing and rejection only
- `account_id` is not recomputed and not split
- `PqRequired / PrivacyRequired` reject explicitly when requirements are unmet
- no silent fallback exists
- `receipt / trace / audit` expose:
  - `execution_policy`
  - `policy_enforced`
  - `policy_rejection_reason`
- gateway is pass-through only and does not perform a second resolve
- no `AccountMode`
- no asset-layer change

Recommended code-review wording:

`If a change attempts to let ExecutionPolicy alter account_id, asset ownership, privacy ledgers, or unified-account subject splitting, or if it introduces a second resolve/enforcement layer, silent fallback, AccountMode, a mapped-asset protocol, asset_root, or any other Phase 4 behavior, it must not merge into mainline.`

Long-term guardrail:

`Any change that lets ExecutionPolicy trigger a unified asset ledger, a privacy asset space, mapped assets, or a proof-root account tree is treated as an unauthorized jump into Phase 4 / 5 / 6.`
