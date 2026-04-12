# NOVOVM Network Layer Responsibility Matrix (2026-04-11)

## 总则

SuperVM 生产能力必须纳入四层网络职责并由 `novovmctl` 统一调度进入主线运行。

## L1 / L2 / L3 / L4 职责矩阵

### L1（Anchor / Governance / Export Contract）
- 主责：
  - 全局锚定导出
  - 版本/字段集/指纹锁定
  - 治理口径与跨路径一致性契约
- 禁止职责：
  - 直接路由选择
  - 直接 ingress 数据面执行

### L2（Execution / Replay / Reconcile）
- 主责：
  - 执行与恢复闭环（queue/replay/reconcile）
  - state/watch/batch 三路径汇总一致性
  - L2->L1 导出合同字段稳定
- 禁止职责：
  - 自行实现 L3 候选路由策略
  - 绕过 L1 合同字段直接导出

### L3（Edge & Routing）
- 主责：
  - relay 候选链聚合、收敛、反馈
  - policy profile / family / governance / guardrail
  - 统一只读面导出（readonly/state/anchor）
- 禁止职责：
  - 交易执行语义定义
  - transport 底层实现替代 L4

### L4（Access / Local Evidence）
- 主责：
  - 本地可达性证据、freshness、入口接入
  - direct/relay 候选基础信息
- 禁止职责：
  - L1 治理契约定义
  - L2 执行恢复策略定义

## 跨层调用边界

- 允许：L4 -> L3 -> L2 -> L1（只读与执行语义按层上行）
- 允许：L1 对 L2/L3/L4 只读导出字段做一致性门控
- 禁止：L3/L4 直写 L1 合同导出
- 禁止：任一层绕过 `novovmctl` 生产调度入口

