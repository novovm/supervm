# NOVOVM 经济基础设施迁移完成度清单（对照 SVM2026）- 2026-03-07

## 1. 审计目的

回答两个问题：

1. `SUPERVM` 当前是否已达到 `SVM2026/docs/经济系统/核心金融基础设施(全部可用).md` 的“全部可用”口径。
2. 已从 `SVM2026` 迁入到 `NOVOVM` 主链路的经济功能有哪些，哪些仍在 `vendor/reference` 阶段。

## 2. 审计口径（强约束）

- 以 `代码主链路 + 门禁证据` 为准。
- 文档宣称若与代码冲突，以代码/门禁为准。
- `vendor/web30-core` 中仅库实现但未接入 `novovm-consensus/novovm-node` 主路径的能力，不计为“主链路完成”。

## 3. 总体结论（当前）

- 结论：`已达到“受限主链路全部可用”`（9/9 门禁项可验收通过）。
- 当前状态：`9 Done + 0 InProgress + 0 NotStarted`（按本清单 9 大能力）。
- 可发布口径：`MVP+（共识 + 交易 + 读查询 + 受限治理 + 经济治理主链路）`。
- 说明：该结论是“主链路可验收”口径，不等于“完整主网经济开放业务面”。

## 4. 逐项迁移完成度（9 大能力）

| 能力 | SVM2026 文档宣称 | SUPERVM 主链路证据 | 门禁证据 | 状态 | 关键缺口 |
| --- | --- | --- | --- | --- | --- |
| Token 系统 | 已完整可用 | `token_runtime` 已接入 `mint/burn/gas fee/service fee/treasury spend`（`protocol.rs` 调用） | `run_governance_token_economics_gate.ps1` + acceptance 汇总 | Done（受限主链路） | 仍以治理驱动和主链路口径为主，非独立 0x1000 原生地址面 |
| AMM | 已完整可用 | `market_engine` 通过 `AMMManager` 接入并受 `MarketGovernancePolicy` 下发 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done（受限主链路） | 目前是治理编排与快照口径，非完整对外业务接口面 |
| NAV 赎回 | 已完整可用 | `market_engine` 接入 `NavRedemptionManager`，NAV 估值源支持 `deterministic/external(feed)` 可切换并具备缺失报价 fallback，输出 nav snapshot/redemption + source 指标；`novovm-node` 已支持 HTTP feed 多源聚合（中位数）+ strict/fallback + 签名校验 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` + `run_market_engine_treasury_negative_gate.ps1` + `run_nav_valuation_source_gate.ps1` | Done（受限主链路） | 已完成多源+签名门禁，后续可扩展权重聚合与链上预言机桥 |
| CDP | 已完整可用 | `market_engine` 接入 `CdpManager`，具备价格更新/清算编排 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done（受限主链路） | 当前以编排与参数治理为主，业务域接口未独立收口 |
| 债券系统 | 已完整可用 | `market_engine` 接入 `BondManager` 与治理参数热更新 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done（受限主链路） | 仍未形成独立主链业务入口与全量门禁包 |
| 国库管理 | 已完整可用 | `TreasurySpend` 已接入治理执行路径；`market_engine` 有 treasury 快照输出；`TreasuryImpl` 已按 policy 执行 reserve/burn/trigger 约束 | `run_governance_treasury_spend_gate.ps1` + `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` + `run_market_engine_treasury_negative_gate.ps1` | Done（受限主链路） | 回购执行仍为确定性执行语义，尚未接外部 AMM 真实成交 |
| 治理系统 | 已完整可用 | I-GOV-01~04 主链路已接线（受限执行面） | `governance_*_gate` 系列 + acceptance | Done（受限主链路） | 仍为受限执行面，非完整主网全开放治理面 |
| 分红池 | 已完整可用 | `market_engine` 已接入 `DividendPoolImpl`（`receive_income/take_daily_snapshot/claim`），并通过 `token_runtime.dividend_eligible_balances` 注入真实运行态账户余额（同时保留 deterministic probe fallback） | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1`（`dividend_pool_pass=true`） + `run_dividend_balance_source_gate.ps1` | Done（受限主链路） | 已完成运行态余额注入，后续可继续升级到跨模块统一账户索引服务 |
| 跨链外币支付 | 已完整可用 | `market_engine` 已接入 `ForeignPaymentProcessorImpl`（`process_foreign_payment/miner_swap_to_foreign`）并输出 reserve/token 信号；`novovm-node` 已支持外部 HTTP 汇率源多源聚合（多数聚合）+ strict/fallback + 签名校验，主链路汇率源采用 `ConfigurableExchangeRateProvider` | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1`（`foreign_payment_pass=true`） + `run_foreign_rate_source_gate.ps1` | Done（受限主链路） | 已完成多源+签名门禁，后续可扩展链上结算桥 |

## 5. 关键证据

### 5.0 同源迁移（SVM2026 -> SUPERVM）证据

- 同源同步脚本：`scripts/migration/sync_web30_core_from_svm2026.ps1`
- 同源门禁脚本：`scripts/migration/run_web30_core_parity_gate.ps1`
- 门禁产物：`artifacts/migration/web30-core-parity-gate/web30-core-parity-gate-summary.json`
  - 结果：`pass=true`
  - 哈希对齐：`exact_match_count=19`
  - 允许漂移：`mismatch_allowed_count=1`（`dividend_pool.rs`，保留本地重入防护修复）

### 5.1 已迁入主链路

- `crates/novovm-consensus/src/protocol.rs`
  - `set_token_economics_policy`
  - `set_market_governance_policy`
  - `spend_treasury_tokens`
  - `execute_governance_proposal_with_executor_approvals` 中对 `UpdateTokenEconomicsPolicy/UpdateMarketGovernancePolicy/TreasurySpend` 的执行分支
- `crates/novovm-consensus/src/token_runtime.rs`
  - `mint/burn/charge_gas_fee/charge_service_fee/spend_treasury`
- `crates/novovm-consensus/src/market_engine.rs`
  - `AMMManager/CdpManager/BondManager/NavRedemptionManager` 接线
  - `run_cross_module_orchestration` 输出 `oracle/cdp/nav + dividend + foreign_payment` 编排信号
- `scripts/migration/run_governance_token_economics_gate.ps1`
- `scripts/migration/run_governance_treasury_spend_gate.ps1`
- `scripts/migration/run_governance_market_policy_gate.ps1`
- `scripts/migration/run_economic_infra_dedicated_gate.ps1`
- `scripts/migration/run_nav_valuation_source_gate.ps1`
- `scripts/migration/run_dividend_balance_source_gate.ps1`
- `scripts/migration/run_migration_acceptance_gate.ps1`

### 5.1.1 经济基础设施专项门禁（新增）

- 专项门禁脚本：`scripts/migration/run_economic_infra_dedicated_gate.ps1`
- 专项门禁产物：`artifacts/migration/economic-infra-dedicated-gate-2026-03-07/economic-infra-dedicated-gate-summary.json`
  - 结果：`pass=true`
  - 子项：`token_system/amm/nav_redemption/cdp/bond/treasury/governance_system/dividend_pool/foreign_payment` 全部 `true`
- acceptance 产物：`artifacts/migration/acceptance-economic-infra-dedicated-smoke-2026-03-07/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`economic_infra_dedicated_pass=true`

