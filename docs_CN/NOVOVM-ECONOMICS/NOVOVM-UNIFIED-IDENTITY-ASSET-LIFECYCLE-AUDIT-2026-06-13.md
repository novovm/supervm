# NOVOVM 统一身份与资产生命周期审阅

日期：2026-06-13  
范围：`D:\WEB3_AI\SUPERVM` 本地代码与文档  
不在范围：`D:\WEB3_AI\MEV`、外部钱包前端、真实 Ethereum lock 合约部署、安全审计

## 1. 总结结论

NOVOVM 现在已经具备“统一身份 + EVM 产品入口 + native NOV 经济模块”的基础闭环，但还不能声明“用户从 Ethereum 锁 ETH 后自动铸造 NOV 并可全自动赎回”的完整跨链资产生命周期。

可以签收的部分：

- 统一身份注册、主密钥、策略、persona 绑定、EVM 地址绑定、Web30 地址绑定、绑定查询、nonce 查询、审计事件，已经有生产入口。
- EVM 用户路径已经进入宿主：`eth_sendRawTransaction -> gateway -> native pending -> host execution -> canonical receipt -> eth_getTransactionReceipt`。
- native NOV 经济模块已有余额、treasury reserve、fee settlement、redeem、AMM swap、credit vault/mint debt asset 等本地执行与 receipt。
- `account_balance` / `account_assets` 已经能把 native liquid balance、mapped asset、credit vault、treasury exposure 汇总成账户视图。
- 统一账户唯一入口已收敛为 `novovm-node -> mainline_query -> unified_account_surface`；`evm-gateway` 不再拥有本地 UCA store/router，只保留 EVM RPC adapter 职责。
- gateway adapter-only 边界已冻结并打 tag：`evm-gateway-adapter-only-v1`。
- 上层经济制度已冻结为：`NOV = M0/M1`；`NETH/NUSDT/nAsset = M2`；多资产可支付 Execution Fee，但必须按协议清算价折算为 NOV value，并进入 Treasury Reserve Pool。

不能过度声明的部分：

- 当前没有发现 NOVO Wallet 前端/钱包应用代码；仓库里是 RPC/节点/gateway 能力，不是完整用户 App。
- `crates/novovm-node/src/main.rs` 是 dead/historical source，不是当前 Cargo 产品 binary；不得再向该文件增加主线语义。当前统一账户产品入口以 `crates/novovm-node/src/bin/novovm-node.rs -> mainline_query -> unified_account_surface` 为准。
- ETH 锁仓合约不是已部署真实 Solidity lock contract 路径；当前 `ua_registerMappedLock` live 模式已要求结构化 Ethereum lock event evidence，并通过 `source_chain_id + block_hash + receipts_root + receipt_index + receipt_proof` 验证 receipt MPT inclusion，且要求 `receipts_root` 匹配本地 `novovm-network` runtime canonical finalized block anchor；治理开启 `ua_setMappedHeaderSourcePolicy(required=true)` 后，该 runtime header 还必须来自许可 `source_peer_id`，并满足 `min_source_quorum`。runtime 会按同一 `block_hash` 已观测到的许可 source peer 集合计算 quorum，不满足时 fail-closed。通过后再解析 receipt log 的 contract/topic0，目标资产为 `NETH`。
- `phase4_mode=live` 的内部 MVP 接线已能把 mapped lock 记为 `NETH` M2 credit，写入 native account balance、Treasury reserve 和 settlement journal；`phase4_mode=shadow` 仍只做审计映射，不入账。
- M2 bridge pause v1 已接入 mainline native execution store / governance policy：live register、burn、release 可分别被 `mapped_lock_bridge_paused`、`mapped_asset_burn_paused`、`mapped_asset_release_paused` fail-closed 阻断，暂停时不推进 mapped asset 生命周期。
- M2 source anchor reorg gate v1 已接入 mapped asset 生命周期：live register 持久化 source anchor；`ua_getMappedAsset` 暴露 `source_anchor_status`；burn/release 前复查本地 canonical finalized anchor，unsafe 时 fail-closed 阻断。治理化 header source whitelist/quorum gate v1 已接入 native execution store：开启后 live lock proof 的 runtime header source peer 必须在许可列表中，并满足 `min_source_quorum`。
- M2 manual freeze/recovery/rollback v1 已接入 `ua_freezeMappedAsset` / `ua_unfreezeMappedAsset` / `ua_rollbackFrozenMappedAsset`：active live NETH 冻结会扣用户 native 可用余额、保留 Treasury reserve，并把 mapped asset 状态置为 `frozen`；source anchor 恢复 canonical finalized 后才能 unfreeze，恢复用户 native 可用余额；source anchor 仍 unsafe 时可 rollback，扣回内部 Treasury NETH reserve 并把 mapped asset 置为 `rejected`，不返还用户余额、不触发外部链上出金、不 mint NOV。
- M2 auto heal v1 已接入 `ua_autoHealMappedAssets`：默认 dry-run 只报告 unsafe source anchor；`apply=true` 必须先由 governance/Treasury policy 开启 `mapped_asset_auto_heal_enabled`，否则 fail-closed。开启后只自动冻结 active/burn_pending live NETH，扣减用户 native 可用余额并保留 Treasury reserve；frozen asset 只给出 unfreeze/rollback 建议，不自动处置。
- M2 finality policy v1 已接入 governance/Treasury policy：`mapped_lock_min_confirmations` 可治理设置，live ETH lock proof 优先使用 native store policy，未设置时 fallback 到 env/default。
- 仍没有真实“完整 external finality source 管理 -> compensation -> Treasury policy -> NOV emission”的完整链上桥接。治理化 header source peer quorum 和 Ed25519 header attestation signature quorum 已有，但 NOV mint 在 consensus token runtime 中存在，不能直接接在 ETH lock proof 上。
- EVM 合约币可以在 EVM 产品面执行/查询 receipt，但和 NOV native account balance/treasury 是两套状态面，当前没有完整 ERC20 -> native asset 自动映射桥。
- 并发量不能直接引用 README 的 L0/L1 百万 TPS 来代表钱包/gateway 入口吞吐。gateway 当前是单 HTTP loop，EVM pending consumer 默认 16 笔/250ms；native NOV store 是 JSON load-modify-write，适合单进程顺序产品闭环，不适合多进程高并发账本写入口。
- 当前不声明 DAPP、网站、钱包进入本轮范围；本轮已完成上层经济规则、协议清算价 v1 和审阅边界，但不声明真实外部桥接自动闭环。

