# NOVOVM 核心层 / 插件层 / 对外能力层分层图（2026-04-17）

## 1. 目的

统一团队口径，避免把“当前最成熟的 EVM 能力线”误解为“EVM 是系统宿主”。

## 2. 三层结构（冻结口径）

```text
NOVOVM / SUPERVM (Host)
├─ Core Host Layer（核心层）
│  ├─ AOEM 执行内核
│  ├─ 调度与运行时（scheduler/runtime/gate）
│  ├─ 预算隔离（网络/执行/存储/查询）
│  └─ canonical chain / lifecycle / reorg 裁决
│
├─ Plugin Layer（插件层）
│  ├─ EVM 插件（当前已维护态）
│  ├─ 未来链插件（BTC/SOL/...）
│  └─ 其他执行插件（AI/专项协议）
│
└─ External Capability Layer（对外能力层）
   ├─ 标准查询与提交接口（如 eth_*）
   ├─ parity gate / nightly soak / duty report
   └─ 运维入口与审计产物
```

## 3. 角色定义

- Host（宿主）只属于 `NOVOVM/SUPERVM`。
- `EVM` 是 Plugin（被承载能力），不是 Host。
- “EVM 主线完成”表示插件能力成熟，不表示系统本体等于 EVM。

## 4. 对外表述规范

推荐：

- “NOVOVM 已完成 EVM 插件主线并进入维护态”
- “EVM 是 NOVOVM 上第一个成熟插件能力”

避免：

- “EVM 宿主系统”
- “SUPERVM = EVM 改造版节点”

## 5. 研发资源分配原则

- EVM 线：维护态（样本喂养、nightly 守门、预算稳态）
- 主资源：回归 NOVOVM 核心层与下一插件能力建设

