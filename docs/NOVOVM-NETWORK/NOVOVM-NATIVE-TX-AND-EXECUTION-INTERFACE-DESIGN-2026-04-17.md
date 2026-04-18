# NOV Mainchain Native Transaction and Native Execution Interface Design (P0 Freeze)
_2026-04-17_

## 1. Purpose

On top of the frozen monetary boundary (M0/M1/M2, multi-asset payment, NOV internal settlement), freeze "how developers call the NOV mainchain directly" as first-class interfaces, and prevent fallback to "users go through EVM while NOV is background only."

Related prerequisite resolution:
- `docs/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`

## 2. Design principles (frozen)

1. `nov_*` is the native primary entry, not a compatibility helper.
2. Native execution goes through `novovm-exec`; host calls AOEM through AOEM FFI V2 typed ABI.
3. Native transactions are no longer transfer-only; minimum support is `Transfer / Execute / Governance`.
4. EVM remains compatible but is positioned as a `Plugin`, not the mainchain identity.

## 3. Native transaction model (target shape)

```rust
enum NovTxKind {
    Transfer(NovTransferTx),
    Execute(NovExecuteTx),
    Governance(NovGovernanceTx),
}
```

### 3.1 Transfer

```rust
struct NovTransferTx {
    from: Address,
    to: Address,
    asset: AssetId,
    amount: u128,
    nonce: u64,
    fee_policy: FeePolicy,
}
```

### 3.2 Execute (mainchain core)

```rust
struct NovExecuteTx {
    caller: Address,
    target: ExecutionTarget,
    method: String,
    args: Vec<u8>,
    execution_mode: ExecutionMode,
    privacy_mode: PrivacyMode,
    verification_mode: VerificationMode,
    fee_policy: FeePolicy,
    gas_like_limit: Option<u64>,
    nonce: u64,
}
```

```rust
enum ExecutionTarget {
    NativeModule(String),
    WasmApp(AppId),
    Plugin(PluginId),
}
```

```rust
struct FeePolicy {
    pay_asset: AssetId,
    max_pay_amount: u128,
    slippage_bps: u32,
}
```

### 3.3 Governance

```rust
struct NovGovernanceTx {
    proposer: Address,
    proposal_type: ProposalType,
    payload: Vec<u8>,
    nonce: u64,
}
```

## 4. Internal unified execution request (mandatory convergence)

```rust
struct NovExecutionRequest {
    tx_id: TxId,
    caller: Address,
    target: ExecutionTarget,
    method: String,
    args: Vec<u8>,
    consistency: ConsistencyLevel,
    privacy: PrivacyMode,
    verification: VerificationMode,
    fee: SettledFee,
    context: ExecutionContext,
}
```

Rule: node/plugin/query layers consume one unified request/receipt structure. Do not reintroduce private AOEM semantic interpretation in per-module branches.

## 5. External interfaces (P0 minimum)

### 5.1 Submit

- `nov_sendRawTransaction(raw_tx)`
- `nov_sendTransaction(tx_json)`
- `nov_execute(target, method, args, fee_policy, privacy_mode, verification_mode)`

### 5.2 Query

- `nov_getTransactionByHash`
- `nov_getTransactionReceipt`
- `nov_getBalance`
- `nov_getAssetBalance`
- `nov_call`
- `nov_estimate`
- `nov_getState`
- `nov_getModuleInfo`

## 6. Relationship with EVM plugin (frozen)

```text
NOV Native
 |- NativeModule
 |- WasmApp
 `- Plugin(EVM)
```

EVM is `ExecutionTarget::Plugin(EVM)`, not a parallel mainchain identity.

## 7. Fee model integration

Users can pay in multiple assets; internal settlement is unified in NOV:

```rust
struct SettledFee {
    nov_amount: u128,
    source_asset: AssetId,
    source_amount: u128,
}
```

External terminology is unified as `Execution Fee`; `gas_*` remains compatibility-only.

## 8. First native module set (P0)

- `treasury`
- `credit_engine`
- `amm`
- `governance`
- `account`
- `asset`

Example methods:

- `treasury.deposit_reserve`
- `credit_engine.open_vault`
- `credit_engine.mint_nusd`
- `amm.swap_exact_in`
- `governance.submit_proposal`

## 9. Receipt and state

```rust
struct NovReceipt {
    tx_hash: TxId,
    status: ExecutionStatus,
    settled_fee_nov: u128,
    paid_asset: AssetId,
    paid_amount: u128,
    logs: Vec<NovLog>,
    proof_ref: Option<ProofRef>,
    route_ref: Option<RouteRef>,
}
```

Must express: payment source, NOV-settled amount, proof reference, and route reference.

## 10. Current implementation gap (code fact)

1. Native tx wire is still simplified transfer shape:
   - `crates/novovm-protocol/src/tx_wire.rs`
2. Ingress mapping is still transfer-heavy:
   - `crates/novovm-node/src/tx_ingress.rs`
3. P0 interface must be wired directly into `novovm-exec` internal request model.

## 11. Recommended P0 commit order

1. `feat(protocol): introduce NovTxKind {Transfer, Execute, Governance}`
2. `feat(node): map native execute tx into unified NovExecutionRequest`
3. `feat(rpc): promote nov_send* / nov_call / nov_estimate / nov_getReceipt to first-class APIs`
4. `feat(runtime): register core native modules (treasury, credit_engine, amm, governance)`
5. `feat(fee): connect multi-asset payment policy to internal NOV settlement`

## 12. P0 acceptance gates

1. `nov_sendRawTransaction` can submit `Execute`.
2. `Execute` can enter canonical pending/inclusion.
3. `nov_call` and `nov_estimate` are no longer placeholder returns.
4. At least one native module is callable end-to-end through `nov_*`.
5. EVM compatibility path remains, but is no longer the primary mainchain narrative.

---

This file is the frozen NOV mainchain native interface draft. If implementation deviates, update this file first and explain the deviation explicitly.