## 1.1 主线 `unified_account_surface` 产品闭环审阅

当前可签收：

- `mainline_query` 已将 `ua_createUca`、`ua_bindPersona`、`ua_setPolicy`、`ua_registerMappedLock`、`account_balance`、`account_assets` 等统一账户方法路由到 `run_mainline_unified_account_query`。
- `unified_account_surface` 持久化 `UnifiedAccountRouter`、audit cursor 和 `UnifiedMappedAssetState`，mapped asset 能进入 `account_balance/account_assets` 聚合视图。
- `ua_checkRoute` 使用 clone/probe 路径做只读预检，返回 `read_only: true`，不推进 nonce，不写 UCA 状态。
- `evm-gateway` 的 `eth_sendRawTransaction` / `eth_sendTransaction` 只通过 mainline `ua_checkRoute` 做只读 route/policy 校验；gateway 不再持有本地 UCA store/router。

当前边界锁定：

- 当前 Cargo 产品 binary 是 `crates/novovm-node/src/bin/novovm-node.rs`，不是 dead `crates/novovm-node/src/main.rs`。
- 产品 binary 通过 `run_mainline_query_from_path` 与 `is_mainline_unified_account_query_method` 接入 mainline 统一账户 surface。
- 产品 binary 不嵌入 legacy `run_public_rpc` / `run_unified_account_rpc` / `public_unified_account_runtime`。
- `crates/novovm-node/src/main.rs` 只能作为历史验证残留看待；后续开发不得把它重新定义为产品入口，也不得在其中新增主线统一账户语义。

## 1.2 上层经济法条与协议清算边界

本审阅采用以下经济制度口径：

- `NOV` 是唯一基础货币、最终结算货币、矿工/算力结算货币，归属 `M0/M1`。
- `NETH`、`NUSDT`、`nAsset` 是外部锁仓、储备或信用生成的 M2 资产，不进入 `M0/M1`。
- `NETH` 是锁仓 ETH 的 1:1 储备/存款凭证，不是 NOV，也不是自动铸 NOV 的中间态。
- 用户可用白名单 M2 资产支付 Execution Fee；系统按协议清算价折算为 `NOV value`。
- 支付资产进入 `Treasury Reserve Pool`，中文统一称为“国库储备池 / 外汇储备池”。
- 矿工只收 NOV；NOV 发行或矿工结算额度必须受 `reserve bucket`、`fee bucket`、`risk buffer`、`emission policy` 约束。
- AMM 只负责市场价格发现、用户交易和套利收敛，不直接决定 Execution Fee 清算价。
- 外部 oracle 只能作为治理许可参考源，用于偏离检测、熔断和兜底，不能开放喂价，不能单独决定协议清算价。

