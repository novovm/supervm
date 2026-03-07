# WEB30 Protocol Migration Index (2026-03-05)

## 1. 目标

本索引用于把 `SVM2026` 的 WEB30 协议族群文档快速切回主线，并明确：

- 文档在 NOVOVM 架构中的位置
- 已迁移的参考文档快照
- 代码迁移是否已进入 NOVOVM 主链路

## 2. 架构归位

WEB30 协议族群定位在应用/经济治理层，位于共识内核之上：

- 共识内核：`novovm-consensus`（投票、QC、view-change、pacemaker、slash policy）
- WEB30 协议族群：发币/治理/DeFi/域服务/多链扩展等业务语义

对应迁移台账维度（功能口径）：

- `F-10` Web3 存储服务
- `F-11` 域名系统
- `F-12` DeFi 核心
- `F-13` 多链插件能力

## 3. 文档快照迁移（Source -> SUPERVM）

源目录：

- `D:\WEB3_AI\SVM2026\contracts\web30`

目标目录：

- `D:\WEB3_AI\SUPERVM\docs_CN\WEB30-PROTOCOL\SVM2026-REFERENCE`

映射表：

| Source | Snapshot Path | Status |
| --- | --- | --- |
| `contracts/web30/INDEX.md` | `SVM2026-REFERENCE/INDEX.md` | Migrated |
| `contracts/web30/README.md` | `SVM2026-REFERENCE/WEB30-README.md` | Migrated |
| `contracts/web30/QUICKSTART.md` | `SVM2026-REFERENCE/QUICKSTART.md` | Migrated |
| `contracts/web30/IMPLEMENTATION.md` | `SVM2026-REFERENCE/IMPLEMENTATION.md` | Migrated |
| `contracts/web30/DELIVERY-REPORT.md` | `SVM2026-REFERENCE/DELIVERY-REPORT.md` | Migrated |
| `contracts/web30/TOKEN-IMPLEMENTATION.md` | `SVM2026-REFERENCE/TOKEN-IMPLEMENTATION.md` | Migrated |
| `contracts/web30/TOKEN-QUICKREF.md` | `SVM2026-REFERENCE/TOKEN-QUICKREF.md` | Migrated |
| `contracts/web30/sdk/README.md` | `SVM2026-REFERENCE/WEB30-SDK-README.md` | Migrated |
| `contracts/web30/sdk/TROUBLESHOOTING.md` | `SVM2026-REFERENCE/WEB30-SDK-TROUBLESHOOTING.md` | Migrated |

兼容别名：

- `SVM2026-REFERENCE/README.md` -> `WEB30-README.md`
- `SVM2026-REFERENCE/TROUBLESHOOTING.md` -> `WEB30-SDK-TROUBLESHOOTING.md`

## 4. 标准规范迁移（权威设计源）

源目录：

- `D:\WEB3_AI\SVM2026\standards`

目标目录：

- `D:\WEB3_AI\SUPERVM\docs_CN\WEB30-PROTOCOL\SVM2026-REFERENCE\standards`

状态：

- 共 15 份标准文档已迁入。
- 详细清单见：`SVM2026-REFERENCE/STANDARDS-INDEX.md`

## 5. 标准到功能域映射（新增）

- 映射文档：`NOVOVM-WEB30-STANDARDS-F10-F13-MAPPING-2026-03-07.md`
- 用途：把 `SVM2026/standards` 逐项映射到 `F-10~F-13` 并展示当前进度。

## 6. 主链路迁移状态（代码）

截至 2026-03-07：

- WEB30 参考文档已迁入 `SUPERVM/docs_CN/WEB30-PROTOCOL`。
- WEB30 标准规范（`SVM2026/standards`）已迁入参考区。
- NOVOVM 主链路中，WEB30 对应能力（`F-10~F-13`）仍处于未完成迁移阶段。
- 因此发布口径仍为“共识主干可发布”，不是“完整主网经济治理版已全量迁移”。

## 7. 下一步（建议）

1. 以 `SVM2026-REFERENCE/standards` 为语义基线，给 `F-10~F-13` 建立标准到能力的映射矩阵。
2. 为 `F-10~F-13` 建立最小骨架 crate 与 RPC/CLI 接口草图，先打通路由与门禁。
3. 对每个子域增加正/负向门禁，纳入统一 acceptance gate 汇总。
