# NOVOVM 双轨清算与市场定价制度（P2-A 实施冻结）
_2026-04-17_

## 1. 目的

本文件用于把“货币制度、储备制度、清算制度、市场定价制度”的交叉边界一次收敛，作为 `P2-A` 实施冻结稿。

目标不是扩功能，而是先把制度边界写成可执行约束，避免后续实现再次出现混账、混价、混责任。

## 2. 与现有阶段的关系

- `P0`：已签收（原生交易三分型与原生执行主入口成立）
- `P1-A`：已签收（quote 主线成立）
- `P1-B`：已签收（clearing 最小真实主线成立）
- `P1-C`：已签收（settlement 账务主线、journal、snapshot、query 成立）

本稿用于定义 `P2-A`：把 `pay_asset != NOV` 从“最小可用 clearing”升级为“制度轨 + 市场轨并存”的最小路由竞争主线。

## 3. 三池结构（冻结）

### 3.1 清算储备池（Reserve Settlement Pool, RSP）

来源：执行费收入、清算收入、外部结算流入等。

作用：

1. 支撑规则化清算兑换。
2. 提供系统风险缓冲。
3. 作为信用扩张约束的底层储备之一。

### 3.2 镜像托管池（Mirror Custody Pool, MCP）

来源：外链资产锁仓（例如 ETH/USDT 在插件侧锁仓）。

作用：

1. 支撑镜像资产的 1:1 回滚赎回。
2. 保证托管资产与镜像负债对应。

硬约束：MCP 资产不进入市场做市，不作为系统自由支配储备。

### 3.3 市场流动池（Market Liquidity Pool, MLP）

来源：做市流动性（系统投放或外部 LP）。

作用：

1. 提供市场价格发现。
2. 提供交易深度与滑点缓冲。
3. 承接套利收敛。

## 4. 双轨价格制度（冻结）

### 4.1 规则清算价（轨道 A）

由公开规则决定，不等于市场成交价。

参考形式：

`清算价 = 参考价 × 储备折扣 × 风险折扣 × 费率因子`

特点：有限容量、可审计参数、可治理调整。

### 4.2 市场交易价（轨道 B）

由 AMM/撮合市场决定，允许自由波动。

特点：无固定承诺、由流动性与交易行为形成。

### 4.3 冻结结论

- 清算价不等于市场价。
- AMM 价不等于规则清算价。
- 两者通过套利在区间内收敛，但职责不同。
- AMM spot price 禁止直接进入 Execution Fee / gas 协议清算。
- 协议清算价按 epoch 固定，不随单笔交易实时波动。

## 4.4 协议清算价公式（2026-06-13 补充冻结）

为每个可支付资产 `A` 计算每个 epoch 的协议清算价：

```text
P_clear[A, e] = NOV per 1 unit of asset A
```

输入：

```text
P_prev[A]        = 上一 epoch 协议清算价
P_amm_twap[A]    = 白名单 AMM 池的时间加权均价，不使用 spot price
P_nav[A]         = Treasury reserve / NAV 口径下的资产价值
P_oracle_ref[A]  = 许可 oracle 参考价，可为空，只做校验和兜底
L[A]             = AMM 深度/流动性指标
V[A]             = 波动率指标
R[A]             = 储备率/风险状态
```

基础参考价：

```text
P_ref[A] = weighted_median(
  P_amm_twap[A],
  P_nav[A],
  P_oracle_ref[A] if enabled
)
```

异常源处理：

- 如果 oracle 与 AMM/NAV 偏离超过阈值，不采用异常源。
- 如果 AMM TWAP 的池深度、成交量、时间窗口不满足阈值，不采用 AMM 源。
- 如果所有市场源都不可用，降级到 `P_nav[A]` 或进入 `constrained/blocked`。

epoch 限幅：

```text
P_epoch[A] = clamp(
  P_ref[A],
  P_prev[A] * (1 - max_epoch_down),
  P_prev[A] * (1 + max_epoch_up)
)
```

支付清算价采用保守折扣：

```text
P_pay[A] = P_epoch[A]
         * (1 - reserve_haircut[A])
         * (1 - liquidity_haircut[A])
         * (1 - volatility_haircut[A])
```

用户用资产 `A` 支付 fee 时：

```text
settled_nov = paid_amount[A] * P_pay[A]
```

赎回或 Treasury 付出资产时采用反向保守价：

