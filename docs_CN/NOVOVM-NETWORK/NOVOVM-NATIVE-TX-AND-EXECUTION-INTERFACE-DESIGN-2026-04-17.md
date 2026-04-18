# NOV 主链原生交易与原生执行接口设计（P0 冻结）  
_2026-04-17_

## 1. 目的

在已冻结的货币制度口径基础上（M0/M1/M2、多币支付、NOV 内部结算），把“开发者如何直接调用 NOV 主链”定为一等公民接口，避免系统继续退回“用户走 EVM，NOV 做后台”。

相关前置决议：  
- [NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md](/d:/WEB3_AI/SUPERVM/docs_CN/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md)

## 2. 设计原则（冻结）

1. `nov_*` 为原生主入口，不是兼容辅助接口。  
2. 原生执行统一走 `novovm-exec` 门面，宿主通过 AOEM FFI V2 typed ABI 调用 AOEM。  
3. 原生交易不再是 transfer-only，至少支持 `Transfer / Execute / Governance`。  
4. EVM 保持兼容，但定位为 `Plugin`，不再承担主链身份。  

## 3. 原生交易模型（目标形态）

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

### 3.2 Execute（主链核心）

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

## 4. 系统内部统一执行请求（必须收口）

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

规则：节点、插件、查询层都消费统一请求/回执结构，不允许再次在各模块内私有解释 AOEM 执行语义。

## 5. 对外接口（P0 必须）

### 5.1 提交

- `nov_sendRawTransaction(raw_tx)`
- `nov_sendTransaction(tx_json)`
- `nov_execute(target, method, args, fee_policy, privacy_mode, verification_mode)`

### 5.2 查询

- `nov_getTransactionByHash`
- `nov_getTransactionReceipt`
- `nov_getBalance`
- `nov_getAssetBalance`
- `nov_call`
- `nov_estimate`
- `nov_getState`
- `nov_getModuleInfo`

## 6. 与 EVM 插件关系（冻结）

```text
NOV Native
 ├─ NativeModule
 ├─ WasmApp
 └─ Plugin(EVM)
```

EVM 是 `ExecutionTarget::Plugin(EVM)`，不是并列主链身份。

## 7. 费用模型接入

用户侧可多币支付，系统内部统一 NOV 结算：

```rust
struct SettledFee {
    nov_amount: u128,
    source_asset: AssetId,
    source_amount: u128,
}
```

对外术语统一为 `Execution Fee`，EVM 兼容字段中的 `gas_*` 仅保留兼容语义。

## 8. 原生模块首批清单（P0）

- `treasury`
- `credit_engine`
- `amm`
- `governance`
- `account`
- `asset`

示例方法：

- `treasury.deposit_reserve`
- `credit_engine.open_vault`
- `credit_engine.mint_nusd`
- `amm.swap_exact_in`
- `governance.submit_proposal`

## 9. 回执与状态

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

必须可表达：支付来源、NOV 结算金额、证明引用、路由引用。

## 10. 当前代码差距（实现事实）

1. 原生 tx wire 仍为简化 transfer 结构：  
   - [tx_wire.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-protocol/src/tx_wire.rs:9)
2. ingress 映射仍以 transfer 语义为主：  
   - [tx_ingress.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-node/src/tx_ingress.rs:69)
3. 需把 P0 接口与 `novovm-exec` 内部执行请求模型直接打通。

## 11. P0 提交顺序（建议）

1. `feat(protocol): introduce NovTxKind {Transfer, Execute, Governance}`
2. `feat(node): map native execute tx into unified NovExecutionRequest`
3. `feat(rpc): promote nov_send* / nov_call / nov_estimate / nov_getReceipt to first-class APIs`
4. `feat(runtime): register core native modules (treasury, credit_engine, amm, governance)`
5. `feat(fee): connect multi-asset payment policy to internal NOV settlement`

## 12. P0 验收门

1. `nov_sendRawTransaction` 能提交 `Execute`。  
2. `Execute` 能进入 canonical pending / inclusion。  
3. `nov_call` 与 `nov_estimate` 不再是占位返回。  
4. 至少一个原生模块通过 `nov_*` 接口端到端可调用。  
5. EVM 兼容路径保留，但不再作为主入口叙事。  

---

本文件是 NOV 主链原生接口冻结稿。后续实现若偏离本文件，必须先更新本文件并说明偏离理由。

