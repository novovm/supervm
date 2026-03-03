# NOVOVM 迁移前审计报告（SVM2026）- 2026-03-03

## 1. 审计目标

- 对工作区 `D:\WorksArea\SVM2026` 做迁移前结构审计。
- 明确哪些能力可直接迁、哪些必须重构、哪些应暂缓。
- 为 NOVOVM 新架构文档与迁移计划提供证据基础。

> 说明：本次是“架构与工程审计”，不是密码学/共识算法正确性再验证。

## 2. 审计输入

- `D:\WorksArea\SVM2026\ROADMAP.md`
- `D:\WorksArea\SVM2026\Cargo.toml`
- `D:\WorksArea\SVM2026\src\vm-runtime\src\lib.rs`
- `D:\WorksArea\SVM2026\supervm-node\src\main.rs`
- `D:\WorksArea\SVM2026\supervm-consensus\src\lib.rs`
- `D:\WorksArea\SVM2026\supervm-chainlinker-api\src\lib.rs`
- `D:\WorksArea\SVM2026\src\l0-atomic\src\*.rs`
- `D:\WorksArea\SVM2026\src\l2-executor\src\*.rs`
- `D:\WorksArea\SVM2026\src\l4-network\src\*.rs`

## 3. 关键事实（可量化）

### 3.1 Workspace 规模与混合度

- `Cargo.toml` workspace members：`58`
- 其中 AOEM 相关成员：`23`
- `src/*` 成员：`13`
- 顶层 `supervm-*` 成员：`7`
- `plugins/*` 成员：`8`

结论：`SVM2026` 既包含 AOEM 子树，又包含历史 `src/*` 和顶层 `supervm-*`，混合度高。

### 3.2 单体热点

`src/vm-runtime` Rust 文件数约 `313`，显著高于其他 crate（如 `gpu-executor=108`、`l2-executor=32`、`l4-network=24`）。

结论：`vm-runtime` 仍是高耦合中心，迁移时不能直接整体搬运。

### 3.3 未完成功能热点（TODO/FIXME/unimplemented）

高频热点（节选）：

- `src/vm-runtime/src/chain_linker/cross_mining.rs`（9）
- `src/vm-runtime/src/chain_linker/atomic_swap.rs`（8）
- `src/vm-runtime/src/chain_linker/cross_contract.rs`（6）
- `src/vm-runtime/src/shard_coordinator.rs`（6）
- `src/l4-network/src/storage.rs`（6）
- `src/vm-runtime/src/financial_metrics.rs`（5）

结论：跨链适配、跨分片协调、部分网络存储与经济指标仍有明显“骨架先行”痕迹。

### 3.4 ROADMAP 一致性问题

`ROADMAP.md` 同时存在：

- “核心系统完成度 100%”的表述；
- 以及 “L3 5%/25%” 与 “L4 85%/100%” 等并存状态块。

结论：`ROADMAP` 更像历史迭代日志，不再适合作为 NOVOVM 生产建设基线。

## 4. 架构层面审计结论

## 4.1 旧结构的核心问题

- 核心执行、协议接口、共识、应用能力在 `vm-runtime + node` 时代存在交叉。
- 名称与目录双轨并存（`src/*`、`supervm-*`、`aoem/*`），认知成本高。
- “完成度叙事”与“代码可生产化状态”不是同一口径。

## 4.2 可迁移性分级

### A 级（优先复用，低风险）

- AOEM 执行内核与运行时寄宿链路（你当前已完成的基础）
- `supervm-consensus`（边界相对清晰）
- `supervm-chainlinker-api`（接口层可继续沿用）

### B 级（可迁移，但需重构接入）

- `src/l2-executor`（作为 NOVOVM 证明服务需与新执行回执打通）
- `src/l4-network`、`src/web3-storage`（网络与存储边界需拆分）
- `supervm-node`（从“历史宿主”改为 NOVOVM 编排入口）

### C 级（暂缓，最后处理）

- `src/vm-runtime/src/chain_linker/*` 大量跨链业务逻辑
- `plugins/*` 各链插件全量能力
- 历史应用层杂糅模块（需按新分类进入独立工程）

## 5. 对 NOVOVM 的直接约束

1. AOEM 只作为底座层，不再被业务模块直接调用 C ABI。
2. 宿主统一经 `novovm-exec` 门面层提交执行请求。
3. 新文档必须把“核心发布范围”和“可选生态范围”分离。
4. “逐项迁入已验证能力”必须放在最终阶段执行，不前置。

## 6. 审计后的下一决策

- 先冻结 NOVOVM 新全景架构与模块分类。
- 先完成执行回执/状态根/性能口径三类契约。
- 再进入“逐项能力迁入”。

这与当前你的判断一致：先把设计、架构、功能开发文档探讨清楚，再开迁移实施。
