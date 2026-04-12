# NOVOVM Network Module Layer Map (2026-04-11)

## 映射原则

模块归层按“主职责”确定。跨层调用只允许通过既定接口，不允许反向穿透。

## L1（Anchor / Governance / Export）

- `crates/novovm-node/src/bin/novovm-node.rs` 中 L2/L1 contract/export/anchor 汇总区
- `crates/novovm-consensus` 中治理与合同字段绑定部分

## L2（Execution / Recovery）

- `crates/novovm-network/src/availability/*`
- `crates/novovm-node/src/bin/novovm-node.rs` 中 queue/replay/reconcile 路径

## L3（Edge / Routing）

- `crates/novovm-network/src/routing/*`
- `crates/novovm-network/src/relay/*`
- `crates/novovm-node/src/bin/novovm-node.rs` 中 L3 policy profile/family/governance/guardrail 决策接线

## L4（Ingress / Local Evidence）

- `crates/novovm-network/src/transport/*`
- `crates/novovm-node/src/bin/novovm-node.rs` 中 ingress source、local observed、L4 readonly 导出

## 调度与控制面

- 统一调度入口：`crates/novovmctl/*`
- 受控启动接线：
  - `crates/novovmctl/src/integration/node_binary.rs`
  - `crates/novovmctl/src/commands/lifecycle.rs`

## 运维与脚本层

- `scripts/*.ps1` 仅允许作为 `novovmctl` 兼容壳或迁移工具。
- 生产路径不得把脚本层变成独立调度主线。