协议清算价按 epoch 固定：

```text
P_clear[A, e] = NOV per 1 unit of asset A
```

支付时采用保守价：

```text
P_pay[A] = P_epoch[A]
         * (1 - reserve_haircut[A])
         * (1 - liquidity_haircut[A])
         * (1 - volatility_haircut[A])
```

赎回时采用反向保守价：

```text
P_redeem[A] = P_epoch[A]
            * (1 + redemption_spread[A])
            * (1 + risk_surcharge[A])
```

当前状态：制度规则已写入审阅口径；完整代码实现仍需后续阶段，不在本轮声明完成。

## 2. 生命周期现状

### 2.1 注册统一身份

入口：

- `ua_createUca`

实现位置：

- `crates/novovm-adapter-api/src/unified_account.rs`
- `crates/novovm-node/src/unified_account_surface.rs`

生命周期：

1. 用户提供 `uca_id/account_id` 和主密钥信息。
2. `UnifiedAccountRouter` 创建 UCA，状态为 active。
3. `unified_account_surface` 持久化 UCA snapshot，并写审计事件。

状态：可用。

风险：

- UCA 是统一身份核心，生产默认应使用 RocksDB 后端；bincode/file backend 已被环境变量限制为非生产路径。

### 2.2 绑定 NOVOVM / Web30 账户

入口：

- `ua_bindPersona`

生命周期：

1. UCA 已存在。
2. 用户绑定 `PersonaType::Web30`，带 `chain_id` 和外部地址。
3. 后续 Web30 交易通过绑定 owner 解析 UCA。

状态：可用。

风险：

- Web30/统一账户 host 入口必须走 `novovm-node -> mainline_query -> unified_account_surface`，不能从 `evm-gateway` 创建第二份账户状态。

### 2.3 绑定 Ethereum 账户

入口：

- `ua_bindPersona`
- `eth_sendRawTransaction`

生命周期：

1. 用户先绑定 `PersonaType::Evm`，`chain_id=1` 或目标 EVM chain。
2. `eth_sendRawTransaction` 会从 raw tx 恢复 sender。
3. 如果 sender 已绑定 UCA，则可省略 `uca_id`；如果显式提供 `uca_id`，必须与 mainline 绑定 owner 一致。
4. `eth_sendRawTransaction` 通过 mainline `ua_checkRoute` 做只读预检；mainline UCA 不可用或策略拒绝时 fail closed，不 fallback 到 gateway 本地状态。
5. gateway 只作为 EVM RPC adapter 对 raw tx 执行 chain id、nonce、tx type、fork activation、sender、fee 等 EVM 兼容校验；统一账户状态的注册/绑定/策略以 mainline surface 为准。

状态：可用。

风险：

- 外部钱包必须先完成 mainline UCA/EVM persona 绑定；仅传 `uca_id` 不能绕过绑定与策略预检。钱包接入层需要封装注册/绑定引导。

### 2.4 查询账户、绑定、余额、资产

入口：

- `ua_getAccount`
- `ua_getPolicy`
- `ua_listBindings`
- `ua_getBindingOwner`
- `ua_getNextNonce`
- `ua_getAuditEvents`
- `account_balance`
- `account_assets`

生命周期：

1. UCA 查询返回账户、策略、绑定数量。
2. Binding owner 查询可从 Ethereum/Web30 persona 反查 UCA。
3. `account_balance` 聚合：
   - native liquid balance
   - mapped asset active balance
   - mapped asset shadow balance
   - credit vault locked collateral
   - debt outstanding
   - treasury source flow
   - treasury NOV bucket exposure
4. `account_assets` 返回资产列表、pledge、vault、treasury exposure、mapped assets。

状态：可用。

风险：

- 查询口径是“聚合视图”，不是单一资产账本。前端必须展示 component/source，否则用户会误以为 mapped/shadow/treasury exposure 都是可立即提现吗。

### 2.5 转账、付款、使用

