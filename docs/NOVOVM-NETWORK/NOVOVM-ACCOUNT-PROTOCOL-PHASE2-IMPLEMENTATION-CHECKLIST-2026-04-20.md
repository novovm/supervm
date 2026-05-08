# NOVOVM Account Protocol Phase 2 Implementation Checklist (2026-04-20)

Status: AUTHORITATIVE CHECKLIST (Phase 2)  
Scope: mainline migration of `execution subject` from `address-first` to `account-first`

## Goal

`Phase 2` does one thing only:

`migrate execution subject from address-first to account-first`

This phase does not expand:

- account objects
- the asset layer
- the privacy layer

The acceptance target is:

- new execution entry points default to `account_id`
- fee ownership is explicitly bound to `account_id`
- nonce ownership is explicitly bound to `account_id`

## Three-cut order

1. Real mainline execution entry points
   - update `crates/novovm-node/src/mainline_query.rs`
   - update `crates/novovm-node/src/tx_ingress.rs`
   - move `nov_swap / nov_redeem / nov_openVault / nov_execute` and similar real mainline paths to `account-first`
2. Adapter ingress
   - then update `crates/novovm-adapter-novovm/src/lib.rs`
   - converge `tx.from -> adapter_uca_id -> autoprovision -> route` into “explicit `account_id` first; address only as fallback / binding proof”
3. Write gateways
   - finally update `crates/gateways/evm-gateway/src/main.rs`
   - make `eth_sendRawTransaction / web30_sendTransaction / nov_sendRawTransaction / nov_execute` explicitly resolve subject, fee, and nonce ownership to `account_id`

## Dual-track compatibility boundary

Dual-track compatibility is allowed in this phase, but only within these boundaries:

- `account_id`: primary semantics
- `uca_id`: transition alias
- `from / caller / external_address`: fallback or binding proof only, not the default semantics for new paths

Every cut must include at least one “legacy input compatibility -> new subject landing” sample that shows:

- whether the request explicitly carries `account_id`
- if not, how fallback resolves it
- which `account_id` the final receipt / trace / audit lands on
- which `account_id` owns fee and nonce at the end

## Explicit prohibitions

The following are explicitly forbidden in this phase:

- adding execution entry points that accept only `from / caller` and not `account_id`
- adding address-generated subject logic such as `adapter_uca_id(&tx.from)`
- adding address-level nonce ownership logic
- adding fee ownership that stays on addresses rather than `account_id`
- adding new mainline semantics to dead `crates/novovm-node/src/main.rs`
- expanding `root / asset / privacy` objects in this phase

## Merge gate

The merge gate is one sentence:

`Any new execution path that cannot explicitly assign subject, fee, and nonce ownership to account_id must not enter mainline.`

Minimum PR acceptance must satisfy all of the following:

- the new path explicitly accepts `account_id`, or clearly documents fallback resolution into `account_id`
- receipt shows subject ownership
- trace shows subject ownership
- audit shows subject ownership
- fee ownership is explicitly tied to `account_id`
- nonce ownership is explicitly tied to `account_id`

Recommended code-review wording:

`If an execution path cannot answer “who is the subject, who pays the fee, and who owns the nonce”, and all three cannot be explicitly resolved to account_id, the change must not merge into mainline.`