```text
P_redeem[A] = P_epoch[A]
            * (1 + redemption_spread[A])
            * (1 + risk_surcharge[A])
```

用户用 NOV 赎回资产 `A` 时：

```text
asset_out[A] = nov_amount / P_redeem[A]
```

制度效果：

- 系统收资产时不高估外币。
- 系统付资产时不低估外币。
- NOV 宏观发行和矿工结算不受 AMM spot price 直接影响。
- AMM 只做市场价格发现与套利收敛，不做协议清算锚。

## 5. 资产分层（冻结）

### 5.1 NOV

基础结算币（M0/M1 主体）。

### 5.2 镜像资产

制度命名可使用 `m*` 表示 1:1 托管映射与回滚赎回权；当前 EVM mapped asset 产品口径中 `NETH` / `NUSDT` 也属于此类 M2 资产，不进入 `M0/M1`。

### 5.3 信用扩张资产

保留 `n*`（如 `nUSD`、`nRWA`），归属 M2。

冻结约束：镜像/存款凭证资产与信用扩张资产不得混同风控；二者都属于 M2，但发行依据、赎回责任、抵押率、清算线必须分开。

## 6. 与当前代码主线的对齐

### 6.1 已成立能力

- `fee.quote.*` 与 `fee.clearing.*` 失败码边界已分离。
- `quote -> clearing -> settlement` 已有最小闭环。
- `settled_fee_nov / paid_asset / paid_amount / route_ref` 可见。
- settlement journal 与 accounting snapshot 已可查询。

### 6.2 当前缺口（P2-A）

- route source 仍偏最小实现。
- 市场轨与规则轨尚未形成标准化“多 route 可选”主线。
- 多 source 选择与回退策略尚未固定。

## 7. P2-A 实施冻结（最小可执行）

### 7.1 范围

只做“多 route 最小聚合器”，不做复杂聚合器。

### 7.2 最小模型

1. Route source 至少支持两类：
   - `reserve_direct`
   - `amm_pool`
2. Router 固定三段：
   - `quote_routes`
   - `select_best_route`
   - `execute_selected_route`
3. 选择策略先固定为：`max_expected_out`。

### 7.3 失败码（保持前缀冻结）

- `fee.clearing.route_unavailable`
- `fee.clearing.insufficient_liquidity`
- `fee.clearing.quote_expired`
- `fee.clearing.slippage_exceeded`

不新增跨前缀混合失败码。

### 7.4 回执与查询必带

- `route_id`
- `route_source`
- `expected_nov_out`
- `route_fee_ppm`

并继续保持 `settled_fee_nov / paid_asset / paid_amount`。

## 8. 风险控制边界（P2-A）

1. 报价有效期（TTL）硬校验。
2. 滑点上限硬校验。
3. `max_pay_amount` 硬校验。
4. 流动性不足必须失败，不允许静默降级为错误结算。
5. MCP 与 MLP 账务隔离，不允许资产跨池挪用。
6. 禁止使用 AMM spot price 参与协议清算。
7. AMM TWAP 必须满足最小时间窗口、最小池深度、最小成交量、最大偏离阈值。
8. 单 epoch 清算价涨跌必须有限幅，防止几分钟内价格污染 NOV 宏观结算。
9. 低流动性时不使用 AMM 价，降级到 NAV/oracle 兜底或进入 `constrained/blocked`。
10. oracle 只允许治理白名单来源；不能由外部用户注册价格源；当前代码用 `fee_oracle_allowed_sources` 约束可参与清算的 `fee_oracle_source`，并用 `fee_oracle_disabled_sources`、disabled reason、source rotation 记录治理停用异常源。
11. oracle 不能单独决定价格，只能参与 `weighted_median`、偏离检测、熔断和兜底；fee quote / clearing liquidity / clearing route / TreasuryDirect 非 NOV 支付路径不得在协议清算价拒绝 oracle-only 或 no-anchor 资产后回退到直接 oracle 报价或默认价。
12. AMM、NAV、oracle 三者偏离过大时，暂停该资产新支付或只允许 NOV 支付。
13. MCP 镜像托管池资产不得挪入 AMM 做市，不得作为系统自由支配储备。
14. MLP 市场流动池可用于交易和价格发现，但不得被当作 1:1 赎回储备。
15. RSP 清算储备池用于规则化清算、风险缓冲和信用扩张约束。

## 9. 验收门（P2-A）