EVM 入口：

- `eth_sendRawTransaction`
- `eth_getTransactionReceipt`
- `eth_getTransactionByHash`
- `eth_getBlockByNumber`
- `eth_call`
- `eth_estimateGas`

native NOV 入口：

- `nov_sendRawTransaction`
- `nov_sendTransaction`
- `nov_execute`

生命周期：

1. EVM raw tx 立即返回 tx hash。
2. gateway 后台 consumer 从 native pending 消费。
3. mainline EVM execution 产出 canonical batch。
4. pending tx 标记为 included/onchain。
5. 用户通过 receipt/block/tx 查询结果。
6. native NOV execute 可以调用 treasury、AMM、credit_engine 等模块，并持久化 native receipt。

状态：EVM 产品 v1 可用；native NOV 模块可用。

风险：

- `eth_sendRawTransaction` 是异步提交语义，不应等待执行完成。这符合 Ethereum RPC 习惯，但钱包/产品层必须做 receipt 轮询。
- native NOV 执行 store 当前是 JSON load-modify-write；单 gateway 顺序执行可用，多进程/多 writer 需要收口为单 writer 或 RocksDB/事务化后端。

### 2.6 Ethereum 合约币

现状：

- EVM 插件已支持合约部署、调用、receipt/log、block/tx 查询产品路径。
- EVM 合约内的 ERC20/合约币可以作为 EVM 状态的一部分被执行和查询。

边界：

- 当前未发现 ERC20 balance 自动同步到 `account_assets` 的 native asset balance。
- 当前未发现 ERC20 Transfer event 自动映射到 native treasury/asset ledger 的产品接线。

状态：EVM 内可用；跨入 NOVOVM native 账户资产视图为 partial。

### 2.7 ETH 锁仓合约与 mapped asset

入口：

- `ua_registerMappedLock`
- `ua_getMappedAsset`
- `ua_burnMappedAsset`
- `ua_releaseMappedLock`

当前实现：

1. 用户提供 mapped lock proof。
2. surface 校验目标 UCA 存在。
3. proof 校验要求：
   - amount > 0
   - source asset 必须是 `ETH`
   - source chain 必须是 Ethereum
   - proof format 是 `EthereumLockEventV1`
   - proof payload 必须等于本地 `mapped_lock_proof_digest_v1`
   - live 模式必须提供结构化 Ethereum lock event evidence，并由 evidence 派生 `source_lock_ref`
4. 注册后创建 mapped asset record。
5. 目标资产固定为 `NETH`。
6. `NETH` 作为 M2 储备/存款凭证，active 状态计入 `account_balance/account_assets` 的 mapped asset component。
7. burn 后进入 `BurnPending`。
8. release 后进入 `Released`。

状态：内部 MVP 闭环可用；live 模式可形成 NETH/M2 native credit，shadow 模式仍为 non-settlement 审计映射。

关键边界：