### 5.1.2 国库负向门禁（新增）

- 脚本：`scripts/migration/run_market_engine_treasury_negative_gate.ps1`
- 产物：`artifacts/migration/market-engine-treasury-negative-gate-2026-03-07/market-engine-treasury-negative-gate-summary.json`
  - 结果：`pass=true`
  - 负向覆盖：`buyback_zero_budget_reject`、`buyback_not_triggered_below_threshold`、`buyback_reserve_and_burn_share`、`market_engine_reject_zero_buyback_budget`
- acceptance 产物：`artifacts/migration/acceptance-market-engine-treasury-negative-smoke-2026-03-07/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`market_engine_treasury_negative_pass=true`

### 5.1.3 外币汇率源专项门禁（新增）

- 脚本：`scripts/migration/run_foreign_rate_source_gate.ps1`
- 产物：`artifacts/migration/foreign-rate-source-gate-2026-03-07/foreign-rate-source-gate-summary.json`
  - 结果：`pass=true`
  - 覆盖：`foreign_rate_spec_ok`、`foreign_rate_invalid_rate_reject`、`foreign_rate_invalid_slippage_reject`、`foreign_rate_processing_configurable_provider`、`market_engine_foreign_path_regression`、`foreign_source_external_feed_probe_ok`、`foreign_source_external_feed_fallback_ok`、`foreign_source_external_feed_signature_strict_reject_ok`
- 远端 feed 烟雾证据：`artifacts/migration/foreign-rate-source-gate-remote-smoke-2026-03-07/foreign-rate-source-gate-summary.json`
  - 结果：`pass=true`
- 多源 + 签名门禁证据：`artifacts/migration/foreign-rate-source-gate-multisig-smoke-2026-03-07/foreign-rate-source-gate-summary.json`
  - 结果：`pass=true`
  - 覆盖：`foreign_source_external_feed_probe_ok`、`foreign_source_external_feed_fallback_ok`、`foreign_source_external_feed_signature_strict_reject_ok`
- acceptance 产物：`artifacts/migration/acceptance-economic-treasury-foreignrate-smoke-2026-03-07/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`foreign_rate_source_pass=true`

