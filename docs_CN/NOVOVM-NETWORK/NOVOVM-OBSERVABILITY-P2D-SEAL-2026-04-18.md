# NOVOVM 可观测层 P2-D 封盘（2026-04-18）

Status: SEALED（Authoritative）  
Scope: 原生 clearing/policy/settlement 主线的 Execution Trace + Metrics Summary + Debug Query 合同

## 目的

本文件用于封盘 P2-D 可观测层，冻结当前已进入主线的 trace/query/metrics 合同面，作为后续 P3 是否启用的证据层。

P2-D 是观测层，不改变 clearing、路由或 policy 的执行语义。

## 阶段状态

- `P0`：已封盘
- `P1`：已封盘
- `P2-A`：已签收
- `P2-B1`：已签收
- `P2-B2`：已签收
- `P2-C`：FINAL 封盘
- `P2-D`：已签收并封盘

## 已封盘范围（代码事实）

### 1）Execution Trace 合同已成立

冻结对象：

- `NovExecutionTraceV1`
- `NovTraceQuotePhaseV1`
- `NovTraceRoutingPhaseV1`
- `NovTraceClearingPhaseV1`
- `NovTraceSettlementPhaseV1`

单笔 trace 可覆盖 quote、候选/最终路由、clearing 结果、settlement 结果、policy 上下文与最终失败码。

### 2）Trace 持久化已成立

冻结持久化合同：

- `last_execution_trace`
- `execution_traces_by_tx`
- `execution_trace_order`
- `NOV_EXECUTION_TRACE_MAX_ENTRIES_V1` 上限保留

### 3）原生调试查询已成立

冻结 `treasury` 模块方法：

- `get_last_execution_trace`
- `get_execution_trace_by_tx`
- `get_clearing_metrics_summary`
- `get_policy_metrics_summary`

### 4）外部 `nov_*` 调试包装已成立

冻结外部包装：

- `nov_getExecutionTrace`
- `nov_getTreasuryClearingMetricsSummary`
- `nov_getTreasuryPolicyMetricsSummary`

### 5）Metrics 合同已成立

clearing summary 固定统计项（示例）：

- `total_clearing_attempts`
- `successful_clearings`
- `failed_clearings`
- `route_source_hits`
- `route_source_failures`
- `selection_reason_hits`
- `failure_counts`

policy summary 固定统计项（示例）：

- `policy_contract_id`
- `policy_source`
- `threshold_state`
- `constrained_strategy`
- `threshold_state_hits`
- `constrained_strategy_hits`
- `policy_event_state_hits`

## 冻结的非语义边界

P2-D 冻结边界：

- 只记录/暴露执行事实
- 不改变已封盘的 policy/routing 主线行为
- 不自动启用任何 P3 路由能力

## 验收基线

- `cargo fmt --all --check`
- `cargo check -p novovm-node`
- `cargo clippy -p novovm-node --all-targets -- -D warnings`
- `cargo test -p novovm-node`
- `cargo run -p novovm-node --bin supervm-mainline-gate`
- `cargo deny check --disable-fetch`（按现行策略允许非阻塞 warning）

## 明确不包含

以下不属于 P2-D：

- multi-hop 路由启用
- split-order 路由启用
- 自动策略调参引擎
- 对 P2-C 已封盘 policy 合同的隐式变更

## 封盘后 P3 判定边界

P3 是否启用应基于 P2-D 真实观测数据，不以预设扩展为依据。  
最少应以 route 命中/失败分布与 policy 状态分布作为判定输入。

## 建议对外口径

`P2-D 已建立原生 clearing 与 policy 主线的可观测合同（trace + metrics + debug query）；P3 启用应基于真实数据判定。`