- 这不是完整真实外部桥；live 模式的 receipt MPT proof 已要求锚定本地 runtime canonical finalized block，并可通过 `ua_setMappedHeaderSourcePolicy` 约束 header source peer 和 `min_source_quorum`，且 quorum 已按同一 `block_hash` 的多 source 观测计数；`ua_setMappedHeaderAttestationPolicy` 已可约束 Ed25519 header attestation public key 和 `min_attestation_quorum`，live proof 必须携带对 `chain_id/block_number/block_hash/receipts_root` 的 `header_attestations` 签名；`mapped_lock_min_confirmations` 已可由 governance/Treasury policy 设置；`ua_autoHealMappedAssets` 只提供治理开启后的 unsafe asset 自动冻结执行入口，但还没有完整 finality source 管理、完整自动调度、治理赔付或链上出金。
- 这不是 NOV 铸造路径；ETH lock 只能先形成 `NETH` M2 凭证。
- live mapped lock 会写入 native `account_asset_balances[NETH]`、`treasury_reserves[NETH]` 和 `treasury_settlement_journal`，但 `settled_nov=0`、`nov_minted=0`。
- live 模式已校验 lock contract address、`Locked(address,bytes32,uint256,string)` topic0、source chain id、block number、block hash、finalized block number、`source_lock_ref` 派生一致性、receipt MPT proof、receipt envelope 与 proof value 一致性、receipt status 成功、receipt log address/topic0、`receiptsRoot` 与本地 runtime canonical finalized block anchor 一致，并可在治理开启后校验 header source peer 白名单 / `min_source_quorum` 和 Ed25519 header attestation 签名白名单 / `min_attestation_quorum`。
- live register、burn、release 已有 bridge pause 门禁；暂停由 native store / governance policy 或 env 触发，失败时不推进 active/burn_pending/released 状态。
- live register 已持久化 source anchor；burn/release 前会复查本地 runtime canonical finalized anchor，reorg out、finality 丢失或 receiptsRoot mismatch 时拒绝推进。
- `ua_freezeMappedAsset` 可人工冻结 active/burn_pending mapped asset；active live NETH 冻结会从用户 native liquid balance 扣减，但 Treasury reserve 保留用于后续恢复或风险处置。`ua_unfreezeMappedAsset` 会先复查 source anchor，只有 canonical/finalized/receiptsRoot 重新安全时才恢复用户 native liquid balance。`ua_rollbackFrozenMappedAsset` 只允许 source anchor 仍 unsafe 的 frozen asset 执行，扣回内部 Treasury NETH reserve 并把 mapped asset 置为 `rejected`，不返还用户余额、不 mint NOV、不链上出金。
- `ua_autoHealMappedAssets` 提供最小自动 reorg heal 执行入口：dry-run 报告候选；`apply=true` 必须由 governance/Treasury policy 开启，开启后只冻结 unsafe active/burn_pending live NETH；frozen 后仍需治理选择 unfreeze 或 rollback。
- 仍没有完整 external finality source 管理、治理赔付或链上出金。
- Phase4 shadow/no-go 环境变量可阻断 live register 路径。
- `settlement_effect=neth_m2_credit` 只表示内部 NETH/M2 入账，不应直接解释为链上 ETH 已释放。

### 2.8 NOV 铸造

已存在能力：

- `crates/novovm-consensus/src/token_runtime.rs` 中 `TokenRuntime::mint` 可向 NodeId 对应地址 mint NOV，并维护 `minted_locked_total`。
- native treasury settlement 可把支付资产折算成 NOV bucket。
- credit engine `open_vault` 可在抵押后 mint debt asset，例如默认 `NUSD`，不是 NOV。

制度边界：

- `ua_registerMappedLock(ETH)` 成功后不得直接调用 `TokenRuntime::mint` 铸造 NOV。
- ETH lock 先生成 `NETH` M2 凭证。
- NOV mint 或矿工 NOV 结算必须经过 Treasury policy / emission policy，并受 reserve/fee/risk bucket 约束。

未完成接线：

- MVP live 模式已有 receipt MPT inclusion + 本地 canonical finalized block anchor + Ethereum lock event evidence -> NETH/M2 credit 的内部账本接线，并已有 mapped bridge pause 门禁、source anchor reorg gate、按同一 `block_hash` 多 source 观测计数的 header source whitelist/quorum gate、Ed25519 header attestation signature quorum gate、governed min confirmations、manual freeze/recovery/rollback 和最小 auto-freeze heal；但没有完整 finality source 管理、治理赔付或链上自动出入金。
- 没有发现真实外部 ETH reserve 与 NOV supply、NETH 负债、M2 credit exposure 的完整约束关系。

状态：NOV mint 能力存在于 consensus runtime；ETH 锁仓 live MVP 只能先形成 NETH/M2 native credit，不能声明直接触发 NOV 铸造。

### 2.9 余额赎回

入口：

- native module `treasury.redeem`
- native module `treasury.redeem_reserve`
- mapped asset `ua_burnMappedAsset`
- mapped asset `ua_releaseMappedLock`

生命周期：

1. native `treasury.redeem/redeem_reserve` 从 treasury reserve 扣除资产。
2. NOV 赎回会检查 reserve bucket、min reserve、total reserve。
3. 成功后 credit 用户 native balance，并写 settlement journal。
4. mapped asset 赎回分两步：burn mapped asset，再 release source lock。
5. live mapped asset burn 会先扣用户 native `NETH` M2 credit；release 后扣 Treasury `NETH` reserve。

状态：native reserve 赎回可用；mapped asset burn/release 内部闭环可用，live 模式已接入 native NETH/M2 扣减和 reserve 释放。

风险：

- mapped asset release 不是 Ethereum 链上解锁交易。
- native treasury redeem 是内部账本状态变化，不等于外部资产链上出金。

## 3. 发现的问题