### 5.1.4 分红余额源专项门禁（新增）

- 脚本：`scripts/migration/run_dividend_balance_source_gate.ps1`
- 产物：`artifacts/migration/dividend-balance-source-gate-2026-03-07/dividend-balance-source-gate-summary.json`
  - 结果：`pass=true`
  - 覆盖：`dividend_pool_injected_balances_claim_ok`、`dividend_pool_reentrancy_guard_ok`、`market_engine_runtime_dividend_seed_ok`、`protocol_market_dividend_sync_ok`、`market_engine_regression_ok`
- acceptance 产物：`artifacts/migration/acceptance-economic-dividend-source-smoke-2026-03-07/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`dividend_balance_source_pass=true`

### 5.1.5 NAV 估值源专项门禁（新增）

- 脚本：`scripts/migration/run_nav_valuation_source_gate.ps1`
- 产物：`artifacts/migration/nav-valuation-source-gate-2026-03-07/nav-valuation-source-gate-summary.json`
  - 结果：`pass=true`
  - 覆盖：`nav_valuation_external_with_price_ok`、`nav_valuation_missing_quote_fallback_ok`、`nav_valuation_invalid_price_reject_ok`、`market_engine_nav_regression_ok`、`nav_source_external_feed_probe_ok`、`nav_source_external_feed_fallback_ok`、`nav_source_external_feed_signature_strict_reject_ok`
- 远端 feed 烟雾证据：`artifacts/migration/nav-valuation-source-gate-remote-smoke-2026-03-07/nav-valuation-source-gate-summary.json`
  - 结果：`pass=true`
- 多源 + 签名门禁证据：`artifacts/migration/nav-valuation-source-gate-multisig-smoke-2026-03-07/nav-valuation-source-gate-summary.json`
  - 结果：`pass=true`
  - 覆盖：`nav_source_external_feed_probe_ok`、`nav_source_external_feed_fallback_ok`、`nav_source_external_feed_signature_strict_reject_ok`
- acceptance 产物：`artifacts/migration/acceptance-economic-navfx-dividend-smoke-2026-03-07/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`nav_valuation_source_pass=true`

### 5.2 尚未主链路完成（占位/未接线）

- `vendor/web30-core/src/privacy.rs`
  - 存在 TODO（环签名验证）。
- `vendor/web30-core/src/dividend_pool.rs`
  - 已通过上层 `token_runtime` 注入接入运行态余额；后续可升级为跨模块统一账户索引服务（替代注入式快照）。

## 6. 与“核心金融基础设施(全部可用)”文档的对比结论

- 该文档在 SVM2026 中宣称“100% 完成”。
- 但同目录 `TOKEN-COMPLETION-REPORT.md` 记载“实现进行中”。
- 在 SUPERVM 迁移视角下，当前应认定为：
  - `经济治理主链路子集已完成并门禁化`。
  - `完整经济业务系统未全量迁完`。

## 7. 下一步（直接可执行）

1. （已完成）`ForeignPayment` 与 NAV feed 已从 HTTP 单源扩展为多源聚合 + 签名校验门禁（见 5.1.3 / 5.1.5）。
2. 将 buyback 从确定性执行语义升级到真实流动性执行（AMM/订单簿），补成交失败/滑点上限负向门禁。
3. 将分红账户余额注入升级为跨模块统一账户索引服务，并补大规模账户快照性能门禁。
4. 在 `full_snapshot_ga_v1` 基础上跑一版完整 acceptance 快照，把 `economic_infra_dedicated_*` + `market_engine_treasury_negative_*` + `foreign_rate_source_*` + `nav_valuation_source_*` + `dividend_balance_source_*` 字段纳入发布证据。
