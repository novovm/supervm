# NOVOVM 当前权威文档入口（2026-04-17）

## 目的

本文件用于明确“当前有效口径”和“历史归档口径”，避免把历史迁移文档、实验文档、旧设计文档误当成现行生产标准。

## 现行权威入口（按优先级）

1. 仓库根 README（产品定位与主线入口）
   - `README.md`
2. 运行与守门入口（EVM 维护态）
   - `.github/workflows/ci.yml`
   - `.github/workflows/mainline-nightly-soak.yml`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-EVM-NIGHTLY-SOAK-SOP-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-CORE-PLUGIN-EXTERNAL-LAYER-MAP-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-MONETARY-ARCHITECTURE-M0-M1-M2-AND-MULTI-ASSET-PAYMENT-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-TX-AND-EXECUTION-INTERFACE-DESIGN-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-NATIVE-PAYMENT-AND-TREASURY-P1-SEAL-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-DUAL-TRACK-SETTLEMENT-AND-MARKET-SYSTEM-P2A-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-CLEARING-ROUTER-P2A-SEAL-2026-04-17.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-STAGE2-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-CONSTRAINED-STRATEGY-SEAL-2026-04-18.md`
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-TREASURY-POLICY-P2C-SEAL-2026-04-18.md`（FINAL）
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-OBSERVABILITY-P2D-SEAL-2026-04-18.md`（FINAL）
3. P3 功能开关决策规范（仅决策，不启用）
   - `docs_CN/NOVOVM-NETWORK/NOVOVM-P3-FEATURE-GATE-DECISION-THRESHOLDS-2026-04-18.md`（AUTHORITATIVE）
4. 主线状态与交付契约产物
   - `artifacts/mainline-status.json`
   - `artifacts/mainline-delivery-contract.json`
   - `artifacts/mainline/mainline-nightly-soak-gate-report.json`
4. EVM 对拍门禁与样本入口
   - `crates/novovm-node/tests/fixtures/geth-parity/README.md`
   - `crates/novovm-node/tests/fixtures/geth-parity-external/README.md`

## 历史/归档文档（默认不作为现行规范）

以下目录默认视为历史上下文或专项归档，除非文档内明确声明“Current/Active”：

- `docs_CN/Old Design/`
- `docs_CN/MEV/`
- `docs_CN/SVM2026-MIGRATION/`
- `docs_CN/AOEM-FFI/archive/`
- `artifacts/audit/` 下带日期的阶段性审计清单

## 冲突处理规则

若文档口径冲突，按以下顺序裁决：

1. 代码与可执行 gate（CI/mainline/nightly）结果
2. `artifacts/mainline-status.json` 与 `artifacts/mainline-delivery-contract.json`
3. 本文件列出的“现行权威入口”
4. 其他文档（视为说明性材料）

## 维护要求

- 新增运维入口或守门入口时，必须同步更新本文件。
- 历史文档不得再写“当前已完成/当前主线”而不加日期和范围说明。
- 若后续进入 `P2-B1/P2-B2/P2-C/P2-D/P3`，需先在对应封盘文档中明确“已完成/未完成边界”，再更新本文件入口。

## 术语冻结（防止角色写反）

- `NOVOVM/SUPERVM`：宿主系统（Host）
- `AOEM`：统一执行内核
- `EVM`：插件能力线（Guest/Plugin），不是宿主系统本体

对外推荐表述：

- “EVM 插件主线已进入维护态”
- 避免使用“EVM 宿主主线”这类易被理解为“EVM=Host”的写法
