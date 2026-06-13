# NOVOVM 货币架构决议（M0/M1/M2 与多币支付）  
_2026-04-17_

## 1. 目的与范围

本文件用于冻结以下口径，避免后续实现再次分叉：

1. `NOV` 与 `EVM/ETH` 的协议定位差异  
2. `Gas` 与 NOVOVM 费用模型的术语与实现边界  
3. `M0/M1/M2` 的货币分层与“外币支付、NOV 结算”的主线模型

本文件是实现约束，不是概念草稿。

## 2. 输入依据（已比对）

- 宏观稿件（外部）：  
  - `C:\Users\leadb\Desktop\苹果电脑的代币经济学和商业叙述\代币宏观经济学\Final\现代代币宏观经济学(2025-12第一版出版稿).pdf`
- 仓库内抽取文本：  
  - [macro-econ-fulltext-2026-04-17.txt](/d:/WEB3_AI/SUPERVM/artifacts/audit/macro-econ-fulltext-2026-04-17.txt)  
  - [macro-econ-key-extract-2026-04-17.txt](/d:/WEB3_AI/SUPERVM/artifacts/audit/macro-econ-key-extract-2026-04-17.txt)
- 当前实现位点：  
  - [token_runtime.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/token_runtime.rs:65)（`NOV` 主币符号）  
  - [protocol.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-consensus/src/protocol.rs:111)（HotStuff/BFT 主线）  
  - [tx_wire.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-protocol/src/tx_wire.rs:9)（当前原生 tx wire 仍是 transfer 核心字段）  
  - [tx_ingress.rs](/d:/WEB3_AI/SUPERVM/crates/novovm-node/src/tx_ingress.rs:69)（当前 ingress 映射仍以 transfer 语义为主）

## 3. 问题 1：NOV 协议对比 EVM/ETH 的先进性

### 3.1 冻结结论

- NOVOVM 的先进性不在“TPS 口号”，而在“执行优先 + 可验证结算 + 宿主化多链能力”。
- 对外可表述为：`proof-driven execution network`（以可验证执行为中心的网络）。
- **但当前实现仍有 canonical chain / HotStuff / block lifecycle。**  
  结论：现在是“执行证明导向的链上系统”，不是“彻底无块结构”。

### 3.2 对外口径（固定）

- 可说：`我们不是以区块打包为中心，而是以执行与可验证结果为中心。`
- 不可说：`我们已经完全不是区块链模式。`

## 4. 问题 2：GAS 与费用模型

### 4.1 冻结结论

- 对开发者兼容层可继续保留 `gas_*` 字段（兼容 EVM 工具链）。
- 对 NOV 原生口径统一使用：`Execution Fee`（执行费），不再把主叙事写成 Gas。

### 4.2 费用模型约束

- 原生费用应按资源分项计量：`compute + storage + bandwidth + proof + routing`。
- 结算货币唯一：`NOV`（内部结算不分叉）。

## 5. 问题 3：M0/M1/M2 与多币支付

### 5.1 分层冻结（本次关键）

- `M0`：基础货币层，只是 `NOV`。  
- `M1`：流通货币层，只统计 NOV 体系内可流通货币，不放镜像外币条目。  
- `M2`：信用扩张层，包含外部锁仓、储备或信用生成的 `nAsset` / `n*` 系列资产，例如 `NETH`、`NUSDT`、`NUSD`。

### 5.2 明确禁止

- 禁止把 `pETH / pUSDT / pSOL` 这类镜像资产放在 M1。
- 禁止绕过 NOV 结算直接把外币当内部结算币。
- 禁止把 `NETH/NUSDT/nAsset` 写成 NOV 或 M1；它们是 M2 存款/信用/映射资产。
- 禁止把外部锁仓事件直接解释为 NOV 铸造事件；NOV 铸造必须经过 Treasury policy / emission policy。

### 5.3 M2 生成主线（冻结）