### P0：真实 ETH lock proof -> NETH(M2) -> Treasury policy 的外部桥接仍未完成

影响：

- 不能向用户声明“锁 ETH 自动铸 NOV，NOV 赎回自动释放 ETH”已经完成。
- 不能把 `NETH` 写成 NOV；`NETH` 是 M2 储备/存款凭证。

证据：

- mapped lock proof live 模式目前已验证 receipt MPT inclusion、receipt status、receipt log address/topic0、结构化 event evidence、本地 runtime canonical finalized block anchor，并可在治理开启后验证 header source peer 白名单、`min_source_quorum`、Ed25519 header attestation public key 白名单和 `min_attestation_quorum`。
- target asset 是 `NETH`，归属 M2，不是 NOV。
- live `ua_registerMappedLock` 已能写入 native NETH/M2 credit、Treasury reserve 和 settlement journal，且不 mint NOV。
- live `ua_burnMappedAsset -> ua_releaseMappedLock` 已能扣减用户 NETH credit 并释放 Treasury NETH reserve。
- live proof 已固定 lock contract 配置、事件 topic、source_chain_id、block_hash、receipt_index、receipt_log_index、receipt MPT proof、finalized block number、本地 finalized canonical block anchor 和 `source_lock_ref` 派生校验。
- `mapped_lock_min_confirmations` 已可由 governance/Treasury policy 设置，live ETH lock proof 优先用 native store policy 校验 finalized depth。
- `ua_setMappedHeaderAttestationPolicy` 已可设置治理许可 Ed25519 header attestation public key 集合和 quorum；live ETH lock proof 携带的 `header_attestations` 必须对 `chain_id/block_number/block_hash/receipts_root` 签名，签名无效或达不到 quorum 时 fail-closed。
- live bridge pause 已固定 register/burn/release 三个 gate，防止 header/reorg/reserve 异常时继续扩张或释放 NETH M2。
- live mapped asset record 已持久化 source anchor；`ua_getMappedAsset` 暴露 `source_anchor_status`；burn/release 在 anchor unsafe 时拒绝推进，避免 reorg 后继续释放。
- `ua_freezeMappedAsset` 已能把异常 mapped asset 置为 `frozen`；active live NETH 冻结会扣减用户 native 可用余额，Treasury reserve 不释放。`ua_unfreezeMappedAsset` 只能在 source anchor 恢复安全后把 frozen NETH 返还为 active/liquid。`ua_rollbackFrozenMappedAsset` 只能在 source anchor 仍 unsafe 时扣回内部 Treasury NETH reserve，并把 frozen asset 终止为 `rejected`。
- `ua_autoHealMappedAssets` 已能扫描 unsafe source anchor，并在 governance/Treasury policy 开启 `mapped_asset_auto_heal_enabled` 后用 `apply=true` 自动冻结 active/burn_pending live mapped asset；默认 dry-run 不改状态，未开启时 fail-closed。
- `TokenRuntime::mint` 没有接到 Treasury policy / emission policy 路径。

建议：

- 下一步只做最小产品桥：
  - 把当前 header source whitelist/quorum gate 和 Ed25519 header attestation quorum gate 继续升级为完整 external finality source 管理。
  - 固定一个 lock contract address 配置已经进入 live proof gate，后续要从可信配置/治理读取。
  - 当前已验 receipt MPT inclusion、`Locked(address indexed owner, bytes32 lockId, uint256 amount, string targetUca)` 的 receipt log address/topic0、本地 finalized canonical block anchor、可选治理 header source whitelist/quorum 和可选治理 Ed25519 header attestation signature quorum；source quorum 已按同一 `block_hash` 多 source 观测计数，最小 finalized confirmations 已可治理设置，下一步补完整 finality source 管理 / reorg heal。
  - `receipts_root` 已不再只信任用户自报；finalized block number 仍只作为确认数约束字段，最终锚定以本地 canonical finalized block 为准。
  - bridge pause 已能阻断 live register/burn/release；source anchor reorg gate 已能阻断 unsafe burn/release；header source whitelist/quorum gate 已能阻断非许可 source peer 或 quorum 不足；Ed25519 header attestation quorum gate 已能阻断无效签名或许可 signer quorum 不足；manual freeze/unfreeze/rollback 已能冻结、安全恢复或终止异常 NETH 暴露；`ua_autoHealMappedAssets` 已能在治理开启后自动冻结 unsafe active/burn_pending live NETH；下一步补完整 finality source、治理赔付规则和自动化调度。
  - 只把 ETH 映射为 `NETH`，不要直接铸 NOV。
  - NOV mint / 矿工结算必须通过 Treasury policy，并使用 epoch 固定的协议清算价。

