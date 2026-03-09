# EVM Adapter 迁移文档索引（SUPERVM）

## 文档入口

- 迁移方案与实施步骤  
  `NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`
  - 含：`EVM` 分支镜像模式架构（`eth_*` 入口）与 `SUPERVM First` 功能重叠策略。
  - 口径：`内核统一、外观多态`；`web30_*` 保持主链语义，EVM Persona 不覆盖 WEB30 入口。

- 迁移进度台账  
  `NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
  - 含：EVM-A13~A15（镜像交互面、重叠盘点、路由策略）跟踪项。

- go-ethereum 功能清单与迁移取舍建议  
  `NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md`
  - 含：geth 功能全景、逐项“需要/不需要”建议、`P0/P1/P2` 路由取舍。
  - 口径：以太坊是多链插件之一（后续 BTC/Solana 同框架扩展），当前仅聚焦 EVM 分支。

- 全链唯一账户映射规范  
  `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md`
  - 含：UCA 与 EVM Persona 地址映射、签名域隔离、nonce/权限、Type 4 策略位。

- 多链原子交易协调层规范  
  `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md`
  - 含：原子能力归属、状态机、补偿机制、`eth_*` 与 `web30_*` 边界。

- WEB30 ↔ EVM 语义映射矩阵  
  `NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md`
  - 含：等价/桥接/不可映射分类、默认路由与拒绝策略。

- Ethereum Profile 2026 兼容基线  
  `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md`
  - 含：profile 必填字段、tx type 0/1/2/3/4、M0~M2 兼容阶段与 gate 基线。

- 外部入口边界与二进制流水线架构约束  
  `NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md`
  - 含：`HTTP/JSON-RPC` 仅外部边界层可用；`D1/D2/D3/AOEM` 内部统一二进制流水线；插件接入不修改主入口。
  - 含：`novovm-node` 支持 `NOVOVM_OPS_WIRE_DIR` 批量消费 `.opsw1`，用于承接边界层二进制落盘队列。

- 生产路径一键脚本（边界网关 -> 主线二进制消费）  
  `scripts/migration/run_gateway_node_pipeline.ps1`
  - 含：启动 `novovm-edge-gateway`、轮询 `spool`、调用 `novovm-node` 消费 `.opsw1`，成功/失败分流归档。

- 最小真实链路冒烟脚本（3 请求产出 `.opsw1`）  
  `scripts/migration/run_gateway_node_smoke.ps1`
  - 含：`ua_createUca -> ua_bindPersona -> eth_sendRawTransaction` 最小链路，验证边界层产线输出。

## 备注

- `TEMP-LOG/` 用于临时沟通记录与短期笔记，不作为正式发布文档。
- 正式进度状态以迁移台账为准。
