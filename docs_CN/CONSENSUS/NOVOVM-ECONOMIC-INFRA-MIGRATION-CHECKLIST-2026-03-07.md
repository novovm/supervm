# NOVOVM 经济基础设施能力与门禁清单（当前口径）

## 0. 当前对外口径

- NOVOVM 经济基础设施 9 大能力已经全部成立，并已形成可验证门禁证据。
- 原生经济用户入口 `nov_getAssetBalance / nov_swap / nov_redeem / nov_openVault` 已进入真实 `novovm-node` 产物入口，并通过真实产物级门禁。
- 本文用于说明“当前已经成立的经济能力、门禁证据与当前边界”，不再把开发过程作为对外主叙述。
- 当前系统总览见：
  - `docs_CN/NOVOVM-NETWORK/NOVOVM-CURRENT-SYSTEM-ARCHITECTURE-2026-04-19.md`
  - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-ECONOMIC-USER-SURFACE-SEAL-2026-04-18.md`

## 1. 文档目的

回答两个问题：

1. `SUPERVM` 当前已经成立的经济能力有哪些。
2. 这些能力当前通过什么门禁与主线路径对外成立。

## 2. 审计口径（强约束）

- 以 `代码主链路 + 门禁证据` 为准。
- 文档宣称若与代码冲突，以代码/门禁为准。
- `vendor/web30-core` 中仅库实现但未接入 `novovm-consensus/novovm-node` 主路径的能力，不计为“主链路完成”。

## 3. 总体结论

- 结论：`经济基础设施 9/9 已完成`。
- 当前状态：`9 Done + 0 InProgress + 0 NotStarted`。
- 当前可对外成立的口径：
  - 经济能力本体已成立
  - 真实 `novovm-node` 用户入口已接通
  - 真实产物级门禁已通过
- 当前系统边界：
  - 不宣称完整公共 HTTP 业务面全部开放
  - 不宣称 P3 已启用
  - 不宣称 WEB30 标准族已经全部完成

## 4. 当前能力状态（9 大能力）

| 能力 | 对外能力说明 | SUPERVM 主链路证据 | 门禁证据 | 状态 | 当前边界 |
| --- | --- | --- | --- | --- | --- |
| Token 系统 | 已完成 `mint / burn / gas fee / service fee / treasury spend` 主线 | `token_runtime` 已接入 `mint/burn/gas fee/service fee/treasury spend`（`protocol.rs` 调用） | `run_governance_token_economics_gate.ps1` + acceptance 汇总 | Done | 当前以主线能力与治理参数为主，不单独宣称完整独立原生地址面 |
| AMM | 已完成主线 AMM 与原生 `nov_swap` 用户入口 | `market_engine` 通过 `AMMManager` 接入并受 `MarketGovernancePolicy` 下发 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done | 当前已开放单条原生用户入口，不宣称全部扩展业务面 |
| NAV 赎回 | 已完成估值、赎回、多源 feed 与签名校验主线 | `market_engine` 接入 `NavRedemptionManager`，NAV 估值源支持 `deterministic/external(feed)` 可切换并具备缺失报价 fallback，输出 nav snapshot/redemption + source 指标；`novovm-node` 已支持 HTTP feed 多源聚合（中位数）+ strict/fallback + 签名校验 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` + `run_market_engine_treasury_negative_gate.ps1` + `run_nav_valuation_source_gate.ps1` | Done | 当前已完成主线能力，不宣称链上预言机桥已全部开放 |
| CDP | 已完成主线 CDP 与原生 `nov_openVault` 用户入口 | `market_engine` 接入 `CdpManager`，具备价格更新/清算编排 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done | 当前已开放最小真实用户入口，不宣称全部业务域接口已经展开 |
| 债券系统 | 已完成债券主线与治理参数热更新 | `market_engine` 接入 `BondManager` 与治理参数热更新 | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` | Done | 当前以主线引擎能力为主，不单独宣称完整对外独立业务面 |
| 国库管理 | 已完成国库治理、赎回与成交约束主线 | `TreasurySpend` 已接入治理执行路径；`market_engine` 有 treasury 快照输出；`TreasuryImpl` 已按 policy 执行 reserve/burn/trigger + 流动性/滑点约束成交语义 | `run_governance_treasury_spend_gate.ps1` + `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1` + `run_market_engine_treasury_negative_gate.ps1` | Done | 当前已完成国库主线与 `nov_redeem`，不宣称外部 AMM/订单簿桥已全部开放 |
| 治理系统 | 已完成真实治理主线入口与治理扩展验签路径 | I-GOV-01~04 主链路已接线，真实 `novovm-node` 入口已成立 | `governance_*_gate` 系列 + acceptance | Done | 当前治理边界为受控治理面：`committee / threshold / timelock / allowlist` 继续有效 |
| 分红池 | 已完成分红主线、账户索引同步与领取能力 | `market_engine` 已接入 `DividendPoolImpl`（`receive_income/take_daily_snapshot/claim`），并通过 `account_index` 统一账户索引服务同步 `token_runtime.dividend_eligible_balances`（保留 deterministic probe fallback） | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1`（`dividend_pool_pass=true`） + `run_dividend_balance_source_gate.ps1` | Done | 当前已完成主线与性能门禁，不宣称更高规模阈值已全部展开 |
| 跨链外币支付 | 已完成外币支付、汇率多源聚合与签名校验主线 | `market_engine` 已接入 `ForeignPaymentProcessorImpl`（`process_foreign_payment/miner_swap_to_foreign`）并输出 reserve/token 信号；`novovm-node` 已支持外部 HTTP 汇率源多源聚合（多数聚合）+ strict/fallback + 签名校验，主链路汇率源采用 `ConfigurableExchangeRateProvider` | `run_governance_market_policy_gate.ps1` + `run_economic_infra_dedicated_gate.ps1`（`foreign_payment_pass=true`） + `run_foreign_rate_source_gate.ps1` | Done | 当前已完成主线能力，不宣称链上结算桥已全部开放 |