### P1：钱包/用户入口缺失

影响：

- RPC 能力已经有，但普通用户没有完整注册、绑定、提交、查询、赎回界面。

证据：

- 仓库未发现 NOVO Wallet app/front-end。
- 现有能力集中在 gateway/RPC/node。

建议：

- 不要先做大钱包工程。先做一个最小 wallet adapter 文档和 1 个 CLI/API smoke：
  - create UCA
  - bind EVM address
  - submit raw tx
  - poll receipt
  - query account_assets

### P1：dead `src/main.rs` 历史 UCA 分支不得重新进入产品路径

影响：

- `crates/novovm-node/src/main.rs` 仍保留历史 `run_public_rpc -> run_unified_account_rpc` 代码，但该文件不是当前 Cargo 产品 binary。真实风险不是当前产品入口分裂，而是后续开发误把 dead source 当成可继续扩展的主线入口。

证据：

- `crates/novovm-node/Cargo.toml` 设置 `autobins = false`，产品 bin 显式为 `crates/novovm-node/src/bin/novovm-node.rs`。
- `docs_CN/NOVOVM-NETWORK/NOVOVM-ACCOUNT-PROTOCOL-PHASE2-IMPLEMENTATION-CHECKLIST-2026-04-20.md` 已明确“禁止再往 dead `crates/novovm-node/src/main.rs` 增加主线语义”。
- 产品 binary 已有边界锁定测试：不得嵌入 legacy public RPC UCA surface，并必须保留 `mainline_query` 统一账户识别与路由。

建议：

- 不修改 dead `src/main.rs` 来“修主线”，避免制造新的死代码语义。
- 所有统一账户产品接入继续走 `mainline_query -> unified_account_surface`。
- 后续若要彻底清理，可单独做 dead source 删除/归档任务，并先确认没有外部脚本依赖。

### P1：native NOV 账本写入后端不适合多 writer

影响：

- `novovm-native-execution-store.json` 是 load-modify-write；如果多个进程同时写，会丢更新。

证据：

- `load_nov_native_execution_store_v1` 直接读 JSON。
- `save_nov_native_execution_store_v1` 直接 `fs::write`。

建议：

- 产品部署上先规定单 gateway writer。
- 如果要并发写入，最小改法是把 native execution store 切 RocksDB 或加单进程队列，不要做复杂工程化。

### P1：余额视图需要前端解释 source/component

影响：

- 用户可能把 mapped/shadow/treasury exposure 误解为可立即转账余额。

证据：

- `account_balance` 返回 liquid、mapped、pledge、debt、treasury exposure 多 component。

建议：

- 钱包展示分层：
  - 可用余额
  - 锁定/抵押
  - 映射资产
  - 债务
  - treasury 流水/敞口

### P2：EVM 合约币和 native asset ledger 未完全打通

影响：

- EVM ERC20 可以在 EVM 内执行，但不会自动成为 NOVOVM native `account_assets` 的 liquid asset。

建议：

- 不要做全 ERC20 桥。先选白名单 ERC20，按 receipt log 索引为只读资产视图，再决定是否允许映射为 native asset。

### P2：并发口径需要拆层

影响：

- 容易把 AOEM/consensus benchmark 的 TPS 当成 wallet/gateway TPS。

当前口径：

- AOEM/consensus benchmark：文档内有百万级 TPS seal，但属于内核/consensus plane。
- EVM gateway 用户入口：`tiny_http` 单 loop；默认 pending execution `16` 笔/`250ms`。
- native NOV store：JSON store，单 writer 可用，不是高并发存储层。

建议：

- 对外写法必须分层：
  - “内核/共识吞吐”
  - “EVM gateway 产品入口吞吐”
  - “Ethereum mainnet RLPx 同步吞吐”
- “native treasury/account store 写入吞吐”

### P2：协议清算价 v1 已落地，外部桥接自动化仍未完成

影响：