1. 外链资产（ETH/USDT/DAI 等）先在 EVM 插件侧锁仓。  
2. 锁仓结果进入 NOVOVM 国库储备记账。  
3. 通过清算/兑换规则转换为 NOV 抵押基础。  
4. 仅在满足抵押与风险参数时，铸造 `M2` 信用货币（`n*`）。  
5. `M2` 资产可流通（含 RWA 类资产），但其风险归属在 M2，不回写为 M1。

## 6. 多币支付模型（实现口径）

### 6.1 核心规则

- 用户可用外币支付（ETH/USDT/DAI...）。
- 系统自动兑换/清算后，内部仍以 NOV 结算。

### 6.2 标准流程

`外币支付 -> 清算池/AMM -> NOV 结算 -> 执行记账 -> 国库储备/分账`

### 6.3 边界约束

- 必须有报价有效期、滑点保护、流动性不足回退。
- 费用扣收不可直接绕开国库结算链路。

## 7. 上层经济法条（2026-06-13 补充冻结）

本节是 NOVOVM 货币制度的上层边界，优先级高于产品叙事。后续实现、钱包、DAPP、网站、EVM adapter 均不得绕开。

### 7.1 货币层级法条

1. `NOV` 是 NOVOVM 唯一基础货币、最终结算货币、矿工/算力结算货币，归属 `M0/M1`。
2. `NETH`、`NUSDT`、`nAsset` 是外部锁仓、储备或信用生成的 M2 资产，不进入 `M0/M1`。
3. M2 资产可以支付、抵押、兑换或进入信用扩张；`NOV <-> NETH/nAsset` 是内部自由兑换/购买，其中 `NETH/nAsset -> NOV` 走 `nov_swap`/AMM 市场兑换，`NOV -> NETH/nAsset` 走 `nov_buyAsset`/Treasury 协议清算购买；`NETH -> ETH` 才是外部锁仓合约赎回/释放，其风险与负债归属在 M2，不回写为 NOV 基础货币。
4. `NETH` 是锁仓 ETH 的 1:1 储备/存款凭证，不是 NOV，也不是自动铸 NOV 的中间态。
5. NOV 新增发行或矿工结算额度必须受 `reserve bucket`、`fee bucket`、`risk buffer`、`emission policy` 约束。

### 7.1A nAsset 原生账本法条

1. `NETH / NUSDT / NDAI / NBTC / NUSDC` 等资产在 NOVOVM 内部是 `native M2 asset ledger` 条目，不是 ERC20/TRC20 合约实例。
2. nAsset 余额保存在主线统一账户的 native asset balance 视图中，权威状态源为 `novovm-node -> mainline_query -> unified_account_surface / native_execution_store`，不得由 EVM gateway 或单独合约维护第二份余额。
3. nAsset 账本最小结构是 `account_id -> asset_balances[asset_symbol]`、`Treasury reserves[asset_symbol]`、`mapped_asset_records` 和 `treasury_settlement_journal` 的组合，而不是 `contract.balanceOf(address)`。
4. 外部资产进入 NOVOVM 的会计动作是：外部 lock/proof 通过后生成 `mapped_asset_record`，增加用户 native M2 balance，同时增加 Treasury reserve/liability；该动作不 mint NOV。
5. 外部资产退出 NOVOVM 的会计动作是：用户 burn 对应 nAsset，扣减用户 native M2 balance 和 Treasury reserve，再由桥/锁仓合约 release 外部资产；该动作不是 `NOV redeem`。
6. ERC20/TRC20 只属于外部链或 EVM 兼容层的资产表现形式；进入 NOVOVM 主线后必须映射为原生 nAsset 账本资产，接受 Treasury、M2 风险门禁和统一账户策略约束。

### 7.1B 隐私与选择性披露法条

