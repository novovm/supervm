# EVM Adapter 文档索引（SUPERVM）

## 1. 当前主线目标

- 终局：`EVM Rust 全功能镜像节点`，不是“兼容适配层”。
- 架构：外部可用 `HTTP/JSON-RPC`，内部固定二进制流水线（`opsw1 -> novovm-node -> AOEM`）。
- 原则：性能优先、生产优先、减少非必要工程化包装。

## 2. 进度与规范入口

- 迁移台账（生产主线口径）  
  `NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`

- 全功能镜像节点模式规范  
  `NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md`

- 迁移方案（阶段拆解）  
  `NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`

- geth 能力清单与迁移取舍  
  `NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md`

- Ethereum 兼容基线  
  `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md`

- 统一账户与 EVM Persona 映射  
  `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md`

- 原子协调层边界  
  `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md`

- 外部入口边界与内部二进制流水线约束  
  `NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md`

## 3. 去工程化说明

- `NOVOVM-EVM-ADAPTER-STRICT-V2-MERGE-CHECKLIST-2026-03-07.md` 已转为历史归档。
- 脚本与观测产物可作为调试工具，但不再作为“完成度定义”。
- 主线推进只认生产代码接线与可复现实跑闭环。