- 当前代码已实现 `P_epoch/P_pay/P_redeem` 的最小生产语义：按 epoch 固定、使用显式 AMM TWAP / Treasury NAV / 许可 oracle reference / 上一 epoch 价格，AMM spot 不直接进入 Execution Fee 清算。
- `P_pay` 已接入多资产 Execution Fee quote 和 TreasuryDirect clearing。
- `P_redeem` 已接入 `treasury.redeem` 的 `asset_out + nov_amount` 形态：先扣用户 NOV，再按反向保守价从 Treasury reserve 出资产。
- 仍未完成真实外部桥自动化、真实 reserve proof、NOV emission policy 自动接线、真实外部链出金和高并发事务后端。

建议：

- 继续禁止在产品面声明“AMM 即时报价自动结算 gas”。
- 许可 oracle 只能作为治理白名单参考源，不能开放喂价。
- 低流动性、oracle 偏离、储备不足时必须进入 constrained/blocked 或只允许 NOV 支付。

## 4. 端到端产品流程说明

### 流程 A：普通 EVM 用户提交交易

```text
ua_createUca
  -> ua_bindPersona(Evm, chain_id, address)
  -> eth_sendRawTransaction(raw_tx)
  -> gateway validates EVM envelope(chain/sender/nonce/tx type/fork/fee)
  -> mainline ua_checkRoute validates UCA route/policy(read-only)
  -> native pending
  -> gateway background consumer
  -> mainline EVM execution
  -> canonical batch
  -> eth_getTransactionReceipt
```

状态：已闭环。

### 流程 B：NOVOVM native 支付/使用

```text
ua_createUca
  -> ua_bindPersona(Web30 or native account)
  -> nov_sendTransaction / nov_execute
  -> fee settlement
  -> native module dispatch
  -> native receipt
  -> account_balance/account_assets
```

状态：基础闭环可用。

### 流程 C：ETH 映射资产

```text
ua_createUca
  -> ua_bindPersona(Evm)
  -> ua_registerMappedLock(ETH proof)
  -> NETH mapped asset active(M2 reserve/deposit claim)
  -> account_assets shows mapped asset
  -> ua_burnMappedAsset
  -> ua_releaseMappedLock
```

状态：内部 MVP 闭环可用；真实 Ethereum lock contract proof/链上释放未完成。

### 流程 D：ETH 锁仓、NETH、NOV 结算与 M2 信用

```text
Ethereum lock contract
  -> verified Ethereum receipt/log proof
  -> NOVOVM mapped lock
  -> NETH mapped asset(M2)
  -> Treasury policy / protocol clearing price
  -> NOV miner settlement or controlled NOV emission
  -> optional M2 credit through vault/collateral rules
  -> account_balance/account_assets
```

状态：目标流程，当前未闭环；不得声明 ETH lock 直接铸 NOV。

## 5. 并发量判断

当前可负载的层次应这样说：

- EVM mainnet long-sync：已通过 6h/24h soak，说明同步链路可长期运行和重启恢复，不代表钱包请求 TPS。
- EVM gateway 产品入口：默认每轮最多消费 16 笔 pending，cooldown 250ms，理论调度上限约 64 tx/s/进程，实际受 EVM 执行、IO、receipt/canonical store 影响。
- native NOV execution：当前 JSON store 单 writer 可用，不应做多进程并发写入。
- AOEM/consensus：可继续引用已有 seal，但必须注明不是 gateway/wallet 产品入口吞吐。

结论：

- 当前适合产品 v1 小规模真实用户/内部公网节点路径。
- 若要公测高并发，需要先把 gateway/native store 写入口统一成单 writer 队列或 RocksDB/事务后端，再压测。

## 6. 最小下一步

不要继续扩散 EVM fixture、BAL、debug/admin/trace。下一步只做 3 个最小闭环：

1. 钱包接入 smoke：用真实 JSON-RPC 顺序跑 `ua_createUca -> ua_bindPersona(Evm) -> eth_sendRawTransaction -> eth_getTransactionReceipt -> account_assets`。
2. ETH mapped asset smoke：跑 `ua_registerMappedLock -> account_assets -> ua_burnMappedAsset -> ua_releaseMappedLock`，并在输出里明确 `internal MVP / non-settlement`。
3. NOV native treasury smoke：跑 `treasury.deposit_reserve -> treasury.redeem -> account_balance/account_assets`，验证 native 余额和 journal。

如果这三条 smoke 都稳定，才能进入“真实钱包入口接入”。如果目标是“ETH 锁仓合约 -> NOV 铸造”，应单独开一个小阶段，不要混进 EVM v1。