1. 用户的 nAsset 余额、交易流水、KYC 身份和外部地址绑定关系默认不作为公开全网索引数据暴露。
2. 对外公开的应是系统级汇总：Treasury reserve、M2 liability、reserve proof、协议清算价、风险状态、熔断状态和治理参数。
3. KYC 不得以明文实名资料写入公开账本；主线只应保存许可 attestations、policy result、hash/reference 或审计凭证。
4. 钱包、DAPP、网站和 gateway 不得绕过 mainline unified account surface 直接读取或公开用户完整资产明细。
5. AOEM 自带的 RingCT / ZK 能力定位为 NOVOVM 的隐私执行与隐私证明层，可用于后续 encrypted balance、commitment、range proof、membership proof、选择性披露和审计证明。
6. RingCT / ZK 不得被实现成第二套账户或资产账本；隐私 proof 只能证明 mainline native M2 ledger 的状态转移合法性，最终真相源仍是 `mainline unified_account_surface / native_execution_store`。
7. 监管、审计、争议处理和恢复流程应通过治理授权的 selective disclosure 机制完成；公开链上不得泄露用户实名 KYC、完整余额或完整交易流水。
8. 当前已落地的是 native M2 read gate：`privacy_redacted=true` 默认隐藏用户级 N* 资产明细；AOEM RingCT / ZK 属于下一层密码学隐私能力，本文档不把 read gate 夸大为完整 encrypted balance 隐私账本已完成。

### 7.2 多资产支付与 NOV 结算法条

1. 用户可用白名单 M2 资产支付 Execution Fee。
2. 系统按协议清算价把支付资产折算为 `NOV value`。
3. 支付资产进入 `Treasury Reserve Pool`，中文统一称为“国库储备池 / 外汇储备池”。
4. 矿工/算力提供者只以 NOV 结算，避免收入碎片化。
5. 费用扣收、矿工结算、国库分账不得绕开 Treasury settlement。
6. 当前制度写入不声明真实 ETH lock、NOV mint、M2 credit 全自动闭环已完成。

### 7.3 Treasury / AMM / Oracle 职责

1. `Treasury` 命名保留，中文统一写作“国库”。
2. Treasury 负责协议清算、储备、风险缓冲、分桶和 NOV 结算。
3. AMM 负责市场价格发现、用户交易、套利收敛，不直接决定 Execution Fee 清算价。
4. AMM spot price 禁止进入协议清算。
5. 外部 oracle 只能作为治理许可参考源，用于偏离检测、熔断和兜底。
6. 外部 oracle 不能开放给任意第三方喂价，不能单独决定协议清算价。
7. 许可 oracle source 由治理/Treasury policy 管理；`fee_oracle_allowed_sources` 是当前执行约束，非白名单 `fee_oracle_source` 不进入 `P_ref`；`fee_oracle_disabled_sources`、禁用原因和 source rotation 记录用于治理停用异常源。

## 8. 当前代码差距（P0 可执行）