1. `pay_asset != NOV` 至少可在两条 route 之间择优执行。
2. route 不可用与流动性不足能稳定返回标准化失败码。
3. 成功回执可见 route 元数据。
4. clearing result 继续进入 settlement 主线，不绕开 NOV 内部结算。
5. 文档与实现不得出现“AMM 即时报价直接决定 gas/Execution Fee 清算价”的路径。
6. `P_clear` 必须按 epoch 固定，不按每笔交易实时波动；代码 v1 已以 `protocol_clearing_prices` 快照记录 `P_epoch/P_pay/P_redeem`。
7. oracle 必须是许可参考源，不是开放喂价系统；代码 v1 只读取 governance/runtime 许可参考价，不提供开放喂价入口；`get_fee_oracle_rates` / `get_protocol_clearing_price` 必须暴露 `oracle_allowed_sources`、`oracle_disabled_sources`、`oracle_source_allowed`、`oracle_source_disabled`、`oracle_open_feed_allowed=false`。
8. `cargo check / clippy / test / supervm-mainline-gate` 全绿。

## 9.1 2026-06-13 代码落地状态

- `crates/novovm-node/src/tx_ingress.rs` 已新增 `NovProtocolClearingPriceV1`。
- fee quote 优先使用 `P_pay`，支付资产按保守价折算为 NOV value。
- Treasury direct clearing route 使用同一协议清算价，不再直接使用 AMM spot。
- AMM TWAP 进入 `P_ref` 前必须通过最小 NOV 深度门禁；低流动性 AMM 池会被记录为 `amm_twap:low_liquidity` rejected source，并使协议清算价进入 `constrained`，不得直接参与 gas/Execution Fee 清算，也不得作为 oracle 偏离检测的锚点。
- `treasury.redeem` 在 `asset_out + nov_amount` 形态下使用 `P_redeem`，先扣用户 NOV，再按反向保守价从 Treasury reserve 出资产。
- 非 NOV reserve redeem 禁止使用 `asset_out/asset + amount` 直接指定外币出库；产品路径必须使用 `asset_out + nov_amount`，经 `P_redeem` 折算并先扣 NOV。
- 只读查询 `nov_call treasury.get_protocol_clearing_price` 与 mainline wrapper `nov_getProtocolClearingPrice` 可返回 `P_epoch/P_pay/P_redeem`、source、rejected source、epoch、attack-resistance 状态和 oracle source allowlist/disabled/rotation 状态；非白名单、已禁用、过期或缺失更新时间的 oracle source 会被记录为 rejected source，不进入 `P_ref`，也不得作为无快照时的 `P_prev` 来源。许可 oracle 只有在存在 NAV、合格 AMM TWAP 或历史协议价等非 oracle 锚点时才能参与清算价；oracle-only 初始定价会 fail-closed。fee quote / clearing liquidity / clearing route / TreasuryDirect 非 NOV 支付路径在协议清算价拒绝 oracle-only 或 no-anchor 资产时必须 fail-closed，不能回退到直接 oracle rate 或默认价。主线产品 smoke 已覆盖 disabled/stale/missing-timestamp/oracle-only 被拒绝、fee quote oracle-only fallback 被拒绝、oracle-only clearing liquidity 返回 blocked/unavailable、oracle-only clearing route 不暴露 TreasuryDirect、`epoch_fixed=true`、`amm_spot_allowed=false`、`P_pay < P_epoch < P_redeem`。
- governance/Treasury policy 可通过 mainline wrapper `nov_applyTreasuryPolicy` 写入 `fee_oracle_allowed_sources`、`fee_oracle_disabled_sources`、禁用原因、source rotations、`mapped_lock_min_confirmations` 和 mapped auto-heal policy；`nov_call treasury.get_fee_oracle_rates` 与 mainline wrapper `nov_getFeeOracleRates` 暴露当前 oracle source、allowlist、disabled list、source 是否允许/禁用和 `oracle_open_feed_allowed=false`。主线产品 smoke 已覆盖治理未启用时 fail-closed、治理启用后写入并可查询。
- 主线 `nov_redeem(asset_out + nov_amount)` 产品 smoke 已覆盖 `P_redeem` 路径：先扣用户 NOV，再按 `protocol_clearing_redeem:*` 的反向保守价从 Treasury reserve 出资产，settlement journal 记录 `clearing_rate_ppm = P_redeem`；非 NOV `asset_out/asset + amount` legacy 直接出库会 fail-closed，不扣 NOV、不出外币。
- Treasury reserve proof effective-status 与 amount-cap gate 已接入非 NOV 资产路径：如果某资产已登记 reserve proof 且 effective status 非 `active`，该资产不能继续用于 `nov_depositReserve`、fee clearing，也不能从 Treasury redeem 出库；如果 active proof 的 `reserve_amount` 低于 projected Treasury reserve/exposure，`nov_depositReserve` 和 fee clearing 不得继续扩张，treasury redeem 不得在操作后继续高于 proof cap；缺失 proof 仍保持 non-claim，不被误写成自动外部验真。
- 主线产品只读查询已暴露 `nov_getTreasuryReserveProof` / `nov_getTreasuryReserveSnapshot`；产品 smoke 已覆盖 active proof cap 足够时 `nov_redeem` 成功、proof cap 调低后同一路径 fail-closed 且用户余额不变。
- 主线 Treasury native 产品 smoke 已覆盖 `nov_depositReserve -> nov_getTreasuryReserveSnapshot -> nov_redeem -> nov_getAssetBalance / nov_getTreasurySettlementJournal`，并验证 proof cap 调低后 `nov_depositReserve` fail-closed 且 reserve 不变。
- 主线统一账户产品 smoke 已覆盖 shadow/internal MVP mapped asset 生命周期：`ua_registerMappedLock -> account_assets -> ua_burnMappedAsset -> ua_releaseMappedLock -> ua_getMappedAsset`，输出固定 `phase4_mode=shadow`、`settlement_effect=none`；也已覆盖 live `Ethereum lock evidence -> NETH/M2 credit -> burn -> Treasury reserve debit/release` 主线入口闭环，固定 `nov_minted=0`，不声明真实外部链释放或自动桥调度。
- `ua_registerMappedLock(phase4_mode=live)` 已要求结构化 Ethereum lock event evidence、receipt MPT proof 和本地 `novovm-network` runtime canonical finalized block anchor，并能把通过校验的 ETH lock MVP 映射为 `NETH` M2 credit，写入 native account balance、Treasury reserve 和 settlement journal；`ua_burnMappedAsset -> ua_releaseMappedLock` 已能扣减用户 NETH credit 并释放 Treasury NETH reserve。
- M2 bridge pause v1 已接到 native execution store 和 governance policy：`mapped_lock_bridge_paused` 阻断 live register；`mapped_asset_burn_paused` 阻断 burn；`mapped_asset_release_paused` 阻断 release；env 级全局/单项暂停用于紧急 fail-closed。
- M2 source anchor reorg gate v1 已接入 mapped asset record：live register 持久化 source anchor；`ua_getMappedAsset` 暴露 `source_anchor_status`；burn/release 前复查本地 canonical finalized anchor，unsafe 时 fail-closed，不推进状态。ETH lock contract address 已可通过主线 `nov_applyTreasuryPolicy(mapped_lock_contract_address)` 治理写入，live proof 优先采用 native store 治理配置，参数级 expected address 只作为兼容 fallback。治理化 header source whitelist/quorum gate v1 已接入 native execution store：主线治理包装 `nov_setMappedHeaderSourcePolicy` 可要求 live lock proof 的 runtime header 必须来自治理许可 `source_peer_id`，并配置 `min_source_quorum`；runtime 会按同一 `block_hash` 已观测到的许可 source peer 集合计算 quorum，不满足时 fail-closed。该 policy 已支持 `disabled_peer_ids`、`disabled_peer_reasons/slashing_reasons` 和 `peer_rotations`，被治理禁用的 source peer 不得作为证明来源且不计入 quorum，old peer -> new peer 的 `peer_rotations` 可作为治理 source rotation 记录。治理化 Ed25519 header attestation quorum v1 已接入 native execution store：主线治理包装 `nov_setMappedHeaderAttestationPolicy` 可要求 live lock proof 携带治理许可 `header_attestations`，每个 attestation 用许可 public key 对 `chain_id/block_number/block_hash/receipts_root` 签名，并配置 `min_attestation_quorum`；签名无效、quorum 不足或 signer 被 `disabled_signers` 禁用时 fail-closed，禁用原因由 `disabled_signer_reasons` 持久化并可查询，old signer -> new signer 的 `signer_rotations` 可作为治理 key rotation 记录。`nov_getMappedFinalitySourceStatus` 只读聚合当前 lock contract/source/attestation/min confirmations/auto-heal 状态；这些 wrapper 不新增第二状态源。
- M2 manual freeze/recovery/rollback v1 已接入 `ua_freezeMappedAsset` / `ua_unfreezeMappedAsset` / `ua_rollbackFrozenMappedAsset`：active live NETH 冻结会扣用户 native 可用余额、保留 Treasury reserve，并把 mapped asset 状态置为 `frozen`；source anchor 恢复 canonical finalized 后才能 unfreeze，恢复用户 native 可用余额；source anchor 仍 unsafe 时可 rollback，扣回内部 Treasury NETH reserve 并把 mapped asset 置为 `rejected`，不返还用户余额、不 mint NOV、不触发外部链释放。
- M2 auto heal v1 已接入 `ua_autoHealMappedAssets` 和主线显式调度入口 `nov_runMappedAssetAutoHeal`：默认 dry-run 报告 unsafe anchor，并对每个候选返回 `required_policy/policy_would_apply`；`apply=true` 必须同时满足 `scheduler_authorized=true` 和 governance/Treasury policy。policy 可通过 `nov_applyTreasuryPolicy(mapped_asset_reorg_response_policy)` 统一配置为 `report_only / freeze_only / freeze_and_rollback`。`freeze_only` 可自动冻结 active/burn_pending live NETH，扣用户 native 可用余额、保留 Treasury reserve；`freeze_and_rollback` 额外允许对已 frozen 且 source anchor 仍 unsafe 的资产自动执行内部 rollback，扣回 Treasury NETH reserve 并把 mapped asset 置为 `rejected`。主线产品 smoke 已覆盖 live NETH 入账后 source anchor reorg -> explicit tick dry-run -> unauthorized apply fail-closed -> mainline policy freeze -> scheduler-authorized policy rollback；该路径不赔付、不链上出金、不 mint NOV，也不等于常驻后台 scheduler 已完成。
- M2 finality policy v1 已接入 governance/Treasury policy：`mapped_lock_min_confirmations` 可治理设置，live ETH lock proof 优先使用 native store policy，未设置时 fallback 到 env/default；主线产品 smoke 已覆盖 governed min confirmations 未达标 fail-closed 与降低确认数后通过。
- Treasury reserve proof v1 已接入最小治理登记/查询与主线产品面：`nov_setTreasuryReserveProof` / `governance.set_reserve_proof` 登记 proof metadata；`treasury.get_reserve_proof` / `treasury.get_reserve_snapshot` 与 `nov_getTreasuryReserveProof` / `nov_getTreasuryReserveSnapshot` 只读暴露 proof effective status、amount cap 与 non-claim 标记。主线产品 smoke 已覆盖 revoked proof 写入、non-claim 标记查询，以及 revoked proof 阻断 `nov_depositReserve`。
- Native execution store 主写路径已加 lockfile single-writer guard，用于单机/低并发 load-modify-save 互斥；该 guard 不是高并发事务后端。
- 当前仍不声明完整 external finality source 管理、治理赔付、真实链上出金、真实自动 reserve proof verification、跨链自动 mint/redeem、NOV 直接铸造、高并发账本入口或钱包/DAPP/网站完成；当前 finality source 管理只覆盖 source peer whitelist/quorum、disabled peer slashing reason/fail-closed、source peer rotation 记录、Ed25519 attestation quorum、disabled signer reason/fail-closed、signer rotation 记录、最小 confirmations 和 `nov_getMappedFinalitySourceStatus` 只读状态聚合；主线产品 smoke 已覆盖未授权 finality policy 写入 fail-closed、source quorum 不足 fail-closed 与第二个许可 source peer 观测后通过、attestation quorum 不足/disabled signer fail-closed 与两个 active signer 通过、governed min confirmations 未达标 fail-closed 与降低确认数后通过；protocol clearing price / oracle read-only wrappers、`P_redeem` reserve redeem、Treasury policy mainline write 和 reserve proof mainline write 也已有主线产品 smoke。

## 10. 本稿替换与冲突规则

若后续实现与本稿冲突：

1. 先更新本稿并写明偏离理由。
2. 再改实现。
3. 未更新文档的偏离不视为有效决议。

---

本文件是 `P2-A` 的制度级冻结稿，用于保证“规则清算轨 + 市场交易轨 + 三池隔离”先成立，再扩展复杂聚合与高级策略。
