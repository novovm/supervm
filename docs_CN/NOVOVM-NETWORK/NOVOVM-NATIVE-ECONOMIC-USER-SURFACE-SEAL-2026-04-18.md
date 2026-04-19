# NOVOVM 原生经济用户入口封盘（2026-04-18）

Status: SEALED（Authoritative）  
Scope: `novovm-node` 真实产物入口上的原生经济用户面（`nov_getAssetBalance` / `nov_swap` / `nov_redeem` / `nov_openVault`）

## 目的

本文件用于封盘一条已经完成的关键链路：

`真实 novovm-node 入口 -> mainline_query -> native module dispatch -> treasury/amm/credit_engine 主线`

本次封盘的意义不是“新增经济能力”，而是把已存在的原生经济能力正式接入真实可运行产物入口，并用真实产物级门禁验证其未破坏主线。

## 最终结论

`NOV` 原生经济能力已接入真实 `novovm-node` 用户入口，并通过真实产物级门禁验证。

当前可成立的项目状态：

- `底层经济能力`：已成立
- `真实 novovm-node 用户入口`：已接通
- `真实产物级门禁`：已通过
- `P3`：Decision Only / Not Enabled
- `当前阶段`：Run Phase + 用户入口已可用

## 已封盘范围（代码事实）

### 1）真实入口识别已成立

`src/bin/novovm-node.rs` 已能识别 `mainline query` 模式下的 `native_execution_method`，不再把 `nov_*` 用户方法提前误判为 unsupported canonical query。

关键接线点：

- `crates/novovm-node/src/bin/novovm-node.rs`
  - `mainline_query_method_from_env`
  - `is_mainline_native_execution_query_method`
  - `run_mainline_query_from_path`

### 2）真实用户入口已成立

已进入真实产物入口的方法：

- `nov_getAssetBalance`
- `nov_swap`
- `nov_redeem`
- `nov_openVault`

这些方法已在 `crates/novovm-node/src/mainline_query.rs` 的原生执行查询面注册，并进入真实 bin 的 query-mode 路由。

### 3）用户入口只做接线，不复制业务逻辑

`mainline_query` 层只负责：

- 识别方法
- 做最小参数装配
- 路由到现有 native execution 主线

不成立第二套业务逻辑，不引入旁路。

### 4）底层执行主线已统一

真实入口最终统一进入：

- `treasury.redeem`
- `amm.swap_exact_in`
- `credit_engine.open_vault`

对应执行点位于 `crates/novovm-node/src/tx_ingress.rs`：

- `dispatch_treasury_redeem_v1`
- `dispatch_amm_swap_exact_in_v1`
- `dispatch_credit_engine_open_vault_v1`
- `dispatch_native_module_execute_v1`

这意味着当前用户面已经能调到真实业务主线，而不是只停留在测试桩或 dead code。

## 冻结的入口合同

本次冻结的用户入口合同如下：

| 方法 | 模块目标 | 执行语义 | 状态 |
| --- | --- | --- | --- |
| `nov_getAssetBalance` | native execution store | 查询原生账户资产余额 | 已封盘 |
| `nov_swap` | `amm.swap_exact_in` | 在当前 policy/risk 约束下执行单跳 exact-in swap | 已封盘 |
| `nov_redeem` | `treasury.redeem` | 在当前 treasury/policy 约束下执行赎回 | 已封盘 |
| `nov_openVault` | `credit_engine.open_vault` | 在当前 collateral/risk 约束下开仓并可选铸造债务资产 | 已封盘 |

## 治理路径与用户路径边界

本次封盘明确保留如下边界：

- `用户路径`：调用现有规则执行
  - `nov_swap`
  - `nov_redeem`
  - `nov_openVault`
- `治理路径`：修改规则本身
  - `submit -> vote -> execute`
  - `UpdateTokenEconomicsPolicy`
  - `UpdateMarketGovernancePolicy`
  - `TreasurySpend`

本次封盘不把经济执行动作改成“必须先治理提案”，也不把治理路径混入用户执行路径。

## 验收基线（2026-04-18 本地实际执行）

本次封盘基于以下真实执行结果：

- `cargo fmt --all`
- `cargo check -p novovm-node`
- `cargo test -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- bin smoke：
  - `cargo run -p novovm-node --bin novovm-node --quiet`
  - 在 `NOVOVM_MAINLINE_QUERY_METHOD=nov_getAssetBalance` 下返回正常 JSON
  - 输出口径为 `source=native_execution_store`
- 真实产物级门禁：
  - `cargo run -p novovm-node --bin supervm-mainline-gate`
  - 结果：`supervm mainline gate passed`
  - 结果：`L1=100% L2=100% L3=100% L4=100% Overall=100%`

## 当前仍未开放的边界

本次封盘只宣称“真实用户入口已接通”，不宣称以下事项已经全部开放：

- 不宣称完整公共 HTTP RPC 业务面已全开放
- 不宣称完整独立 `0x1000` 原生地址面已开放
- 不宣称全部 native module registry 已对外开放
- 不宣称 `P3` 已启用
- 不宣称 multi-hop / split-order / 自动策略调参已经启用

换言之，本次封盘成立的是：

`真实用户入口可用`

而不是：

`所有经济扩展面全部开放`

## 与历史文档的关系

本文件不改写以下历史快照的原始结论，只补充一个后续里程碑事实：

- `docs_CN/CONSENSUS/NOVOVM-CONSENSUS-PUBLISHABLE-AUDIT-AND-RULES-2026-03-05.md`
- `docs_CN/CONSENSUS/NOVOVM-ECONOMIC-INFRA-MIGRATION-CHECKLIST-2026-03-07.md`
- `docs_CN/SVM2026-MIGRATION/NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md`

统一读取方式应为：

- `2026-03-05`：可发布基线 / 受限主链路快照
- `2026-03-07`：经济基础设施迁移完成度快照
- `2026-03-13`：开放业务面收口
- `2026-04-18`：真实 `novovm-node` 用户入口接通并通过真实产物级门禁

## 建议对外口径

`NOV 原生经济能力已经从“内部能力存在”推进到“真实 novovm-node 用户入口可用”，并通过真实产物级门禁验证；当前处于 Run Phase，P3 仍为 Decision Only / Not Enabled。`