1. 原生 `tx_wire` 仍偏 transfer，需升级为原生执行/治理可表达结构。  
2. 原生 `nov_*` 入口虽已存在基础能力，但 NOV 原生执行与费用术语仍需进一步“主链优先化”。  
3. 多币支付路由、清算、国库 settlement 已有 native execution store v1 主线；`phase4_mode=live` mapped lock 已要求结构化 Ethereum lock event evidence、receipt MPT proof 和本地 `novovm-network` runtime canonical finalized block anchor，并能把通过校验的 ETH lock MVP 映射为 `NETH` M2 credit、写入 native account balance / Treasury reserve / settlement journal，并在 burn/release 时扣减 NETH credit 和 reserve。
4. 协议清算价 v1 已落代码：`P_epoch/P_pay/P_redeem` 按 epoch 固定，输入为显式 AMM TWAP、Treasury NAV、许可 oracle reference 和上一 epoch 价格；AMM spot 不参与清算；`fee_oracle_allowed_sources` 已由 governance/Treasury policy 持久化并在清算价构建时执行，非白名单 oracle source 会进入 rejected source，不参与清算；被 `fee_oracle_disabled_sources` 禁用的 source 即使仍在 allowlist 中也不能参与清算，并会暴露 disabled reason / rotation target。主线写入口 `nov_applyTreasuryPolicy` 已可治理写入 oracle allowlist/disabled reason/rotation、`mapped_lock_min_confirmations` 和 auto-heal policy；主线只读查询 `nov_getProtocolClearingPrice` / `nov_getFeeOracleRates` 已覆盖该状态，主线产品 smoke 已验证 disabled oracle 被拒绝、`epoch_fixed=true`、`amm_spot_allowed=false`、`P_pay < P_epoch < P_redeem`，`nov_buyAsset` 使用 `protocol_clearing_redeem:*` 的 `P_redeem` 从 Treasury reserve 出 nAsset，`nov_swap` 覆盖 `NETH/nAsset -> NOV` 市场兑换入口，`nov_redeem` 仅作为 legacy alias，且非 NOV legacy direct amount redeem 被拒绝，不能绕过 NOV 扣款和 `P_redeem`。
5. M2 bridge 风险门禁 v1 已落代码：native execution store / governance policy 可持久化 `mapped_lock_bridge_paused`、`mapped_asset_burn_paused`、`mapped_asset_release_paused`；env 可紧急暂停 live register、burn、release，暂停时 fail-closed 且不推进 mapped asset 生命周期。
6. M2 source anchor reorg gate v1 已落代码：live register 会持久化 `source_chain_id/block_number/block_hash/receipts_root` 等 anchor；`ua_getMappedAsset` 会暴露 `source_anchor_status`；burn/release 前会复查本地 runtime canonical finalized block，source anchor unsafe 时拒绝推进生命周期。ETH lock contract address 已可通过主线 `nov_applyTreasuryPolicy(mapped_lock_contract_address)` 治理写入 native store，live proof 优先采用治理配置，参数级 `expected_lock_contract_address` 只作为兼容 fallback，不能覆盖治理配置。治理化 header source whitelist/quorum gate v1 已接入 native execution store：主线治理包装 `nov_setMappedHeaderSourcePolicy` 可要求 live lock proof 的 runtime header 必须来自治理许可 `source_peer_id`，并配置 `min_source_quorum`；runtime 会按同一 `block_hash` 已观测到的许可 source peer 集合计算 quorum，不满足时 fail-closed。该 policy 已支持 `disabled_peer_ids`、`disabled_peer_reasons/slashing_reasons` 和 `peer_rotations`，治理可带原因禁用异常 source peer，并记录 old peer -> new peer 轮换关系；被禁用 peer 即使在 allowed 列表里也不能作为证明来源或计入 quorum。治理化 Ed25519 header attestation quorum v1 已接入 native execution store：主线治理包装 `nov_setMappedHeaderAttestationPolicy` 可要求 live lock proof 携带治理许可 `header_attestations`，每个 attestation 用许可 public key 对 `chain_id/block_number/block_hash/receipts_root` 签名，并配置 `min_attestation_quorum`；签名无效或 quorum 不足时 fail-closed。该 policy 已支持 `disabled_signers`、`disabled_signer_reasons` 和 `signer_rotations`，治理可带原因禁用旧 attestation key，并记录 old signer -> new signer 轮换关系；被禁用 key 即使签名有效也不计入 quorum。`nov_getMappedFinalitySourceStatus` 只读聚合 lock contract/source/attestation/min confirmations/auto-heal 状态；三个 mainline wrapper 都委托统一账户 surface / native store，不新增第二状态源。
7. M2 manual freeze/recovery/rollback v1 已落代码：`ua_freezeMappedAsset` 可把 active/burn_pending mapped asset 标记为 `frozen`；对 active live NETH 会扣减用户 native 可用余额，但保留 Treasury reserve，不触发链上出金。`ua_unfreezeMappedAsset` 只允许在 source anchor 重新通过 canonical finalized 校验后，把 frozen NETH 恢复为 active 并返还用户 native 可用余额。`ua_rollbackFrozenMappedAsset` 只允许 frozen 且 source anchor 仍 unsafe 时执行，把内部 NETH/M2 reserve 暴露扣回并把 mapped asset 置为 `rejected`，不返还用户余额、不 mint NOV、不触发外部链释放。
8. M2 auto heal v1 已落代码：`ua_autoHealMappedAssets` 默认 dry-run，只报告 unsafe source anchor 和每个候选的 `required_policy/policy_would_apply`；governance/Treasury policy 可通过主线 `nov_applyTreasuryPolicy` 的 `mapped_asset_reorg_response_policy` 统一配置为 `report_only / freeze_only / freeze_and_rollback`。主线显式调度入口 `nov_runMappedAssetAutoHeal` 已接入，同一入口默认 dry-run，`apply=true` 必须同时满足 `scheduler_authorized=true` 和对应 Treasury policy，否则 fail-closed 或只报告。`freeze_only` 可自动冻结 active/burn_pending live NETH，扣减用户 native 可用余额并保留 Treasury reserve；`freeze_and_rollback` 额外允许对已 frozen 且 source anchor 仍 unsafe 的资产执行内部 rollback：扣回 Treasury NETH reserve、把 mapped asset 置为 `rejected`。主线产品 smoke 已覆盖 live NETH 入账后 source anchor reorg -> `nov_runMappedAssetAutoHeal` dry-run -> 未授权 apply fail-closed -> `nov_applyTreasuryPolicy(freeze_only)` -> scheduler-authorized freeze apply -> `nov_applyTreasuryPolicy(freeze_and_rollback)` -> scheduler-authorized rollback apply；它不是后台 daemon，不赔付、不链上出金、不 mint NOV。
9. 主线产品 smoke 已覆盖 shadow/internal MVP mapped asset 生命周期：`ua_registerMappedLock -> account_assets -> ua_burnMappedAsset -> ua_releaseMappedLock -> ua_getMappedAsset`，固定 `phase4_mode=shadow`、`settlement_effect=none`；也已覆盖 live `Ethereum lock evidence -> NETH/M2 credit -> burn -> Treasury reserve debit/release` 主线入口闭环，固定 `nov_minted=0`，不声明真实外部桥、自动外部释放或链上出金。
10. M2 finality policy v1 已落代码：`mapped_lock_min_confirmations` 可由 governance/Treasury policy 设置；live ETH lock proof 会优先使用 native store policy，未设置时才 fallback 到 env/default。主线产品 smoke 已覆盖 governed min confirmations 未达标 fail-closed 与降低确认数后通过。它只管理最小 finalized confirmations，不等于完整 finality source 管理。
11. Treasury reserve proof v1 已具备最小治理登记/查询与执行门禁面：`nov_setTreasuryReserveProof` / `governance.set_reserve_proof` 可按资产登记 proof type/digest/source/reference/amount/status；`treasury.get_reserve_proof` 和 `treasury.get_reserve_snapshot` 可只读暴露 effective status 与 non-claim 标记；主线查询面 `nov_getTreasuryReserveProof` / `nov_getTreasuryReserveSnapshot` 已可观察该状态。若某资产已登记 proof 且 effective status 非 `active`，该资产的 `nov_depositReserve`、非 NOV fee clearing 和 treasury redeem 会 fail-closed；若 active proof 的 `reserve_amount` 低于 projected Treasury reserve/exposure，`nov_depositReserve` 和非 NOV fee clearing 会拒绝扩张，treasury redeem 也必须在操作后不继续高于 proof cap。主线产品 smoke 已覆盖治理未启用时 fail-closed、治理启用后写入 revoked proof、查询 non-claim 标记，以及 revoked proof 阻断 `nov_depositReserve`。该路径仍不做自动外部验真、不授权 NOV mint、不授权外部赎回。
12. nAsset 读隐私 v1 已落代码：`account_balance`、`account_assets` 和 `nov_getAssetBalance` 对 `N*` M2 资产默认返回 `privacy_redacted=true`，只有 `viewer_account_id/requester_account_id` 匹配账户或显式 `asset_view_authorized/account_view_authorized` 时才返回余额和资产列表明细。该门禁保护 native M2 余额和 mapped asset 明细，不影响 Treasury reserve、M2 liability、reserve proof、协议清算价和风险状态等系统级公开查询。
13. nAsset 支付隐私与扣账 v1 已落代码：`nov_execute` / `nov_sendTransaction` / `nov_sendRawTransaction` 用户执行入口中，只要 `fee_policy.pay_asset` 是 `N*` M2 资产且不是 `NOV`，执行策略会自动提升为 `privacy_required`；public 路径 fail-closed，`private/confidential` 隐私路径才可继续。`MLDSA` 只归属 `pq_required` 抗量子签名策略，不等同隐私交易。M2 fee clearing 在进入 Treasury settlement 前必须先扣减 fee owner 的 native M2 balance，余额不足返回 `fee.clearing.insufficient_user_balance`，不能凭空增加 Treasury reserve。该门禁把 AOEM RingCT / ZK 隐私能力接入产品入口，但仍不声明完整 encrypted balance 隐私账本已完成。
14. Native execution store 已具备最小 lockfile single-writer guard：主写路径先获取写锁，再执行 load/modify/save，适合单机/低并发产品闭环；该 guard 不是 RocksDB/事务后端，也不代表高并发账本入口完成。
15. 当前仍不声明真实外部桥、完整 external finality source 管理、治理赔付、真实链上出金、完整自动 reserve proof verification、完整桥接铸造/赎回自动化、完整 encrypted balance/ZK proof 或多进程高并发账本入口完成；当前 finality source 管理只覆盖 source peer whitelist/quorum、disabled peer slashing reason/fail-closed、source peer rotation 记录、Ed25519 attestation quorum、disabled signer reason/fail-closed、signer rotation 记录、最小 confirmations 和 `nov_getMappedFinalitySourceStatus` 只读状态聚合。主线产品 smoke 已覆盖未授权 finality policy 写入 fail-closed、source quorum 不足 fail-closed 与第二个许可 source peer 观测后通过、attestation quorum 不足/disabled signer fail-closed 与两个 active signer 通过、governed min confirmations 未达标 fail-closed 与降低确认数后通过。治理层 oracle source allowlist / disabled reason / rotation mainline write、reserve proof mainline write、mainline protocol clearing price query、`P_redeem` reserve redeem、reserve proof effective-status gate、nAsset 最小 read gate 和 N* fee asset privacy-required gate 已具备最小执行约束，但不是开放喂价系统，也不等于完整 oracle 网络、自动 reserve verification 或完整密码学隐私账本。

