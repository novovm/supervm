# NOVOVM-NETWORK-PHASE-E-CANDIDATE-EVALUATION-2026-04-09

## 1. 目的

在 `Baseline v4 (A + B + C.2 + D.4)` 已冻结前提下，给出 Phase E 的最小候选评估，不直接进入代码实现。

本文件只回答：

1. 下一阶段优先做什么
2. 明确不做什么
3. 进入实现前的门槛

---

## 2. 当前已冻结基线

1. Phase A：拓扑闭环（routing/selector）
2. Phase B：可用性最小恢复闭环（queue/replay/reconcile）
3. Phase C.2：relay 质量反馈可解释
4. Phase D.4：capability 只读增强（token + state + anchor 导出）

结论：系统已具备“选路、降级、持久化、恢复、中继执行、反馈、能力状态统一导出”。

---

## 3. Phase E 候选方向

### 候选 E-A（推荐）

`Capability -> Policy` 最小软约束接线（保持非绑定默认）。

目标：

1. 将 `capability_route_hint / capability_availability_hint` 接入策略解释层
2. 默认只做 `advisory_first`，不强制改路由/可用性行为
3. 输出可观测“建议是否被采纳”

不做：

1. 强绑定硬切换
2. transport 改造
3. relay 多跳/NAT/discovery

价值：

1. 把 D.4 的“可见状态”推进到“可解释策略影响”
2. 风险低，不破坏现有行为

---

### 候选 E-B

Relay 深化（例如 replay 走 relay、relay 失败策略拓展）。

风险：

1. 扩执行路径复杂度
2. 与当前 Availability/Capability 收口线并行，增加回归面

建议：

1. 暂缓，待 E-A 完成后再评估

---

## 4. Phase E 选型决议

当前决议：**优先 E-A，暂缓 E-B**。

---

## 5. E-A 最小实现窗口（下刀范围）

仅允许改动：

1. `crates/novovm-network/src/capability/*`
2. `crates/novovm-network/src/availability/*`（只读解释层）
3. `crates/novovm-node/src/bin/novovm-node.rs`（日志与状态导出）

禁止改动：

1. transport 主干
2. relay data plane 行为
3. queue/replay/reconcile 语义

---

## 6. E-A 验收标准（进入实现前固定）

1. 新增日志能回答“建议是否被采纳”
2. 默认行为不变（`binding=false` 下不得影响主执行结果）
3. `cargo check -p novovm-network` 通过
4. `cargo check -p novovm-node` 通过
5. `queue_replay_smoke` 与 `relay_path_tests` 不回退

---

## 7. 下一步执行口径

下一步进入：`Phase E.0：capability-policy 只读解释接线（advisory_first）`

注意：先做最小实现与验证，再做 RFC 一行同步。