## 5. 关键证据

### 5.0 同源一致性证据

- 同源同步脚本：`scripts/migration/sync_web30_core_from_svm2026.ps1`
- 同源门禁脚本：`scripts/migration/run_web30_core_parity_gate.ps1`
- 门禁产物：`artifacts/migration/web30-core-parity-gate/web30-core-parity-gate-summary.json`
  - 结果：`pass=true`
  - 哈希对齐：`exact_match_count=19`
  - 允许漂移：`mismatch_allowed_count=1`（`dividend_pool.rs`，保留本地重入防护修复）

### 5.1 当前主链路证据

- `crates/novovm-consensus/src/protocol.rs`
  - `set_token_economics_policy`
  - `set_market_governance_policy`
  - `spend_treasury_tokens`
  - `execute_governance_proposal_with_executor_approvals` 中对 `UpdateTokenEconomicsPolicy/UpdateMarketGovernancePolicy/TreasurySpend` 的执行分支
- `crates/novovm-consensus/src/token_runtime.rs`
  - `mint/burn/charge_gas_fee/charge_service_fee/spend_treasury`
- `crates/novovm-consensus/src/market_engine.rs`
  - `AMMManager/CdpManager/BondManager/NavRedemptionManager` 接线
  - `set_dividend_account_index_snapshot` 统一账户索引快照接线
  - `run_cross_module_orchestration` 输出 `oracle/cdp/nav + dividend + foreign_payment` 编排信号
- `crates/novovm-consensus/src/account_index.rs`
  - `UnifiedAccountIndex` 跨模块统一账户索引服务（分红账户快照）
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
  - 负向覆盖：`buyback_zero_budget_reject`、`buyback_not_triggered_below_threshold`、`buyback_reserve_and_burn_share`、`buyback_liquidity_unavailable_rejected`、`buyback_slippage_cap_rejected`、`market_engine_reject_zero_buyback_budget`
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
  - 覆盖：`market_engine_runtime_dividend_seed`、`protocol_market_policy_syncs_dividend_balances`、`unified_account_index_large_scale_perf`
  - 聚合字段：`runtime_seed_pass=true`、`protocol_sync_pass=true`、`perf_budget_pass=true`
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

### 5.1.6 当前 acceptance 基线

- 脚本：`scripts/migration/run_migration_acceptance_gate.ps1 -FullSnapshotProfileGA`
- 产物：`artifacts/migration/acceptance-full-snapshot-ga-v1-2026-03-18-r2/acceptance-gate-summary.json`
  - 结果：`overall_pass=true`
  - 关键字段：`economic_infra_dedicated_pass=true`、`market_engine_treasury_negative_pass=true`、`foreign_rate_source_pass=true`、`nav_valuation_source_pass=true`、`dividend_balance_source_pass=true`
  - 说明：当前本地 AOEM 动态库符号存在兼容差异，因此按 `IncludePerformanceGate=false` 执行（`performance_gate_enabled=false`）。

### 5.2 主链路收口状态（2026-03-18 复核）

- `vendor/web30-core/src/privacy.rs`
  - 环签名能力已完成 AOEM FFI 主链路接线：`verify_ring_signature_via_aoem` / `generate_ring_signature_via_aoem` / `generate_ring_keypair_via_aoem` 已落地并由 `aoem-ring-ffi` 特性控制。
  - 失败语义为 fail-closed（DLL/能力缺失时拒绝通过），不再属于“未接线 TODO”。
  - 本地复核：`cargo test -p web30-core --manifest-path Cargo.toml` 通过（`84 passed`）。
- `vendor/web30-core/src/dividend_pool.rs`
  - 上层已由 `token_runtime` 直接注入升级为 `account_index` 跨模块统一账户索引服务。
  - 大规模账户快照性能门禁已补齐：`test_unified_account_index_refresh_large_scale_perf_budget`（默认 `20_000` 账户、`8_000ms` 预算，可通过环境变量调节）。

## 6. 当前对外结论

- 当前应直接认定为：
  - `经济基础设施 9 大能力已全部完成并门禁化`
  - `真实 novovm-node 经济用户入口已可用`
  - `真实产物级门禁已通过`
- 当前仍应明确保留的边界为：
  - `P3` 未启用
  - `WEB30` 标准族未整体宣称完成
  - 并非所有扩展业务面都已全部开放

## 7. 当前补充说明

1. （已完成）`ForeignPayment` 与 NAV feed 已从 HTTP 单源扩展为多源聚合 + 签名校验门禁（见 5.1.3 / 5.1.5）。
2. （已完成）buyback 已从确定性语义升级到流动性/滑点约束成交模型；后续可接外部 AMM/订单簿桥。
3. （已完成）在统一账户索引服务基础上补齐大规模账户快照性能门禁。
4. （已完成）完整 acceptance 已覆盖 `economic_infra_dedicated_*` + `market_engine_treasury_negative_*` + `foreign_rate_source_*` + `nav_valuation_source_*` + `dividend_balance_source_*`，并已纳入发布证据（见 5.1.6）。