## 9. 术语冻结

- 品牌：`NOVOVM`  
- 技术简称：`NVM`  
- 基础货币：`NOV`  
- 原生收费术语：`Execution Fee`  
- EVM 层 `gas` 为兼容字段，不代表 NOV 原生经济术语
- 国库：`Treasury`
- 国库储备池 / 外汇储备池：`Treasury Reserve Pool`
- 协议清算价：`Protocol Clearing Price`
- AMM 市场价：`AMM Market Price`
- 许可参考源：`Governance-permitted Oracle Reference`

## 10. 执行优先级（仅列下一刀）

1. 把当前治理化 header source whitelist/quorum/disabled peer gate 和 Ed25519 header attestation quorum gate 继续升级到完整 external finality source 管理，包括 reorg response policy、赔付规则和自动处置；`receipts_root` 已不再只信任用户输入，quorum 已按同一 `block_hash` 的多 source 观测计数，source quorum fail-closed/二源通过和 attestation quorum fail-closed/双 active signer 通过已由主线产品 smoke 覆盖，source peer slashing reason、source peer rotation、最小 finalized confirmations、attestation signature quorum、disabled signer、disabled reason 和 signer rotation 已可由 mainline governance wrapper 设置。
2. 把 `ua_autoHealMappedAssets` 接到主线调度：当前已具备 `nov_runMappedAssetAutoHeal` 显式 scheduler tick，dry-run、未授权 apply fail-closed、通过 `nov_applyTreasuryPolicy` 启用的 scheduler-authorized freeze apply 和 frozen unsafe rollback apply 已覆盖；仍需要常驻后台 scheduler、治理赔付规则和完整外部 finality source 管理。
3. 把当前 native store lockfile single-writer guard 升级为真正单 writer 队列或 RocksDB/事务后端，支撑公测级并发写入。
4. 把当前最小 reserve proof 登记/查询面升级为真实自动 reserve proof verification，并把当前最小治理 oracle source allowlist / disabled / rotation 扩展为更完整的签名验证、来源轮换、停用和审计流程。

---

本文件用于“先对齐货币制度，再落代码”，后续代码变更需遵守本文件口径，除非有新版决议文档替代。

